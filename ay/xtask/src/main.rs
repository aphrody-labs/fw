use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const REPOS: &[&str] = &[
    "aphrody",
    "ay",
    "browser",
    "fw",
    "nie",
    "rosegriffon",
    "shenron",
    "wiki",
];

fn workspace_root() -> PathBuf {
    if let Ok(root) = env::var("WORKSPACE_ROOT") {
        return PathBuf::from(root);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn run_capture(program: &str, args: &[&str], cwd: Option<&Path>) -> Option<String> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn cmd_status(root: &Path) -> ExitCode {
    println!("=== WORKSPACE STATUS ({}) ===", root.display());
    for repo in REPOS {
        let dir = root.join(repo);
        if !dir.join(".git").exists() {
            println!("{repo:<14} | missing");
            continue;
        }
        let branch = run_capture("git", &["branch", "--show-current"], Some(&dir))
            .unwrap_or_else(|| "detached".to_string());
        let dirty = run_capture("git", &["status", "--short"], Some(&dir))
            .map(|s| if s.is_empty() { 0 } else { s.lines().count() })
            .unwrap_or(0);
        println!("{repo:<14} | branch={branch:<10} | dirty={dirty}");
    }
    ExitCode::SUCCESS
}

fn cmd_tools() -> ExitCode {
    let tools = [
        ("bun", "--version"),
        ("rustc", "--version"),
        ("cargo", "--version"),
        ("sccache", "--version"),
        ("pwsh", "--version"),
        ("dotnet", "--version"),
        ("go", "version"),
        ("python", "--version"),
        ("clang", "--version"),
        ("cmake", "--version"),
        ("ninja", "--version"),
        ("gh", "--version"),
    ];
    println!("=== TOOLCHAIN ===");
    for (bin, flag) in tools {
        let ver = run_capture(bin, &[flag], None)
            .and_then(|s| s.lines().next().map(ToOwned::to_owned))
            .unwrap_or_else(|| "NOT FOUND".to_string());
        println!("{bin:<10} | {ver}");
    }
    ExitCode::SUCCESS
}

fn cmd_cache() -> ExitCode {
    println!("=== SHARED CACHE & SCCACHE ===");
    for var in [
        "CARGO_HOME",
        "RUSTUP_HOME",
        "CARGO_TARGET_DIR",
        "SCCACHE_DIR",
        "BUN_INSTALL",
        "VCPKG_DEFAULT_BINARY_CACHE",
    ] {
        let val = env::var(var).unwrap_or_else(|_| "(unset)".to_string());
        println!("{var:<28} = {val}");
    }
    if let Some(stats) = run_capture("sccache", &["--show-stats"], None) {
        println!("\n{stats}");
    }
    ExitCode::SUCCESS
}

fn cmd_check(root: &Path, target: Option<&str>) -> ExitCode {
    let selected: Vec<&str> = match target {
        Some(t) => vec![t],
        None => vec!["ay"],
    };
    for repo in selected {
        let dir = root.join(repo);
        if dir.join("Cargo.toml").exists() {
            println!("-> cargo check ({repo})");
            let status = Command::new("cargo")
                .args(["check", "--workspace"])
                .current_dir(&dir)
                .status();
            if !matches!(status, Ok(s) if s.success()) {
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let sub = args.next().unwrap_or_else(|| "help".to_string());
    let root = workspace_root();
    match sub.as_str() {
        "status" => cmd_status(&root),
        "tools" => cmd_tools(),
        "cache" => cmd_cache(),
        "check" => cmd_check(&root, args.next().as_deref()),
        _ => {
            println!("Usage: cargo xtask <status|tools|cache|check [repo]>");
            ExitCode::SUCCESS
        }
    }
}
