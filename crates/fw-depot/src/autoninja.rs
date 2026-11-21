//! `autoninja`: choose Siso or Ninja for an output directory and compute the command line, the way depot_tools' `autoninja.py`, `siso.py` and
//! `ninja.py` do. `plan` is pure (arguments, environment and files in, a command out); `run` executes the plan.
//!
//! Not ported: build telemetry and the log uploads, the Android build server helper, the Siso collector, and the AI-agent flag injection.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A command to run, or the reason there is none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub code: u8,
    pub message: String,
}

fn fail<T>(message: impl Into<String>) -> Result<T, Failure> {
    Err(Failure {
        code: 1,
        message: message.into(),
    })
}

/// What the plan depends on besides the arguments.
#[derive(Debug, Clone)]
pub struct Context {
    pub cwd: PathBuf,
    pub env: BTreeMap<String, String>,
    pub cpus: usize,
    /// The soft limit of open files; Ninja's remote `-j` stays under 80% of it.
    pub nofile_limit: u64,
    pub linux_or_mac: bool,
    pub x86_64: bool,
    /// The subcommands `siso help` lists; computed lazily by `siso_subcommands`.
    pub siso_subcommands: Option<Vec<String>>,
}

impl Context {
    pub fn detect() -> Context {
        Context {
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            env: std::env::vars().collect(),
            cpus: std::thread::available_parallelism().map_or(1, |count| count.get()),
            nofile_limit: 1024,
            linux_or_mac: cfg!(unix),
            x86_64: cfg!(target_arch = "x86_64"),
            siso_subcommands: None,
        }
    }
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

fn absolute(base: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    normalize(&if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    })
}

/// The nearest ancestor of `from` holding `.gclient`.
pub fn find_gclient_root(from: &Path) -> Option<PathBuf> {
    from.ancestors()
        .find(|directory| directory.join(".gclient").exists())
        .map(Path::to_path_buf)
}

/// The primary solution: the first solution of the `.gclient` above `from`, else the enclosing git checkout that has `buildtools`.
pub fn primary_solution(from: &Path) -> Option<PathBuf> {
    if let Some(root) = find_gclient_root(from) {
        let name = std::fs::read_to_string(root.join(".gclient"))
            .ok()
            .and_then(|text| crate::gclient::Config::parse(&text, &root).ok())
            .and_then(|config| config.solutions.first().map(|solution| solution.name.clone()))
            .unwrap_or_else(|| "src".to_owned());
        return Some(root.join(name));
    }
    let top = from.ancestors().find(|directory| directory.join(".git").exists())?;
    top.join("buildtools").exists().then(|| top.to_path_buf())
}

/// The `key = value` pairs of `args.gn`, imports followed and comments dropped.
pub fn gn_args(output: &Path) -> Vec<(String, String)> {
    fn lines(output: &Path, path: &Path, out: &mut Vec<String>) {
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        for line in text.lines() {
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("import(\"")
                && let Some(raw) = rest.split('"').next()
            {
                let target = if let Some(relative) = raw.strip_prefix("//") {
                    find_gn_root(output).map(|root| normalize(&root.join(relative)))
                } else if raw.starts_with('/') {
                    Some(PathBuf::from(raw))
                } else {
                    path.parent().map(|parent| normalize(&parent.join(raw)))
                };
                if let Some(target) = target {
                    lines(output, &target, out);
                }
                continue;
            }
            out.push(line.split('#').next().unwrap_or_default().to_owned());
        }
    }
    fn find_gn_root(output: &Path) -> Option<PathBuf> {
        let start = std::path::absolute(output).ok()?;
        start
            .ancestors()
            .find(|directory| directory.join(".gn").exists())
            .map(Path::to_path_buf)
    }
    let mut collected = Vec::new();
    lines(output, &output.join("args.gn"), &mut collected);
    collected
        .iter()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            let key = key.trim();
            if key.is_empty() || key.contains(char::is_whitespace) {
                return None;
            }
            let value = value.trim();
            let value = value.split_whitespace().next().unwrap_or_default();
            (!value.is_empty() || line.trim_end().ends_with('=')).then(|| (key.to_owned(), value.to_owned()))
        })
        .collect()
}

fn use_siso_default(output: &Path) -> bool {
    let Some(root) = primary_solution(output) else {
        return false;
    };
    if !root.join("build/config/siso/.sisoenv").exists() {
        return false;
    }
    if let Ok(text) = std::fs::read_to_string(root.join(".gn"))
        && text.lines().any(|line| {
            let line = line.trim_start();
            line.strip_prefix("use_siso").is_some_and(|rest| {
                rest.trim_start()
                    .strip_prefix('=')
                    .is_some_and(|value| value.trim() == "true")
            })
        })
    {
        return true;
    }
    if std::fs::read_to_string(root.join("build/config/gclient_args.gni"))
        .is_ok_and(|text| text.contains("build_with_chromium = true"))
    {
        return true;
    }
    output.join(".siso_deps").exists()
}

fn convert_j(j_value: &str, use_remoteexec: bool, cpus: usize, args: &[String], notes: &mut Vec<String>) -> Vec<String> {
    let mut local_jobs = None;
    let mut remote_jobs = None;
    if use_remoteexec {
        remote_jobs = Some(j_value.to_owned());
    } else if j_value.parse::<usize>().is_ok_and(|jobs| jobs <= cpus) {
        local_jobs = Some(j_value.to_owned());
    } else {
        notes.push(format!(
            "WARNING: Ignoring -j {j_value} because it is larger than num_cpus={cpus}. Use -local_jobs={j_value} instead if it's intentional."
        ));
    }
    let mut out = Vec::new();
    let mut j_index = None;
    for (index, arg) in args.iter().enumerate() {
        if arg.starts_with("-j") {
            if arg == "-j" {
                j_index = Some(index + 1);
            }
            if let Some(jobs) = &local_jobs {
                out.push(format!("-local_jobs={jobs}"));
            }
            if let Some(jobs) = &remote_jobs {
                out.push(format!("-remote_jobs={jobs}"));
            }
            continue;
        }
        if Some(index) == j_index {
            continue;
        }
        out.push(arg.clone());
    }
    out
}

/// `.sisorc`: global flags (lines starting with `-`) and per-subcommand flags.
fn load_sisorc(path: &Path) -> (Vec<String>, BTreeMap<String, Vec<String>>) {
    let mut global = Vec::new();
    let mut per_command = BTreeMap::new();
    let Ok(text) = std::fs::read_to_string(path) else {
        return (global, per_command);
    };
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let words = split_shell(line);
        let Some((first, rest)) = words.split_first() else {
            continue;
        };
        if line.starts_with('-') {
            global.extend(words.iter().cloned());
        } else {
            per_command.insert(first.clone(), rest.to_vec());
        }
    }
    (global, per_command)
}

/// A small `shlex.split`: whitespace separated words with single and double quotes and backslash escapes.
pub fn split_shell(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\'' => {
                started = true;
                for inner in chars.by_ref() {
                    if inner == '\'' {
                        break;
                    }
                    current.push(inner);
                }
            }
            '"' => {
                started = true;
                while let Some(inner) = chars.next() {
                    match inner {
                        '"' => break,
                        '\\' => {
                            if let Some(next) = chars.next() {
                                if !matches!(next, '"' | '\\') {
                                    current.push('\\');
                                }
                                current.push(next);
                            }
                        }
                        other => current.push(other),
                    }
                }
            }
            '\\' => {
                started = true;
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            ch if ch.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            other => {
                started = true;
                current.push(other);
            }
        }
    }
    if started {
        words.push(current);
    }
    words
}

/// The subcommands `siso help` lists (indented lines, first word).
pub fn siso_subcommands(siso: &Path) -> Vec<String> {
    let Ok(output) = std::process::Command::new(siso).arg("help").output() else {
        return Vec::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.starts_with([' ', '\t']))
        .filter_map(|line| line.split_whitespace().next().map(str::to_owned))
        .collect()
}

fn output_directory(args: &[String]) -> String {
    let mut out = ".".to_owned();
    for (index, arg) in args.iter().enumerate() {
        if arg == "-C" {
            if let Some(next) = args.get(index + 1) {
                out = next.clone();
            }
        } else if let Some(rest) = arg.strip_prefix("-C") {
            out = rest.to_owned();
        }
    }
    out
}

/// Plans a build. `arguments` are the arguments after `autoninja`. Notes for the user go to `notes`.
pub fn plan(arguments: &[String], context: &Context, notes: &mut Vec<String>) -> Result<Plan, Failure> {
    let mut j_value: Option<String> = None;
    let mut t_specified = false;
    let mut offline = false;
    let mut output = ".".to_owned();
    for (index, arg) in arguments.iter().enumerate() {
        let next = arguments.get(index + 1).cloned();
        if let Some(rest) = arg.strip_prefix("-j") {
            j_value = if arg == "-j" { next.clone() } else { Some(rest.to_owned()) };
        }
        if arg.starts_with("-t") {
            t_specified = true;
        }
        if arg == "-C" {
            output = next.clone().unwrap_or(output);
        } else if let Some(rest) = arg.strip_prefix("-C") {
            output = rest.to_owned();
        } else if arg == "-o" || arg == "--offline" {
            offline = true;
        }
    }
    let output_path = absolute(&context.cwd, &output);

    let mut use_remoteexec = false;
    let mut use_siso: Option<bool> = None;
    if output_path.join("args.gn").exists() {
        for (key, value) in gn_args(&output_path) {
            match (key.as_str(), value.as_str()) {
                ("use_remoteexec", "true") => use_remoteexec = true,
                ("use_remoteexec", "false") => use_remoteexec = false,
                ("use_siso", "true") => use_siso = Some(true),
                ("use_siso", "false") => use_siso = Some(false),
                _ => {}
            }
        }
        if use_siso.is_none() {
            let default = use_siso_default(&output_path);
            use_siso = Some(default);
            if default && output_path.join(".ninja_deps").exists() {
                notes.push(format!(
                    "You're still using Ninja.\nPlease run 'gn clean {output}' when convenient to\nupgrade this output directory to Siso (Chromium's Ninja replacement).\nIf you run into any issues by switching to Siso, please file a bug via\ngo/siso-bug and switch back temporarily by setting the GN arg\n'use_siso = false'"
                ));
                use_siso = Some(false);
            }
        }
    }
    let use_siso = use_siso.unwrap_or_else(|| use_siso_default(&output_path));
    let siso_marker = output_path.join(".siso_deps");

    if use_siso {
        if output_path.join(".ninja_log").exists() && !siso_marker.exists() {
            return fail(format!("Run gn clean before switching from ninja to siso in {output}"));
        }
        let mut args = arguments.to_vec();
        if let Some(j) = &j_value {
            // The converted list starts at the program name in autoninja.py; ours starts after it.
            let mut with_program = vec!["autoninja".to_owned()];
            with_program.extend(args);
            args = convert_j(j, use_remoteexec, context.cpus, &with_program, notes)
                .into_iter()
                .skip(1)
                .collect();
        }
        let mut siso_args = vec!["ninja".to_owned()];
        if !use_remoteexec {
            siso_args.push("--offline".to_owned());
        }
        siso_args.extend(args);
        return siso_plan(&siso_args, &context.cwd, &context.env, notes);
    }

    if siso_marker.exists() {
        return fail(format!("Run gn clean before switching from siso to ninja in {output}"));
    }
    let remaining: Vec<String> = arguments
        .iter()
        .filter(|arg| !matches!(arg.as_str(), "-o" | "--offline" | "--virtual-build-path"))
        .cloned()
        .collect();
    let mut ninja_args = Vec::new();
    let mut cores = context.cpus;
    if j_value.is_none() && !t_specified {
        if !offline && use_remoteexec {
            ninja_args.push("-j".to_owned());
            if context.x86_64 {
                cores /= 2;
            }
            let multiplier: usize = context
                .env
                .get("NINJA_CORE_MULTIPLIER")
                .and_then(|value| value.parse().ok())
                .unwrap_or(80);
            let mut jobs = cores * multiplier;
            let limit: usize = context
                .env
                .get("NINJA_CORE_LIMIT")
                .and_then(|value| value.parse().ok())
                .unwrap_or(jobs);
            jobs = jobs.min(limit);
            if cfg!(windows) || cfg!(target_os = "macos") {
                jobs = jobs.min(1000);
            }
            if context.linux_or_mac {
                jobs = jobs.min((context.nofile_limit as f64 * 0.8) as usize);
            }
            ninja_args.push(jobs.to_string());
        } else {
            let addition: usize = context
                .env
                .get("NINJA_CORE_ADDITION")
                .and_then(|value| value.parse().ok())
                .unwrap_or(2);
            ninja_args.push("-j".to_owned());
            ninja_args.push((cores + addition).to_string());
        }
    }
    if context.env.get("NINJA_SUMMARIZE_BUILD").is_some_and(|value| value == "1") {
        ninja_args.extend(["-d".to_owned(), "stats".to_owned()]);
    }
    ninja_args.extend(remaining);
    ninja_plan(&ninja_args, &context.cwd, &context.env)
}

fn candidate_roots(output: &Path, cwd: &Path) -> Vec<PathBuf> {
    let primary = primary_solution(output).or_else(|| primary_solution(cwd));
    let gclient = find_gclient_root(output).or_else(|| find_gclient_root(cwd));
    let mut roots = Vec::new();
    for root in [primary, gclient.clone(), gclient.map(|root| root.join("src"))]
        .into_iter()
        .flatten()
    {
        if !roots.contains(&root) {
            roots.push(root);
        }
    }
    roots
}

fn ninja_plan(args: &[String], cwd: &Path, env: &BTreeMap<String, String>) -> Result<Plan, Failure> {
    let mut tool = String::new();
    let mut output = ".".to_owned();
    for (index, arg) in args.iter().enumerate() {
        if arg == "-t" {
            tool = args.get(index + 1).cloned().unwrap_or_default();
        } else if let Some(rest) = arg.strip_prefix("-t") {
            tool = rest.to_owned();
        } else if arg == "-C" && index + 1 < args.len() {
            output = args[index + 1].clone();
        } else if let Some(rest) = arg.strip_prefix("-C")
            && !rest.is_empty()
        {
            output = rest.to_owned();
        }
    }
    let output_path = absolute(cwd, &output);
    let program = candidate_roots(&output_path, cwd)
        .into_iter()
        .map(|root| root.join("third_party/ninja").join(exe("ninja")))
        .find(|path| path.is_file())
        .or_else(|| find_in_path(env, &exe("ninja")));
    let Some(program) = program else {
        return fail("Could not find Ninja in the third_party of the current project, nor in your PATH.");
    };
    if !matches!(tool.as_str(), "list" | "commands" | "inputs" | "targets") {
        if output_path.join(".siso_deps").exists() {
            return fail(format!(
                "{output} contains Siso state file.\nUse `autoninja` to choose appropriate build tool,\nor run `gn clean {output}` to switch from siso to ninja"
            ));
        }
        if !env.contains_key("AUTONINJA_BUILD_ID")
            && gn_args(&output_path)
                .iter()
                .any(|(key, value)| key == "use_remoteexec" && value == "true")
        {
            return fail("detect use_remoteexec=true\nUse `autoninja` to choose appropriate build tool");
        }
    }
    Ok(Plan {
        program,
        args: args.to_vec(),
        env: BTreeMap::new(),
    })
}

fn find_in_path(env: &BTreeMap<String, String>, name: &str) -> Option<PathBuf> {
    let path = env.get("PATH")?;
    std::env::split_paths(path)
        .filter(|directory| directory.file_name().is_none_or(|name| name != "depot_tools"))
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

/// `siso.py`: find Siso, load `.sisoenv`, add `.sisorc` flags. `args` start at the subcommand (`ninja`).
pub fn siso_plan(
    args: &[String],
    cwd: &Path,
    env: &BTreeMap<String, String>,
    notes: &mut Vec<String>,
) -> Result<Plan, Failure> {
    let output = absolute(cwd, &output_directory(args));
    let override_path = env.get("SISO_PATH").cloned().unwrap_or_default();
    if !override_path.is_empty() {
        notes.push(format!("Using Siso binary from SISO_PATH: {override_path}."));
        if !Path::new(&override_path).is_file() {
            return fail("Could not find Siso at provided SISO_PATH.");
        }
    }
    let mut merged = env.clone();
    for base in candidate_roots(&output, cwd) {
        let sisoenv = base.join("build/config/siso/.sisoenv");
        let Ok(text) = std::fs::read_to_string(&sisoenv) else {
            continue;
        };
        for line in text.lines() {
            if let Some((key, value)) = line.trim_end().split_once('=') {
                merged.insert(key.to_owned(), value.to_owned());
            }
        }
        let backend = base.join("build/config/siso/backend_config");
        if backend.exists() && !backend.join("backend.star").exists() {
            return fail("build/config/siso/backend_config/backend.star does not exist.\nSee build/config/siso/backend_config/README.md");
        }
        let (global, per_command) = load_sisorc(&base.join("build/config/siso/.sisorc"));
        let candidates = [
            PathBuf::from(&override_path),
            base.join("third_party/siso/cipd").join(exe("siso")),
            base.join("third_party/siso").join(exe("siso")),
        ];
        for siso in candidates {
            if override_path.is_empty() && siso.as_os_str().is_empty() || !siso.is_file() {
                continue;
            }
            let subcommands = siso_subcommands(&siso);
            let split = subcommands
                .iter()
                .find_map(|name| args.iter().position(|arg| arg == name));
            let mut new_args = global.clone();
            match split {
                Some(index) => {
                    new_args.extend(args[..index].iter().cloned());
                    let command = &args[index];
                    new_args.push(command.clone());
                    if let Some(flags) = per_command.get(command) {
                        new_args.extend(flags.iter().cloned());
                    }
                    new_args.extend(args[index + 1..].iter().cloned());
                }
                None => new_args.extend(args.iter().cloned()),
            }
            let env_delta: BTreeMap<String, String> = merged
                .iter()
                .filter(|(key, value)| env.get(*key) != Some(*value))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
            if !global.is_empty() || per_command.keys().any(|name| new_args.contains(name)) {
                notes.push(format!("siso.py: {}", new_args.join(" ")));
            }
            if output.join(".ninja_deps").exists() {
                return fail(format!(
                    "{} contains Ninja state file.\nUse `autoninja` to use ninja,\nor run `gn clean {}` to switch from ninja to siso",
                    output.display(),
                    output.display()
                ));
            }
            return Ok(Plan {
                program: siso,
                args: new_args,
                env: env_delta,
            });
        }
        return fail("Could not find siso in third_party/siso of the current project. Did you run gclient sync?");
    }
    if !override_path.is_empty() {
        return Ok(Plan {
            program: PathBuf::from(override_path),
            args: args.to_vec(),
            env: BTreeMap::new(),
        });
    }
    fail("Could not find .sisoenv under build/config/siso of the current project. Did you run gclient sync?")
}

/// Runs a plan with the caller's environment plus the plan's.
pub fn run(plan: &Plan) -> std::io::Result<i32> {
    let status = std::process::Command::new(&plan.program)
        .args(&plan.args)
        .envs(&plan.env)
        .status()?;
    Ok(status.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_words_follow_shlex() {
        assert_eq!(
            split_shell(r#"-flag "a b" 'c d' e\ f"#),
            ["-flag", "a b", "c d", "e f"]
        );
    }

    #[test]
    fn j_becomes_local_or_remote_jobs_for_siso() {
        let args: Vec<String> = ["autoninja", "-j", "8", "-C", "out", "chrome"].map(str::to_owned).into();
        let mut notes = Vec::new();
        assert_eq!(
            convert_j("8", false, 12, &args, &mut notes),
            ["autoninja", "-local_jobs=8", "-C", "out", "chrome"]
        );
        assert_eq!(
            convert_j("800", true, 12, &args, &mut notes),
            ["autoninja", "-remote_jobs=800", "-C", "out", "chrome"]
        );
    }
}
