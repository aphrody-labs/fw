//! A CIPD client: resolve a package version, download the instance, verify it, and install it into a directory, over the public pRPC JSON API
//! of the package repository. Instances are cached by id; a package installed again at the same id is not downloaded again.
//!
//! An instance id is the URL-safe base64 of the SHA-256 digest of the package file followed by one byte naming the algorithm (`C` for SHA-256),
//! so an id says what it must hash to and a download is checked against it.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result, fs};
use crate::http;

pub const DEFAULT_SERVICE: &str = "https://chrome-infra-packages.appspot.com";

/// `(os, arch)` names of CIPD for the machine running the code.
pub fn host_platform() -> (&'static str, &'static str) {
    let os = match std::env::consts::OS {
        "macos" => "mac",
        "windows" => "windows",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "x86" => "386",
        other => other,
    };
    (os, arch)
}

/// Replaces `${platform}`, `${os}` and `${arch}` the way `cipd ensure` does.
pub fn expand(template: &str, os: &str, arch: &str) -> String {
    template
        .replace("${platform}", &format!("{os}-{arch}"))
        .replace("${os}", os)
        .replace("${arch}", arch)
}

/// The instance id of a package file with the given SHA-256 digest.
pub fn instance_id(digest: &[u8]) -> String {
    let mut bytes = digest.to_vec();
    bytes.push(2);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// The digest an instance id stands for, if it is one.
pub fn digest_of(id: &str) -> Option<Vec<u8>> {
    if id.len() != 44 || !id.ends_with('C') {
        return None;
    }
    let bytes = URL_SAFE_NO_PAD.decode(id).ok()?;
    (bytes.len() == 33 && bytes[32] == 2).then(|| bytes[..32].to_vec())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instance {
    pub package: String,
    pub id: String,
}

/// One line of an ensure file: a package at a version, in a subdirectory of the root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Want {
    pub subdir: String,
    pub package: String,
    pub version: String,
}

/// An ensure file: `$ServiceURL`, `@Subdir`, and `package version` lines, with `${platform}`, `${os}` and `${arch}` expanded.
pub fn parse_ensure_file(text: &str, os: &str, arch: &str) -> Result<(Option<String>, Vec<Want>)> {
    let mut service = None;
    let mut subdir = String::new();
    let mut wants = Vec::new();
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("$ServiceURL") {
            service = Some(rest.trim().to_owned());
        } else if let Some(rest) = line.strip_prefix("@Subdir") {
            subdir = rest.trim().to_owned();
        } else if line.starts_with('$') {
            continue;
        } else {
            let (package, version) = line
                .split_once(char::is_whitespace)
                .ok_or_else(|| Error::message(format!("ensure file: bad line `{line}`")))?;
            wants.push(Want {
                subdir: subdir.clone(),
                package: expand(package, os, arch),
                version: expand(version.trim(), os, arch),
            });
        }
    }
    Ok((service, wants))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct State {
    /// `subdir` -> package -> installed
    packages: BTreeMap<String, BTreeMap<String, Installed>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Installed {
    pub id: String,
    pub files: Vec<String>,
}

pub struct Client {
    pub service: String,
    pub cache: PathBuf,
}

/// The directory instances are cached in: `$FW_DEPOT_CACHE/cipd`, else the user cache directory.
pub fn default_cache() -> PathBuf {
    if let Ok(path) = std::env::var("FW_DEPOT_CACHE") {
        return PathBuf::from(path).join("cipd");
    }
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        })
        .unwrap_or_else(std::env::temp_dir);
    base.join("fw-depot").join("cipd")
}

impl Client {
    pub fn new(service: &str) -> Client {
        Client {
            service: service.trim_end_matches('/').to_owned(),
            cache: default_cache(),
        }
    }

    fn rpc(&self, method: &str, body: &serde_json::Value) -> Result<serde_json::Value> {
        http::post_json(
            &format!("{}/prpc/cipd.Repository/{method}", self.service),
            &[],
            body,
        )
    }

    /// The instance a version (an id, a ref, a `key:value` tag) of a package stands for.
    pub fn resolve(&self, package: &str, version: &str) -> Result<Instance> {
        if digest_of(version).is_some() {
            return Ok(Instance {
                package: package.to_owned(),
                id: version.to_owned(),
            });
        }
        let reply = self.rpc(
            "ResolveVersion",
            &serde_json::json!({ "package": package, "version": version }),
        )?;
        let hex_digest = reply["instance"]["hexDigest"]
            .as_str()
            .ok_or_else(|| Error::message(format!("{package}@{version}: no instance in {reply}")))?;
        let digest = hex::decode(hex_digest)
            .map_err(|error| Error::message(format!("{package}@{version}: {error}")))?;
        Ok(Instance {
            package: package.to_owned(),
            id: instance_id(&digest),
        })
    }

    fn cached(&self, instance: &Instance) -> PathBuf {
        self.cache
            .join(instance.package.replace('/', "_"))
            .join(format!("{}.zip", instance.id))
    }

    /// The package file of an instance in the cache, downloaded and verified when it is not there.
    pub fn fetch(&self, instance: &Instance) -> Result<PathBuf> {
        let target = self.cached(instance);
        if target.is_file() {
            return Ok(target);
        }
        let digest = digest_of(&instance.id)
            .ok_or_else(|| Error::message(format!("{}: not an instance id", instance.id)))?;
        let reply = self.rpc(
            "GetInstanceURL",
            &serde_json::json!({
                "package": instance.package,
                "instance": { "hashAlgo": "SHA256", "hexDigest": hex::encode(&digest) },
            }),
        )?;
        let url = reply["signedUrl"]
            .as_str()
            .ok_or_else(|| Error::message(format!("{}: no signedUrl in {reply}", instance.package)))?;
        let got = http::download(url, &[], &target)?;
        if got != hex::encode(&digest) {
            let _ = std::fs::remove_file(&target);
            return Err(Error::message(format!(
                "{}@{}: downloaded file hashes to {got}",
                instance.package, instance.id
            )));
        }
        Ok(target)
    }

    /// Unpacks an instance into `dest` and returns the files it created, relative to `dest`.
    pub fn install(&self, instance: &Instance, dest: &Path) -> Result<Vec<String>> {
        let archive = self.fetch(instance)?;
        let file = std::fs::File::open(&archive).map_err(|source| Error::io(&archive, source))?;
        let mut zip = zip::ZipArchive::new(file)
            .map_err(|error| Error::message(format!("{}: {error}", archive.display())))?;
        let mut created = Vec::new();
        for index in 0..zip.len() {
            let mut entry = zip
                .by_index(index)
                .map_err(|error| Error::message(format!("{}: {error}", archive.display())))?;
            let Some(relative) = entry.enclosed_name() else {
                return Err(Error::message(format!(
                    "{}: entry {} escapes the install directory",
                    instance.package,
                    entry.name().map(|name| name.into_owned()).unwrap_or_default()
                )));
            };
            let name = relative.to_string_lossy().replace('\\', "/");
            if name.starts_with(".cipdpkg") {
                continue;
            }
            let target = dest.join(&relative);
            if entry.is_dir() {
                fs::create_dir_all(&target)?;
                continue;
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let mode = entry.unix_mode().unwrap_or(0o644);
            let _ = std::fs::remove_file(&target);
            if mode & 0o170000 == 0o120000 {
                let mut link = String::new();
                entry
                    .read_to_string(&mut link)
                    .map_err(|source| Error::io(&target, source))?;
                symlink(&link, &target)?;
            } else {
                let mut output =
                    std::fs::File::create(&target).map_err(|source| Error::io(&target, source))?;
                std::io::copy(&mut entry, &mut output).map_err(|source| Error::io(&target, source))?;
                drop(output);
                // cipd installs files read-only: 0555 for an executable, 0444 otherwise.
                set_mode(&target, if mode & 0o111 != 0 { 0o555 } else { 0o444 })?;
            }
            created.push(name);
        }
        Ok(created)
    }

    /// Brings `root` to exactly `wants`: each package installed at the resolved instance, files of the previous version of a package removed,
    /// a package that is no longer wanted removed.
    pub fn ensure(&self, root: &Path, wants: &[Want], jobs: usize) -> Result<Vec<(Want, Instance, bool)>> {
        let state_path = root.join(".fw-depot").join("cipd.json");
        let mut state: State = match std::fs::read(&state_path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(_) => State::default(),
        };
        let resolved = parallel(wants, jobs, |want| {
            self.resolve(&want.package, &want.version)
        })?;
        let mut work = Vec::new();
        for (want, instance) in wants.iter().zip(&resolved) {
            let current = state
                .packages
                .get(&want.subdir)
                .and_then(|packages| packages.get(&want.package));
            let intact = current.is_some_and(|installed| {
                installed.id == instance.id
                    && installed
                        .files
                        .iter()
                        .all(|file| root.join(&want.subdir).join(file).symlink_metadata().is_ok())
            });
            work.push((want.clone(), instance.clone(), intact));
        }
        for (subdir, packages) in &state.packages {
            for (package, installed) in packages {
                if !wants.iter().any(|want| &want.subdir == subdir && &want.package == package) {
                    remove_files(&root.join(subdir), &installed.files);
                }
            }
        }
        let pending: Vec<&(Want, Instance, bool)> = work.iter().filter(|item| !item.2).collect();
        for (want, _, _) in &pending {
            if let Some(previous) = state
                .packages
                .get(&want.subdir)
                .and_then(|packages| packages.get(&want.package))
            {
                remove_files(&root.join(&want.subdir), &previous.files);
            }
        }
        let installed = parallel(&pending, jobs, |(want, instance, _)| {
            self.install(instance, &root.join(&want.subdir))
        })?;
        let mut next = State::default();
        for (want, instance, intact) in &work {
            let files = if *intact {
                state
                    .packages
                    .get(&want.subdir)
                    .and_then(|packages| packages.get(&want.package))
                    .map(|installed| installed.files.clone())
                    .unwrap_or_default()
            } else {
                pending
                    .iter()
                    .position(|item| item.0 == *want)
                    .and_then(|position| installed.get(position))
                    .cloned()
                    .unwrap_or_default()
            };
            next.packages.entry(want.subdir.clone()).or_default().insert(
                want.package.clone(),
                Installed {
                    id: instance.id.clone(),
                    files,
                },
            );
        }
        state = next;
        fs::write(&state_path, serde_json::to_vec_pretty(&state)?)?;
        Ok(work
            .into_iter()
            .map(|(want, instance, intact)| (want, instance, !intact))
            .collect())
    }
}

fn remove_files(dir: &Path, files: &[String]) {
    for file in files {
        let _ = std::fs::remove_file(dir.join(file));
    }
}

#[cfg(unix)]
fn symlink(target: &str, link: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, link).map_err(|source| Error::io(link, source))
}

#[cfg(windows)]
fn symlink(target: &str, link: &Path) -> Result<()> {
    std::fs::write(link, target).map_err(|source| Error::io(link, source))
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode & 0o777))
        .map_err(|source| Error::io(path, source))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

/// Maps `items` through `work` on up to `jobs` threads, in order; the first error wins.
pub fn parallel<T: Sync, R: Send>(
    items: &[T],
    jobs: usize,
    work: impl Fn(&T) -> Result<R> + Sync,
) -> Result<Vec<R>> {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<Option<Result<R>>>> =
        Mutex::new((0..items.len()).map(|_| None).collect());
    std::thread::scope(|scope| {
        for _ in 0..jobs.clamp(1, items.len().max(1)) {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::SeqCst);
                    let Some(item) = items.get(index) else { break };
                    let outcome = work(item);
                    if let Ok(mut guard) = results.lock() {
                        guard[index] = Some(outcome);
                    }
                }
            });
        }
    });
    let mut out = Vec::with_capacity(items.len());
    let slots = results
        .into_inner()
        .map_err(|_| Error::message("a worker panicked"))?;
    for slot in slots {
        out.push(slot.ok_or_else(|| Error::message("a worker did not finish"))??);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_ids_round_trip_the_digest() {
        let digest = hex::decode("b2fdd4f4ba88cfc5faacfb2a04fa4361522f2ecef32cda5bb18244b8ab4ef8df").unwrap();
        let id = instance_id(&digest);
        assert_eq!(id.len(), 44);
        assert!(id.ends_with('C'));
        assert_eq!(digest_of(&id), Some(digest));
        assert_eq!(digest_of("version:3@1.12.1"), None);
    }

    #[test]
    fn ensure_files_expand_platform_variables_and_subdirs() {
        let (service, wants) = parse_ensure_file(
            "$ServiceURL https://example.com\n# c\ninfra/ninja/${platform} version:3@1\n@Subdir tools\ngn/gn/linux-${arch} latest # x\n",
            "linux",
            "amd64",
        )
        .unwrap();
        assert_eq!(service.as_deref(), Some("https://example.com"));
        assert_eq!(
            wants,
            vec![
                Want { subdir: String::new(), package: "infra/ninja/linux-amd64".into(), version: "version:3@1".into() },
                Want { subdir: "tools".into(), package: "gn/gn/linux-amd64".into(), version: "latest".into() },
            ]
        );
    }
}
