#![forbid(unsafe_code)]

//! Extended GNU/Linux Coreutils + Best Modern Rust CLIs (`jaq`/`jq`, `dust`/`du`, `tokei`, `bat`,
//! `sort`, `uniq`, `cut`, `tr`, `nl`, `rev`, `seq`, `basename`, `dirname`, `realpath`, `stat`,
//! `mkdir`, `touch`, `cp`, `mv`, `rm`, `base64`, `which`, and zero-fork in-process POSIX pipe `sh`).
//! Compiles cleanly on native x86_64/arm64 AND `wasm32-unknown-unknown`.

use crate::{run_applet, AppletOutput};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const EXTENDED_APPLETS: &[&str] = &[
    "sort", "uniq", "cut", "tr", "nl", "rev", "seq", "basename", "dirname",
    "realpath", "stat", "mkdir", "touch", "cp", "mv", "rm", "base64", "which",
    "du", "tokei", "jq", "bat", "sh",
];

fn ok(applet: &str, stdout: String) -> AppletOutput {
    AppletOutput {
        applet: applet.to_owned(),
        exit_code: 0,
        stdout,
        stderr: String::new(),
    }
}

fn err(applet: &str, msg: String) -> AppletOutput {
    AppletOutput {
        applet: applet.to_owned(),
        exit_code: 1,
        stdout: String::new(),
        stderr: format!("{applet}: {msg}\n"),
    }
}

fn read_input(args: &[&str], stdin: &str) -> Result<String, String> {
    let file_arg = args.iter().rev().find(|a| !a.starts_with('-') && !a.is_empty());
    if let Some(p) = file_arg
        && Path::new(p).is_file()
    {
        return fs::read_to_string(p).map_err(|e| format!("{p}: {e}"));
    }
    Ok(stdin.to_owned())
}

pub fn run_extended_applet(applet: &str, args: &[&str], stdin: &str) -> Option<AppletOutput> {
    Some(match applet {
        "sh" => run_inproc_pipe_shell(args, stdin),
        "sort" => {
            let input = match read_input(args, stdin) {
                Ok(s) => s,
                Err(e) => return Some(err(applet, e)),
            };
            let reverse = args.contains(&"-r");
            let unique = args.contains(&"-u");
            let numeric = args.contains(&"-n");
            let mut lines: Vec<&str> = input.lines().collect();
            if numeric {
                lines.sort_by(|a, b| {
                    let na = a.trim().parse::<f64>().unwrap_or(0.0);
                    let nb = b.trim().parse::<f64>().unwrap_or(0.0);
                    na.total_cmp(&nb)
                });
            } else {
                lines.sort();
            }
            if reverse {
                lines.reverse();
            }
            if unique {
                lines.dedup();
            }
            let out = if lines.is_empty() {
                String::new()
            } else {
                format!("{}\n", lines.join("\n"))
            };
            ok(applet, out)
        }
        "uniq" => {
            let input = match read_input(args, stdin) {
                Ok(s) => s,
                Err(e) => return Some(err(applet, e)),
            };
            let count = args.contains(&"-c");
            let mut out = String::new();
            let mut prev: Option<&str> = None;
            let mut n = 0usize;
            for line in input.lines() {
                if prev == Some(line) {
                    n += 1;
                } else {
                    if let Some(p) = prev {
                        if count {
                            out.push_str(&format!("{n:>7} {p}\n"));
                        } else {
                            out.push_str(&format!("{p}\n"));
                        }
                    }
                    prev = Some(line);
                    n = 1;
                }
            }
            if let Some(p) = prev {
                if count {
                    out.push_str(&format!("{n:>7} {p}\n"));
                } else {
                    out.push_str(&format!("{p}\n"));
                }
            }
            ok(applet, out)
        }
        "cut" => {
            let delim = args
                .windows(2)
                .find(|w| w[0] == "-d")
                .map(|w| w[1])
                .unwrap_or("\t");
            let field: usize = args
                .windows(2)
                .find(|w| w[0] == "-f")
                .and_then(|w| w[1].parse().ok())
                .unwrap_or(1)
                .max(1);
            let input = match read_input(args, stdin) {
                Ok(s) => s,
                Err(e) => return Some(err(applet, e)),
            };
            let mut out = String::new();
            for line in input.lines() {
                let col = line.split(delim).nth(field - 1).unwrap_or("");
                out.push_str(col);
                out.push('\n');
            }
            ok(applet, out)
        }
        "tr" => {
            let from = args.first().copied().unwrap_or("");
            let to = args.get(1).copied().unwrap_or("");
            let from_chars: Vec<char> = from.chars().collect();
            let to_chars: Vec<char> = to.chars().collect();
            let out: String = stdin
                .chars()
                .map(|c| {
                    if let Some(pos) = from_chars.iter().position(|&fc| fc == c) {
                        to_chars.get(pos).or_else(|| to_chars.last()).copied().unwrap_or(c)
                    } else {
                        c
                    }
                })
                .collect();
            ok(applet, out)
        }
        "nl" => {
            let input = match read_input(args, stdin) {
                Ok(s) => s,
                Err(e) => return Some(err(applet, e)),
            };
            let mut out = String::new();
            for (i, line) in input.lines().enumerate() {
                out.push_str(&format!("{:>6}\t{line}\n", i + 1));
            }
            ok(applet, out)
        }
        "rev" => {
            let input = match read_input(args, stdin) {
                Ok(s) => s,
                Err(e) => return Some(err(applet, e)),
            };
            let mut out = String::new();
            for line in input.lines() {
                let r: String = line.chars().rev().collect();
                out.push_str(&r);
                out.push('\n');
            }
            ok(applet, out)
        }
        "seq" => {
            let nums: Vec<i64> = args.iter().filter_map(|a| a.parse::<i64>().ok()).collect();
            let (start, step, end) = match nums.as_slice() {
                [e] => (1, 1, *e),
                [s, e] => (*s, 1, *e),
                [s, st, e] => (*s, (*st).max(1), *e),
                _ => return Some(err(applet, "usage: seq [FIRST [INC]] LAST".to_owned())),
            };
            let mut out = String::new();
            let mut cur = start;
            let mut guard = 0usize;
            while cur <= end && guard < 100_000 {
                out.push_str(&format!("{cur}\n"));
                cur += step;
                guard += 1;
            }
            ok(applet, out)
        }
        "basename" => {
            let p = args.first().copied().unwrap_or("");
            let name = Path::new(p).file_name().and_then(|s| s.to_str()).unwrap_or(p);
            let stripped = args
                .get(1)
                .and_then(|suf| name.strip_suffix(suf))
                .unwrap_or(name);
            ok(applet, format!("{stripped}\n"))
        }
        "dirname" => {
            let p = args.first().copied().unwrap_or(".");
            let parent = Path::new(p)
                .parent()
                .and_then(|s| s.to_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(".");
            ok(applet, format!("{parent}\n"))
        }
        "realpath" => {
            let p = args.first().copied().unwrap_or(".");
            match fs::canonicalize(p) {
                Ok(c) => ok(applet, format!("{}\n", c.display())),
                Err(e) => err(applet, format!("{p}: {e}")),
            }
        }
        "stat" => {
            let p = args.first().copied().unwrap_or(".");
            match fs::metadata(p) {
                Ok(m) => ok(
                    applet,
                    format!(
                        "file={p} type={} bytes={} readonly={}\n",
                        if m.is_dir() { "dir" } else { "file" },
                        m.len(),
                        m.permissions().readonly()
                    ),
                ),
                Err(e) => err(applet, format!("{p}: {e}")),
            }
        }
        "mkdir" => {
            let parents = args.contains(&"-p");
            let targets: Vec<&str> = args.iter().copied().filter(|a| !a.starts_with('-')).collect();
            for t in targets {
                let res = if parents { fs::create_dir_all(t) } else { fs::create_dir(t) };
                if let Err(e) = res {
                    return Some(err(applet, format!("{t}: {e}")));
                }
            }
            ok(applet, String::new())
        }
        "touch" => {
            for t in args.iter().copied().filter(|a| !a.starts_with('-')) {
                if !Path::new(t).exists()
                    && let Err(e) = fs::write(t, b"")
                {
                    return Some(err(applet, format!("{t}: {e}")));
                }
            }
            ok(applet, String::new())
        }
        "cp" => {
            let positional: Vec<&str> = args.iter().copied().filter(|a| !a.starts_with('-')).collect();
            if positional.len() < 2 {
                return Some(err(applet, "usage: cp <src> <dst>".to_owned()));
            }
            match fs::copy(positional[0], positional[1]) {
                Ok(_) => ok(applet, String::new()),
                Err(e) => err(applet, e.to_string()),
            }
        }
        "mv" => {
            let positional: Vec<&str> = args.iter().copied().filter(|a| !a.starts_with('-')).collect();
            if positional.len() < 2 {
                return Some(err(applet, "usage: mv <src> <dst>".to_owned()));
            }
            match fs::rename(positional[0], positional[1]) {
                Ok(()) => ok(applet, String::new()),
                Err(e) => err(applet, e.to_string()),
            }
        }
        "rm" => {
            let rec = args.iter().any(|a| *a == "-r" || *a == "-rf" || *a == "-fr");
            let force = args.iter().any(|a| *a == "-f" || *a == "-rf" || *a == "-fr");
            for t in args.iter().copied().filter(|a| !a.starts_with('-')) {
                let p = Path::new(t);
                if !p.exists() {
                    if force {
                        continue;
                    }
                    return Some(err(applet, format!("{t}: no such file")));
                }
                let r = if p.is_dir() && rec {
                    fs::remove_dir_all(p)
                } else {
                    fs::remove_file(p)
                };
                if let Err(e) = r {
                    return Some(err(applet, format!("{t}: {e}")));
                }
            }
            ok(applet, String::new())
        }
        "base64" => {
            let decode = args.contains(&"-d") || args.contains(&"--decode");
            let input = match read_input(args, stdin) {
                Ok(s) => s,
                Err(e) => return Some(err(applet, e)),
            };
            if decode {
                match b64_decode(input.trim()) {
                    Ok(bytes) => ok(applet, String::from_utf8_lossy(&bytes).into_owned()),
                    Err(e) => err(applet, e),
                }
            } else {
                ok(applet, format!("{}\n", b64_encode(input.as_bytes())))
            }
        }
        "which" => {
            let Some(name) = args.first().copied() else {
                return Some(err(applet, "usage: which <cmd>".to_owned()));
            };
            if let Some(path_var) = std::env::var_os("PATH") {
                let cands: Vec<String> = if cfg!(windows) {
                    vec![format!("{name}.exe"), format!("{name}.cmd"), name.to_owned()]
                } else {
                    vec![name.to_owned()]
                };
                for dir in std::env::split_paths(&path_var) {
                    for c in &cands {
                        let full = dir.join(c);
                        if full.is_file() {
                            return Some(ok(applet, format!("{}\n", full.display())));
                        }
                    }
                }
            }
            err(applet, format!("{name} not found"))
        }
        "du" => {
            // Pure-Rust `dust`/`du` disk usage summary
            let root = Path::new(args.first().copied().unwrap_or("."));
            let mut rows: Vec<(u64, String)> = Vec::new();
            if let Ok(rd) = fs::read_dir(root) {
                for ent in rd.flatten() {
                    let p = ent.path();
                    let sz = dir_bytes(&p, 0);
                    rows.push((sz, ent.file_name().to_string_lossy().to_string()));
                }
            }
            rows.sort_by_key(|a| std::cmp::Reverse(a.0));
            let mut out = String::new();
            for (sz, name) in rows.into_iter().take(20) {
                out.push_str(&format!("{:>10.2} MiB  {name}\n", (sz as f64) / 1_048_576.0));
            }
            ok(applet, out)
        }
        "tokei" => {
            // Pure-Rust `tokei` LOC counter by language
            let root = Path::new(args.first().copied().unwrap_or("."));
            let mut by_lang: BTreeMap<&'static str, (usize, usize)> = BTreeMap::new();
            count_loc_dir(root, 0, &mut by_lang);
            let mut out = format!("{:<14} {:>8} {:>10}\n", "Language", "Files", "Lines");
            let mut tot_f = 0usize;
            let mut tot_l = 0usize;
            for (lang, (f, l)) in by_lang {
                tot_f += f;
                tot_l += l;
                out.push_str(&format!("{lang:<14} {f:>8} {l:>10}\n"));
            }
            out.push_str(&format!("{:<14} {:>8} {:>10}\n", "Total", tot_f, tot_l));
            ok(applet, out)
        }
        "jq" => {
            // Pure-Rust `jaq`/`jq` JSON query filter (`.`, `.a.b`, `.a[0]`, `keys`, `length`)
            let expr = args.first().copied().unwrap_or(".");
            let input = if let Some(path) = args.get(1)
                && Path::new(path).is_file()
            {
                match fs::read_to_string(path) {
                    Ok(s) => s,
                    Err(e) => return Some(err(applet, e.to_string())),
                }
            } else {
                stdin.to_owned()
            };
            let val: Value = match serde_json::from_str(&input) {
                Ok(v) => v,
                Err(e) => return Some(err(applet, format!("parse JSON: {e}"))),
            };
            let res = eval_jq(&val, expr);
            ok(
                applet,
                format!(
                    "{}\n",
                    serde_json::to_string_pretty(&res).unwrap_or_else(|_| "null".to_owned())
                ),
            )
        }
        "bat" => {
            let path = args.first().copied().unwrap_or("");
            match fs::read_to_string(path) {
                Ok(content) => {
                    let mut out = format!("── {path} ──\n");
                    for (idx, line) in content.lines().take(200).enumerate() {
                        out.push_str(&format!("{:>4} │ {line}\n", idx + 1));
                    }
                    ok(applet, out)
                }
                Err(e) => err(applet, format!("{path}: {e}")),
            }
        }
        _ => return None,
    })
}

/// Zero-fork in-process POSIX pipeline shell (`fw sh -c "cat Cargo.toml | grep version | sort -u | wc"`).
/// Works identically on Windows, Linux, macOS, and `wasm32-unknown-unknown`.
pub fn run_inproc_pipe_shell(args: &[&str], initial_stdin: &str) -> AppletOutput {
    let script = if args.first() == Some(&"-c") {
        args.get(1..).map(|s| s.join(" ")).unwrap_or_default()
    } else {
        args.join(" ")
    };
    if script.trim().is_empty() {
        return err("sh", "usage: fw sh -c \"cmd1 | cmd2 | cmd3\"".to_owned());
    }

    let mut pipe_buf = initial_stdin.to_owned();
    let mut last_code = 0i32;
    for stage in script.split('|') {
        let tokens = split_shell_words(stage.trim());
        if tokens.is_empty() {
            continue;
        }
        let cmd = tokens[0].as_str();
        let cmd_args: Vec<&str> = tokens[1..].iter().map(String::as_str).collect();
        let res = run_applet(cmd, &cmd_args, &pipe_buf);
        last_code = res.exit_code;
        if last_code != 0 && !res.stderr.is_empty() {
            return res;
        }
        pipe_buf = res.stdout;
    }

    AppletOutput {
        applet: "sh".to_owned(),
        exit_code: last_code,
        stdout: pipe_buf,
        stderr: String::new(),
    }
}

fn split_shell_words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quote: Option<char> = None;
    for ch in s.chars() {
        match in_quote {
            Some(q) if ch == q => in_quote = None,
            Some(_) => cur.push(ch),
            None if ch == '\'' || ch == '"' => in_quote = Some(ch),
            None if ch.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(core::mem::take(&mut cur));
                }
            }
            None => cur.push(ch),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn eval_jq(root: &Value, expr: &str) -> Value {
    let e = expr.trim();
    if e == "." || e.is_empty() {
        return root.clone();
    }
    if e == "keys" {
        return match root {
            Value::Object(m) => Value::Array(m.keys().map(|k| Value::String(k.clone())).collect()),
            _ => Value::Null,
        };
    }
    if e == "length" {
        return match root {
            Value::Array(a) => Value::from(a.len()),
            Value::Object(m) => Value::from(m.len()),
            Value::String(s) => Value::from(s.len()),
            _ => Value::from(0),
        };
    }
    let mut cur = root;
    for seg in e.trim_start_matches('.').split('.') {
        if seg.is_empty() {
            continue;
        }
        if let Some((key, rest)) = seg.split_once('[') {
            if !key.is_empty() {
                cur = cur.get(key).unwrap_or(&Value::Null);
            }
            if let Some(idx_s) = rest.strip_suffix(']')
                && let Ok(idx) = idx_s.parse::<usize>()
            {
                cur = cur.get(idx).unwrap_or(&Value::Null);
            }
        } else {
            cur = cur.get(seg).unwrap_or(&Value::Null);
        }
    }
    cur.clone()
}

fn dir_bytes(path: &Path, depth: usize) -> u64 {
    if depth > 6 {
        return 0;
    }
    let Ok(m) = fs::metadata(path) else {
        return 0;
    };
    if m.is_file() {
        return m.len();
    }
    let mut sum = 0u64;
    if let Ok(rd) = fs::read_dir(path) {
        for ent in rd.flatten() {
            let name = ent.file_name();
            if name == ".git" || name == "target" || name == "node_modules" {
                continue;
            }
            sum += dir_bytes(&ent.path(), depth + 1);
        }
    }
    sum
}

fn count_loc_dir(dir: &Path, depth: usize, map: &mut BTreeMap<&'static str, (usize, usize)>) {
    if depth > 8 {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for ent in rd.flatten() {
        let p: PathBuf = ent.path();
        let name = ent.file_name();
        if name == ".git" || name == "target" || name == "node_modules" || name == "dist" {
            continue;
        }
        if p.is_dir() {
            count_loc_dir(&p, depth + 1, map);
        } else if p.is_file() {
            let lang = match p.extension().and_then(|s| s.to_str()).unwrap_or("") {
                "rs" => "Rust",
                "ts" | "tsx" => "TypeScript",
                "js" | "mjs" => "JavaScript",
                "py" => "Python",
                "c" | "h" => "C",
                "cpp" | "hpp" => "C++",
                "kt" => "Kotlin",
                "wgsl" => "WGSL",
                "toml" => "TOML",
                "html" => "HTML",
                "css" => "CSS",
                _ => continue,
            };
            if let Ok(raw) = fs::read_to_string(&p) {
                let e = map.entry(lang).or_insert((0, 0));
                e.0 += 1;
                e.1 += raw.lines().count();
            }
        }
    }
}

fn b64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 { TABLE[((n >> 6) & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[(n & 63) as usize] as char } else { '=' });
    }
    out
}

fn b64_decode(s: &str) -> Result<Vec<u8>, String> {
    let val = |b: u8| -> Option<u32> {
        match b {
            b'A'..=b'Z' => Some((b - b'A') as u32),
            b'a'..=b'z' => Some((b - b'a' + 26) as u32),
            b'0'..=b'9' => Some((b - b'0' + 52) as u32),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    };
    let clean: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    for chunk in clean.chunks(4) {
        if chunk.len() < 2 {
            break;
        }
        let c0 = val(chunk[0]).ok_or_else(|| "invalid base64".to_owned())?;
        let c1 = val(chunk[1]).ok_or_else(|| "invalid base64".to_owned())?;
        let c2 = chunk.get(2).and_then(|&b| if b == b'=' { None } else { val(b) });
        let c3 = chunk.get(3).and_then(|&b| if b == b'=' { None } else { val(b) });
        let n = (c0 << 18) | (c1 << 12) | (c2.unwrap_or(0) << 6) | c3.unwrap_or(0);
        out.push(((n >> 16) & 0xFF) as u8);
        if c2.is_some() {
            out.push(((n >> 8) & 0xFF) as u8);
        }
        if c3.is_some() {
            out.push((n & 0xFF) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inproc_pipe_shell_and_jq_and_base64_work() {
        let pipe = run_inproc_pipe_shell(&["-c", "seq 5 | sort -r | head -n 2"], "");
        assert_eq!(pipe.exit_code, 0);
        assert_eq!(pipe.stdout, "5\n4\n");

        let jq_out = run_extended_applet("jq", &[".a.b[1]"], r#"{"a":{"b":[10,20,30]}}"#).unwrap();
        assert_eq!(jq_out.stdout.trim(), "20");

        let enc = run_extended_applet("base64", &[], "fw-wasm").unwrap();
        let dec = run_extended_applet("base64", &["-d"], enc.stdout.trim()).unwrap();
        assert_eq!(dec.stdout, "fw-wasm");
    }
}
