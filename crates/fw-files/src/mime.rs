//! MIME detection. yazi shells out to `file --mime-type`; this does it in-process: magic bytes
//! (`infer`), then the extension (`mime_guess`), then a text sniff, and normalises the result the
//! way yazi's `mime-local` plugin does (`x-` and `vnd.` prefixes dropped from the subtype) so that
//! the rule table in [`crate::rules`] matches unchanged.

use std::io::{self, Read};
use std::path::Path;

pub const DIRECTORY: &str = "inode/directory";
pub const EMPTY: &str = "inode/empty";
pub const TEXT: &str = "text/plain";
pub const BINARY: &str = "application/octet-stream";

/// Bytes read from the start of a file for content sniffing.
pub const SNIFF_LEN: usize = 8192;

pub fn detect(path: &Path) -> io::Result<String> {
    let meta = std::fs::metadata(path)?; // follows symlinks like `file -L`
    if meta.is_dir() {
        return Ok(DIRECTORY.into());
    }
    let mut head = Vec::with_capacity(SNIFF_LEN);
    std::fs::File::open(path)?.take(SNIFF_LEN as u64).read_to_end(&mut head)?;
    let name = path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
    Ok(detect_bytes(&name, &head))
}

/// `name` supplies the extension; `head` is the start of the content.
pub fn detect_bytes(name: &str, head: &[u8]) -> String {
    if head.is_empty() {
        return EMPTY.into();
    }
    if let Some(t) = infer::get(head) {
        return normalize(t.mime_type());
    }
    let looks_text = looks_like_text(head);
    if let Some(guess) = mime_guess::from_path(name).first_raw() {
        // `.ts` is `video/mp2t` for mime_guess but TypeScript for every text file, so an extension that
        // names a binary type only wins when the content is not plain text.
        if is_text_type(guess) || !looks_text {
            return normalize(guess);
        }
    }
    if looks_text { TEXT.into() } else { BINARY.into() }
}

/// Drop `x-` and `vnd.` from the subtype and lowercase it: `application/x-7z-compressed` becomes
/// `application/7z-compressed`, `application/vnd.rar` becomes `application/rar`.
pub fn normalize(mime: &str) -> String {
    let mime = mime.split(';').next().unwrap_or(mime).trim().to_ascii_lowercase();
    let Some((ty, sub)) = mime.split_once('/') else { return mime };
    let sub = sub.strip_prefix("x-").unwrap_or(sub);
    let sub = sub.strip_prefix("vnd.").unwrap_or(sub);
    format!("{ty}/{sub}")
}

fn is_text_type(mime: &str) -> bool {
    let m = mime.to_ascii_lowercase();
    m.starts_with("text/") || m.ends_with("+xml") || m.ends_with("+json") || matches!(m.as_str(), "application/json" | "application/javascript" | "application/xml" | "application/toml" | "application/yaml" | "application/x-yaml" | "application/sql" | "application/x-sh" | "application/x-ndjson")
}

/// Valid UTF-8 (allowing a multi-byte character cut at the end of the window) without NUL or
/// other control characters besides tab, newline, carriage return, form feed and escape.
pub fn looks_like_text(head: &[u8]) -> bool {
    let s = match std::str::from_utf8(head) {
        Ok(s) => s,
        Err(e) if e.error_len().is_none() => std::str::from_utf8(&head[..e.valid_up_to()]).unwrap_or(""),
        Err(_) => return false,
    };
    !s.chars().any(|c| c.is_control() && !matches!(c, '\t' | '\n' | '\r' | '\x0c' | '\x1b'))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, b'I', b'H', b'D', b'R'];

    #[test]
    fn magic_bytes_beat_a_lying_extension() {
        assert_eq!(detect_bytes("photo.txt", PNG), "image/png");
    }

    #[test]
    fn zip_archive_matches_the_archive_rule_shape() {
        assert_eq!(detect_bytes("x.bin", b"PK\x03\x04\x14\0\0\0\0\0rest-of-zip"), "application/zip");
    }

    #[test]
    fn extension_decides_for_text_formats() {
        assert_eq!(detect_bytes("data.json", b"{\"a\":1}"), "application/json");
        assert_eq!(detect_bytes("pic.svg", b"<svg xmlns='http://www.w3.org/2000/svg'/>"), "image/svg+xml");
        assert!(detect_bytes("main.rs", b"fn main() {}\n").starts_with("text/"));
    }

    #[test]
    fn typescript_is_text_not_mpeg_ts() {
        assert_eq!(detect_bytes("app.ts", b"export const x: number = 1;\n"), TEXT);
        // A real transport stream (0x47 sync byte, binary payload) keeps the extension's type.
        assert!(detect_bytes("clip.ts", &[0x47, 0x40, 0x00, 0x10, 0, 0, 0xff, 0xfe, 0x01, 0x02]).starts_with("video/"));
    }

    #[test]
    fn empty_text_and_binary_fallbacks() {
        assert_eq!(detect_bytes("anything", b""), EMPTY);
        assert_eq!(detect_bytes("NOTES", "plain words\n".as_bytes()), TEXT);
        assert_eq!(detect_bytes("blob", &[0, 1, 2, 3, 0xff, 0xfe]), BINARY);
        assert!(looks_like_text("héllo".as_bytes()));
        assert!(looks_like_text(&"é".as_bytes()[..1]));
        assert!(!looks_like_text(b"a\0b"));
    }

    #[test]
    fn normalize_strips_vendor_prefixes() {
        assert_eq!(normalize("application/x-7z-compressed"), "application/7z-compressed");
        assert_eq!(normalize("application/vnd.rar"), "application/rar");
        assert_eq!(normalize("Text/Plain; charset=utf-8"), "text/plain");
        assert_eq!(normalize("image/svg+xml"), "image/svg+xml");
    }

    #[test]
    fn detects_files_and_directories_on_disk() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a.png"), PNG).unwrap();
        std::fs::write(d.path().join("empty"), b"").unwrap();
        assert_eq!(detect(&d.path().join("a.png")).unwrap(), "image/png");
        assert_eq!(detect(&d.path().join("empty")).unwrap(), EMPTY);
        assert_eq!(detect(d.path()).unwrap(), DIRECTORY);
        assert!(detect(&d.path().join("missing")).is_err());
    }
}
