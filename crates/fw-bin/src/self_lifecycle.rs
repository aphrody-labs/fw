#![forbid(unsafe_code)]

//! Pure-Rust Zero-Subprocess End-User Lifecycle Manager (`fw self-install`, `fw self-update`, `fw self-uninstall`).
//! Handles automatic installation into `~/.local/bin`, TLS 1.3 GitHub Releases self-update with SHA-256 verification
//! and atomic `.prev` rollback swap, and clean uninstallation without external scripts or subprocesses.

use fw_fx::config::FxConfig;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const REPO_SLUG: &str = "aphrody-labs/fw";

pub fn default_install_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".local").join("bin")
}

pub fn binary_filename() -> &'static str {
    if cfg!(windows) { "fw.exe" } else { "fw" }
}

pub fn platform_asset_candidates() -> &'static [&'static str] {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => &[
            "fw-x86_64-windows.exe",
            "fw-x86_64-pc-windows-msvc.exe",
            "fw-windows-x86_64.exe",
            "fw.exe",
        ],
        ("linux", "x86_64") => &[
            "fw-x86_64-linux-musl",
            "fw-x86_64-unknown-linux-musl",
            "fw-x86_64-unknown-linux-gnu",
            "fw-linux-x86_64",
        ],
        ("linux", "aarch64") => &[
            "fw-aarch64-linux-musl",
            "fw-aarch64-unknown-linux-musl",
            "fw-aarch64-unknown-linux-gnu",
            "fw-linux-aarch64",
        ],
        ("macos", "aarch64") => &[
            "fw-aarch64-macos",
            "fw-aarch64-apple-darwin",
            "fw-darwin-arm64",
        ],
        ("macos", "x86_64") => &[
            "fw-x86_64-macos",
            "fw-x86_64-apple-darwin",
            "fw-darwin-x86_64",
        ],
        _ => &["fw"],
    }
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let digest = h.finalize();
    let mut s = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(&mut s, "{b:02x}");
    }
    s
}

fn flag_str<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.windows(2).find(|w| w[0] == name).map(|w| w[1].as_str())
}

/// `fw self-install [--dir <bin_dir>]`
pub fn run_self_install(args: &[String]) -> Result<Value, String> {
    let dest_dir = flag_str(args, "--dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_install_dir);
    let src = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    install_binary_to(&src, &dest_dir)
}

pub fn install_binary_to(src: &Path, dest_dir: &Path) -> Result<Value, String> {
    fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
    let dest = dest_dir.join(binary_filename());
    let bytes = fs::read(src).map_err(|e| format!("read {}: {e}", src.display()))?;
    let hash = sha256_bytes(&bytes);
    if src != dest {
        atomic_swap_bytes(&dest, &bytes)?;
    }
    let in_path = std::env::var("PATH")
        .unwrap_or_default()
        .contains(&dest_dir.display().to_string());
    Ok(json!({
        "ok": true,
        "zeroSubprocess": true,
        "installed": dest.display().to_string(),
        "version": env!("CARGO_PKG_VERSION"),
        "bytes": bytes.len(),
        "sha256": hash,
        "inPath": in_path
    }))
}

fn atomic_swap_bytes(dest: &Path, bytes: &[u8]) -> Result<(), String> {
    let prev = dest.with_extension(if cfg!(windows) { "exe.prev" } else { "prev" });
    let tmp = dest.with_extension(if cfg!(windows) { "exe.new" } else { "new" });
    fs::write(&tmp, bytes).map_err(|e| format!("write {}: {e}", tmp.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755));
    }

    if dest.exists() {
        let _ = fs::remove_file(&prev);
        fs::rename(dest, &prev).map_err(|e| format!("backup {} -> {}: {e}", dest.display(), prev.display()))?;
    }
    if let Err(e) = fs::rename(&tmp, dest) {
        if prev.exists() {
            let _ = fs::rename(&prev, dest);
        }
        return Err(format!("promote {} -> {}: {e}", tmp.display(), dest.display()));
    }
    Ok(())
}

fn http_get_json(agent: &ureq::Agent, url: &str) -> Result<Value, String> {
    let mut resp = agent
        .get(url)
        .header("User-Agent", concat!("fw/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("GET {url}: {e}"))?;
    let raw = resp.body_mut().read_to_string().map_err(|e| format!("GET {url}: {e}"))?;
    serde_json::from_str(&raw).map_err(|e| format!("GET {url}: invalid JSON ({e})"))
}

/// The release to install: the tag given with `--version`, else the latest stable release, else the newest published
/// release (a project that only ships pre-releases has no "latest": GitHub answers 404 there).
fn resolve_release(agent: &ureq::Agent, tag: Option<&str>) -> Result<Value, String> {
    if let Some(tag) = tag {
        return http_get_json(agent, &format!("https://api.github.com/repos/{REPO_SLUG}/releases/tags/{tag}"));
    }
    if let Ok(release) = http_get_json(agent, &format!("https://api.github.com/repos/{REPO_SLUG}/releases/latest")) {
        return Ok(release);
    }
    let list = http_get_json(agent, &format!("https://api.github.com/repos/{REPO_SLUG}/releases?per_page=10"))?;
    list.as_array()
        .and_then(|releases| releases.iter().find(|r| r["draft"] != true))
        .cloned()
        .ok_or_else(|| format!("{REPO_SLUG} has no published release"))
}

/// Expected SHA-256 of `asset_name` from the release's `SHA256SUMS`-style asset (`<hex>  <name>` lines).
fn checksum_from_sums(agent: &ureq::Agent, assets: &[Value], asset_name: &str) -> Option<String> {
    let sums = assets.iter().find(|a| {
        a["name"].as_str().is_some_and(|n| {
            let n = n.to_ascii_lowercase();
            n.contains("checksum") || n.contains("sha256sums")
        })
    })?;
    let url = sums["browser_download_url"].as_str()?;
    let text = agent
        .get(url)
        .header("User-Agent", concat!("fw/", env!("CARGO_PKG_VERSION")))
        .call()
        .ok()?
        .body_mut()
        .read_to_string()
        .ok()?;
    digest_for(&text, asset_name)
}

fn digest_for(sums: &str, asset_name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let hash = fields.next()?;
        let name = fields.next()?.trim_start_matches('*');
        name.eq_ignore_ascii_case(asset_name).then(|| hash.to_ascii_lowercase())
    })
}

/// `fw self-update [--check] [--version <tag>]`
pub fn run_self_update(args: &[String], offline: bool) -> Result<Value, String> {
    if offline {
        return Err("disabled by --offline".to_owned());
    }
    let check_only = args.iter().any(|a| a == "--check");
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(120)))
        .build()
        .into();

    let release = resolve_release(&agent, flag_str(args, "--version"))?;
    let latest_tag = release["tag_name"].as_str().unwrap_or(env!("CARGO_PKG_VERSION"));
    let current = env!("CARGO_PKG_VERSION");
    let assets = release["assets"].as_array().cloned().unwrap_or_default();
    let candidates = platform_asset_candidates();

    let Some(asset) = candidates.iter().find_map(|cand| {
        assets.iter().find(|a| a["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(cand)))
    }) else {
        return Err(format!(
            "release {latest_tag} has no asset for this platform (looked for {})",
            candidates.join(", ")
        ));
    };
    let asset_name = asset["name"].as_str().unwrap_or("fw");
    let expected_sha = checksum_from_sums(&agent, &assets, asset_name);

    let target_exe = std::env::current_exe().unwrap_or_else(|_| default_install_dir().join(binary_filename()));
    let current_sha = fs::read(&target_exe).ok().map(|bytes| sha256_bytes(&bytes));
    // The running binary against the published digest: exact, and independent of how versions are named.
    let up_to_date = matches!((&current_sha, &expected_sha), (Some(have), Some(want)) if have == want);

    if check_only || up_to_date {
        return Ok(json!({
            "ok": true,
            "zeroSubprocess": true,
            "currentVersion": current,
            "latestTag": latest_tag,
            "updateAvailable": !up_to_date,
            "upToDate": up_to_date,
            "matchedAsset": asset_name,
            "checkOnly": check_only
        }));
    }

    let dl_url = asset["browser_download_url"]
        .as_str()
        .ok_or_else(|| "release asset missing browser_download_url".to_owned())?;
    let bytes = agent
        .get(dl_url)
        .header("User-Agent", concat!("fw/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|e| format!("download {dl_url}: {e}"))?
        .body_mut()
        .read_to_vec()
        .map_err(|e| format!("download {dl_url}: {e}"))?;
    let actual_sha = sha256_bytes(&bytes);
    let checksum_verified = match &expected_sha {
        Some(want) if *want != actual_sha => {
            return Err(format!("SHA-256 mismatch for {asset_name}: expected {want}, got {actual_sha}"));
        }
        Some(_) => true,
        None => false,
    };

    atomic_swap_bytes(&target_exe, &bytes)?;

    Ok(json!({
        "ok": true,
        "zeroSubprocess": true,
        "updated": true,
        "previousVersion": current,
        "newTag": latest_tag,
        "asset": asset_name,
        "path": target_exe.display().to_string(),
        "bytes": bytes.len(),
        "sha256": actual_sha,
        "checksumVerified": checksum_verified
    }))
}

/// `fw self-uninstall [--dir <bin_dir>] [--purge-data]`
pub fn run_self_uninstall(args: &[String]) -> Result<Value, String> {
    let dest_dir = flag_str(args, "--dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_install_dir);
    let purge_data = args.iter().any(|a| a == "--purge-data");
    uninstall_from(&dest_dir, purge_data)
}

pub fn uninstall_from(dest_dir: &Path, purge_data: bool) -> Result<Value, String> {
    let mut removed = Vec::new();
    for name in [
        binary_filename().to_owned(),
        format!("{}.prev", binary_filename()),
        format!("{}.new", binary_filename()),
    ] {
        let p = dest_dir.join(name);
        if p.exists() && fs::remove_file(&p).is_ok() {
            removed.push(p.display().to_string());
        }
    }

    if purge_data {
        let cfg = FxConfig::from_env();
        if cfg.fx_home.exists() && fs::remove_dir_all(&cfg.fx_home).is_ok() {
            removed.push(cfg.fx_home.display().to_string());
        }
    }

    Ok(json!({
        "ok": true,
        "zeroSubprocess": true,
        "uninstalled": true,
        "purgedData": purge_data,
        "removed": removed
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_for_reads_sha256sums_lines() {
        let sums = "AB12  fw-x86_64-linux-musl\ncd34 *fw-x86_64-windows.exe\n";
        assert_eq!(digest_for(sums, "fw-x86_64-linux-musl").as_deref(), Some("ab12"));
        assert_eq!(digest_for(sums, "FW-X86_64-WINDOWS.EXE").as_deref(), Some("cd34"));
        assert_eq!(digest_for(sums, "fw-aarch64-macos"), None);
    }

    #[test]
    fn self_install_and_uninstall_roundtrip_in_temp_dir() {
        let tmp = std::env::temp_dir().join("fw_self_lifecycle_test");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let fake_src = tmp.join("source_bin");
        fs::write(&fake_src, b"fw-binary-test-payload").unwrap();

        let bin_dir = tmp.join("bin");
        let inst = install_binary_to(&fake_src, &bin_dir).unwrap();
        assert_eq!(inst["ok"], true);
        assert!(bin_dir.join(binary_filename()).is_file());

        let uninst = uninstall_from(&bin_dir, false).unwrap();
        assert_eq!(uninst["uninstalled"], true);
        assert!(!bin_dir.join(binary_filename()).exists());

        let _ = fs::remove_dir_all(&tmp);
    }
}
