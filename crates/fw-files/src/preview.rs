//! Structured previews, one per yazi previewer name (`code`, `json`, `folder`, `empty`, `file` run in
//! process; the rest report the external tool yazi's preset plugin uses for them).

use crate::entry::{Entry, list_dir};
use crate::mime;
use crate::rules;
use crate::sort::Sorter;
use serde::Serialize;
use std::io::{self, Read};
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub max_lines: usize,
    pub max_bytes: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self { max_lines: 60, max_bytes: 64 * 1024 }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Preview {
    pub path: String,
    pub mime: String,
    /// yazi previewer name chosen by the rule table.
    pub previewer: String,
    /// `text`, `json`, `folder`, `hex`, `empty` or `external`.
    pub kind: &'static str,
    pub size: u64,
    pub lines: Vec<String>,
    pub truncated: bool,
    /// For `external` previews: programs that render this type in yazi, any one of which suffices.
    pub tools: Vec<&'static str>,
}

/// Programs yazi's preset plugins spawn per previewer (yazi-plugin/preset/plugins/*.lua at v26.9.1).
pub fn external_tools(previewer: &str) -> Option<&'static [&'static str]> {
    Some(match previewer {
        "archive" => &["7zz", "7z"],
        "video" => &["ffmpeg", "ffprobe"],
        "pdf" => &["pdftoppm"],
        "svg" => &["resvg"],
        "magick" | "font" => &["magick"],
        // Decoded by yazi's own image adapter (kitty, iTerm2, sixel, ueberzug++, chafa).
        "image" => &[],
        _ => return None,
    })
}

/// What a preview is rendered from: a directory listing or the start of a file.
pub enum Source {
    Folder(Vec<Entry>),
    Bytes { head: Vec<u8>, more: bool },
}

pub fn preview(path: &Path, opts: Options) -> io::Result<Preview> {
    let entry = Entry::from_path(path)?;
    let mime = mime::detect(path)?;
    let source = if mime == mime::DIRECTORY {
        Source::Folder(list_dir(path, true)?)
    } else {
        let (head, more) = read_head(path, opts.max_bytes)?;
        Source::Bytes { head, more }
    };
    Ok(render(&entry.path, &mime, entry.size, source, opts))
}

/// Build the preview for `path` (used for the previewer rule's `url` patterns) once the content is in hand;
/// shared by local and SFTP paths.
pub fn render(path: &str, mime: &str, size: u64, source: Source, opts: Options) -> Preview {
    let url = if mime == mime::DIRECTORY && !path.ends_with('/') { format!("{path}/") } else { path.to_string() };
    let previewer = rules::previewer(mime, &url).to_string();
    let mut out = Preview { path: path.to_string(), mime: mime.to_string(), previewer: previewer.clone(), kind: "text", size, lines: Vec::new(), truncated: false, tools: Vec::new() };

    match (previewer.as_str(), source) {
        (_, Source::Folder(mut items)) => {
            out.kind = "folder";
            Sorter::default().sort(&mut items);
            out.truncated = items.len() > opts.max_lines;
            out.lines = items.iter().take(opts.max_lines).map(|e| if e.is_dir() { format!("{}/", e.name) } else { e.name.clone() }).collect();
        }
        ("empty", _) => out.kind = "empty",
        (p, _) if external_tools(p).is_some() => {
            out.kind = "external";
            out.tools = external_tools(p).unwrap_or_default().to_vec();
        }
        (_, Source::Bytes { head, more }) => {
            if mime::looks_like_text(&head) {
                let text = String::from_utf8_lossy(&head);
                if previewer == "json" {
                    out.kind = "json";
                    if !more && let Some(pretty) = pretty_json(&text) {
                        fill_lines(&mut out, &pretty, opts.max_lines, more);
                        return out;
                    }
                }
                fill_lines(&mut out, &text, opts.max_lines, more);
            } else {
                out.kind = "hex";
                out.truncated = more || head.len() > 256;
                out.lines = head.chunks(16).take(16).enumerate().map(|(i, c)| hex_line(i * 16, c)).collect();
            }
        }
    }
    out
}

fn fill_lines(out: &mut Preview, text: &str, max_lines: usize, more_bytes: bool) {
    let mut lines = text.lines();
    out.lines = lines.by_ref().take(max_lines).map(str::to_owned).collect();
    out.truncated = more_bytes || lines.next().is_some();
}

pub fn read_head(path: &Path, max: usize) -> io::Result<(Vec<u8>, bool)> {
    let mut buf = Vec::with_capacity(max.min(1 << 20) + 1);
    std::fs::File::open(path)?.take(max as u64 + 1).read_to_end(&mut buf)?;
    let more = buf.len() > max;
    buf.truncate(max);
    Ok((buf, more))
}

fn pretty_json(text: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    serde_json::to_string_pretty(&v).ok()
}

/// `offset  hex bytes  |ascii|`, the layout `xxd` and yazi's hex-ish fallback use.
pub fn hex_line(offset: usize, chunk: &[u8]) -> String {
    let hex: Vec<String> = (0..16).map(|i| chunk.get(i).map_or("  ".to_string(), |b| format!("{b:02x}"))).collect();
    let ascii: String = chunk.iter().map(|&b| if (0x20..0x7f).contains(&b) { b as char } else { '.' }).collect();
    format!("{offset:08x}  {} {}  |{ascii}|", hex[..8].join(" "), hex[8..].join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(name: &str, data: &[u8], opts: Options) -> Preview {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(name);
        std::fs::write(&p, data).unwrap();
        preview(&p, opts).unwrap()
    }

    #[test]
    fn text_is_cut_to_max_lines() {
        let body: String = (1..=100).map(|i| format!("line {i}\n")).collect();
        let p = run("notes.txt", body.as_bytes(), Options { max_lines: 5, ..Options::default() });
        assert_eq!((p.kind, p.previewer.as_str(), p.lines.len(), p.truncated), ("text", "code", 5, true));
        assert_eq!(p.lines[4], "line 5");
    }

    #[test]
    fn json_is_pretty_printed() {
        let p = run("a.json", br#"{"b":[1,2],"a":null}"#, Options::default());
        assert_eq!((p.kind, p.previewer.as_str()), ("json", "json"));
        assert!(p.lines.len() > 3 && p.lines[0] == "{");
    }

    #[test]
    fn broken_json_falls_back_to_raw_lines() {
        let p = run("a.json", b"{\"a\":", Options::default());
        assert_eq!((p.kind, p.lines), ("json", vec!["{\"a\":".to_string()]));
    }

    #[test]
    fn binary_gets_a_hex_dump() {
        let p = run("blob", &[0, 1, 2, 0xff, b'A', b'B'], Options::default());
        assert_eq!((p.kind, p.previewer.as_str()), ("hex", "file"));
        let full = hex_line(0, &[0u8; 16]);
        assert_eq!(p.lines[0].find('|'), full.find('|'), "short rows keep the ascii column aligned");
        assert!(p.lines[0].starts_with("00000000  00 01 02 ff 41 42 ") && p.lines[0].ends_with("|....AB|"));
    }

    #[test]
    fn external_types_name_their_tools() {
        let zip = run("x.zip", b"PK\x03\x04\x14\0\0\0\0\0rest-of-zip", Options::default());
        assert_eq!((zip.kind, zip.tools), ("external", vec!["7zz", "7z"]));
        let png = run("x.png", &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d], Options::default());
        assert_eq!((png.kind, png.previewer.as_str(), png.tools.len()), ("external", "image", 0));
    }

    #[test]
    fn empty_file_and_folder() {
        let e = run("zero", b"", Options::default());
        assert_eq!((e.kind, e.mime.as_str()), ("empty", "inode/empty"));

        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("b")).unwrap();
        std::fs::write(d.path().join("a10"), b"").unwrap();
        std::fs::write(d.path().join("a2"), b"").unwrap();
        let f = preview(d.path(), Options::default()).unwrap();
        assert_eq!((f.kind, f.previewer.as_str()), ("folder", "folder"));
        assert_eq!(f.lines, ["b/", "a2", "a10"]);
    }
}
