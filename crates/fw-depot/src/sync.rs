//! `gclient sync`: bring a checkout to the revisions its DEPS files pin. Git dependencies are fetched in parallel as soon as the DEPS file of
//! their parent is on disk; CIPD packages are installed in one ensure; GCS objects are downloaded, verified and unpacked; then the hooks run.

use std::collections::{BTreeMap, VecDeque};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use crate::cipd::{self, Client, Instance, Want};
use crate::deps::Platform;
use crate::error::{Error, Result, fs};
use crate::gclient::{Config, Graph, HookEntry, Kind};
use crate::git::{self, History};
use crate::http;

#[derive(Debug, Clone)]
pub struct Options {
    pub jobs: usize,
    pub run_hooks: bool,
    pub history: History,
    /// `path` -> revision, from `--revision path@rev`.
    pub revisions: Vec<(String, String)>,
    pub cipd: bool,
    pub gcs: bool,
    /// Print what would be done and touch nothing.
    pub dry_run: bool,
    /// Only dependencies whose path starts with one of these.
    pub only: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            jobs: std::thread::available_parallelism().map_or(8, |count| count.get().min(16)),
            run_hooks: true,
            history: History::Full,
            revisions: Vec::new(),
            cipd: true,
            gcs: true,
            dry_run: false,
            only: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Cloned,
    Updated,
    Unchanged,
    Planned,
}

#[derive(Debug, Clone)]
pub struct GitResult {
    pub path: String,
    pub revision: String,
    pub action: Action,
    pub elapsed: Duration,
}

#[derive(Debug, Default)]
pub struct Report {
    pub git: Vec<GitResult>,
    pub cipd: Vec<(Want, Instance, bool)>,
    pub gcs: Vec<(String, bool)>,
    pub hooks: Vec<(String, Duration)>,
    pub elapsed: Duration,
}

enum Task {
    Git {
        index: usize,
        dir: PathBuf,
        url: String,
        revision: Option<String>,
    },
    Gcs {
        index: usize,
        dir: PathBuf,
        bucket: String,
        object_name: String,
        sha256sum: String,
        size_bytes: u64,
        output_file: Option<String>,
    },
}

enum Done {
    Git(usize, Result<(String, Action, Duration)>),
    Gcs(usize, Result<bool>),
}

fn with_revision(url: &str, revision: &str) -> String {
    match url.rsplit_once('@') {
        Some((base, old)) if !old.contains('/') && !old.contains(':') => format!("{base}@{revision}"),
        _ => format!("{url}@{revision}"),
    }
}

fn wanted(only: &[String], path: &str) -> bool {
    only.is_empty() || only.iter().any(|prefix| path.starts_with(prefix.as_str()))
}

pub fn sync(config: Config, host: &Platform, options: &Options) -> Result<(Graph, Report)> {
    let started = Instant::now();
    let root = config.root.clone();
    let mut graph = Graph::new(config, host);
    let mut report = Report::default();
    let (task_sender, task_receiver) = mpsc::channel::<Task>();
    let (done_sender, done_receiver) = mpsc::channel::<Done>();
    let task_receiver = Arc::new(Mutex::new(task_receiver));
    let history = options.history;
    let dry_run = options.dry_run;

    let outcome: Result<()> = std::thread::scope(|scope| {
        for _ in 0..options.jobs.max(1) {
            let receiver = Arc::clone(&task_receiver);
            let sender = done_sender.clone();
            scope.spawn(move || {
                loop {
                    let task = match receiver.lock() {
                        Ok(guard) => guard.recv(),
                        Err(_) => break,
                    };
                    let Ok(task) = task else { break };
                    let message = match task {
                        Task::Git { index, dir, url, revision } => {
                            Done::Git(index, git_task(&dir, &url, revision.as_deref(), history, dry_run))
                        }
                        Task::Gcs {
                            index,
                            dir,
                            bucket,
                            object_name,
                            sha256sum,
                            size_bytes,
                            output_file,
                        } => Done::Gcs(
                            index,
                            gcs_task(&dir, &bucket, &object_name, &sha256sum, size_bytes, output_file.as_deref(), dry_run),
                        ),
                    };
                    if sender.send(message).is_err() {
                        break;
                    }
                }
            });
        }
        drop(done_sender);

        let mut ready: VecDeque<usize> = (0..graph.deps.len()).collect();
        let mut inflight = 0usize;
        let mut failure: Option<Error> = None;
        let mut paths: BTreeMap<usize, String> = BTreeMap::new();
        while failure.is_none() && (!ready.is_empty() || inflight > 0) {
            while let Some(index) = ready.pop_front() {
                let dep = graph.deps[index].clone();
                if !wanted(&options.only, &dep.path) && !dep.is_solution {
                    continue;
                }
                match &dep.kind {
                    Kind::Git => {
                        let Some((url, revision)) = dep.git() else {
                            expand_from_disk(&mut graph, &root, index, &mut ready)?;
                            continue;
                        };
                        let overridden = options
                            .revisions
                            .iter()
                            .find(|(path, _)| *path == dep.path)
                            .map(|(_, revision)| revision.as_str());
                        let revision = overridden.or(revision).map(str::to_owned);
                        if let Some(revision) = overridden {
                            graph.deps[index].url = Some(with_revision(url, revision));
                        }
                        let managed = !dep.is_solution
                            || graph.config.solutions.iter().any(|solution| {
                                solution.name == dep.path && solution.managed
                            });
                        if !managed {
                            expand_from_disk(&mut graph, &root, index, &mut ready)?;
                            continue;
                        }
                        paths.insert(index, dep.path.clone());
                        let _ = task_sender.send(Task::Git {
                            index,
                            dir: root.join(&dep.path),
                            url: url.to_owned(),
                            revision,
                        });
                        inflight += 1;
                    }
                    Kind::Cipd { .. } => {}
                    Kind::Gcs { bucket, object_name, sha256sum, size_bytes, output_file } => {
                        if options.gcs {
                            let _ = task_sender.send(Task::Gcs {
                                index,
                                dir: root.join(&dep.path),
                                bucket: bucket.clone(),
                                object_name: object_name.clone(),
                                sha256sum: sha256sum.clone(),
                                size_bytes: *size_bytes,
                                output_file: output_file.clone(),
                            });
                            inflight += 1;
                        }
                    }
                }
            }
            if inflight == 0 {
                break;
            }
            let Ok(done) = done_receiver.recv() else { break };
            inflight -= 1;
            match done {
                Done::Git(index, Ok((revision, action, elapsed))) => {
                    report.git.push(GitResult {
                        path: graph.deps[index].path.clone(),
                        revision,
                        action,
                        elapsed,
                    });
                    if graph.deps[index].recurse {
                        let fresh = expand_from_disk(&mut graph, &root, index, &mut ready);
                        if let Err(error) = fresh {
                            failure = Some(error);
                            continue;
                        }
                        if !dry_run {
                            let hooks = graph.pre_deps_hooks(index);
                            if let Err(error) = run_hooks(&graph, &hooks, &mut report) {
                                failure = Some(error);
                            }
                        }
                    }
                }
                Done::Git(index, Err(error)) => {
                    failure = Some(Error::message(format!(
                        "{}: {error}",
                        paths.get(&index).map_or("?", String::as_str)
                    )));
                }
                Done::Gcs(index, Ok(changed)) => {
                    report.gcs.push((graph.deps[index].name.clone(), changed));
                }
                Done::Gcs(index, Err(error)) => {
                    failure = Some(Error::message(format!("{}: {error}", graph.deps[index].name)));
                }
            }
        }
        drop(task_sender);
        failure.map_or(Ok(()), Err)
    });
    outcome?;

    if options.cipd {
        let wants = cipd_wants(&graph, &options.only);
        if !wants.is_empty() {
            report.cipd = if dry_run {
                let client = Client::new(cipd::DEFAULT_SERVICE);
                let mut planned = Vec::new();
                for want in wants {
                    let instance = client.resolve(&want.package, &want.version)?;
                    planned.push((want, instance, true));
                }
                planned
            } else {
                Client::new(cipd::DEFAULT_SERVICE).ensure(&root, &wants, options.jobs)?
            };
        }
    }
    if !dry_run {
        for index in 0..graph.deps.len() {
            if let Some((path, text)) = graph.gn_args_text(index)? {
                fs::write(&path, text)?;
            }
        }
        write_entries(&graph, &root)?;
        if options.run_hooks {
            let hooks = graph.hooks()?;
            run_hooks(&graph, &hooks, &mut report)?;
        }
    }
    report.elapsed = started.elapsed();
    Ok((graph, report))
}

/// The CIPD packages of the graph, with gclient's `${platform}` left for CIPD's own expansion on the host.
pub fn cipd_wants(graph: &Graph, only: &[String]) -> Vec<Want> {
    let (os, arch) = cipd::host_platform();
    graph
        .deps
        .iter()
        .filter(|dep| wanted(only, &dep.path))
        .filter_map(|dep| match &dep.kind {
            Kind::Cipd { package, version } => Some(Want {
                subdir: dep.path.clone(),
                package: cipd::expand(package, os, arch),
                version: cipd::expand(version, os, arch),
            }),
            _ => None,
        })
        .collect()
}

fn expand_from_disk(
    graph: &mut Graph,
    root: &Path,
    index: usize,
    ready: &mut VecDeque<usize>,
) -> Result<()> {
    let file = root.join(&graph.deps[index].path).join(&graph.deps[index].deps_file);
    if !graph.deps[index].recurse || !file.is_file() {
        return Ok(());
    }
    let text = fs::read_to_string(&file)?;
    ready.extend(graph.expand(index, &text)?);
    Ok(())
}

fn git_task(
    dir: &Path,
    url: &str,
    revision: Option<&str>,
    history: History,
    dry_run: bool,
) -> Result<(String, Action, Duration)> {
    let started = Instant::now();
    if dry_run {
        return Ok((revision.unwrap_or("HEAD").to_owned(), Action::Planned, started.elapsed()));
    }
    let before = git::head(dir);
    let existed = git::is_repository(dir);
    let revision = git::checkout(dir, url, revision, history)?;
    let action = if !existed {
        Action::Cloned
    } else if before.as_deref() == Some(revision.as_str()) {
        Action::Unchanged
    } else {
        Action::Updated
    };
    Ok((revision, action, started.elapsed()))
}

fn gcs_task(
    dir: &Path,
    bucket: &str,
    object_name: &str,
    sha256sum: &str,
    size_bytes: u64,
    output_file: Option<&str>,
    dry_run: bool,
) -> Result<bool> {
    let file_name = object_name.replace(['/', '\\'], "_");
    let prefix = file_name.replace('.', "_");
    let hash_file = dir.join(format!(".{prefix}_hash"));
    let artifact = dir.join(output_file.unwrap_or(&format!(".{file_name}")));
    let current = std::fs::read_to_string(&hash_file).ok();
    if current.as_deref().map(str::trim) == Some(sha256sum) && artifact.exists() {
        return Ok(false);
    }
    if dry_run {
        return Ok(true);
    }
    for stale in [&hash_file, &artifact] {
        if stale.symlink_metadata().is_ok() {
            std::fs::remove_file(stale).map_err(|source| Error::io(stale, source))?;
        }
    }
    let encoded: String = object_name
        .split('/')
        .map(|part| part.replace('%', "%25").replace(' ', "%20").replace('+', "%2B"))
        .collect::<Vec<_>>()
        .join("/");
    let url = format!("https://storage.googleapis.com/{bucket}/{encoded}");
    let got = http::download(&url, &[], &artifact)?;
    if got != sha256sum {
        return Err(Error::message(format!(
            "sha256sum does not match calculated hash. {sha256sum} vs {got}"
        )));
    }
    let size = std::fs::metadata(&artifact)
        .map_err(|source| Error::io(&artifact, source))?
        .len();
    if size != size_bytes {
        return Err(Error::message(format!(
            "size_bytes does not match calculated size bytes. {size_bytes} vs {size}"
        )));
    }
    let names = extract(&artifact, dir)?;
    if let Some(names) = &names {
        fs::write(
            &dir.join(format!(".{prefix}_content_names")),
            serde_json::to_vec(names)?,
        )?;
    } else {
        make_executable(&artifact)?;
    }
    fs::write(&hash_file, sha256sum)?;
    Ok(true)
}

enum Compression {
    None,
    Gzip,
    Xz,
}

/// Unpacks a tar (plain, gzip or xz) or a zip into `dir`; `None` when the file is neither.
fn extract(artifact: &Path, dir: &Path) -> Result<Option<Vec<String>>> {
    let mut magic = [0u8; 6];
    let mut file = std::fs::File::open(artifact).map_err(|source| Error::io(artifact, source))?;
    let count = file.read(&mut magic).map_err(|source| Error::io(artifact, source))?;
    let magic = &magic[..count];
    let compression = if magic.starts_with(&[0x1f, 0x8b]) {
        Compression::Gzip
    } else if magic.starts_with(&[0xfd, b'7', b'z', b'X', b'Z', 0]) {
        Compression::Xz
    } else if magic.starts_with(b"PK") {
        return extract_zip(artifact, dir).map(Some);
    } else {
        Compression::None
    };
    let open = || std::fs::File::open(artifact).map_err(|source| Error::io(artifact, source));
    let reader: Box<dyn Read> = match compression {
        Compression::Gzip => Box::new(flate2::read::GzDecoder::new(open()?)),
        Compression::Xz => Box::new(lzma_rust2::XzReader::new(open()?, false)),
        Compression::None => Box::new(open()?),
    };
    if matches!(compression, Compression::None) && !is_tar(artifact) {
        return Ok(None);
    }
    let mut archive = tar::Archive::new(reader);
    let mut names = Vec::new();
    let entries = match archive.entries() {
        Ok(entries) => entries,
        Err(_) => return Ok(None),
    };
    for (position, entry) in entries.enumerate() {
        let mut entry = match entry {
            Ok(entry) => entry,
            Err(_) if position == 0 => return Ok(None),
            Err(source) => return Err(Error::io(artifact, source)),
        };
        let name = entry
            .path()
            .map_err(|source| Error::io(artifact, source))?
            .to_string_lossy()
            .replace('\\', "/");
        let cleaned = name.trim_start_matches("./").to_owned();
        if cleaned.split('/').any(|part| part == "..") || cleaned.starts_with('/') {
            return Err(Error::message(format!("tarfile contains invalid entry {name}")));
        }
        if cleaned.starts_with(".git") || cleaned.starts_with(".jj") {
            return Err(Error::message(format!("archive has a top-level {cleaned} entry")));
        }
        entry
            .unpack_in(dir)
            .map_err(|source| Error::io(&dir.join(&cleaned), source))?;
        names.push(name);
    }
    Ok(Some(names))
}

/// A tar header carries `ustar` at offset 257.
fn is_tar(path: &Path) -> bool {
    let mut header = [0u8; 262];
    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut header))
        .is_ok()
        && &header[257..262] == b"ustar"
}

fn extract_zip(artifact: &Path, dir: &Path) -> Result<Vec<String>> {
    let file = std::fs::File::open(artifact).map_err(|source| Error::io(artifact, source))?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|error| Error::message(format!("{}: {error}", artifact.display())))?;
    let names: Vec<String> = zip
        .file_names()
        .filter_map(|name| name.ok().map(|name| name.into_owned()))
        .collect();
    zip.extract(dir)
        .map_err(|error| Error::message(format!("{}: {error}", artifact.display())))?;
    Ok(names)
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .map_err(|source| Error::io(path, source))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

fn write_entries(graph: &Graph, root: &Path) -> Result<()> {
    let mut text = String::from("entries = {\n");
    for line in graph.revinfo() {
        if let Some((name, url)) = line.split_once(": ") {
            text.push_str(&format!("  '{name}': '{url}',\n"));
        }
    }
    text.push_str("}\n");
    fs::write(&root.join(".gclient_entries"), text)
}

/// Runs hooks in order. `vpython3` is looked up on `PATH`; without it, `python3` stands in.
pub fn run_hooks(graph: &Graph, hooks: &[HookEntry], report: &mut Report) -> Result<()> {
    for hook in hooks {
        if !graph.hook_applies(hook)? {
            continue;
        }
        let Some((program, arguments)) = hook.action.split_first() else {
            continue;
        };
        let program = resolve_program(program);
        let started = Instant::now();
        let label = hook.name.clone().unwrap_or_else(|| program.clone());
        eprintln!("Running hook: {label}");
        let status = Command::new(&program)
            .args(arguments)
            .current_dir(&hook.cwd)
            .status()
            .map_err(|source| Error::io(Path::new(&program), source))?;
        report.hooks.push((label.clone(), started.elapsed()));
        if !status.success() {
            return Err(Error::message(format!(
                "hook '{label}' ({}) exited with {status}",
                hook.action.join(" ")
            )));
        }
    }
    Ok(())
}

fn resolve_program(program: &str) -> String {
    if program == "vpython3" && which("vpython3").is_none() {
        return if cfg!(windows) { "python".to_owned() } else { "python3".to_owned() };
    }
    if program == "vpython3" && cfg!(windows) {
        return "vpython3.bat".to_owned();
    }
    program.to_owned()
}

fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(program))
        .find(|candidate| candidate.is_file())
}

/// SHA-256 of a file, hex.
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path).map_err(|source| Error::io(path, source))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let count = file.read(&mut buffer).map_err(|source| Error::io(path, source))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// The raw text of `file` at `revision` of the repository at `url`, through Gitiles or raw.githubusercontent.com, without a clone.
pub fn remote_file(url: &str, revision: &str, file: &str) -> Result<Option<String>> {
    use base64::Engine;
    let base = url.trim_end_matches('/').trim_end_matches(".git");
    let target = if let Some(rest) = base.strip_prefix("https://github.com/") {
        format!("https://raw.githubusercontent.com/{rest}/{revision}/{file}")
    } else {
        format!("{base}/+/{revision}/{file}?format=TEXT")
    };
    let response = http::get(&target, &[])?;
    if response.status == 404 {
        return Ok(None);
    }
    if !(200..300).contains(&response.status) {
        return Err(Error::Http { url: target, message: format!("status {}", response.status) });
    }
    if target.contains("format=TEXT") {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(response.body.iter().copied().filter(|byte| !byte.is_ascii_whitespace()).collect::<Vec<u8>>())
            .map_err(|error| Error::message(format!("{target}: {error}")))?;
        return Ok(Some(String::from_utf8_lossy(&decoded).into_owned()));
    }
    Ok(Some(response.text()))
}

/// Fills a checkout with the DEPS files (and the files a CIPD `version_file` names) of every recursed dependency, at the pinned revisions and
/// without any clone, so the dependency graph can be read offline. The solutions' own DEPS must already be in place or are fetched too.
/// Returns the number of files written.
pub fn skeleton(config: Config, host: &Platform) -> Result<usize> {
    let root = config.root.clone();
    let mut graph = Graph::new(config, host);
    let mut written = 0;
    let mut next = 0;
    while next < graph.deps.len() {
        let index = next;
        next += 1;
        let dep = graph.deps[index].clone();
        if !dep.recurse || dep.kind != Kind::Git {
            continue;
        }
        let Some((url, revision)) = dep.git() else {
            continue;
        };
        let directory = root.join(&dep.path);
        let deps_path = directory.join(&dep.deps_file);
        if !deps_path.is_file() {
            let Some(text) = remote_file(url, revision.unwrap_or("main"), &dep.deps_file)? else {
                continue;
            };
            fs::write(&deps_path, text)?;
            written += 1;
            if !git::is_repository(&directory) {
                git::run(&directory, &["init", "--quiet"])?;
                git::run(&directory, &["remote", "add", "origin", url])?;
            }
        }
        let text = fs::read_to_string(&deps_path)?;
        let deps = crate::deps::Deps::parse(&text)?;
        for (_, value) in &deps.deps {
            if let Some(crate::deps::Value::List(packages)) = value.get("packages") {
                for package in packages {
                    if let Some(file) = package.get("version_file").and_then(crate::deps::Value::text) {
                        let target = directory.join(file);
                        if !target.is_file()
                            && let Some(body) = remote_file(url, revision.unwrap_or("main"), file)?
                        {
                            fs::write(&target, body)?;
                            written += 1;
                        }
                    }
                }
            }
        }
        graph.expand(index, &text)?;
    }
    Ok(written)
}

/// The tools Chromium pins as CIPD packages or GCS objects, which `fw depot tools` installs by default.
pub const CHROMIUM_TOOLS: &[&str] = &[
    "buildtools/linux64",
    "third_party/ninja",
    "third_party/siso",
    "buildtools/linux64-format",
    "third_party/rust-toolchain",
    "third_party/llvm-libclang",
];

/// Installs the CIPD and GCS dependencies of one DEPS file whose path contains one of `patterns`, below `root`, without any Git checkout. The
/// DEPS file is that of a project whose paths start at `root` (for Chromium, the `src` directory is `root/src`).
pub fn install_tools(
    deps_text: &str,
    solution: &str,
    root: &Path,
    host: &Platform,
    patterns: &[String],
    jobs: usize,
) -> Result<Report> {
    let started = Instant::now();
    let source = format!("solutions = [{{'name': '{solution}', 'url': 'file:///none'}}]\n");
    let config = Config::parse(&source, root)?;
    let mut graph = Graph::new(config, host);
    graph.expand(0, deps_text)?;
    let matches = |path: &str| patterns.iter().any(|pattern| path.contains(pattern.as_str()));
    let mut report = Report::default();
    let wants: Vec<Want> = cipd_wants(&graph, &[]).into_iter().filter(|want| matches(&want.subdir)).collect();
    if !wants.is_empty() {
        report.cipd = Client::new(cipd::DEFAULT_SERVICE).ensure(root, &wants, jobs)?;
    }
    let objects: Vec<&crate::gclient::Dep> = graph
        .deps
        .iter()
        .filter(|dep| matches(&dep.path) && matches!(dep.kind, Kind::Gcs { .. }))
        .collect();
    let outcomes = cipd::parallel(&objects, jobs, |dep| {
        let Kind::Gcs { bucket, object_name, sha256sum, size_bytes, output_file } = &dep.kind else {
            return Ok(false);
        };
        gcs_task(&root.join(&dep.path), bucket, object_name, sha256sum, *size_bytes, output_file.as_deref(), false)
            .map_err(|error| Error::message(format!("{}: {error}", dep.name)))
    })?;
    for (dep, changed) in objects.iter().zip(outcomes) {
        report.gcs.push((dep.name.clone(), changed));
    }
    report.elapsed = started.elapsed();
    Ok(report)
}
