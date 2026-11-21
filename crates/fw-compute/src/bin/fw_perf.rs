#![deny(unsafe_code)]

//! `fw_perf`: the one benchmark harness every optimization lot uses (`fw bench perf`).
//!
//! `fw_perf run` builds, sizes and times the monorepo per target and writes one JSON document, rewritten after every
//! record so a long run never loses data. Metrics per record: wall time, user and system CPU time (stable under shared
//! host load, unlike wall), the peak resident set of the largest process in the tree, sizes in bytes, and for repeated
//! measurements n, mean, stdev, min, p50 and max. A step that cannot run is recorded with its reason, never as zero.
//!
//! Suites: `host` (machine, load, toolchain, commit), `build` (clean and touch-rebuild per package, profile and
//! target, then stripped size), `startup` (spawn to exit of a built binary), `link` (the same object set linked by
//! each linker), `wasm` (raw, gzip, brotli of wasm artifacts, then wasm-bindgen and wasm-opt), `workers` (wrangler
//! dry-run bundles of every Worker), `shader` (naga WGSL parse, validate and emit time of every WGSL file), `gpu` (gpu-bench executables: shader module, pipeline, frame time).

use std::collections::BTreeMap;
use std::env;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[allow(unsafe_code)]
mod sys {
    //! Process accounting without a dependency: wait4 on Linux, GetProcessTimes and GetProcessMemoryInfo on Windows.
    use std::process::Child;

    pub struct Usage {
        pub user_s: f64,
        pub system_s: f64,
        pub max_rss_kb: u64,
        pub code: i32,
    }

    #[cfg(target_os = "linux")]
    pub fn wait(child: Child) -> Option<Usage> {
        #[repr(C)]
        struct Timeval {
            sec: i64,
            usec: i64,
        }
        #[repr(C)]
        struct Rusage {
            utime: Timeval,
            stime: Timeval,
            maxrss: i64,
            rest: [i64; 13],
        }
        unsafe extern "C" {
            fn wait4(pid: i32, status: *mut i32, options: i32, usage: *mut Rusage) -> i32;
        }
        let pid = child.id() as i32;
        let mut status = 0i32;
        let mut usage = Rusage { utime: Timeval { sec: 0, usec: 0 }, stime: Timeval { sec: 0, usec: 0 }, maxrss: 0, rest: [0; 13] };
        let reaped = unsafe { wait4(pid, &mut status, 0, &mut usage) };
        if reaped != pid {
            return None;
        }
        let code = if status & 0x7f == 0 { (status >> 8) & 0xff } else { 128 + (status & 0x7f) };
        let seconds = |t: &Timeval| t.sec as f64 + t.usec as f64 / 1e6;
        Some(Usage { user_s: seconds(&usage.utime), system_s: seconds(&usage.stime), max_rss_kb: usage.maxrss.max(0) as u64, code })
    }

    #[cfg(windows)]
    pub fn wait(mut child: Child) -> Option<Usage> {
        use std::os::windows::io::AsRawHandle;
        #[repr(C)]
        #[derive(Default)]
        struct FileTime {
            low: u32,
            high: u32,
        }
        #[repr(C)]
        #[derive(Default)]
        struct Counters {
            cb: u32,
            page_faults: u32,
            peak_working_set: usize,
            rest: [usize; 8],
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetProcessTimes(process: *mut core::ffi::c_void, create: *mut FileTime, exit: *mut FileTime, kernel: *mut FileTime, user: *mut FileTime) -> i32;
            fn K32GetProcessMemoryInfo(process: *mut core::ffi::c_void, counters: *mut Counters, cb: u32) -> i32;
        }
        let status = child.wait().ok()?;
        let handle = child.as_raw_handle();
        let (mut create, mut exit, mut kernel, mut user) = (FileTime::default(), FileTime::default(), FileTime::default(), FileTime::default());
        let mut counters = Counters { cb: std::mem::size_of::<Counters>() as u32, ..Default::default() };
        let times = unsafe { GetProcessTimes(handle, &mut create, &mut exit, &mut kernel, &mut user) };
        let memory = unsafe { K32GetProcessMemoryInfo(handle, &mut counters, counters.cb) };
        let ticks = |t: &FileTime| ((u64::from(t.high) << 32) | u64::from(t.low)) as f64 * 100e-9;
        Some(Usage {
            user_s: if times != 0 { ticks(&user) } else { 0.0 },
            system_s: if times != 0 { ticks(&kernel) } else { 0.0 },
            max_rss_kb: if memory != 0 { (counters.peak_working_set / 1024) as u64 } else { 0 },
            code: status.code().unwrap_or(-1),
        })
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    pub fn wait(mut child: Child) -> Option<Usage> {
        let status = child.wait().ok()?;
        Some(Usage { user_s: 0.0, system_s: 0.0, max_rss_kb: 0, code: status.code().unwrap_or(-1) })
    }
}

enum Json {
    /// A JSON document produced by a child tool, embedded as is.
    Raw(String),
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

fn obj(items: Vec<(&str, Json)>) -> Json {
    Json::Obj(items.into_iter().map(|(key, value)| (key.to_string(), value)).collect())
}

fn text(value: impl Into<String>) -> Json {
    Json::Str(value.into())
}

fn num(value: f64) -> Json {
    Json::Num(value)
}

impl Json {
    fn render(&self, out: &mut String, depth: usize) {
        let pad = |out: &mut String, level: usize| out.push_str(&"  ".repeat(level));
        match self {
            Json::Raw(document) => out.push_str(document.trim()),
            Json::Null => out.push_str("null"),
            Json::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
            Json::Num(value) if value.is_finite() => {
                if value.fract() == 0.0 && value.abs() < 9e15 {
                    out.push_str(&format!("{}", *value as i64));
                } else {
                    out.push_str(&format!("{value:.6}"));
                }
            }
            Json::Num(_) => out.push_str("null"),
            Json::Str(value) => {
                out.push('"');
                for ch in value.chars() {
                    match ch {
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        '\n' => out.push_str("\\n"),
                        '\r' => out.push_str("\\r"),
                        '\t' => out.push_str("\\t"),
                        c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                        c => out.push(c),
                    }
                }
                out.push('"');
            }
            Json::Arr(items) => {
                if items.iter().all(|item| matches!(item, Json::Num(_) | Json::Str(_))) {
                    out.push('[');
                    for (index, item) in items.iter().enumerate() {
                        if index > 0 {
                            out.push_str(", ");
                        }
                        item.render(out, depth);
                    }
                    out.push(']');
                } else {
                    out.push_str("[\n");
                    for (index, item) in items.iter().enumerate() {
                        pad(out, depth + 1);
                        item.render(out, depth + 1);
                        out.push_str(if index + 1 < items.len() { ",\n" } else { "\n" });
                    }
                    pad(out, depth);
                    out.push(']');
                }
            }
            Json::Obj(items) => {
                out.push_str("{\n");
                for (index, (key, value)) in items.iter().enumerate() {
                    pad(out, depth + 1);
                    Json::Str(key.clone()).render(out, depth + 1);
                    out.push_str(": ");
                    value.render(out, depth + 1);
                    out.push_str(if index + 1 < items.len() { ",\n" } else { "\n" });
                }
                pad(out, depth);
                out.push('}');
            }
        }
    }
}

#[derive(Clone)]
struct Stats {
    n: usize,
    mean: f64,
    stdev: f64,
    min: f64,
    p50: f64,
    max: f64,
}

fn stats(samples: &[f64]) -> Stats {
    let n = samples.len();
    if n == 0 {
        return Stats { n, mean: 0.0, stdev: 0.0, min: 0.0, p50: 0.0, max: 0.0 };
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let mean = samples.iter().sum::<f64>() / n as f64;
    let variance = if n > 1 { samples.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / (n - 1) as f64 } else { 0.0 };
    let middle = if n % 2 == 1 { sorted[n / 2] } else { (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0 };
    Stats { n, mean, stdev: variance.sqrt(), min: sorted[0], p50: middle, max: sorted[n - 1] }
}

fn stats_json(samples: &[f64]) -> Json {
    let s = stats(samples);
    obj(vec![
        ("n", num(s.n as f64)),
        ("mean", num(s.mean)),
        ("stdev", num(s.stdev)),
        ("min", num(s.min)),
        ("p50", num(s.p50)),
        ("max", num(s.max)),
        ("samples", Json::Arr(samples.iter().map(|v| num(*v)).collect())),
    ])
}

struct Measured {
    wall_s: f64,
    user_s: f64,
    system_s: f64,
    max_rss_kb: u64,
    code: i32,
}

/// Runs a command to completion; stdout and stderr go to `log` (or are discarded) so nothing blocks on a pipe.
fn measure(command: &mut Command, log: Option<&Path>) -> Result<Measured, String> {
    match log {
        Some(path) => {
            let file = File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let twin = file.try_clone().map_err(|e| e.to_string())?;
            command.stdout(Stdio::from(file)).stderr(Stdio::from(twin));
        }
        None => {
            command.stdout(Stdio::null()).stderr(Stdio::null());
        }
    }
    command.stdin(Stdio::null());
    let started = Instant::now();
    let child = command.spawn().map_err(|e| format!("spawn {:?}: {e}", command.get_program()))?;
    let usage = sys::wait(child).ok_or_else(|| "wait failed".to_string())?;
    Ok(Measured { wall_s: started.elapsed().as_secs_f64(), user_s: usage.user_s, system_s: usage.system_s, max_rss_kb: usage.max_rss_kb, code: usage.code })
}

fn capture(program: &str, args: &[&str], dir: Option<&Path>) -> Option<String> {
    let mut command = Command::new(program);
    command.args(args);
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn which(program: &str) -> Option<PathBuf> {
    let suffixes: &[&str] = if cfg!(windows) { &["", ".exe", ".cmd", ".bat"] } else { &[""] };
    env::split_paths(&env::var_os("PATH")?).find_map(|dir| suffixes.iter().map(|s| dir.join(format!("{program}{s}"))).find(|p| p.is_file()))
}

fn utc_date() -> String {
    let days = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() / 86_400).unwrap_or(0) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let doe = days.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

fn read_trimmed(path: &str) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

fn load_snapshot() -> Json {
    let loadavg = read_trimmed("/proc/loadavg").unwrap_or_default();
    let pressure = read_trimmed("/proc/pressure/cpu").unwrap_or_default();
    obj(vec![("loadavg", text(loadavg)), ("psi_cpu", text(pressure.replace('\n', " | ")))])
}

fn host_record(root: &Path) -> Json {
    let cpu_model = fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|info| info.lines().find(|l| l.starts_with("model name")).and_then(|l| l.split(':').nth(1)).map(|s| s.trim().to_string()))
        .or_else(|| env::var("PROCESSOR_IDENTIFIER").ok())
        .unwrap_or_default();
    let memory_kb = read_trimmed("/proc/meminfo").and_then(|m| m.lines().find(|l| l.starts_with("MemTotal")).and_then(|l| l.split_whitespace().nth(1)).and_then(|v| v.parse::<f64>().ok())).unwrap_or(0.0);
    let rustc = capture("rustc", &["-vV"], Some(root)).unwrap_or_default();
    let cargo = capture("cargo", &["-V"], Some(root)).unwrap_or_default();
    let commit = capture("git", &["rev-parse", "HEAD"], Some(root)).unwrap_or_default();
    let dirty = capture("git", &["status", "--porcelain", "--untracked-files=no"], Some(root)).map(|s| s.lines().count()).unwrap_or(0);
    let kernel = capture("uname", &["-sr"], None).unwrap_or_else(|| env::consts::OS.to_string());
    let hostname = env::var("HOSTNAME").ok().or_else(|| env::var("COMPUTERNAME").ok()).or_else(|| capture("hostname", &[], None)).unwrap_or_default();
    let tools: Vec<Json> = ["hyperfine", "gzip", "brotli", "zstd", "strip", "mold", "wild", "ld.lld", "wasm-opt", "wasm-bindgen", "wasm-tools", "cargo-xwin", "bun"]
        .iter()
        .map(|name| obj(vec![("tool", text(*name)), ("path", which(name).map(|p| text(p.display().to_string())).unwrap_or(Json::Null))]))
        .collect();
    obj(vec![
        ("hostname", text(hostname)),
        ("os", text(env::consts::OS)),
        ("arch", text(env::consts::ARCH)),
        ("kernel", text(kernel)),
        ("cpu_model", text(cpu_model)),
        ("logical_cpus", num(std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f64)),
        ("memory_gib", num((memory_kb / 1024.0 / 1024.0 * 10.0).round() / 10.0)),
        ("rustc", text(rustc)),
        ("cargo", text(cargo)),
        ("commit", text(commit)),
        ("dirty_tracked_files", num(dirty as f64)),
        ("tools", Json::Arr(tools)),
    ])
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Bin,
    Wasm,
}

#[derive(Clone, Copy, PartialEq)]
enum Launcher {
    Cargo,
    Xwin,
}

struct Build {
    id: &'static str,
    workspace: &'static str,
    package: &'static str,
    /// Artifact name: the binary, or the cdylib library name for wasm.
    artifact: &'static str,
    profile: &'static str,
    target: &'static str,
    launcher: Launcher,
    kind: Kind,
    /// wasm-bindgen (target web) then wasm-opt on a wasm artifact.
    bindgen: bool,
    /// Entry file touched for the incremental rebuild, relative to the workspace.
    entry: &'static str,
    /// Argument sets timed by the startup suite after the build, host-runnable targets only.
    startup: &'static [&'static [&'static str]],
}

const GNU: &str = "x86_64-unknown-linux-gnu";
const MUSL: &str = "x86_64-unknown-linux-musl";
const MSVC: &str = "x86_64-pc-windows-msvc";
const WASM: &str = "wasm32-unknown-unknown";
const WASIP2: &str = "wasm32-wasip2";
const FW_STARTUP: &[&[&str]] = &[&["--version"], &["status"]];
const APHRODY_STARTUP: &[&[&str]] = &[&["--version"]];
const AY_STARTUP: &[&[&str]] = &[&["--version"]];

#[rustfmt::skip]
const BUILDS: &[Build] = &[
    Build { id: "fw-bin.release.gnu", workspace: ".", package: "fw-bin", artifact: "fw", profile: "release", target: GNU, launcher: Launcher::Cargo, kind: Kind::Bin, bindgen: false, entry: "crates/fw-bin/src/main.rs", startup: FW_STARTUP },
    Build { id: "fw-bin.release.musl", workspace: ".", package: "fw-bin", artifact: "fw", profile: "release", target: MUSL, launcher: Launcher::Cargo, kind: Kind::Bin, bindgen: false, entry: "crates/fw-bin/src/main.rs", startup: FW_STARTUP },
    Build { id: "fw-bin.release.msvc", workspace: ".", package: "fw-bin", artifact: "fw", profile: "release", target: MSVC, launcher: Launcher::Xwin, kind: Kind::Bin, bindgen: false, entry: "crates/fw-bin/src/main.rs", startup: &[] },
    Build { id: "fw-bin.dev.gnu", workspace: ".", package: "fw-bin", artifact: "fw", profile: "dev", target: GNU, launcher: Launcher::Cargo, kind: Kind::Bin, bindgen: false, entry: "crates/fw-bin/src/main.rs", startup: &[] },
    Build { id: "fw-bin.release.msvc-native", workspace: ".", package: "fw-bin", artifact: "fw", profile: "release", target: MSVC, launcher: Launcher::Cargo, kind: Kind::Bin, bindgen: false, entry: "crates/fw-bin/src/main.rs", startup: FW_STARTUP },
    Build { id: "fw-bin.dev.msvc", workspace: ".", package: "fw-bin", artifact: "fw", profile: "dev", target: MSVC, launcher: Launcher::Cargo, kind: Kind::Bin, bindgen: false, entry: "crates/fw-bin/src/main.rs", startup: &[] },
    Build { id: "fw-wasm.release.wasm32-unknown-unknown", workspace: ".", package: "fw-wasm", artifact: "fw_wasm", profile: "release", target: WASM, launcher: Launcher::Cargo, kind: Kind::Wasm, bindgen: false, entry: "crates/fw-wasm/src/lib.rs", startup: &[] },
    Build { id: "fw-wasm.release.wasm32-wasip2", workspace: ".", package: "fw-wasm", artifact: "fw_wasm", profile: "release", target: WASIP2, launcher: Launcher::Cargo, kind: Kind::Wasm, bindgen: false, entry: "crates/fw-wasm/src/lib.rs", startup: &[] },
    Build { id: "aphrody.dist.gnu", workspace: "aphrody", package: "zed", artifact: "aphrody", profile: "dist", target: GNU, launcher: Launcher::Cargo, kind: Kind::Bin, bindgen: false, entry: "crates/zed/src/main.rs", startup: APHRODY_STARTUP },
    Build { id: "aphrody.release.gnu", workspace: "aphrody", package: "zed", artifact: "aphrody", profile: "release", target: GNU, launcher: Launcher::Cargo, kind: Kind::Bin, bindgen: false, entry: "crates/zed/src/main.rs", startup: &[] },
    Build { id: "aphrody.dist.msvc", workspace: "aphrody", package: "zed", artifact: "aphrody", profile: "dist", target: MSVC, launcher: Launcher::Xwin, kind: Kind::Bin, bindgen: false, entry: "crates/zed/src/main.rs", startup: &[] },
    Build { id: "nsh-web.web-lto.wasm32-unknown-unknown", workspace: "aphrody", package: "nsh-platform-web", artifact: "nsh_platform_web", profile: "web-lto", target: WASM, launcher: Launcher::Cargo, kind: Kind::Wasm, bindgen: true, entry: "crates/nsh-platform-web/src/lib.rs", startup: &[] },
    Build { id: "ay-auto.release.gnu", workspace: "ay", package: "auto-sdk", artifact: "auto", profile: "release", target: GNU, launcher: Launcher::Cargo, kind: Kind::Bin, bindgen: false, entry: "crates/auto-sdk/src/bin/auto.rs", startup: AY_STARTUP },
    Build { id: "ay-auto.release.musl", workspace: "ay", package: "auto-sdk", artifact: "auto", profile: "release", target: MUSL, launcher: Launcher::Cargo, kind: Kind::Bin, bindgen: false, entry: "crates/auto-sdk/src/bin/auto.rs", startup: &[] },
    Build { id: "ay-auto.release.msvc", workspace: "ay", package: "auto-sdk", artifact: "auto", profile: "release", target: MSVC, launcher: Launcher::Xwin, kind: Kind::Bin, bindgen: false, entry: "crates/auto-sdk/src/bin/auto.rs", startup: &[] },
    Build { id: "ay-auto.dev.gnu", workspace: "ay", package: "auto-sdk", artifact: "auto", profile: "dev", target: GNU, launcher: Launcher::Cargo, kind: Kind::Bin, bindgen: false, entry: "crates/auto-sdk/src/bin/auto.rs", startup: &[] },
    Build { id: "ay-auto-ffi.wasm-lto.wasm32-unknown-unknown", workspace: "ay", package: "auto-ffi", artifact: "auto_ffi", profile: "wasm-lto", target: WASM, launcher: Launcher::Cargo, kind: Kind::Wasm, bindgen: true, entry: "crates/auto-ffi/src/lib.rs", startup: &[] },
];

struct Options {
    root: PathBuf,
    work: PathBuf,
    out: PathBuf,
    suites: Vec<String>,
    only: Vec<String>,
    runs: usize,
    build_runs: usize,
    warmup: usize,
    incremental: bool,
    exes: Vec<PathBuf>,
    startup_args: Vec<Vec<String>>,
    gpu_bench: PathBuf,
    wgsl: Vec<PathBuf>,
}

struct Report {
    path: PathBuf,
    header: Vec<(String, Json)>,
    records: Vec<Json>,
    markdown: String,
}

impl Report {
    fn push(&mut self, record: Json, summary: String) {
        println!("{summary}");
        self.markdown.push_str(&summary);
        self.markdown.push('\n');
        self.records.push(record);
        self.flush();
    }

    fn flush(&self) {
        let mut document = vec![("schema".to_string(), text("fw.perf.v1"))];
        document.extend(self.header.iter().map(|(key, value)| (key.clone(), clone_json(value))));
        document.push(("records".to_string(), Json::Arr(self.records.iter().map(clone_json).collect())));
        let mut rendered = String::new();
        Json::Obj(document).render(&mut rendered, 0);
        rendered.push('\n');
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).ok();
        }
        if let Err(error) = fs::write(&self.path, rendered) {
            eprintln!("cannot write {}: {error}", self.path.display());
        }
        let mut markdown_path = self.path.clone().into_os_string();
        markdown_path.push(".md");
        fs::write(markdown_path, &self.markdown).ok();
    }
}

fn clone_json(value: &Json) -> Json {
    match value {
        Json::Raw(document) => Json::Raw(document.clone()),
        Json::Null => Json::Null,
        Json::Bool(b) => Json::Bool(*b),
        Json::Num(n) => Json::Num(*n),
        Json::Str(s) => Json::Str(s.clone()),
        Json::Arr(items) => Json::Arr(items.iter().map(clone_json).collect()),
        Json::Obj(items) => Json::Obj(items.iter().map(|(k, v)| (k.clone(), clone_json(v))).collect()),
    }
}

fn failure(id: &str, suite: &str, reason: &str, command: &str) -> (Json, String) {
    let record = obj(vec![("id", text(id)), ("suite", text(suite)), ("status", text("failed")), ("reason", text(reason)), ("command", text(command)), ("load", load_snapshot())]);
    (record, format!("| {id} | {suite} | failed | {reason} |"))
}

fn tail(path: &Path, lines: usize) -> String {
    let content = fs::read_to_string(path).unwrap_or_default();
    let all: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    all[all.len().saturating_sub(lines)..].join(" / ")
}

fn profile_dir(profile: &str) -> &str {
    if profile == "dev" { "debug" } else { profile }
}

fn artifact_path(build: &Build, target_dir: &Path) -> PathBuf {
    let file = match (build.kind, build.target) {
        (Kind::Wasm, _) => format!("{}.wasm", build.artifact),
        (Kind::Bin, MSVC) => format!("{}.exe", build.artifact),
        (Kind::Bin, _) => build.artifact.to_string(),
    };
    target_dir.join(build.target).join(profile_dir(build.profile)).join(file)
}

fn host_triple() -> &'static str {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        GNU
    } else if cfg!(all(windows, target_arch = "x86_64")) {
        MSVC
    } else {
        ""
    }
}

fn cargo_command(build: &Build, workspace: &Path, target_dir: &Path, subcommand: &[&str]) -> Command {
    let mut command = Command::new("cargo");
    if build.launcher == Launcher::Xwin && !cfg!(windows) {
        command.arg("xwin");
    }
    command.args(subcommand);
    command.args(["--locked", "--profile", build.profile, "-p", build.package, "--target", build.target]);
    if build.kind == Kind::Bin {
        command.args(["--bin", build.artifact]);
    } else {
        command.arg("--lib");
    }
    command.current_dir(workspace).env("CARGO_TARGET_DIR", target_dir).env("RUSTC_WRAPPER", "").env("CARGO_TERM_COLOR", "never");
    command
}

fn command_line(command: &Command) -> String {
    let mut parts = vec![command.get_program().to_string_lossy().to_string()];
    parts.extend(command.get_args().map(|a| a.to_string_lossy().to_string()));
    parts.join(" ")
}

fn compressed_size(tool: &str, args: &[&str], file: &Path, scratch: &Path) -> Option<u64> {
    which(tool)?;
    let out = scratch.join(format!("{tool}.out"));
    let handle = File::create(&out).ok()?;
    let status = Command::new(tool).args(args).arg(file).stdout(Stdio::from(handle)).stderr(Stdio::null()).status().ok()?;
    status.success().then(|| fs::metadata(&out).map(|m| m.len()).ok()).flatten()
}

fn sizes_json(file: &Path, scratch: &Path) -> Json {
    let raw = fs::metadata(file).map(|m| m.len()).unwrap_or(0);
    let mut items = vec![("raw_bytes", num(raw as f64))];
    let optional = |value: Option<u64>| value.map(|v| num(v as f64)).unwrap_or(Json::Null);
    items.push(("gzip9_bytes", optional(compressed_size("gzip", &["-9", "-n", "-c"], file, scratch))));
    items.push(("brotli11_bytes", optional(compressed_size("brotli", &["-q", "11", "-c"], file, scratch))));
    items.push(("zstd19_bytes", optional(compressed_size("zstd", &["-19", "-q", "-c"], file, scratch))));
    obj(items)
}

fn stripped_bytes(file: &Path, scratch: &Path) -> Option<u64> {
    let out = scratch.join("stripped");
    fs::remove_file(&out).ok();
    for tool in ["llvm-strip", "strip"] {
        if which(tool).is_some()
            && Command::new(tool).arg("--strip-all").arg("-o").arg(&out).arg(file).stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
        {
            return fs::metadata(&out).map(|m| m.len()).ok();
        }
    }
    None
}

fn touch(path: &Path) -> Result<(), String> {
    let file = fs::OpenOptions::new().write(true).open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    file.set_modified(SystemTime::now()).map_err(|e| e.to_string())
}

fn series(measured: &[Measured]) -> Vec<(&'static str, Json)> {
    let pick = |f: fn(&Measured) -> f64| measured.iter().map(f).collect::<Vec<f64>>();
    vec![
        ("wall_s", stats_json(&pick(|m| m.wall_s))),
        ("cpu_user_s", stats_json(&pick(|m| m.user_s))),
        ("cpu_system_s", stats_json(&pick(|m| m.system_s))),
        ("max_rss_mib", stats_json(&pick(|m| m.max_rss_kb as f64 / 1024.0))),
    ]
}

fn row(id: &str, what: &str, measured: &[Measured]) -> String {
    let wall = stats(&measured.iter().map(|m| m.wall_s).collect::<Vec<_>>());
    let cpu = stats(&measured.iter().map(|m| m.user_s + m.system_s).collect::<Vec<_>>());
    let rss = measured.iter().map(|m| m.max_rss_kb).max().unwrap_or(0) as f64 / 1024.0;
    format!("| {id} | {what} | n={} | wall {:.1} s (sd {:.1}) | cpu {:.1} s (sd {:.1}) | rss {:.0} MiB |", wall.n, wall.mean, wall.stdev, cpu.mean, cpu.stdev, rss)
}

fn run_build(build: &Build, options: &Options, report: &mut Report) -> Option<PathBuf> {
    let workspace = options.root.join(build.workspace);
    let target_dir = options.work.join("target").join(build.id);
    let scratch = options.work.join("scratch").join(build.id);
    fs::create_dir_all(&scratch).ok();
    let logs = scratch.join("build.log");

    let mut fetch = Command::new("cargo");
    fetch.args(["fetch", "--locked", "--target", build.target]).current_dir(&workspace);
    if let Err(error) = measure(&mut fetch, Some(&scratch.join("fetch.log"))) {
        let (record, summary) = failure(build.id, "build", &format!("cargo fetch: {error}"), &command_line(&fetch));
        report.push(record, summary);
        return None;
    }

    let mut cleans = Vec::new();
    let mut last_command = String::new();
    for run in 0..options.build_runs {
        fs::remove_dir_all(&target_dir).ok();
        let mut command = cargo_command(build, &workspace, &target_dir, &["build"]);
        last_command = command_line(&command);
        match measure(&mut command, Some(&logs)) {
            Ok(m) if m.code == 0 => cleans.push(m),
            Ok(m) => {
                let (record, summary) = failure(build.id, "build", &format!("exit {} on clean run {}: {}", m.code, run + 1, tail(&logs, 4)), &last_command);
                report.push(record, summary);
                return None;
            }
            Err(error) => {
                let (record, summary) = failure(build.id, "build", &error, &last_command);
                report.push(record, summary);
                return None;
            }
        }
    }

    let artifact = artifact_path(build, &target_dir);
    let mut metrics = series(&cleans);
    metrics.push(("sizes", sizes_json(&artifact, &scratch)));
    if build.kind == Kind::Bin {
        metrics.push(("stripped_bytes", stripped_bytes(&artifact, &scratch).map(|b| num(b as f64)).unwrap_or(Json::Null)));
    }
    let size = fs::metadata(&artifact).map(|m| m.len()).unwrap_or(0);
    let record = obj(vec![
        ("id", text(build.id)),
        ("suite", text("build")),
        ("kind", text("clean")),
        ("status", text("ok")),
        ("package", text(build.package)),
        ("profile", text(build.profile)),
        ("target", text(build.target)),
        ("command", text(last_command.clone())),
        ("artifact", text(artifact.display().to_string())),
        ("metrics", obj(metrics)),
        ("load", load_snapshot()),
    ]);
    report.push(record, format!("{} | {size} B", row(build.id, "clean build", &cleans)));

    if options.incremental {
        let mut rebuilds = Vec::new();
        for _ in 0..options.runs {
            touch(&workspace.join(build.entry)).ok();
            let mut command = cargo_command(build, &workspace, &target_dir, &["build"]);
            match measure(&mut command, Some(&logs)) {
                Ok(m) if m.code == 0 => rebuilds.push(m),
                _ => break,
            }
        }
        if rebuilds.is_empty() {
            let (record, summary) = failure(build.id, "build", &format!("touch rebuild failed: {}", tail(&logs, 4)), &last_command);
            report.push(record, summary);
        } else {
            let record = obj(vec![
                ("id", text(build.id)),
                ("suite", text("build")),
                ("kind", text("touch-rebuild")),
                ("status", text("ok")),
                ("command", text(format!("touch {} && {}", build.entry, last_command))),
                ("metrics", obj(series(&rebuilds))),
                ("load", load_snapshot()),
            ]);
            report.push(record, row(build.id, &format!("touch {} then rebuild", build.entry), &rebuilds));
        }
    }
    Some(artifact)
}

fn run_startup(id: &str, exe: &Path, argument_sets: &[Vec<String>], options: &Options, report: &mut Report) {
    for arguments in argument_sets {
        let label = format!("{id} {}", arguments.join(" "));
        let mut walls = Vec::new();
        let mut rss = Vec::new();
        let mut cpu = Vec::new();
        let mut codes = Vec::new();
        for run in 0..options.warmup + options.runs.max(5) * 6 {
            let mut command = Command::new(exe);
            command.args(arguments);
            match measure(&mut command, None) {
                Ok(m) if run >= options.warmup => {
                    walls.push(m.wall_s * 1000.0);
                    rss.push(m.max_rss_kb as f64 / 1024.0);
                    cpu.push((m.user_s + m.system_s) * 1000.0);
                    codes.push(m.code);
                }
                Ok(_) => {}
                Err(error) => {
                    let (record, summary) = failure(&label, "startup", &error, &format!("{} {}", exe.display(), arguments.join(" ")));
                    report.push(record, summary);
                    return;
                }
            }
        }
        let wall = stats(&walls);
        let record = obj(vec![
            ("id", text(label.clone())),
            ("suite", text("startup")),
            ("status", text(if codes.iter().all(|c| *c == 0) { "ok" } else { "nonzero-exit" })),
            ("exit_codes", Json::Arr(codes.iter().take(3).map(|c| num(f64::from(*c))).collect())),
            ("command", text(format!("{} {}", exe.display(), arguments.join(" ")))),
            ("exe_bytes", num(fs::metadata(exe).map(|m| m.len()).unwrap_or(0) as f64)),
            ("metrics", obj(vec![("wall_ms", stats_json(&walls)), ("cpu_ms", stats_json(&cpu)), ("max_rss_mib", stats_json(&rss))])),
            ("load", load_snapshot()),
        ]);
        report.push(record, format!("| {label} | startup | n={} | wall {:.2} ms (sd {:.2}, min {:.2}) | rss {:.1} MiB |", wall.n, wall.mean, wall.stdev, wall.min, stats(&rss).mean));
    }
}

#[cfg(unix)]
fn link_symlink(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).ok();
}

#[cfg(not(unix))]
fn link_symlink(_target: &Path, _link: &Path) {}

fn parse_quoted(line: &str) -> Vec<String> {
    let mut arguments = Vec::new();
    let mut current = String::new();
    let mut inside = false;
    let mut chars = line.chars();
    while let Some(ch) = chars.next() {
        match (inside, ch) {
            (false, '"') => inside = true,
            (true, '"') => {
                inside = false;
                arguments.push(std::mem::take(&mut current));
            }
            (true, '\\') => match chars.next() {
                Some('n') => current.push('\n'),
                Some(escaped) => current.push(escaped),
                None => {}
            },
            (true, c) => current.push(c),
            (false, _) => {}
        }
    }
    arguments
}

/// Links the object set of one build with every linker available: rustc prints its link line (`--print link-args`) and
/// keeps the objects (`-C save-temps`); the same line is replayed with `-fuse-ld=...` swapped, so only the linker differs.
fn run_link(build: &Build, options: &Options, report: &mut Report) {
    let id = format!("link.{}", build.id);
    if !cfg!(any(target_os = "linux", windows)) {
        let (record, summary) = failure(&id, "link", "link replay is implemented for Linux gnu and Windows msvc", "");
        report.push(record, summary);
        return;
    }
    let workspace = options.root.join(build.workspace);
    let target_dir = options.work.join("target").join(format!("{}-link", build.id));
    let scratch = options.work.join("scratch").join(&id);
    fs::create_dir_all(&scratch).ok();
    let log = scratch.join("rustc.log");
    let mut command = cargo_command(build, &workspace, &target_dir, &["rustc"]);
    command.args(["--", "-C", "save-temps", "--print", "link-args"]);
    let line = command_line(&command);
    match measure(&mut command, Some(&log)) {
        Ok(m) if m.code == 0 => {}
        _ => {
            let (record, summary) = failure(&id, "link", &format!("build for link line failed: {}", tail(&log, 3)), &line);
            report.push(record, summary);
            return;
        }
    }
    let content = fs::read_to_string(&log).unwrap_or_default();
    let Some(link_line) = content.lines().rfind(|l| l.starts_with('"') && (l.contains("\"-o\"") || l.to_ascii_lowercase().contains("\"/out:"))) else {
        let (record, summary) = failure(&id, "link", "no link line in rustc output", &line);
        report.push(record, summary);
        return;
    };
    let original = parse_quoted(link_line);
    let Some(linker_program) = original.first().cloned() else { return };
    let rest: Vec<String> = original.iter().skip(1).cloned().collect();
    // (name, program, arguments): only the linker differs between candidates.
    let mut linkers: Vec<(String, String, Vec<String>)> = vec![("rustc-default".to_string(), linker_program.clone(), rest.clone())];
    if cfg!(windows) {
        let sysroot = capture("rustc", &["--print", "sysroot"], None).unwrap_or_default();
        let host = capture("rustc", &["-vV"], None).and_then(|v| v.lines().find_map(|l| l.strip_prefix("host: ").map(str::to_string))).unwrap_or_default();
        let rust_lld = Path::new(&sysroot).join("lib").join("rustlib").join(host).join("bin").join("rust-lld.exe");
        if rust_lld.is_file() {
            let mut arguments = vec!["-flavor".to_string(), "link".to_string()];
            arguments.extend(rest.iter().cloned());
            linkers.push(("lld-link".to_string(), rust_lld.display().to_string(), arguments));
        }
    } else {
        let base: Vec<String> = rest.iter().filter(|a| !a.starts_with("-fuse-ld") && !a.starts_with("-B")).cloned().collect();
        let with = |flags: Vec<String>| -> Vec<String> { flags.into_iter().chain(base.iter().cloned()).collect() };
        for name in ["bfd", "gold", "lld", "mold"] {
            if which(&format!("ld.{name}")).is_some() {
                linkers.push((name.to_string(), linker_program.clone(), with(vec![format!("-fuse-ld={name}")])));
            }
        }
        if let Some(dir) = original.iter().find_map(|a| a.strip_prefix("-B").filter(|d| d.contains("gcc-ld")).map(str::to_string)) {
            linkers.push(("rust-lld".to_string(), linker_program.clone(), with(vec!["-fuse-ld=lld".to_string(), format!("-B{dir}")])));
        }
        if let Some(wild) = which("wild") {
            let wild_dir = scratch.join("wild-bin");
            fs::create_dir_all(&wild_dir).ok();
            let link = wild_dir.join("ld");
            fs::remove_file(&link).ok();
            link_symlink(&wild, &link);
            if link.exists() {
                linkers.push(("wild".to_string(), linker_program.clone(), with(vec![format!("-B{}", wild_dir.display())])));
            }
        }
    }
    for (name, program, arguments) in linkers {
        let output = scratch.join(format!("out-{name}{}", if cfg!(windows) { ".exe" } else { "" }));
        let mut replaced = false;
        let mut final_args = Vec::new();
        let mut iterator = arguments.iter();
        while let Some(argument) = iterator.next() {
            if argument == "-o" {
                iterator.next();
                final_args.push("-o".to_string());
                final_args.push(output.display().to_string());
                replaced = true;
            } else if argument.to_ascii_lowercase().starts_with("/out:") {
                final_args.push(format!("/OUT:{}", output.display()));
                replaced = true;
            } else {
                final_args.push(argument.clone());
            }
        }
        let label = format!("{id}.{name}");
        if !replaced {
            let (record, summary) = failure(&label, "link", "no output argument in the link line", &program);
            report.push(record, summary);
            continue;
        }
        let link_log = scratch.join(format!("link-{name}.log"));
        let mut samples = Vec::new();
        let mut failed = None;
        for run in 0..options.warmup.max(1) + options.runs {
            fs::remove_file(&output).ok();
            let mut link = Command::new(&program);
            link.args(&final_args).current_dir(&workspace);
            match measure(&mut link, Some(&link_log)) {
                Ok(m) if m.code == 0 => {
                    if run >= options.warmup.max(1) {
                        samples.push(m);
                    }
                }
                Ok(m) => {
                    failed = Some(format!("exit {}: {}", m.code, tail(&link_log, 3)));
                    break;
                }
                Err(error) => {
                    failed = Some(error);
                    break;
                }
            }
        }
        if let Some(reason) = failed {
            let (record, summary) = failure(&label, "link", &reason, &format!("{program} {}", final_args.join(" ")));
            report.push(record, summary);
            continue;
        }
        let bytes = fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
        let runs = Command::new(&output).arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false);
        let record = obj(vec![
            ("id", text(label.clone())),
            ("suite", text("link")),
            ("status", text("ok")),
            ("linker", text(name.as_str())),
            ("output_bytes", num(bytes as f64)),
            ("output_runs_version", Json::Bool(runs)),
            ("command", text(format!("{program} {}", final_args.join(" ")))),
            ("metrics", obj(series(&samples))),
            ("load", load_snapshot()),
        ]);
        report.push(record, format!("{} | {bytes} B", row(&label, "link only", &samples)));
    }
}

fn run_wasm_pipeline(build: &Build, artifact: &Path, options: &Options, report: &mut Report) {
    let id = format!("wasm.{}", build.id);
    let scratch = options.work.join("scratch").join(&id);
    fs::create_dir_all(&scratch).ok();
    let mut stages: Vec<(&str, Json)> = vec![("cargo-output", sizes_json(artifact, &scratch))];
    let mut final_file = artifact.to_path_buf();
    if build.bindgen {
        if which("wasm-bindgen").is_none() {
            let (record, summary) = failure(&id, "wasm", "wasm-bindgen is not installed", "");
            report.push(record, summary);
            return;
        }
        let out_dir = scratch.join("bindgen");
        fs::remove_dir_all(&out_dir).ok();
        let mut command = Command::new("wasm-bindgen");
        command.args(["--target", "web", "--no-typescript", "--out-dir"]).arg(&out_dir).arg(artifact);
        let log = scratch.join("bindgen.log");
        match measure(&mut command, Some(&log)) {
            Ok(m) if m.code == 0 => {
                let produced = fs::read_dir(&out_dir).ok().and_then(|d| d.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x == "wasm")));
                if let Some(produced) = produced {
                    stages.push(("wasm-bindgen", sizes_json(&produced, &scratch)));
                    final_file = produced;
                }
            }
            _ => {
                let (record, summary) = failure(&id, "wasm", &format!("wasm-bindgen failed: {}", tail(&log, 3)), &command_line(&command));
                report.push(record, summary);
                return;
            }
        }
    }
    if which("wasm-opt").is_some() {
        let optimized = scratch.join("optimized.wasm");
        let mut command = Command::new("wasm-opt");
        command.args(["-Oz", "--all-features", "--strip-debug", "--strip-producers", "-o"]).arg(&optimized).arg(&final_file);
        let log = scratch.join("opt.log");
        if let Ok(m) = measure(&mut command, Some(&log))
            && m.code == 0
        {
            stages.push(("wasm-opt-Oz", sizes_json(&optimized, &scratch)));
            stages.push(("wasm-opt-Oz_seconds", num(m.wall_s)));
        }
    }
    let record = obj(vec![("id", text(id.clone())), ("suite", text("wasm")), ("status", text("ok")), ("target", text(build.target)), ("stages", obj(stages))]);
    let raw = fs::metadata(artifact).map(|m| m.len()).unwrap_or(0);
    report.push(record, format!("| {id} | wasm bundle | cargo output {raw} B |"));
}

fn discover_workers(root: &Path) -> Vec<PathBuf> {
    let listing = capture("git", &["ls-files", "*wrangler.jsonc", "*wrangler.toml", "*wrangler.json"], Some(root)).unwrap_or_default();
    listing.lines().map(|l| root.join(l)).collect()
}

fn run_workers(options: &Options, report: &mut Report) {
    let runner: Vec<&str> = if which("bunx").is_some() { vec!["bunx"] } else if which("npx").is_some() { vec!["npx", "--yes"] } else { Vec::new() };
    for config in discover_workers(&options.root) {
        let directory = config.parent().map(Path::to_path_buf).unwrap_or_default();
        let name = directory.strip_prefix(&options.root).map(|p| p.display().to_string()).unwrap_or_default().replace(['/', '\\'], "-");
        let id = format!("workers.{}", if name.is_empty() { "root" } else { &name });
        let scratch = options.work.join("scratch").join(&id);
        let out_dir = scratch.join("bundle");
        fs::remove_dir_all(&out_dir).ok();
        fs::create_dir_all(&scratch).ok();
        let Some((program, prefix)) = runner.split_first() else {
            let (record, summary) = failure(&id, "workers", "neither bunx nor npx is installed", "");
            report.push(record, summary);
            continue;
        };
        let mut command = Command::new(program);
        command.args(prefix).args(["wrangler", "deploy", "--dry-run", "--outdir"]).arg(&out_dir).arg("--config").arg(&config).current_dir(&directory);
        let log = scratch.join("wrangler.log");
        let line = command_line(&command);
        match measure(&mut command, Some(&log)) {
            Ok(m) if m.code == 0 => {
                let mut files: BTreeMap<String, Json> = BTreeMap::new();
                let mut total = 0u64;
                if let Ok(entries) = fs::read_dir(&out_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() && path.extension().is_some_and(|e| matches!(e.to_str(), Some("js" | "mjs" | "wasm"))) {
                            total += fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                            files.insert(entry.file_name().to_string_lossy().to_string(), sizes_json(&path, &scratch));
                        }
                    }
                }
                let wrangler_total = fs::read_to_string(&log).unwrap_or_default().lines().find(|l| l.contains("Total Upload")).map(|l| l.trim().to_string()).unwrap_or_default();
                let record = obj(vec![
                    ("id", text(id.clone())),
                    ("suite", text("workers")),
                    ("status", text("ok")),
                    ("config", text(config.display().to_string())),
                    ("dry_run_seconds", num(m.wall_s)),
                    ("wrangler_total_upload", text(wrangler_total)),
                    ("bundle_raw_bytes", num(total as f64)),
                    ("files", Json::Obj(files.into_iter().collect())),
                    ("command", text(line)),
                ]);
                report.push(record, format!("| {id} | worker bundle | {total} B raw in {:.1} s |", m.wall_s));
            }
            _ => {
                let (record, summary) = failure(&id, "workers", &format!("wrangler dry-run failed: {}", tail(&log, 3)), &line);
                report.push(record, summary);
            }
        }
    }
}

fn run_gpu(options: &Options, report: &mut Report) {
    let wgsl = options.gpu_bench.join("wgsl");
    let engines = [("wgpu", "gpu-bench-wgpu", if cfg!(windows) { "dx12" } else { "vulkan" }), ("dawn", "gpu-bench-dawn", if cfg!(windows) { "d3d12" } else { "vulkan" })];
    for (engine, binary, backend) in engines {
        let exe = options.gpu_bench.join(".bin").join(format!("{binary}{}", env::consts::EXE_SUFFIX));
        let id = format!("gpu.{engine}.{backend}");
        if !exe.is_file() {
            let (record, summary) = failure(&id, "gpu", &format!("{} not built (bun scripts/run.ts build in gpu-bench)", exe.display()), "");
            report.push(record, summary);
            continue;
        }
        let out = options.work.join("scratch").join(format!("{id}.json"));
        fs::create_dir_all(out.parent().unwrap_or(Path::new("."))).ok();
        let mut command = Command::new(&exe);
        command.arg("--wgsl").arg(&wgsl).args(["--backend", backend]).current_dir(&options.gpu_bench);
        let Ok(file) = File::create(&out) else { continue };
        command.stdout(Stdio::from(file)).stderr(Stdio::null()).stdin(Stdio::null());
        let line = command_line(&command);
        let started = Instant::now();
        let status = command.status();
        if !status.map(|s| s.success()).unwrap_or(false) {
            let head: String = fs::read_to_string(&out).unwrap_or_default().split_whitespace().collect::<Vec<_>>().join(" ").chars().take(200).collect();
            let (record, summary) = failure(&id, "gpu", &format!("gpu-bench executable failed: {head}"), &line);
            report.push(record, summary);
            continue;
        }
        let raw = fs::read_to_string(&out).unwrap_or_default();
        let metrics = gpu_metrics(&raw);
        let summary = metrics.iter().filter(|(k, _)| k.ends_with("gpu_ms") || k.ends_with("_pipeline_ms") || k.as_str() == "shader_module_ms").map(|(k, s)| format!("{k} p50 {:.3}", s.p50)).collect::<Vec<_>>().join(", ");
        let adapter = raw.split("\"description\":").nth(1).and_then(|s| s.split('"').nth(1)).unwrap_or("").to_string();
        let record = obj(vec![
            ("id", text(id.clone())),
            ("suite", text("gpu")),
            ("status", text("ok")),
            ("adapter", text(adapter)),
            ("wall_s", num(started.elapsed().as_secs_f64())),
            ("command", text(line)),
            ("metrics", Json::Obj(metrics.iter().map(|(k, s)| (k.clone(), stats_json_from(s))).collect())),
        ]);
        report.push(record, format!("| {id} | gpu | {summary} |"));
    }
}

fn walk_wgsl(path: &Path) -> Vec<PathBuf> {
    if path.is_dir() {
        let mut entries: Vec<PathBuf> = fs::read_dir(path).map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default();
        entries.sort();
        entries.iter().flat_map(|entry| walk_wgsl(entry)).collect()
    } else if path.extension().is_some_and(|e| e == "wgsl") {
        vec![path.to_path_buf()]
    } else {
        Vec::new()
    }
}

/// gpui_wgpu builds its shaders at run time from `shaders.wgsl` plus one suffix (wgpu_renderer.rs: storage, webgl, and
/// storage plus subpixel behind `enable dual_source_blending;`); the parts do not parse alone, so the three composed
/// sources are compiled instead.
fn compose_gpui_shaders(inputs: &mut Vec<PathBuf>, composed: &Path) {
    let Some(base_path) = inputs.iter().find(|p| p.file_name().is_some_and(|n| n == "shaders.wgsl") && p.with_file_name("shaders_storage.wgsl").is_file()).cloned() else { return };
    let read = |name: &str| fs::read_to_string(base_path.with_file_name(name)).unwrap_or_default();
    let (base, storage, subpixel, webgl) = (read("shaders.wgsl"), read("shaders_storage.wgsl"), read("shaders_subpixel.wgsl"), read("shaders_webgl.wgsl"));
    fs::create_dir_all(composed).ok();
    let variants = [
        ("gpui_wgpu.storage.wgsl", format!("{base}{storage}")),
        ("gpui_wgpu.webgl.wgsl", format!("{base}{webgl}")),
        ("gpui_wgpu.subpixel.wgsl", format!("enable dual_source_blending;\n{base}{storage}{subpixel}")),
    ];
    let parts = ["shaders.wgsl", "shaders_storage.wgsl", "shaders_subpixel.wgsl", "shaders_webgl.wgsl"];
    inputs.retain(|p| !(p.parent() == base_path.parent() && p.file_name().is_some_and(|n| parts.iter().any(|part| n == *part))));
    for (name, source) in variants {
        let path = composed.join(name);
        if fs::write(&path, source).is_ok() {
            inputs.push(path);
        }
    }
}

fn run_shader(options: &Options, report: &mut Report) {
    let id = "shader.naga";
    let manifest = options.gpu_bench.join("bench/wgpu/Cargo.toml");
    let target_dir = options.work.join("target").join("shader-compile");
    let scratch = options.work.join("scratch").join(id);
    fs::create_dir_all(&scratch).ok();
    if !manifest.is_file() {
        let (record, summary) = failure(id, "shader", &format!("{} not found (gpu-bench checkout)", manifest.display()), "");
        report.push(record, summary);
        return;
    }
    let mut build = Command::new("cargo");
    build.args(["build", "--release", "--bin", "shader_compile", "--manifest-path"]).arg(&manifest).env("CARGO_TARGET_DIR", &target_dir).env("RUSTC_WRAPPER", "");
    let line = command_line(&build);
    let log = scratch.join("build.log");
    match measure(&mut build, Some(&log)) {
        Ok(m) if m.code == 0 => {}
        _ => {
            let (record, summary) = failure(id, "shader", &format!("shader_compile build failed: {}", tail(&log, 3)), &line);
            report.push(record, summary);
            return;
        }
    }
    let exe = target_dir.join("release").join(format!("shader_compile{}", env::consts::EXE_SUFFIX));
    let mut inputs: Vec<PathBuf> = if options.wgsl.is_empty() {
        capture("git", &["ls-files", "*.wgsl"], Some(&options.root)).unwrap_or_default().lines().map(|l| options.root.join(l)).collect()
    } else {
        options.wgsl.clone()
    };
    inputs.push(options.gpu_bench.join("wgsl"));
    inputs = inputs.iter().flat_map(|input| walk_wgsl(input)).collect();
    compose_gpui_shaders(&mut inputs, &scratch.join("composed"));
    let out = scratch.join("result.json");
    let Ok(file) = File::create(&out) else { return };
    let mut run = Command::new(&exe);
    run.arg("--runs").arg(options.runs.max(5).to_string()).args(&inputs).stdout(Stdio::from(file)).stderr(Stdio::null());
    let run_line = command_line(&run);
    let started = Instant::now();
    if !run.status().map(|s| s.success()).unwrap_or(false) {
        let (record, summary) = failure(id, "shader", "shader_compile failed", &run_line);
        report.push(record, summary);
        return;
    }
    let raw = fs::read_to_string(&out).unwrap_or_default();
    let shaders = raw.matches("\"file\":").count();
    let record = obj(vec![
        ("id", text(id)),
        ("suite", text("shader")),
        ("status", text("ok")),
        ("wall_s", num(started.elapsed().as_secs_f64())),
        ("build_command", text(line)),
        ("command", text(run_line)),
        ("result", Json::Raw(raw)),
    ]);
    report.push(record, format!("| {id} | shader compile | {shaders} WGSL files, per-stage samples in the JSON |"));
}

fn stats_json_from(stat: &Stats) -> Json {
    obj(vec![("n", num(stat.n as f64)), ("mean", num(stat.mean)), ("stdev", num(stat.stdev)), ("min", num(stat.min)), ("p50", num(stat.p50)), ("max", num(stat.max))])
}

/// Reads `"metrics": { name: [numbers] }` of a gpu-bench result without a JSON dependency.
fn gpu_metrics(raw: &str) -> Vec<(String, Stats)> {
    let Some(start) = raw.find("\"metrics\":") else { return Vec::new() };
    let mut metrics = Vec::new();
    let mut rest = &raw[start + 10..];
    while let Some(open) = rest.find('[') {
        let key_part = &rest[..open];
        let Some(close) = rest[open..].find(']') else { break };
        let numbers: Vec<f64> = rest[open + 1..open + close].split(',').filter_map(|v| v.trim().parse().ok()).collect();
        if let Some(name) = key_part.rsplit('"').nth(1)
            && !name.is_empty()
            && !numbers.is_empty()
        {
            metrics.push((name.to_string(), stats(&numbers)));
        }
        rest = &rest[open + close + 1..];
        if rest.trim_start().starts_with('}') {
            break;
        }
    }
    metrics
}

fn parse_options() -> Result<(String, Options), String> {
    let mut arguments: Vec<String> = env::args().skip(1).collect();
    let command = if arguments.is_empty() { "help".to_string() } else { arguments.remove(0) };
    let value = |name: &str| arguments.iter().position(|a| a == name).and_then(|i| arguments.get(i + 1)).cloned();
    let all = |name: &str| -> Vec<String> { arguments.iter().enumerate().filter(|(_, a)| a.as_str() == name).filter_map(|(i, _)| arguments.get(i + 1).cloned()).collect() };
    let root = PathBuf::from(value("--root").unwrap_or_else(|| ".".to_string()));
    let root = fs::canonicalize(&root).map_err(|e| format!("--root {}: {e}", root.display()))?;
    let work = PathBuf::from(value("--work").unwrap_or_else(|| env::temp_dir().join("fw-perf").display().to_string()));
    let date = utc_date();
    let hostname = env::var("HOSTNAME").ok().or_else(|| env::var("COMPUTERNAME").ok()).or_else(|| capture("hostname", &[], None)).unwrap_or_else(|| "host".to_string());
    let out = PathBuf::from(value("--out").unwrap_or_else(|| root.join("docs/research/perf").join(format!("{hostname}-{date}.json")).display().to_string()));
    let list = |name: &str, default: &str| value(name).unwrap_or_else(|| default.to_string()).split(',').filter(|s| !s.is_empty()).map(str::to_string).collect::<Vec<_>>();
    let number = |name: &str, default: usize| value(name).and_then(|v| v.parse().ok()).unwrap_or(default);
    let startup_args = {
        let sets: Vec<Vec<String>> = all("--args").iter().map(|s| s.split_whitespace().map(str::to_string).collect()).collect();
        if sets.is_empty() { vec![vec!["--version".to_string()]] } else { sets }
    };
    Ok((
        command,
        Options {
            wgsl: all("--wgsl").into_iter().map(PathBuf::from).collect(),
            gpu_bench: PathBuf::from(value("--gpu-bench").unwrap_or_else(|| root.join("..").join("gpu-bench").display().to_string())),
            root,
            work,
            out,
            suites: list("--suite", "host,build,startup,link,wasm,workers"),
            only: list("--only", ""),
            runs: number("--runs", 5),
            build_runs: number("--build-runs", 3),
            warmup: number("--warmup", 3),
            incremental: !arguments.iter().any(|a| a == "--no-incremental"),
            exes: all("--exe").into_iter().map(PathBuf::from).collect(),
            startup_args,
        },
    ))
}

/// msvc builds run natively on Windows and through cargo-xwin elsewhere; each id exists for one of the two.
fn applicable(build: &Build) -> bool {
    if build.target != MSVC {
        return true;
    }
    (build.launcher == Launcher::Cargo) == cfg!(windows)
}

fn selected(options: &Options, id: &str) -> bool {
    options.only.is_empty() || options.only.iter().any(|o| id.contains(o.as_str()))
}

fn main() {
    let (command, options) = match parse_options() {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("fw_perf: {error}");
            std::process::exit(2);
        }
    };
    if command == "list" {
        for build in BUILDS {
            println!("{}\t{} -p {} --profile {} --target {}", build.id, build.workspace, build.package, build.profile, build.target);
        }
        return;
    }
    if command != "run" {
        println!("usage: fw_perf <list|run> [--root DIR] [--work DIR] [--out FILE] [--suite host,build,startup,link,wasm,workers,shader,gpu] [--only ID,...] [--runs N] [--build-runs N] [--warmup N] [--no-incremental] [--exe PATH --args \"--version\"] [--gpu-bench DIR] [--wgsl DIR]");
        std::process::exit(if command == "help" { 0 } else { 2 });
    }
    fs::create_dir_all(&options.work).ok();
    let host = host_record(&options.root);
    let mut report = Report { path: options.out.clone(), header: vec![("host".to_string(), host)], records: Vec::new(), markdown: String::new() };
    report.records.push(obj(vec![("id", text("load-before")), ("suite", text("host")), ("status", text("ok")), ("load", load_snapshot()), ("date_utc", text(utc_date()))]));
    report.flush();
    let wants = |suite: &str| options.suites.iter().any(|s| s == suite);

    for exe in &options.exes {
        run_startup(&exe.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), exe, &options.startup_args, &options, &mut report);
    }
    if wants("gpu") {
        run_gpu(&options, &mut report);
    }
    if wants("shader") {
        run_shader(&options, &mut report);
    }
    for build in BUILDS.iter().filter(|b| applicable(b) && selected(&options, b.id)) {
        let needs_build = wants("build") || if build.kind == Kind::Bin { wants("startup") && options.exes.is_empty() } else { wants("wasm") };
        let artifact = if needs_build { run_build(build, &options, &mut report) } else { None };
        if let Some(artifact) = artifact {
            if wants("startup") && build.target == host_triple() && !build.startup.is_empty() {
                let sets: Vec<Vec<String>> = build.startup.iter().map(|s| s.iter().map(|a| a.to_string()).collect()).collect();
                run_startup(build.id, &artifact, &sets, &options, &mut report);
            }
            if wants("wasm") && build.kind == Kind::Wasm {
                run_wasm_pipeline(build, &artifact, &options, &mut report);
            }
        }
        if wants("link") && build.profile == "dev" && build.target == host_triple() {
            run_link(build, &options, &mut report);
        }
    }
    if wants("workers") {
        run_workers(&options, &mut report);
    }
    report.records.push(obj(vec![("id", text("load-after")), ("suite", text("host")), ("status", text("ok")), ("load", load_snapshot())]));
    report.flush();
    println!("{}", options.out.display());
}
