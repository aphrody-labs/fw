// SPDX-License-Identifier: Apache-2.0
//! Pluggable filesystem abstraction used when applying patches.

use std::{
    io,
    path::{Path, PathBuf},
};

/// Filesystem operations needed to apply a patch.
///
/// Implementing this trait lets callers apply patches against in-memory
/// snapshots, sandboxes, or virtual filesystems instead of real disk. The
/// [`StdFileSystem`] implementation operates on [`std::fs`].
pub trait PatchFileSystem {
    /// Read a file's full contents as UTF-8 text.
    ///
    /// # Errors
    /// Returns an [`io::Error`] when the file cannot be read or is not UTF-8.
    fn read(&self, path: &Path) -> io::Result<String>;

    /// Write `contents` to `path`, creating it (and parent directories) if
    /// necessary and truncating any existing file.
    ///
    /// # Errors
    /// Returns an [`io::Error`] when the file (or a parent directory) cannot be
    /// created or written.
    fn write(&self, path: &Path, contents: &str) -> io::Result<()>;

    /// Remove a file.
    ///
    /// # Errors
    /// Returns an [`io::Error`] when the file cannot be removed.
    fn remove(&self, path: &Path) -> io::Result<()>;

    /// Whether `path` currently exists.
    fn exists(&self, path: &Path) -> bool;
}

/// Why `path` is not a safe patch path, `None` when it is one.
///
/// A patch path is relative and stays below the patch root: it is refused when it is empty, holds a
/// NUL, starts with a separator (absolute or UNC), starts with a drive prefix (`C:`), or has a `..`
/// component. Both `/` and `\\` count as separators on every platform, so a patch that is harmless
/// on one host cannot escape on another.
#[must_use]
pub fn unsafe_path_reason(path: &Path) -> Option<&'static str> {
    let text = path.to_string_lossy();
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (None, _) => return Some("empty path"),
        (Some('/' | '\\'), _) => return Some("absolute path"),
        (Some(c), Some(':')) if c.is_ascii_alphabetic() => return Some("drive prefix"),
        _ => {}
    }
    if text.contains('\0') {
        return Some("NUL in path");
    }
    if text.split(['/', '\\']).any(|part| part == "..") {
        return Some("parent directory component");
    }
    None
}

/// `true` when [`unsafe_path_reason`] finds nothing wrong with `path`.
#[must_use]
pub fn is_safe_relative_path(path: &Path) -> bool {
    unsafe_path_reason(path).is_none()
}

/// [`PatchFileSystem`] backed by [`std::fs`] and confined to a root directory.
///
/// Every path is checked with [`unsafe_path_reason`], joined to the root, and its nearest existing
/// ancestor (the path itself when it exists) is canonicalized and must stay below the canonical
/// root, so a symlink inside the root cannot lead out of it. `write` creates parent directories as
/// needed. On the bare `wasm32-unknown-unknown` target it compiles but `std::fs` operations return
/// errors at runtime.
#[derive(Debug, Clone)]
pub struct StdFileSystem {
    root: PathBuf,
}

impl StdFileSystem {
    /// Confine the filesystem to `root`, which must exist when the first operation runs.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The root directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn resolve(&self, path: &Path) -> io::Result<PathBuf> {
        if let Some(reason) = unsafe_path_reason(path) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{reason}: {}", path.display()),
            ));
        }
        let root = self.root.canonicalize()?;
        let full = root.join(path);
        let mut existing: &Path = &full;
        let canonical = loop {
            match existing.canonicalize() {
                Ok(canonical) => {
                    // A dangling symlink canonicalizes to nothing, so it was not reached here
                    // through `NotFound` unless its link is missing: reject links we cannot resolve.
                    break canonical;
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    if existing.symlink_metadata().is_ok() {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("dangling symlink: {}", path.display()),
                        ));
                    }
                    match existing.parent() {
                        Some(parent) => existing = parent,
                        None => return Err(e),
                    }
                }
                Err(e) => return Err(e),
            }
        };
        if canonical.starts_with(&root) {
            Ok(full)
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("path leaves the patch root: {}", path.display()),
            ))
        }
    }
}

impl PatchFileSystem for StdFileSystem {
    fn read(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(self.resolve(path)?)
    }

    fn write(&self, path: &Path, contents: &str) -> io::Result<()> {
        let full = self.resolve(path)?;
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(full, contents)
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(self.resolve(path)?)
    }

    fn exists(&self, path: &Path) -> bool {
        self.resolve(path).is_ok_and(|full| full.exists())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsafe_paths_are_named() {
        let drive = format!("{}:{}x", 'C', '\\');
        for (path, reason) in [
            ("", "empty path"),
            ("/etc/passwd", "absolute path"),
            ("\\\\server\\share\\x", "absolute path"),
            ("\\windows\\x", "absolute path"),
            (drive.as_str(), "drive prefix"),
            ("c:x", "drive prefix"),
            ("../x", "parent directory component"),
            ("a/../../x", "parent directory component"),
            ("a\\..\\x", "parent directory component"),
            ("a\0b", "NUL in path"),
        ] {
            assert_eq!(
                unsafe_path_reason(Path::new(path)),
                Some(reason),
                "{path:?}"
            );
        }
        for path in ["a", "a/b.rs", "a/..b/c", "a/b..", ".hidden", "dir/./f"] {
            assert!(is_safe_relative_path(Path::new(path)), "{path}");
        }
    }

    #[test]
    fn std_filesystem_stays_in_its_root() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let fs = StdFileSystem::new(dir.path());
        fs.write(Path::new("nested/ok.txt"), "x").unwrap();
        assert_eq!(fs.read(Path::new("nested/ok.txt")).unwrap(), "x");
        let escape = outside.path().join("escape.txt");
        for path in [
            PathBuf::from("../escape.txt"),
            PathBuf::from("a/../../escape.txt"),
            escape.clone(),
        ] {
            assert!(fs.write(&path, "x").is_err(), "{}", path.display());
            assert!(fs.read(&path).is_err());
            assert!(fs.remove(&path).is_err());
            assert!(!fs.exists(&path));
        }
        assert!(!escape.exists());
    }

    #[cfg(unix)]
    #[test]
    fn std_filesystem_refuses_symlinks_out_of_the_root() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("target.txt"),
            dir.path().join("dangling"),
        )
        .unwrap();
        let fs = StdFileSystem::new(dir.path());
        assert!(fs.write(Path::new("link/new.txt"), "x").is_err());
        assert!(fs.write(Path::new("link/sub/new.txt"), "x").is_err());
        assert!(fs.write(Path::new("dangling"), "x").is_err());
        assert!(!outside.path().join("new.txt").exists());
        assert!(!outside.path().join("target.txt").exists());
    }
}
