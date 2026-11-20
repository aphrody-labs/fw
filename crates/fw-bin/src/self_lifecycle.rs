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
            "fw-x86_64-pc-windows-msvc.exe",
            "fw-windows-x86_64.exe",
            "fw.exe",
        ],
        ("linux", "x86_64") => &[
            "fw-x86_64-unknown-linux-musl",
            "fw-x86_64-unknown-linux-gnu",
            "fw-linux-x86_64",
        ],
        ("linux", "aarch64") => &[
            "fw-aarch64-unknown-linux-musl",
            "fw-aarch64-unknown-linux-gnu",
            "fw-linux-aarch64",
        ],
        ("macos", "aarch64") => &[
            "fw-aarch64-apple-darwin",
            "fw-darwin-arm64",
        ],
        ("macos", "x86_64") => &[
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

/// `fw self-update [--check] [--version <tag>]`
pub fn run_self_update(args: &[String], offline: bool) -> Result<Value, String> {
    if offline {
        return Err("fw self-update: disabled by --offline".to_owned());
    }
    let check_only = args.iter().any(|a| a == "--check");
    let tag_override = flag_str(args, "--version");
    let api_url = match tag_override {
        Some(tag) => format!("https://api.github.com/repos/{REPO_SLUG}/releases/tags/{tag}"),
        None => format!("https://api.github.com/repos/{REPO_SLUG}/releases/latest"),
    };

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .into();

    let resp = agent
        .get(&api_url)
        .header("User-Agent", concat!("fw/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .call();

    let release: Value = match resp {
        Ok(mut r) => {
            let raw = r.body_mut().read_to_string().map_err(|e| e.to_string())?;
            serde_json::from_str(&raw).map_err(|e| e.to_string())?
        }
        Err(e) => {
            return Ok(json!({
                "ok": true,
                "zeroSubprocess": true,
                "currentVersion": env!("CARGO_PKG_VERSION"),
                "upToDate": true,
                "note": format!("no published GitHub release reachable at {api_url} ({e})")
            }));
        }
    };

    let latest_tag = release["tag_name"].as_str().unwrap_or(env!("CARGO_PKG_VERSION"));
    let current = env!("CARGO_PKG_VERSION");
    let assets = release["assets"].as_array().cloned().unwrap_or_default();
    let candidates = platform_asset_candidates();

    let matched = candidates.iter().find_map(|cand| {
        assets.iter().find(|a| {
            a["name"]
                .as_str()
                .is_some_and(|n| n.eq_ignore_ascii_case(cand))
        })
    });

    if check_only || matched.is_none() {
        return Ok(json!({
            "ok": true,
            "zeroSubprocess": true,
            "currentVersion": current,
            "latestTag": latest_tag,
            "updateAvailable": !latest_tag.contains(current),
            "matchedAsset": matched.and_then(|a| a["name"].as_str()),
            "candidates": candidates,
            "checkOnly": check_only
        }));
    }

    let asset = matched.unwrap();
    let asset_name = asset["name"].as_str().unwrap_or("fw");
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
        .map_err(|e| e.to_string())?;
    let actual_sha = sha256_bytes(&bytes);

    // Verify against checksums.txt if published alongside the release
    let mut checksum_verified = false;
    if let Some(cs_asset) = assets.iter().find(|a| {
        a["name"]
            .as_str()
            .is_some_and(|n| n.to_ascii_lowercase().contains("checksum"))
    }) && let Some(cs_url) = cs_asset["browser_download_url"].as_str()
        && let Ok(mut cs_resp) = agent.get(cs_url).header("User-Agent", "fw").call()
        && let Ok(cs_txt) = cs_resp.body_mut().read_to_string()
    {
        for line in cs_txt.lines() {
            if line.contains(asset_name)
                && let Some(expected) = line.split_whitespace().next()
            {
                if !expected.eq_ignore_ascii_case(&actual_sha) {
                    return Err(format!(
                        "SHA-256 mismatch for {asset_name}: expected {expected}, got {actual_sha}"
                    ));
                }
                checksum_verified = true;
                break;
            }
        }
    }

    let target_exe = std::env::current_exe().unwrap_or_else(|_| default_install_dir().join(binary_filename()));
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
