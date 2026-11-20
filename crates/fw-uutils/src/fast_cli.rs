//! Zero-dependency, `#![forbid(unsafe_code)]` high-performance CLI engine (`fw ls|find|grep|table`).
//!
//! Distills the core mechanical sympathies of `ripgrep`, `fd`, `eza`, and `nushell` using only `std`:
//! - SWAR 8-byte `u64` broadcast byte search (auto-vectorized by LLVM 23 with zero `unsafe`)
//! - First+last byte anchor literal prefilter (`find_bytes`) without UTF-8 validation
//! - Zero-stat directory listing via `DirEntry::file_type()` (`d_type` / `FindFirstFileW`)
//! - Parallel `std::thread::scope` directory walking with hard-skip of `.git`/`target`/`node_modules`
//! - Reusable per-thread file read buffer + single-flush `BufWriter<StdoutLock>` output

use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::thread;

const SKIP_DIRS: &[&str] = &[".git", "target", "node_modules", "packages", ".venv", "__pycache__", "dist"];

#[inline]
fn should_skip_dir(name: &std::ffi::OsStr) -> bool {
    let Some(s) = name.to_str() else { return false };
    SKIP_DIRS.contains(&s)
}

/// SWAR 8-byte `u64` single-byte finder (Myers bit-hack; zero `unsafe`, LLVM AVX2-friendly).
#[inline]
pub fn find_byte_swar(haystack: &[u8], needle: u8) -> Option<usize> {
    const LO: u64 = 0x0101_0101_0101_0101;
    const HI: u64 = 0x8080_8080_8080_8080;
    let mask = u64::from(needle) * LO;
    let (chunks, remainder) = haystack.as_chunks::<8>();
    let mut offset = 0usize;
    for chunk in chunks {
        let word = u64::from_ne_bytes(*chunk);
        let x = word ^ mask;
        let hit = x.wrapping_sub(LO) & !x & HI;
        if hit != 0 {
            let lane = if cfg!(target_endian = "little") {
                (hit.trailing_zeros() / 8) as usize
            } else {
                (hit.leading_zeros() / 8) as usize
            };
            return Some(offset + lane);
        }
        offset += 8;
    }
    for (i, &b) in remainder.iter().enumerate() {
        if b == needle {
            return Some(offset + i);
        }
    }
    None
}

/// Fast literal byte-slice search using SWAR first-byte scan + last-byte anchor check.
#[inline]
pub fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    let nlen = needle.len();
    if nlen == 0 {
        return Some(0);
    }
    if nlen > haystack.len() {
        return None;
    }
    if nlen == 1 {
        return find_byte_swar(haystack, needle[0]);
    }
    let first = needle[0];
    let last = needle[nlen - 1];
    let last_off = nlen - 1;
    let limit = haystack.len() - last_off;
    let mut pos = 0usize;
    while pos < limit {
        let rel = find_byte_swar(&haystack[pos..limit], first)?;
        let idx = pos + rel;
        if haystack[idx + last_off] == last && &haystack[idx..idx + nlen] == needle {
            return Some(idx);
        }
        pos = idx + 1;
    }
    None
}

/// Ultra-fast zero-stat directory listing (`fw ls [dir] [--tsv]`).
/// Uses `DirEntry::file_type()` directly (zero extra `stat()` syscalls) and single buffered flush.
pub fn fast_ls(dir: &Path, tsv: bool) -> io::Result<usize> {
    let mut entries: Vec<(String, bool)> = Vec::with_capacity(64);
    for entry in fs::read_dir(dir)? {
        let Ok(e) = entry else { continue };
        let is_dir = e.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        let name = e.file_name().to_string_lossy().into_owned();
        entries.push((name, is_dir));
    }
    entries.sort_unstable_by(|a, b| a.0.cmp(&b.0));

    let stdout = io::stdout();
    let mut out = BufWriter::with_capacity(64 * 1024, stdout.lock());
    if tsv {
        out.write_all(b"name\tkind\n")?;
        for (name, is_dir) in &entries {
            out.write_all(name.as_bytes())?;
            out.write_all(if *is_dir { b"\tdir\n" } else { b"\tfile\n" })?;
        }
    } else {
        for (name, is_dir) in &entries {
            out.write_all(name.as_bytes())?;
            if *is_dir {
                out.write_all(b"/\n")?;
            } else {
                out.write_all(b"\n")?;
            }
        }
    }
    out.flush()?;
    Ok(entries.len())
}

/// Structured Nushell-style zero-startup table (`fw table [dir] [--json]`).
pub fn fast_table(dir: &Path, as_json: bool) -> io::Result<usize> {
    let mut rows: Vec<(String, &'static str, u64)> = Vec::with_capacity(64);
    for entry in fs::read_dir(dir)? {
        let Ok(e) = entry else { continue };
        let ft = e.file_type().ok();
        let kind = match ft {
            Some(t) if t.is_dir() => "dir",
            Some(t) if t.is_symlink() => "symlink",
            _ => "file",
        };
        let bytes = if kind == "file" {
            e.metadata().map(|m| m.len()).unwrap_or(0)
        } else {
            0
        };
        rows.push((e.file_name().to_string_lossy().into_owned(), kind, bytes));
    }
    rows.sort_unstable_by(|a, b| a.0.cmp(&b.0));

    let stdout = io::stdout();
    let mut out = BufWriter::with_capacity(64 * 1024, stdout.lock());
    if as_json {
        out.write_all(b"[")?;
        for (i, (name, kind, bytes)) in rows.iter().enumerate() {
            if i > 0 {
                out.write_all(b",")?;
            }
            write!(out, "{{\"name\":\"{}\",\"kind\":\"{kind}\",\"bytes\":{bytes}}}", name.replace('"', "\\\""))?;
        }
        out.write_all(b"]\n")?;
    } else {
        out.write_all(b"name\tkind\tbytes\n")?;
        for (name, kind, bytes) in &rows {
            writeln!(out, "{name}\t{kind}\t{bytes}")?;
        }
    }
    out.flush()?;
    Ok(rows.len())
}

/// Collect top-level subdirs and files, then fan out across `std::thread::scope` workers.
fn walk_parallel<F>(root: &Path, worker_fn: F) -> io::Result<Vec<Vec<u8>>>
where
    F: Fn(&Path, &mut Vec<u8>, &mut Vec<u8>) + Sync,
{
    let mut initial_dirs: Vec<PathBuf> = Vec::with_capacity(32);
    let mut root_files: Vec<PathBuf> = Vec::with_capacity(32);
    for entry in fs::read_dir(root)? {
        let Ok(e) = entry else { continue };
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            if !should_skip_dir(&e.file_name()) {
                initial_dirs.push(e.path());
            }
        } else if ft.is_file() {
            root_files.push(e.path());
        }
    }

    let n_threads = thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(4)
        .min(initial_dirs.len().max(1) + 1)
        .min(12);
    let queue = Mutex::new(initial_dirs);

    let buffers = thread::scope(|s| {
        #[allow(clippy::needless_collect)]
        let handles: Vec<_> = (0..n_threads)
            .map(|tid| {
                let q = &queue;
                let rf = &root_files;
                let f = &worker_fn;
                s.spawn(move || {
                    let mut out = Vec::with_capacity(16 * 1024);
                    let mut scratch = Vec::with_capacity(64 * 1024);
                    if tid == 0 {
                        for p in rf {
                            f(p, &mut scratch, &mut out);
                        }
                    }
                    let mut local_stack: Vec<PathBuf> = Vec::with_capacity(32);
                    loop {
                        let next_dir = if let Some(d) = local_stack.pop() {
                            Some(d)
                        } else {
                            q.lock().ok().and_then(|mut g| g.pop())
                        };
                        let Some(dir) = next_dir else { break };
                        let Ok(rd) = fs::read_dir(&dir) else { continue };
                        for entry in rd {
                            let Ok(e) = entry else { continue };
                            let Ok(ft) = e.file_type() else { continue };
                            if ft.is_dir() {
                                if !should_skip_dir(&e.file_name()) {
                                    local_stack.push(e.path());
                                }
                            } else if ft.is_file() {
                                f(&e.path(), &mut scratch, &mut out);
                            }
                        }
                    }
                    out
                })
            })
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok()).collect::<Vec<_>>()
    });
    Ok(buffers)
}

/// Ultra-fast parallel file finder (`fw find [pattern] [dir]`).
/// Zero regex compilation, zero `.gitignore` globset overhead, per-thread buffered output.
pub fn fast_find(pattern: &str, dir: &Path) -> io::Result<usize> {
    let needle = pattern.as_bytes();
    let buffers = walk_parallel(dir, |path, _scratch, out| {
        let s = path.as_os_str().as_encoded_bytes();
        if needle.is_empty() || find_bytes(s, needle).is_some() {
            out.extend_from_slice(s);
            out.push(b'\n');
        }
    })?;
    let stdout = io::stdout();
    let mut out = BufWriter::with_capacity(64 * 1024, stdout.lock());
    let mut lines = 0usize;
    for buf in &buffers {
        lines += buf.iter().filter(|&&b| b == b'\n').count();
        out.write_all(buf)?;
    }
    out.flush()?;
    Ok(lines)
}

/// Ultra-fast parallel literal byte-grep (`fw grep <needle> [dir]`).
/// Zero UTF-8 validation, SWAR byte scanner, reusable read buffer, lazy line-number computation.
pub fn fast_grep(pattern: &str, dir: &Path) -> io::Result<usize> {
    let needle = pattern.as_bytes();
    if needle.is_empty() {
        return Ok(0);
    }
    let buffers = walk_parallel(dir, |path, scratch, out| {
        let Ok(mut file) = File::open(path) else { return };
        scratch.clear();
        if file.read_to_end(scratch).is_err() || scratch.is_empty() {
            return;
        }
        // Skip binary files (NUL byte in first 512 bytes, same heuristic as ripgrep)
        let probe = &scratch[..scratch.len().min(512)];
        if find_byte_swar(probe, 0).is_some() {
            return;
        }
        // Fast reject: single SWAR scan across entire file; zero line counting on non-matches!
        let Some(first_hit) = find_bytes(scratch, needle) else {
            return;
        };
        let path_bytes = path.as_os_str().as_encoded_bytes();
        let mut pos = first_hit;
        let mut line_no = 1usize + scratch[..first_hit].iter().filter(|&&b| b == b'\n').count();
        let mut counted_to = first_hit;
        while pos < scratch.len() {
            if pos > counted_to {
                line_no += scratch[counted_to..pos].iter().filter(|&&b| b == b'\n').count();
                counted_to = pos;
            }
            let line_start = scratch[..pos].iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
            let line_end = find_byte_swar(&scratch[pos..], b'\n').map_or(scratch.len(), |i| pos + i);
            let line_slice = &scratch[line_start..line_end];
            let trimmed = line_slice.strip_suffix(b"\r").unwrap_or(line_slice);
            out.extend_from_slice(path_bytes);
            out.push(b':');
            let _ = write!(out, "{line_no}:");
            out.extend_from_slice(trimmed);
            out.push(b'\n');

            let next_start = if line_end < scratch.len() { line_end + 1 } else { break };
            let Some(rel) = find_bytes(&scratch[next_start..], needle) else {
                break;
            };
            pos = next_start + rel;
        }
    })?;

    let stdout = io::stdout();
    let mut out = BufWriter::with_capacity(64 * 1024, stdout.lock());
    let mut hits = 0usize;
    for buf in &buffers {
        hits += buf.iter().filter(|&&b| b == b'\n').count();
        out.write_all(buf)?;
    }
    out.flush()?;
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swar_and_literal_search_match_exact_offsets() {
        let hay = b"0123456789abcdef_fw_fast_grep_target_xyz";
        assert_eq!(find_byte_swar(hay, b'_'), Some(16));
        assert_eq!(find_byte_swar(hay, b'Z'), None);
        assert_eq!(find_bytes(hay, b"fw_fast_grep"), Some(17));
        assert_eq!(find_bytes(hay, b"missing"), None);
    }
}
