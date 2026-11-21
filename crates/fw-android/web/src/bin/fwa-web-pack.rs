//! Builds the browser client: `cargo build --target wasm32-unknown-unknown`, `wasm-bindgen --target web` and the
//! generated page, into one directory a static server can serve.
//!
//! `fwa-web-pack [--out <dir>] [--debug] [--target-dir <dir>]` (run from anywhere inside the `fw-android` workspace).
//! It refuses to run when the installed `wasm-bindgen` CLI differs from the `wasm-bindgen` crate in `Cargo.lock`,
//! because the two must match exactly.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use fw_android_web::loader;

fn run(command: &mut Command) -> Result<String, String> {
    let output = command.output().map_err(|error| format!("{command:?}: {error}"))?;
    if !output.status.success() {
        return Err(format!("{command:?} failed with {}", output.status));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn locked_version(lock: &Path, package: &str) -> Option<String> {
    let text = std::fs::read_to_string(lock).ok()?;
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if line == format!("name = \"{package}\"") {
            return lines.next()?.strip_prefix("version = \"")?.strip_suffix('"').map(str::to_string);
        }
    }
    None
}

fn pack() -> Result<(), String> {
    let mut out = PathBuf::from("dist");
    let mut profile = "release";
    let mut target_dir = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--out" => out = args.next().ok_or("--out needs a directory")?.into(),
            "--target-dir" => target_dir = Some(PathBuf::from(args.next().ok_or("--target-dir needs a directory")?)),
            "--debug" => profile = "debug",
            other => return Err(format!("unknown argument {other}")),
        }
    }
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let target_dir = target_dir.unwrap_or_else(|| workspace.join("target"));

    let mut build = Command::new("cargo");
    build
        .args(["build", "-p", "fw-android-web", "--lib", "--target", "wasm32-unknown-unknown", "--target-dir"])
        .arg(&target_dir)
        .arg("--manifest-path")
        .arg(workspace.join("Cargo.toml"));
    if profile == "release" {
        build.arg("--release");
    }
    run(&mut build)?;

    let cli = run(Command::new("wasm-bindgen").arg("--version"))?;
    let cli = cli.split_whitespace().nth(1).unwrap_or_default().to_string();
    let locked = locked_version(&workspace.join("Cargo.lock"), "wasm-bindgen").ok_or("wasm-bindgen is not in Cargo.lock")?;
    if cli != locked {
        return Err(format!("wasm-bindgen CLI {cli} does not match the crate {locked} in Cargo.lock; install the same version"));
    }

    let wasm = target_dir.join("wasm32-unknown-unknown").join(profile).join(format!("{}.wasm", loader::MODULE));
    std::fs::create_dir_all(&out).map_err(|error| format!("{}: {error}", out.display()))?;
    run(Command::new("wasm-bindgen").args(["--target", "web", "--no-typescript", "--out-dir"]).arg(&out).arg(&wasm))?;
    std::fs::write(out.join("index.html"), loader::index_html()).map_err(|error| error.to_string())?;
    std::fs::write(out.join("loader.js"), loader::loader_js()).map_err(|error| error.to_string())?;

    let size = std::fs::metadata(out.join(format!("{}_bg.wasm", loader::MODULE))).map_err(|error| error.to_string())?.len();
    println!("fwa-web-pack: {} (wasm-bindgen {cli}, {profile}, {size} bytes of wasm)", out.display());
    Ok(())
}

fn main() -> ExitCode {
    match pack() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("fwa-web-pack: {message}");
            ExitCode::FAILURE
        }
    }
}
