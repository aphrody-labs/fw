//! Locating and downloading the CEF binary distribution, the way cef-rs's own tooling does: the layout comes from
//! `cef-dll-sys`' build (a versioned directory under `CEF_PATH`, or the build's `OUT_DIR`), and the download goes
//! through the `download-cef` crate against the same index and archive the crate build uses.

use std::path::{Path, PathBuf};

use crate::{Error, Result};

const LIBRARY: &str = if cfg!(windows) {
    "libcef.dll"
} else if cfg!(target_os = "macos") {
    "Chromium Embedded Framework.framework"
} else {
    "libcef.so"
};

/// A directory that holds a complete CEF runtime (library, resources and locales).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CefDir(PathBuf);

impl CefDir {
    pub fn path(&self) -> &Path {
        &self.0
    }

    fn accept(dir: &Path) -> Option<Self> {
        (dir.join(LIBRARY).exists() && dir.join("locales").is_dir())
            .then(|| Self(dir.to_path_buf()))
    }

    /// First match of: the explicit directory, `AY_CHROMIUM_CEF_DIR`, `CEF_PATH` (also its per-version and
    /// per-target subdirectories), the directories next to `anchors` (the helper or the executable), and the
    /// directory this crate was built against.
    pub fn locate(explicit: Option<&Path>, anchors: &[PathBuf]) -> Result<Self> {
        let version = crate::cef_version();
        let short = version.split('+').next().unwrap_or(&version).to_owned();
        let target = download_cef::OsAndArch::try_from(download_cef::DEFAULT_TARGET)
            .ok()
            .map(|t| t.to_string());

        let mut candidates: Vec<PathBuf> = Vec::new();
        candidates.extend(explicit.map(Path::to_path_buf));
        for var in ["AY_CHROMIUM_CEF_DIR", "CEF_PATH"] {
            if let Some(base) = std::env::var_os(var).map(PathBuf::from) {
                candidates.push(base.clone());
                if let Some(target) = &target {
                    candidates.push(base.join(&short).join(target));
                    candidates.push(base.join(target));
                }
            }
        }
        for anchor in anchors {
            candidates.extend(anchor.parent().map(Path::to_path_buf));
        }
        if let Some(built) = option_env!("AY_CHROMIUM_BUILD_CEF_DIR") {
            candidates.push(PathBuf::from(built));
        }

        candidates
            .iter()
            .find_map(|dir| Self::accept(dir))
            .ok_or_else(|| {
                Error::CefNotFound(format!(
                    "looked in {candidates:?}; run `ay-chromium install <dir>`"
                ))
            })
    }
}

/// Download and extract the CEF distribution matching this crate into `<base>/<version>/<target>` and return it.
/// A distribution that is already there is reused.
pub fn install_cef(base: &Path) -> Result<CefDir> {
    let err = |e: download_cef::Error| Error::CefNotFound(e.to_string());
    let target = download_cef::DEFAULT_TARGET;
    let os_arch = download_cef::OsAndArch::try_from(target).map_err(err)?;
    let version = crate::cef_version();
    let short = version.split('+').next().unwrap_or(&version).to_owned();
    let location = base.join(&short);
    let dir = location.join(os_arch.to_string());
    if let Some(found) = CefDir::accept(&dir) {
        return Ok(found);
    }

    let url = download_cef::default_download_url();
    let index = download_cef::CefIndex::download_from(&url).map_err(err)?;
    let release = index
        .platform(target)
        .map_err(err)?
        .version(&short)
        .map_err(err)?;
    let archive = release
        .download_archive_with_retry_from(
            &url,
            &location,
            true,
            std::time::Duration::from_secs(2),
            3,
        )
        .map_err(err)?;
    let extracted =
        download_cef::extract_target_archive(target, &archive, &location, true).map_err(err)?;
    release.write_archive_json(&extracted).map_err(err)?;
    CefDir::accept(&dir).ok_or_else(|| {
        Error::CefNotFound(format!("{} is incomplete after extraction", dir.display()))
    })
}
