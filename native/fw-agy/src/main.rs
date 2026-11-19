use fw_abi::{ABI_VERSION, PeInspectionReport, inspect_pe_headers_fast, inspect_pe_path};
use serde::Serialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const STUDIO_SUBCOMMANDS: &[&str] = &[
    "depot",
    "ingest",
    "gemma",
    "notebook",
    "notebooklm",
    "ultra",
    "collections",
    "mode",
    "goal-loop",
    "web",
    "desktop",
];

#[derive(Debug, PartialEq, Eq)]
enum Route {
    FwInfo,
    FwInspect(Option<String>),
    FwTarget(Vec<String>),
    FwUpgrade(Vec<String>),
    FwBench(Vec<String>),
    FwDaemon(Vec<String>),
    FwService(Vec<String>),
    FwPlugin(Vec<String>),
    FwLearn(Vec<String>),
    FwMcp,
    FwCli(Vec<String>),
    Studio(Vec<String>),
    StudioDoctor(Vec<String>),
    Upstream(Vec<String>),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BinaryArtifact {
    path: String,
    exists: bool,
    bytes: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FwInfoPayload {
    ok: bool,
    product: &'static str,
    binary: &'static str,
    version: &'static str,
    abi_version: u32,
    build: BuildProfile,
    self_binary: BinaryArtifact,
    repo_root: String,
    upstream_agy: Option<UpstreamSummary>,
    fw_cli: BinaryArtifact,
    fw_abi: BinaryArtifact,
    target_manifest_present: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BuildProfile {
    profile: &'static str,
    lto: &'static str,
    aot: bool,
    os: &'static str,
    arch: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpstreamSummary {
    path: String,
    exists: bool,
    read_only_signed_upstream: bool,
    pe: Option<PeInspectionReport>,
}

fn classify_route(args: &[String]) -> Route {
    let Some(first) = args.first() else {
        return Route::Upstream(Vec::new());
    };
    let lower = first.to_ascii_lowercase();
    let rest = args[1..].to_vec();
    match lower.as_str() {
        "--fw-info" | "fw-info" => Route::FwInfo,
        "--fw-inspect" | "fw-inspect" => Route::FwInspect(rest.into_iter().next()),
        "--fw-target" | "fw-target" => Route::FwTarget(rest),
        "--fw-upgrade" | "fw-upgrade" => Route::FwUpgrade(rest),
        "--fw-bench" | "fw-bench" => Route::FwBench(rest),
        "--fw-daemon" | "fw-daemon" => Route::FwDaemon(rest),
        "--fw-service" | "fw-service" => Route::FwService(rest),
        "--fw-plugin" | "fw-plugin" => Route::FwPlugin(rest),
        "--fw-learn" | "fw-learn" => Route::FwLearn(rest),
        "--fw-mcp" | "fw-mcp" => Route::FwMcp,
        "fw" => Route::FwCli(rest),
        "studio-doctor" => Route::StudioDoctor(rest),
        cmd if STUDIO_SUBCOMMANDS.contains(&cmd) => Route::Studio(args.to_vec()),
        _ => Route::Upstream(args.to_vec()),
    }
}

fn is_repo_root(dir: &Path) -> bool {
    dir.join("fw.targets.json").exists()
        || (dir.join("core").is_dir() && dir.join("native").join("fw-abi").is_dir())
}

fn resolve_repo_root() -> PathBuf {
    // FAIRWIND_ROOT is the legacy name, kept as fallback for environments set up before the rename.
    if let Some(env_root) = env::var_os("FW_ROOT").or_else(|| env::var_os("FAIRWIND_ROOT")) {
        let p = PathBuf::from(env_root);
        if is_repo_root(&p) {
            return p;
        }
    }
    if let Ok(exe) = env::current_exe() {
        for ancestor in exe.ancestors().skip(1) {
            if is_repo_root(ancestor) {
                return ancestor.to_path_buf();
            }
        }
    }
    if let Ok(cwd) = env::current_dir() {
        for ancestor in cwd.ancestors() {
            if is_repo_root(ancestor) {
                return ancestor.to_path_buf();
            }
        }
    }
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => a == b,
    }
}

fn resolve_upstream_agy(self_exe: &Path) -> Option<PathBuf> {
    if let Ok(explicit) = env::var("AGY_UPSTREAM_BIN") {
        let p = PathBuf::from(explicit);
        if p.is_file() && !same_path(&p, self_exe) {
            return Some(p);
        }
    }
    if cfg!(windows) {
        if let Ok(local_app) = env::var("LOCALAPPDATA") {
            let p = PathBuf::from(local_app).join("agy").join("bin").join("agy.exe");
            if p.is_file() && !same_path(&p, self_exe) {
                return Some(p);
            }
        }
    } else if let Ok(home) = env::var("HOME") {
        for rel in [".local/share/agy/bin/agy", ".agy/bin/agy"] {
            let p = PathBuf::from(&home).join(rel);
            if p.is_file() && !same_path(&p, self_exe) {
                return Some(p);
            }
        }
    }
    None
}

fn resolve_agy_studio() -> Option<PathBuf> {
    if let Ok(explicit) = env::var("AGY_STUDIO_BIN") {
        let p = PathBuf::from(explicit);
        if p.is_file() {
            return Some(p);
        }
    }
    let home = env::var("USERPROFILE").or_else(|_| env::var("HOME")).ok()?;
    let bin_name = if cfg!(windows) { "agy-studio.exe" } else { "agy-studio" };
    let candidate = PathBuf::from(home).join(".local").join("bin").join(bin_name);
    candidate.is_file().then_some(candidate)
}

fn resolve_fw_cli(root: &Path) -> PathBuf {
    let exe_name = if cfg!(windows) { "fw.exe" } else { "fw" };
    for candidate in [
        root.join("target").join("agy").join(exe_name),
        root.join("dist").join(exe_name),
    ] {
        if candidate.is_file() {
            return candidate;
        }
    }
    root.join("dist").join(exe_name)
}

fn resolve_fw_abi(root: &Path) -> PathBuf {
    let lib_name = if cfg!(windows) {
        "fw_abi.dll"
    } else if cfg!(target_os = "macos") {
        "libfw_abi.dylib"
    } else {
        "libfw_abi.so"
    };
    for candidate in [
        root.join("target").join("agy").join(lib_name),
        root.join("native").join("fw-abi").join("target").join("release").join(lib_name),
        root.join("dist").join(lib_name),
    ] {
        if candidate.is_file() {
            return candidate;
        }
    }
    root.join("native").join("fw-abi").join("target").join("release").join(lib_name)
}

fn describe_artifact(path: &Path) -> BinaryArtifact {
    let bytes = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    BinaryArtifact {
        path: path.display().to_string(),
        exists: path.is_file(),
        bytes,
    }
}

fn run_process(program: &Path, args: &[String], cwd: Option<&Path>) -> ExitCode {
    let mut cmd = Command::new(program);
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    match cmd.status() {
        Ok(status) => {
            let code = status.code().unwrap_or(1);
            ExitCode::from(u8::try_from(code).unwrap_or(1))
        }
        Err(err) => {
            eprintln!("{{\"ok\":false,\"error\":\"failed to spawn {}: {}\"}}", program.display(), err);
            ExitCode::from(1)
        }
    }
}

fn run_fw_subcommand(root: &Path, sub: &str, rest: &[String]) -> ExitCode {
    let cli = resolve_fw_cli(root);
    let mut all_args = Vec::with_capacity(rest.len() + 2);
    if cli.is_file() {
        all_args.push(sub.to_string());
        all_args.extend(rest.iter().cloned());
        return run_process(&cli, &all_args, Some(root));
    }
    all_args.push("src/fw_cli.ts".to_string());
    all_args.push(sub.to_string());
    all_args.extend(rest.iter().cloned());
    run_process(Path::new("bun"), &all_args, Some(root))
}

fn handle_fw_info(root: &Path, self_exe: &Path) -> ExitCode {
    let upstream = resolve_upstream_agy(self_exe).map(|p| {
        let pe = inspect_pe_headers_fast(&p).ok();
        UpstreamSummary {
            path: p.display().to_string(),
            exists: p.is_file(),
            read_only_signed_upstream: true,
            pe,
        }
    });
    let payload = FwInfoPayload {
        ok: true,
        product: "fw",
        binary: "fw-agy",
        version: env!("CARGO_PKG_VERSION"),
        abi_version: ABI_VERSION,
        build: BuildProfile {
            profile: if cfg!(debug_assertions) { "debug" } else { "release" },
            lto: if cfg!(debug_assertions) { "none" } else { "fat" },
            aot: true,
            os: env::consts::OS,
            arch: env::consts::ARCH,
        },
        self_binary: describe_artifact(self_exe),
        repo_root: root.display().to_string(),
        upstream_agy: upstream,
        fw_cli: describe_artifact(&resolve_fw_cli(root)),
        fw_abi: describe_artifact(&resolve_fw_abi(root)),
        target_manifest_present: root.join("target").join("manifest.json").is_file(),
    };
    match serde_json::to_string_pretty(&payload) {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{{\"ok\":false,\"error\":\"serialize failed: {err}\"}}");
            ExitCode::from(1)
        }
    }
}

fn handle_fw_inspect(self_exe: &Path, target: Option<String>) -> ExitCode {
    let path = match target {
        Some(p) => PathBuf::from(p),
        None => match resolve_upstream_agy(self_exe) {
            Some(p) => p,
            None => self_exe.to_path_buf(),
        },
    };
    match inspect_pe_path(&path) {
        Ok(report) => match serde_json::to_string_pretty(&report) {
            Ok(text) => {
                println!("{text}");
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("{{\"ok\":false,\"error\":\"serialize failed: {err}\"}}");
                ExitCode::from(1)
            }
        },
        Err(err) => {
            eprintln!(
                "{}",
                serde_json::json!({ "ok": false, "path": path.display().to_string(), "error": err })
            );
            ExitCode::from(1)
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let self_exe = env::current_exe().unwrap_or_else(|_| PathBuf::from("agy"));
    let root = resolve_repo_root();

    match classify_route(&args) {
        Route::FwInfo => handle_fw_info(&root, &self_exe),
        Route::FwInspect(path) => handle_fw_inspect(&self_exe, path),
        Route::FwTarget(rest) => {
            if rest.is_empty() || (rest.len() == 1 && rest[0] == "status") {
                let manifest_path = root.join("target").join("manifest.json");
                if let Ok(raw) = fs::read_to_string(&manifest_path) {
                    println!("{raw}");
                    return ExitCode::SUCCESS;
                }
            }
            run_fw_subcommand(&root, "target", &rest)
        }
        Route::FwUpgrade(rest) => run_fw_subcommand(&root, "upgrade", &rest),
        Route::FwBench(rest) => run_fw_subcommand(&root, "bench", &rest),
        Route::FwDaemon(rest) => run_fw_subcommand(&root, "daemon", &rest),
        Route::FwService(rest) => run_fw_subcommand(&root, "service", &rest),
        Route::FwPlugin(rest) => run_fw_subcommand(&root, "plugin", &rest),
        Route::FwLearn(rest) => run_fw_subcommand(&root, "learn", &rest),
        Route::FwMcp => run_fw_subcommand(&root, "mcp", &[]),
        Route::FwCli(rest) => {
            if let Some((first, tail)) = rest.split_first() {
                run_fw_subcommand(&root, first, tail)
            } else {
                run_fw_subcommand(&root, "status", &[])
            }
        }
        Route::Studio(studio_args) => match resolve_agy_studio() {
            Some(studio) => run_process(&studio, &studio_args, None),
            None => {
                eprintln!("{{\"ok\":false,\"error\":\"agy-studio binary not found\"}}");
                ExitCode::from(1)
            }
        },
        Route::StudioDoctor(rest) => match resolve_agy_studio() {
            Some(studio) => {
                let mut all = Vec::with_capacity(rest.len() + 1);
                all.push("doctor".to_string());
                all.extend(rest);
                run_process(&studio, &all, None)
            }
            None => {
                eprintln!("{{\"ok\":false,\"error\":\"agy-studio binary not found\"}}");
                ExitCode::from(1)
            }
        },
        Route::Upstream(pass_args) => match resolve_upstream_agy(&self_exe) {
            Some(upstream) => run_process(&upstream, &pass_args, None),
            None => {
                eprintln!("{{\"ok\":false,\"error\":\"upstream signed agy binary not found (set AGY_UPSTREAM_BIN)\"}}");
                ExitCode::from(1)
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_classification_is_unambiguous() {
        assert_eq!(classify_route(&["--fw-info".into()]), Route::FwInfo);
        assert_eq!(
            classify_route(&["--fw-inspect".into(), "foo.exe".into()]),
            Route::FwInspect(Some("foo.exe".into()))
        );
        assert_eq!(
            classify_route(&["--fw-upgrade".into(), "--check".into()]),
            Route::FwUpgrade(vec!["--check".into()])
        );
        assert_eq!(
            classify_route(&["fw".into(), "status".into()]),
            Route::FwCli(vec!["status".into()])
        );
        assert_eq!(
            classify_route(&["depot".into(), "list".into()]),
            Route::Studio(vec!["depot".into(), "list".into()])
        );
        assert_eq!(
            classify_route(&["studio-doctor".into()]),
            Route::StudioDoctor(vec![])
        );
        assert_eq!(
            classify_route(&["--version".into()]),
            Route::Upstream(vec!["--version".into()])
        );
        assert_eq!(
            classify_route(&["-p".into(), "hello".into()]),
            Route::Upstream(vec!["-p".into(), "hello".into()])
        );
    }

    #[test]
    fn resolves_fw_repo_root() {
        let root = resolve_repo_root();
        assert!(root.join("native").join("fw-abi").is_dir());
    }
}
