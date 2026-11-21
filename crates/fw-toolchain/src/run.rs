use crate::config::{Config, Lock, XwinTerms};
use crate::pin::sha256_hex;
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn check(command: &mut Command) -> Result<()> {
    let status = command.status().with_context(|| format!("spawn {command:?}"))?;
    if !status.success() {
        bail!("{command:?} exited with {status}");
    }
    Ok(())
}

fn load_average() -> String {
    std::fs::read_to_string("/proc/loadavg").map(|text| text.split_whitespace().take(3).collect::<Vec<_>>().join(" ")).unwrap_or_default()
}

fn directory_bytes(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else { return 0 };
    entries.flatten().map(|entry| match entry.metadata() {
        Ok(meta) if meta.is_dir() => directory_bytes(&entry.path()),
        Ok(meta) => meta.len(),
        Err(_) => 0,
    }).sum()
}

/// Downloads every pinned artifact (hash checked against the dist manifest value in the lock), installs it into `prefix` with
/// the installer shipped in the tarball and verifies that the resulting rustc is the pinned commit built against the pinned LLVM.
pub fn fetch(lock: &Lock, prefix: &Path, cache: &Path) -> Result<Value> {
    std::fs::create_dir_all(cache)?;
    let started = Instant::now();
    let mut downloaded_bytes = 0u64;
    let mut rows = Vec::new();
    for artifact in &lock.artifact {
        let file = cache.join(artifact.url.rsplit('/').next().ok_or_else(|| anyhow!("bad url {}", artifact.url))?);
        let step = Instant::now();
        let mut bytes = std::fs::read(&file).ok().filter(|bytes| sha256_hex(bytes) == artifact.sha256);
        let cached = bytes.is_some();
        if bytes.is_none() {
            let mut body = Vec::new();
            ureq::get(&artifact.url).call().with_context(|| artifact.url.clone())?.into_body().as_reader().read_to_end(&mut body)?;
            if sha256_hex(&body) != artifact.sha256 {
                bail!("{}: sha256 differs from the manifest hash {}", artifact.url, artifact.sha256);
            }
            std::fs::write(&file, &body)?;
            bytes = Some(body);
        }
        let size = bytes.map(|bytes| bytes.len() as u64).unwrap_or(0);
        downloaded_bytes += size;
        let unpack = cache.join(format!("unpack-{}-{}", artifact.package, artifact.target.replace('*', "any")));
        let _ = std::fs::remove_dir_all(&unpack);
        std::fs::create_dir_all(&unpack)?;
        check(Command::new("tar").arg("-xJf").arg(&file).arg("-C").arg(&unpack).arg("--strip-components=1"))?;
        check(Command::new("sh").arg(unpack.join("install.sh")).arg(format!("--prefix={}", prefix.display())).arg("--disable-ldconfig").stdout(Stdio::null()))?;
        std::fs::remove_dir_all(&unpack)?;
        rows.push(json!({"package": artifact.package, "target": artifact.target, "bytes": size, "cached": cached, "seconds": step.elapsed().as_secs_f64()}));
    }
    let report = String::from_utf8(Command::new(prefix.join("bin/rustc")).arg("-vV").output()?.stdout)?;
    let field = |key: &str| report.lines().find_map(|line| line.strip_prefix(key)).map(str::trim).unwrap_or_default().to_owned();
    let (commit, llvm) = (field("commit-hash:"), field("LLVM version:"));
    if commit != lock.rust.commit {
        bail!("installed rustc commit {commit} is not the pinned {}", lock.rust.commit);
    }
    let llvm_matches = llvm == lock.llvm.version;
    Ok(json!({
        "version": lock.rust.version, "commit": commit, "llvm_reported": llvm, "llvm_pinned": lock.llvm.version, "llvm_matches": llvm_matches,
        "prefix": prefix, "installed_bytes": directory_bytes(prefix), "downloaded_bytes": downloaded_bytes, "seconds": started.elapsed().as_secs_f64(), "artifacts": rows,
    }))
}

/// Splats the Windows SDK and CRT with xwin and records the terms that were accepted, for which manifest and when.
pub fn xwin(config: &Config, lock: &mut Lock, binary: &Path, output: &Path) -> Result<Value> {
    let cache = output.join("cache");
    let started = Instant::now();
    let mut command = Command::new(binary);
    command.arg("--accept-license").arg("--manifest-version").arg(config.xwin.manifest_version.to_string()).arg("--cache-dir").arg(&cache);
    for arch in &config.xwin.arch {
        command.args(["--arch", arch]);
    }
    for variant in &config.xwin.variant {
        command.args(["--variant", variant]);
    }
    check(command.arg("splat").arg("--output").arg(output.join("splat")))?;
    let commit = lock.tool.iter().find(|tool| tool.name == "xwin").map(|tool| tool.commit.clone()).unwrap_or_default();
    lock.xwin_terms = Some(XwinTerms {
        terms: config.xwin.terms.clone(),
        manifest_version: config.xwin.manifest_version,
        accepted_unix: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        accepted_by_flag: "--accept-license".to_owned(),
        xwin_commit: commit,
    });
    Ok(json!({"splat": output.join("splat"), "splat_bytes": directory_bytes(&output.join("splat")), "seconds": started.elapsed().as_secs_f64(), "terms": config.xwin.terms}))
}

/// One cross build, timed, with the size of every executable cargo reports.
pub fn build(config: &Config, package: &str, triple: &str, tool: Option<&str>, bin_dir: &Path, xwin_cache: &Path, extra: &[String]) -> Result<Value> {
    let row = config.targets.iter().find(|row| row.triple == triple && tool.is_none_or(|wanted| wanted == row.tool)).ok_or_else(|| anyhow!("no target row for {triple}{}", tool.map(|t| format!(" with tool {t}")).unwrap_or_default()))?;
    let path = format!("{}:{}", bin_dir.display(), std::env::var("PATH").unwrap_or_default());
    let mut command = Command::new("cargo");
    command.env("PATH", path).env("XWIN_CACHE_DIR", xwin_cache);
    match row.tool.as_str() {
        "msvc" => command.args(["xwin", "build", "--xwin-accept-license", "--target", triple]),
        "zig" => command.args(["zigbuild", "--target", &match &row.libc {
            Some(libc) => format!("{triple}.{libc}"),
            None => triple.to_owned(),
        }]),
        "native" => command.args(["build", "--target", triple]),
        other => bail!("unknown tool {other}"),
    };
    command.args(["--release", "-p", package, "--message-format=json-render-diagnostics"]).args(extra).stdout(Stdio::piped());
    let started = Instant::now();
    let load_before = load_average();
    let output = command.output().with_context(|| format!("spawn {command:?}"))?;
    let wall = started.elapsed().as_secs_f64();
    let mut executables = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Ok(message) = serde_json::from_str::<Value>(line) else { continue };
        if message["reason"] == "compiler-artifact"
            && message["package_id"].as_str().is_some_and(|id| id.contains(package))
            && let Some(executable) = message["executable"].as_str()
        {
            let bytes = std::fs::metadata(executable).map(|meta| meta.len()).unwrap_or(0);
            executables.push(json!({"path": executable, "bytes": bytes}));
        }
    }
    Ok(json!({"package": package, "target": triple, "tool": row.tool, "success": output.status.success(), "wall_seconds": wall, "load_before": load_before, "load_after": load_average(), "executables": executables}))
}

pub fn cache(config: &Config, lock: &Lock, push: bool, directory: &Path, name: &str) -> Result<Value> {
    let remote = format!("{}:{}/{}/{}/{name}", config.cache.rclone_remote, config.cache.bucket, config.cache.prefix, lock.rust.version);
    let started = Instant::now();
    let mut command = Command::new("rclone");
    command.arg("copy");
    if push {
        command.arg(directory).arg(&remote);
    } else {
        command.arg(&remote).arg(directory);
    }
    check(command.args(["--transfers", "16", "--s3-no-check-bucket"]))?;
    Ok(json!({"remote": remote, "direction": if push { "push" } else { "pull" }, "bytes": directory_bytes(directory), "seconds": started.elapsed().as_secs_f64()}))
}

pub fn resolve_bin_dir(arg: Option<&str>) -> PathBuf {
    arg.map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

/// Installs the prebuilt LLVM release of the pinned version: streamed to disk with a running sha256, then unpacked.
pub fn llvm(lock: &Lock, prefix: &Path, cache: &Path, arch: &str) -> Result<Value> {
    use sha2::{Digest, Sha256};
    use std::io::Write;
    let needle = format!("Linux-{arch}.tar.zst");
    let asset = lock.llvm.asset.iter().find(|asset| asset.name.ends_with(&needle)).ok_or_else(|| anyhow!("no pinned LLVM asset for {needle}"))?;
    std::fs::create_dir_all(cache)?;
    let file = cache.join(&asset.name);
    let started = Instant::now();
    let mut cached = false;
    if let Ok(bytes) = std::fs::metadata(&file) {
        cached = bytes.len() == asset.bytes && file_sha256(&file)? == asset.sha256;
    }
    if !cached {
        let mut body = ureq::get(&asset.url).call().with_context(|| asset.url.clone())?.into_body();
        let mut reader = body.as_reader();
        let mut out = std::fs::File::create(&file)?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 1 << 20];
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            out.write_all(&buffer[..read])?;
        }
        if hex::encode(hasher.finalize()) != asset.sha256 {
            bail!("{}: sha256 differs from the release digest {}", asset.url, asset.sha256);
        }
    }
    std::fs::create_dir_all(prefix)?;
    // The release tarballs are compressed with a 1 GiB window; zstd refuses to decode them without --long=30.
    let mut decoder = Command::new("zstd").args(["-d", "--long=30", "-c"]).arg(&file).stdout(Stdio::piped()).spawn().context("spawn zstd")?;
    let decoded = decoder.stdout.take().ok_or_else(|| anyhow!("zstd has no stdout"))?;
    check(Command::new("tar").args(["-xf", "-", "-C"]).arg(prefix).arg("--strip-components=1").stdin(decoded))?;
    if !decoder.wait()?.success() {
        bail!("zstd failed on {}", file.display());
    }
    let version = String::from_utf8(Command::new(prefix.join("bin/clang")).arg("--version").output()?.stdout)?;
    let first = version.lines().next().unwrap_or_default().to_owned();
    Ok(json!({"asset": asset.name, "bytes": asset.bytes, "cached": cached, "clang": first, "pinned": lock.llvm.version, "installed_bytes": directory_bytes(prefix), "seconds": started.elapsed().as_secs_f64()}))
}

fn file_sha256(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            return Ok(hex::encode(hasher.finalize()));
        }
        hasher.update(&buffer[..read]);
    }
}
