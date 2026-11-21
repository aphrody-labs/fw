//! The dataset as files: where it lives, how it is read and written, and the in-process cache.
//!
//! Working data (full): `<home>/meta.json`, `categories.json`, `ranked.jsonl`.
//! Published snapshot (what `fw crates sync --publish docs/crates` writes into the repository): `meta.json`,
//! `categories.json`, `index.jsonl` (the top crates plus every awesome-list entry), `awesome/*.json|mdx`, `RANKING.md`.

use crate::model::{Category, Meta, Record};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

pub const RANKED: &str = "ranked.jsonl";
pub const INDEX: &str = "index.jsonl";

/// Canonical key of a crate name: crates.io treats `-` and `_` as the same character and ignores case.
pub fn key(name: &str) -> String {
    name.trim().to_ascii_lowercase().replace('_', "-")
}

/// Working data directory: `FW_CRATES_HOME`, else `<FX_HOME>/crates`, else `~/.fx/crates`.
pub fn home() -> PathBuf {
    if let Some(p) = env_path("FW_CRATES_HOME") {
        return p;
    }
    if let Some(p) = env_path("FX_HOME") {
        return p.join("crates");
    }
    let user = env_path("HOME").or_else(|| env_path("USERPROFILE")).unwrap_or_else(|| PathBuf::from("."));
    user.join(".fx").join("crates")
}

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var(var).ok().filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// Directories searched for a dataset, best first: the working data, then a published `docs/crates` snapshot found
/// from `FW_ROOT` or by walking up from the current directory.
pub fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![home()];
    if let Some(root) = env_path("FW_ROOT") {
        dirs.push(root.join("docs").join("crates"));
    }
    if let Ok(mut cur) = std::env::current_dir() {
        loop {
            dirs.push(cur.join("docs").join("crates"));
            if !cur.pop() {
                break;
            }
        }
    }
    dirs
}

pub struct Dataset {
    pub dir: PathBuf,
    pub meta: Meta,
    pub categories: Vec<Category>,
    /// Sorted by score, best first.
    pub records: Vec<Record>,
    by_name: HashMap<String, usize>,
}

impl Dataset {
    pub fn new(dir: PathBuf, meta: Meta, categories: Vec<Category>, records: Vec<Record>) -> Self {
        let by_name = records.iter().enumerate().map(|(i, r)| (key(&r.name), i)).collect();
        Self { dir, meta, categories, records, by_name }
    }

    pub fn get(&self, name: &str) -> Option<&Record> {
        self.by_name.get(&key(name)).map(|&i| &self.records[i])
    }

    /// 1-based position by score over the whole dataset.
    pub fn overall_rank(&self, name: &str) -> Option<usize> {
        self.by_name.get(&key(name)).map(|&i| i + 1)
    }

    pub fn category(&self, slug: &str) -> Option<&Category> {
        self.categories.iter().find(|c| c.slug == slug)
    }

    /// Resolves a category by slug, or by a unique case-insensitive substring of its slug or name.
    pub fn find_category(&self, q: &str) -> Option<&Category> {
        let q = q.trim().to_ascii_lowercase();
        if let Some(c) = self.category(&q) {
            return Some(c);
        }
        let mut hits = self.categories.iter().filter(|c| c.slug.contains(&q) || c.name.to_ascii_lowercase().contains(&q));
        let first = hits.next()?;
        hits.next().is_none().then_some(first)
    }

    /// Eligible members of a category, ordered by score: non-yanked with enough recent downloads.
    pub fn category_members(&self, slug: &str) -> impl Iterator<Item = &Record> {
        let slug = slug.to_owned();
        self.records.iter().filter(move |r| !r.yanked && r.recent_downloads >= crate::rank::MIN_RECENT_DOWNLOADS && r.categories.contains(&slug))
    }
}

pub fn read_dir(dir: &Path) -> Result<Dataset, String> {
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).map_err(|e| format!("{}: {e}", dir.join(name).display()));
    let meta: Meta = serde_json::from_str(&read("meta.json")?).map_err(|e| format!("meta.json: {e}"))?;
    let categories: Vec<Category> = serde_json::from_str(&read("categories.json")?).map_err(|e| format!("categories.json: {e}"))?;
    let lines = if dir.join(RANKED).is_file() { read(RANKED)? } else { read(INDEX)? };
    let mut records = Vec::new();
    for (n, line) in lines.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
        records.push(serde_json::from_str::<Record>(line).map_err(|e| format!("line {}: {e}", n + 1))?);
    }
    records.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.name.cmp(&b.name)));
    Ok(Dataset::new(dir.to_path_buf(), meta, categories, records))
}

type Cache = Mutex<Vec<(PathBuf, SystemTime, Arc<Dataset>)>>;
static CACHE: Cache = Mutex::new(Vec::new());

fn stamp(dir: &Path) -> Option<SystemTime> {
    let f = if dir.join(RANKED).is_file() { RANKED } else { INDEX };
    std::fs::metadata(dir.join(f)).and_then(|m| m.modified()).ok()
}

/// Loads the first available dataset (cached while its file is unchanged).
pub fn load() -> Result<Arc<Dataset>, String> {
    for dir in candidate_dirs() {
        if !dir.join("meta.json").is_file() {
            continue;
        }
        let Some(mtime) = stamp(&dir) else { continue };
        let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, t, ds)) = cache.iter().find(|(d, _, _)| *d == dir)
            && *t == mtime
        {
            return Ok(ds.clone());
        }
        let ds = Arc::new(read_dir(&dir)?);
        cache.retain(|(d, _, _)| *d != dir);
        cache.push((dir, mtime, ds.clone()));
        return Ok(ds);
    }
    Err(format!("no crates dataset found (looked in {}); run `fw crates sync`", candidate_dirs().iter().take(2).map(|d| d.display().to_string()).collect::<Vec<_>>().join(", ")))
}

fn write_file(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    if std::fs::read_to_string(path).is_ok_and(|old| old == content) {
        return Ok(());
    }
    std::fs::write(path, content).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn write_text(path: &Path, content: &str) -> Result<(), String> {
    write_file(path, content)
}

pub fn jsonl(records: &[&Record]) -> Result<String, String> {
    let mut out = String::new();
    for r in records {
        out.push_str(&serde_json::to_string(r).map_err(|e| e.to_string())?);
        out.push('\n');
    }
    Ok(out)
}

/// Writes meta, categories and the full ranked JSONL into `dir`.
pub fn write_working(dir: &Path, meta: &Meta, categories: &[Category], records: &[Record]) -> Result<(), String> {
    write_file(&dir.join("meta.json"), &(serde_json::to_string_pretty(meta).map_err(|e| e.to_string())? + "\n"))?;
    write_file(&dir.join("categories.json"), &(serde_json::to_string_pretty(categories).map_err(|e| e.to_string())? + "\n"))?;
    let refs: Vec<&Record> = records.iter().collect();
    write_file(&dir.join(RANKED), &jsonl(&refs)?)
}
