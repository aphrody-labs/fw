//! Native workspace indexer.
//! Exact boundary parity with `vercel-labs/fx/src/core/indexer/`:
//! reads `.gitignore`, `.git/info/exclude`, `.git/HEAD`, and `.git/config`
//! strictly as data and NEVER spawns a subprocess.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitRepoInfo {
    pub is_git_repo: bool,
    pub head_ref: Option<String>,
    pub remote_origin: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrepMatch {
    pub path: String,
    pub line: usize,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct WorkspaceIndexer {
    root: PathBuf,
    ignore_rules: Vec<String>,
}

impl WorkspaceIndexer {
    pub fn open(root: &Path) -> Self {
        let mut ignore_rules = vec![
            ".git".to_owned(),
            "target".to_owned(),
            "node_modules".to_owned(),
            ".next".to_owned(),
            "dist".to_owned(),
        ];
        for rel in [".gitignore", ".git/info/exclude"] {
            let p = root.join(rel);
            if let Ok(raw) = std::fs::read_to_string(&p) {
                for line in raw.lines() {
                    let t = line.trim();
                    if t.is_empty() || t.starts_with('#') || t.starts_with('!') {
                        continue;
                    }
                    ignore_rules.push(t.trim_end_matches('/').to_owned());
                }
            }
        }
        Self {
            root: root.to_path_buf(),
            ignore_rules,
        }
    }

    /// Reads `.git/HEAD` and `.git/config` strictly as data (zero subprocesses).
    pub fn read_git_info(&self) -> GitRepoInfo {
        let git_dir = self.root.join(".git");
        let head_path = git_dir.join("HEAD");
        let Ok(head_raw) = std::fs::read_to_string(&head_path) else {
            return GitRepoInfo {
                is_git_repo: false,
                head_ref: None,
                remote_origin: None,
            };
        };
        let head_trimmed = head_raw.trim();
        let head_ref = head_trimmed
            .strip_prefix("ref: refs/heads/")
            .unwrap_or(head_trimmed)
            .to_owned();

        let mut remote_origin = None;
        if let Ok(cfg) = std::fs::read_to_string(git_dir.join("config")) {
            let mut in_origin = false;
            for line in cfg.lines() {
                let t = line.trim();
                if t.starts_with('[') {
                    in_origin = t == "[remote \"origin\"]";
                } else if in_origin
                    && let Some(val) = t.strip_prefix("url")
                    && let Some((_, rhs)) = val.split_once('=')
                {
                    remote_origin = Some(rhs.trim().to_owned());
                    break;
                }
            }
        }

        GitRepoInfo {
            is_git_repo: true,
            head_ref: Some(head_ref),
            remote_origin,
        }
    }

    pub fn is_ignored(&self, rel: &str) -> bool {
        for seg in rel.split(['/', '\\']) {
            if self.ignore_rules.iter().any(|r| r == seg) {
                return true;
            }
        }
        self.ignore_rules.iter().any(|r| glob_match(r, rel))
    }

    pub fn list_files(&self, max_entries: usize) -> Vec<String> {
        let mut out = Vec::new();
        self.walk_dir(&self.root, "", max_entries, &mut out);
        out.sort();
        out
    }

    fn walk_dir(&self, dir: &Path, prefix: &str, max_entries: usize, out: &mut Vec<String>) {
        if out.len() >= max_entries {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in rd.flatten() {
            if out.len() >= max_entries {
                break;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let rel = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if self.is_ignored(&rel) {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                self.walk_dir(&path, &rel, max_entries, out);
            } else if path.is_file() {
                out.push(rel);
            }
        }
    }

    pub fn glob_files(&self, pattern: &str, max_entries: usize) -> Vec<String> {
        self.list_files(max_entries.saturating_mul(8).max(4096))
            .into_iter()
            .filter(|rel| glob_match(pattern, rel))
            .take(max_entries)
            .collect()
    }

    /// Literal text search (matching fx `grep_files` contract: literal, not regex).
    pub fn grep_literal(&self, query: &str, glob_filter: Option<&str>, max_matches: usize) -> Vec<GrepMatch> {
        if query.is_empty() {
            return Vec::new();
        }
        let files = match glob_filter {
            Some(pat) => self.glob_files(pat, 2048),
            None => self.list_files(2048),
        };
        let mut matches = Vec::new();
        for rel in files {
            if matches.len() >= max_matches {
                break;
            }
            let full = self.root.join(&rel);
            let Ok(bytes) = std::fs::read(&full) else {
                continue;
            };
            if bytes.len() > 2 * 1024 * 1024 || bytes.contains(&0) {
                continue;
            }
            let Ok(text) = std::str::from_utf8(&bytes) else {
                continue;
            };
            for (idx, line) in text.lines().enumerate() {
                if line.contains(query) {
                    matches.push(GrepMatch {
                        path: rel.clone(),
                        line: idx + 1,
                        text: line.chars().take(240).collect(),
                    });
                    if matches.len() >= max_matches {
                        break;
                    }
                }
            }
        }
        matches
    }

    /// Fuzzy `@`-completion over indexed workspace files.
    pub fn fuzzy_complete(&self, needle: &str, limit: usize) -> Vec<String> {
        let needle_lower = needle.to_ascii_lowercase();
        let mut scored: Vec<(i32, String)> = self
            .list_files(4096)
            .into_iter()
            .filter_map(|p| fuzzy_score(&needle_lower, &p.to_ascii_lowercase()).map(|s| (s, p)))
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        scored.into_iter().take(limit).map(|(_, p)| p).collect()
    }
}

pub fn fuzzy_score(needle: &str, haystack: &str) -> Option<i32> {
    if needle.is_empty() {
        return Some(0);
    }
    let mut score = 0i32;
    let mut h_idx = 0usize;
    let h_bytes = haystack.as_bytes();
    let mut consecutive = 0i32;
    for &nb in needle.as_bytes() {
        let mut found = None;
        for (i, &hb) in h_bytes.iter().enumerate().skip(h_idx) {
            if hb == nb {
                found = Some(i);
                break;
            }
        }
        let pos = found?;
        if pos == h_idx {
            consecutive += 1;
            score += 4 + consecutive * 2;
        } else {
            consecutive = 0;
            score += 1;
        }
        if pos == 0 || matches!(h_bytes[pos - 1], b'/' | b'_' | b'-' | b'.') {
            score += 5;
        }
        h_idx = pos + 1;
    }
    Some(score - (haystack.len() as i32 / 8))
}

pub fn glob_match(pattern: &str, text: &str) -> bool {
    let p = pattern.as_bytes();
    let t = text.as_bytes();
    glob_match_bytes(p, t)
}

fn glob_match_bytes(p: &[u8], t: &[u8]) -> bool {
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi + 1 < p.len() && p[pi] == b'*' && p[pi + 1] == b'*' {
            let mut next = pi + 2;
            if next < p.len() && p[next] == b'/' {
                next += 1;
            }
            if glob_match_bytes(&p[next..], &t[ti..]) {
                return true;
            }
            ti += 1;
            continue;
        }
        if pi < p.len() && p[pi] == b'*' {
            star = Some((pi, ti));
            pi += 1;
            continue;
        }
        if pi < p.len() && (p[pi] == b'?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
            continue;
        }
        if let Some((sp, st)) = star {
            if t[st] == b'/' {
                return false;
            }
            let next_st = st + 1;
            star = Some((sp, next_st));
            pi = sp + 1;
            ti = next_st;
            continue;
        }
        return false;
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_and_fuzzy_work() {
        assert!(glob_match("**/*.rs", "crates/fw-fx/src/lib.rs"));
        assert!(glob_match("*.toml", "Cargo.toml"));
        assert!(!glob_match("*.rs", "Cargo.toml"));
        assert!(fuzzy_score("rec", "crates/fw-fx/src/recovery.rs").is_some());
    }
}
