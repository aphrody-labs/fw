//! File metadata and directory listing (yazi `Cha`, `Files`).

use serde::Serialize;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    File,
    Dir,
    Symlink,
    Other,
}

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub path: String,
    pub name: String,
    pub kind: Kind,
    pub size: u64,
    /// Modification time in milliseconds since the Unix epoch.
    pub mtime_ms: Option<i64>,
    pub hidden: bool,
    pub readonly: bool,
    pub executable: bool,
    /// Symlink target as stored in the link.
    pub link: Option<String>,
    /// True when a symlink points at nothing.
    pub broken: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
}

impl Entry {
    /// Directory entries and symlinks that resolve to directories.
    pub fn is_dir(&self) -> bool {
        self.kind == Kind::Dir
    }

    pub fn extension(&self) -> &str {
        match self.name.rfind('.') {
            Some(i) if i > 0 => &self.name[i + 1..],
            _ => "",
        }
    }

    /// `lstat` the path; symlinks report their own kind plus the size and directory-ness of what they point at.
    pub fn from_path(path: &Path) -> io::Result<Self> {
        let meta = std::fs::symlink_metadata(path)?;
        let is_link = meta.file_type().is_symlink();
        let (target_meta, broken) = if is_link {
            match std::fs::metadata(path) {
                Ok(m) => (m, false),
                Err(_) => (meta.clone(), true),
            }
        } else {
            (meta.clone(), false)
        };
        let kind = if is_link {
            Kind::Symlink
        } else if meta.is_dir() {
            Kind::Dir
        } else if meta.is_file() {
            Kind::File
        } else {
            Kind::Other
        };
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string_lossy().into_owned());
        Ok(Self {
            path: path.to_string_lossy().into_owned(),
            hidden: is_hidden(&name, &meta),
            name,
            kind,
            size: if target_meta.is_dir() { 0 } else { target_meta.len() },
            mtime_ms: target_meta.modified().ok().and_then(unix_ms),
            readonly: target_meta.permissions().readonly(),
            executable: is_executable(&target_meta),
            link: if is_link { std::fs::read_link(path).ok().map(|p| p.to_string_lossy().into_owned()) } else { None },
            broken,
            mime: None,
        })
    }
}

fn unix_ms(t: SystemTime) -> Option<i64> {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_millis()).ok(),
        Err(e) => i64::try_from(e.duration().as_millis()).ok().map(|v| -v),
    }
}

#[cfg(unix)]
fn is_executable(m: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    m.is_file() && m.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(m: &std::fs::Metadata) -> bool {
    let _ = m;
    false
}

#[cfg(windows)]
fn is_hidden(name: &str, m: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    name.starts_with('.') || m.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0
}

#[cfg(not(windows))]
fn is_hidden(name: &str, _m: &std::fs::Metadata) -> bool {
    name.starts_with('.')
}

/// Read a directory; entries that vanish between `readdir` and `lstat` are skipped like yazi does.
/// Hidden entries are dropped unless `hidden` is set.
pub fn list_dir(dir: &Path, hidden: bool) -> io::Result<Vec<Entry>> {
    let mut out = Vec::new();
    for item in std::fs::read_dir(dir)? {
        let item = item?;
        match Entry::from_path(&item.path()) {
            Ok(e) if hidden || !e.hidden => out.push(e),
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_files_dirs_and_hides_dotfiles() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a.txt"), b"hello").unwrap();
        std::fs::write(d.path().join(".secret"), b"x").unwrap();
        std::fs::create_dir(d.path().join("sub")).unwrap();

        let shown = list_dir(d.path(), false).unwrap();
        let mut names: Vec<_> = shown.iter().map(|e| e.name.as_str()).collect();
        names.sort();
        assert_eq!(names, ["a.txt", "sub"]);

        let all = list_dir(d.path(), true).unwrap();
        assert_eq!(all.len(), 3);
        let a = all.iter().find(|e| e.name == "a.txt").unwrap();
        assert_eq!((a.kind, a.size, a.extension()), (Kind::File, 5, "txt"));
        assert!(all.iter().find(|e| e.name == ".secret").unwrap().hidden);
        assert!(all.iter().find(|e| e.name == "sub").unwrap().is_dir());
    }

    #[test]
    fn extension_ignores_leading_dot() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join(".bashrc"), b"").unwrap();
        assert_eq!(Entry::from_path(&d.path().join(".bashrc")).unwrap().extension(), "");
    }

    #[test]
    fn missing_path_is_not_found() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(Entry::from_path(&d.path().join("nope")).unwrap_err().kind(), io::ErrorKind::NotFound);
    }
}
