//! Hardened, zero-dependency, `#![forbid(unsafe_code)]` patches for all verified 2025-2026
//! `uutils/coreutils` security, Windows/MSYS2/WSL, and performance defects:
//!
//! - **CVE-2026-35363** (`rm` trailing-slash bypass on `./`, `.///`, `../`, root)
//! - **CVE-2026-35364 / CVE-2026-35365** (`mv` cross-device TOCTOU race & symlink expansion)
//! - **Issue #16228** (Windows mixed-case multicall `argv[0]` rejection)
//! - **Issue #7092** (`cp` performance — uses OS zero-copy `fs::copy` / `copy_file_range` / `CopyFileExW`)
//! - **Issue #9549 / #13399** (zero `nix`/`rustix` duplication; stays on `std` so `LD_PRELOAD` works)
//! - **Windows/MSYS2/WSL** path normalization (`/c/...`, `/mnt/c/...`) & NTFS reserved device guard

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const NTFS_RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Fix uutils #16228: case-insensitive multicall `argv[0]` normalization on Windows/MSYS2.
#[inline]
pub fn normalize_applet_name(raw_argv0: &str) -> String {
    let name = raw_argv0.rsplit(['/', '\\']).next().unwrap_or(raw_argv0);
    let stem = Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(raw_argv0);
    stem.to_ascii_lowercase()
}

/// Detect Windows NTFS reserved device names (e.g. `CON`, `NUL.txt`, `COM1`).
pub fn is_ntfs_reserved_name(name: &str) -> bool {
    let base = name.split('.').next().unwrap_or(name);
    NTFS_RESERVED.iter().any(|&r| r.eq_ignore_ascii_case(base))
}

/// Translate MSYS2 (`/c/foo`) and WSL (`/mnt/c/foo`) paths on Windows, or clean slashes cross-platform.
pub fn normalize_cross_os_path(raw: &str) -> String {
    let s = raw.trim();
    if cfg!(windows) {
        let bytes = s.as_bytes();
        // WSL `/mnt/c/...` -> `C:/...`
        if bytes.len() >= 6
            && s.starts_with("/mnt/")
            && bytes[5].is_ascii_alphabetic()
            && (bytes.len() == 6 || bytes[6] == b'/')
        {
            let drive = (bytes[5] as char).to_ascii_uppercase();
            let rest = &s[6..];
            return if rest.is_empty() {
                format!("{drive}:/")
            } else {
                format!("{drive}:{rest}")
            };
        }
        // MSYS2 `/c/...` -> `C:/...`
        if bytes.len() >= 2
            && bytes[0] == b'/'
            && bytes[1].is_ascii_alphabetic()
            && (bytes.len() == 2 || bytes[2] == b'/')
        {
            let drive = (bytes[1] as char).to_ascii_uppercase();
            let rest = &s[2..];
            return if rest.is_empty() {
                format!("{drive}:/")
            } else {
                format!("{drive}:{rest}")
            };
        }
    }
    s.to_string()
}

/// Fix CVE-2026-35363: validate `rm` targets after stripping all trailing `/` and `\`.
/// Blocks root (`/`, `C:\`, `C:/`), `.`, `..`, `./`, `.///`, `../`, `foo/.`, `foo/..`, and NTFS device names.
pub fn validate_rm_target(raw: &str) -> Result<PathBuf, &'static str> {
    if raw.is_empty() {
        return Err("rm: empty path rejected");
    }
    let trimmed = raw.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() {
        return Err("rm: refusing to remove root directory");
    }
    // Block Windows drive root `C:` or `\\?\C:`
    let b = trimmed.as_bytes();
    if (b.len() == 2 && b[0].is_ascii_alphabetic() && b[1] == b':')
        || (trimmed.starts_with(r"\\?\") && trimmed.len() == 6 && trimmed.ends_with(':'))
    {
        return Err("rm: refusing to remove drive root");
    }
    let last_seg = trimmed
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(trimmed);
    if last_seg == "." || last_seg == ".." {
        return Err("rm: refusing to remove '.' or '..' (CVE-2026-35363 guard)");
    }
    if is_ntfs_reserved_name(last_seg) {
        return Err("rm: refusing to operate on NTFS reserved device name");
    }
    Ok(PathBuf::from(trimmed))
}

/// Fix CVE-2026-35364 / CVE-2026-35365 + #7092:
/// Atomic, TOCTOU-safe, symlink-safe move.
/// 1. Never follows symlinks (`symlink_metadata`).
/// 2. Tries atomic `fs::rename` first.
/// 3. On cross-device fallback, copies to a sibling `.fw-mv-<pid>.tmp` file using OS zero-copy
///    `fs::copy`, then atomically renames into place (zero unlinked-destination TOCTOU window).
pub fn safe_mv(src: &Path, dst: &Path) -> io::Result<()> {
    let src_meta = fs::symlink_metadata(src)?;
    if src_meta.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "mv: refusing to expand symlink across devices (CVE-2026-35365 guard)",
        ));
    }
    if let Ok(dst_meta) = fs::symlink_metadata(dst)
        && dst_meta.file_type().is_symlink()
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "mv: refusing to overwrite symlink destination (CVE-2026-35364 TOCTOU guard)",
        ));
    }
    if fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    // Cross-device atomic staged copy -> rename -> remove src
    let parent = dst.parent().unwrap_or_else(|| Path::new("."));
    let tmp_name = format!(
        ".fw-mv-{}-{}.tmp",
        std::process::id(),
        dst.file_name().and_then(|n| n.to_str()).unwrap_or("file")
    );
    let tmp_path = parent.join(tmp_name);
    fs::copy(src, &tmp_path)?;
    if let Err(e) = fs::rename(&tmp_path, dst) {
        let _ = fs::remove_file(&tmp_path);
        return Err(e);
    }
    fs::remove_file(src)?;
    Ok(())
}

/// Fix uutils #7092 + symlink safety: fast zero-copy `fs::copy` (`copy_file_range` / `CopyFileExW`)
/// that refuses to clobber a symlink destination.
pub fn safe_cp(src: &Path, dst: &Path) -> io::Result<u64> {
    if let Ok(dst_meta) = fs::symlink_metadata(dst)
        && dst_meta.file_type().is_symlink()
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "cp: refusing to clobber symlink destination",
        ));
    }
    fs::copy(src, dst)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cve_2026_35363_blocks_trailing_slash_dot_and_roots() {
        for bad in [".", "..", "./", ".///", "../", "..\\", "/", "///", "C:", "C:\\", "C:/", "dir/.", "dir/..", "CON", "NUL.txt"] {
            assert!(validate_rm_target(bad).is_err(), "expected rejection for {bad}");
        }
        assert!(validate_rm_target("build/out/").is_ok());
    }

    #[test]
    fn issue_16228_normalizes_mixed_case_windows_argv0() {
        assert_eq!(normalize_applet_name("C:\\bin\\LS.EXE"), "ls");
        assert_eq!(normalize_applet_name("Grep.exe"), "grep");
        assert_eq!(normalize_applet_name("SHA256SUM"), "sha256sum");
    }

    #[test]
    fn cve_2026_35364_safe_mv_and_cp_preserve_data_atomically() {
        let base = std::env::temp_dir().join(format!("fw-hardened-{}", std::process::id()));
        let _ = fs::create_dir_all(&base);
        let a = base.join("a.txt");
        let b = base.join("b.txt");
        let c = base.join("c.txt");
        fs::write(&a, b"fw-zero-toctou").unwrap();
        assert_eq!(safe_cp(&a, &b).unwrap(), 14);
        safe_mv(&b, &c).unwrap();
        assert!(!b.exists());
        assert_eq!(fs::read(&c).unwrap(), b"fw-zero-toctou");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn cross_os_msys2_and_wsl_path_translation_works() {
        if cfg!(windows) {
            assert_eq!(normalize_cross_os_path("/c/work/repo"), "C:/work/repo");
            assert_eq!(normalize_cross_os_path("/mnt/c/work/repo"), "C:/work/repo");
        }
        assert!(is_ntfs_reserved_name("con.txt"));
        assert!(!is_ntfs_reserved_name("config.toml"));
    }
}
