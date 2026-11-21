use anyhow::{anyhow, bail, Context, Result};
use fw_toolchain::config::{Config, Lock};
use fw_toolchain::{pin, run};
use std::path::{Path, PathBuf};

const USAGE: &str = "fw toolchain <pin|fetch|llvm|xwin|build|cache|status> [--config cluster/toolchain.toml] [--lock cluster/toolchain.lock.toml]
  pin                                     resolve rustc, LLVM, dist hashes and tool commits from rust-toolchain.toml into the lock
  fetch --prefix DIR [--cache DIR]        install the pinned rustc, cargo, rust-std and rust-src (hash checked) and check commit and LLVM
  llvm --prefix DIR [--arch X64|ARM64]    install the prebuilt LLVM release matching rustc's LLVM version (sha256 from the release digest)
  xwin --output DIR [--bin xwin]          splat the Windows SDK and CRT and record the accepted terms in the lock
  build -p PKG --target TRIPLE [--tool msvc|zig|native] [--bin-dir DIR] [--xwin-cache DIR] [-- cargo args]
                                          cross build with cargo-xwin, cargo-zigbuild or cargo, timed, with executable sizes
  cache push|pull --dir DIR --name NAME   copy a directory to or from the R2 prefix of this toolchain version
  status                                  print the lock";

fn main() {
    if let Err(error) = execute() {
        eprintln!("fw toolchain: {error:#}");
        std::process::exit(2);
    }
}

fn option(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|arg| arg == name).and_then(|index| args.get(index + 1)).cloned()
}

fn execute() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).ok_or_else(|| anyhow!(USAGE))?;
    let config_path = PathBuf::from(option(&args, "--config").unwrap_or_else(|| "cluster/toolchain.toml".into()));
    let lock_path = PathBuf::from(option(&args, "--lock").unwrap_or_else(|| "cluster/toolchain.lock.toml".into()));
    let config = Config::load(&config_path)?;
    let root = Path::new(".");
    match command {
        "pin" => {
            let version = config.version(root)?;
            let lock = pin::pin(&config, &version)?;
            lock.save(&lock_path)?;
            println!("rust {} commit {} (fork has tag: {})", lock.rust.version, lock.rust.commit, lock.rust.fork_has_tag);
            println!("llvm {} commit {} branch {} (fork has commit: {})", lock.llvm.version, lock.llvm.commit, lock.llvm.branch, lock.llvm.fork_has_commit);
            for tool in &lock.tool {
                println!("{} {} commit {} (fork has commit: {})", tool.name, tool.tag, tool.commit, tool.fork_has_commit);
            }
            println!("{} artifacts, manifest sha256 {} dated {}; wrote {}", lock.artifact.len(), lock.rust.manifest_sha256, lock.rust.manifest_date, lock_path.display());
        }
        "fetch" => {
            let lock = Lock::load(&lock_path)?;
            let prefix = PathBuf::from(option(&args, "--prefix").context("--prefix DIR")?);
            let cache = PathBuf::from(option(&args, "--cache").unwrap_or_else(|| format!("{}/.dist-cache", prefix.display())));
            println!("{}", serde_json::to_string_pretty(&run::fetch(&lock, &prefix, &cache)?)?);
        }
        "llvm" => {
            let lock = Lock::load(&lock_path)?;
            let prefix = PathBuf::from(option(&args, "--prefix").context("--prefix DIR")?);
            let cache = PathBuf::from(option(&args, "--cache").unwrap_or_else(|| format!("{}-dist", prefix.display())));
            let arch = option(&args, "--arch").unwrap_or_else(|| "X64".into());
            println!("{}", serde_json::to_string_pretty(&run::llvm(&lock, &prefix, &cache, &arch)?)?);
        }
        "xwin" => {
            let mut lock = Lock::load(&lock_path)?;
            let output = PathBuf::from(option(&args, "--output").context("--output DIR")?);
            let binary = PathBuf::from(option(&args, "--bin").unwrap_or_else(|| "xwin".into()));
            let report = run::xwin(&config, &mut lock, &binary, &output)?;
            lock.save(&lock_path)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        "build" => {
            let package = option(&args, "-p").context("-p PACKAGE")?;
            let target = option(&args, "--target").context("--target TRIPLE")?;
            let bin_dir = run::resolve_bin_dir(option(&args, "--bin-dir").as_deref());
            let xwin_cache = PathBuf::from(option(&args, "--xwin-cache").unwrap_or_else(|| "xwin".into()));
            let extra: Vec<String> = args.iter().position(|arg| arg == "--").map(|index| args[index + 1..].to_vec()).unwrap_or_default();
            let report = run::build(&config, &package, &target, option(&args, "--tool").as_deref(), &bin_dir, &xwin_cache, &extra)?;
            println!("{}", serde_json::to_string(&report)?);
        }
        "cache" => {
            let lock = Lock::load(&lock_path)?;
            let push = match args.get(1).map(String::as_str) {
                Some("push") => true,
                Some("pull") => false,
                _ => bail!(USAGE),
            };
            let directory = PathBuf::from(option(&args, "--dir").context("--dir DIR")?);
            let name = option(&args, "--name").context("--name NAME")?;
            println!("{}", serde_json::to_string_pretty(&run::cache(&config, &lock, push, &directory, &name)?)?);
        }
        "status" => print!("{}", std::fs::read_to_string(&lock_path).with_context(|| lock_path.display().to_string())?),
        _ => bail!(USAGE),
    }
    Ok(())
}
