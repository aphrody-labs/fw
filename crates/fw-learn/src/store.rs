//! One SQLite database for everything learned: `item` rows (one per commit, doc chunk, unit, CI run, release, issue, pull
//! request, graph note or imported legacy record) indexed by an external-content FTS5 table, plus the tables the distiller
//! reads (`commit_area`, `unit`, `ci_run`) and the distilled `rule` rows.

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub struct Store {
    pub root: PathBuf,
    pub conn: Connection,
}

pub struct Item<'a> {
    pub kind: &'a str,
    pub product: &'a str,
    pub repo: &'a str,
    pub reference: &'a str,
    pub title: &'a str,
    pub body: &'a str,
    pub path: &'a str,
    pub ts: i64,
}

/// Store location: `--root`, then `FW_LEARN_XFS_ROOT`, then the build host's shared disk, then the user's data directory.
pub fn resolve_root(explicit: Option<&Path>) -> PathBuf {
    if let Some(root) = explicit {
        return root.to_path_buf();
    }
    if let Some(env) = std::env::var_os("FW_LEARN_XFS_ROOT").filter(|value| !value.is_empty()) {
        return PathBuf::from(env);
    }
    let shared = Path::new("/srv/xbuild");
    if shared.is_dir() {
        return shared.join("fw-learn");
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    home.join(".fx").join("learn-xfs")
}

impl Store {
    pub fn open(root: &Path) -> Result<Self> {
        std::fs::create_dir_all(root).with_context(|| format!("create {}", root.display()))?;
        let path = root.join("learn.db");
        let conn = Connection::open(&path).with_context(|| format!("open {}", path.display()))?;
        conn.execute_batch(SCHEMA).context("create schema")?;
        Ok(Self { root: root.to_path_buf(), conn })
    }

    pub fn begin(&self) -> Result<()> {
        self.conn.execute_batch("BEGIN").context("begin")
    }

    pub fn commit(&self) -> Result<()> {
        self.conn.execute_batch("COMMIT").context("commit")
    }

    /// Inserts the item or replaces it when its content changed; true when the store changed.
    pub fn upsert(&self, item: &Item) -> Result<bool> {
        let hash = content_hash(&[item.title, item.body, item.path, &item.ts.to_string(), item.product]);
        let changed = self
            .conn
            .execute(
                "INSERT INTO item(kind, product, repo, ref, title, body, path, ts, hash) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
                 ON CONFLICT(kind, repo, ref) DO UPDATE SET product=excluded.product, title=excluded.title, body=excluded.body,
                   path=excluded.path, ts=excluded.ts, hash=excluded.hash WHERE item.hash != excluded.hash",
                params![item.kind, item.product, item.repo, item.reference, item.title, item.body, item.path, item.ts, hash],
            )
            .context("upsert item")?;
        Ok(changed > 0)
    }

    pub fn state(&self, key: &str) -> Result<Option<String>> {
        self.conn.query_row("SELECT value FROM state WHERE key = ?1", [key], |row| row.get(0)).optional().context("read state")
    }

    pub fn set_state(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute("INSERT INTO state(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value", [key, value]).context("write state")?;
        Ok(())
    }

    pub fn count_by_kind(&self) -> Result<Vec<(String, i64)>> {
        let mut statement = self.conn.prepare("SELECT kind, COUNT(*) FROM item GROUP BY kind ORDER BY COUNT(*) DESC")?;
        let rows = statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?;
        rows.collect::<std::result::Result<_, _>>().context("count by kind")
    }

    pub fn stats(&self) -> Result<Value> {
        let kinds = self.count_by_kind()?;
        let total: i64 = kinds.iter().map(|(_, count)| count).sum();
        let mut by_product = self.conn.prepare("SELECT product, COUNT(*) FROM item GROUP BY product ORDER BY COUNT(*) DESC")?;
        let products: Vec<Value> = by_product
            .query_map([], |row| Ok(json!({"product": row.get::<_, String>(0)?, "items": row.get::<_, i64>(1)?})))?
            .collect::<std::result::Result<_, _>>()?;
        let scalar = |sql: &str| -> Result<i64> { self.conn.query_row(sql, [], |row| row.get(0)).context("scalar") };
        let db_bytes = std::fs::metadata(self.root.join("learn.db")).map(|meta| meta.len()).unwrap_or(0);
        let mut repos = self.conn.prepare("SELECT key, value FROM state WHERE key LIKE 'git:%' ORDER BY key")?;
        let repos: Vec<Value> = repos
            .query_map([], |row| Ok(json!({"repo": row.get::<_, String>(0)?.trim_start_matches("git:").to_owned(), "head": row.get::<_, String>(1)?})))?
            .collect::<std::result::Result<_, _>>()?;
        Ok(json!({
            "ok": true,
            "root": self.root.display().to_string(),
            "items": total,
            "byKind": kinds.iter().map(|(kind, count)| (kind.clone(), json!(count))).collect::<serde_json::Map<_, _>>(),
            "byProduct": products,
            "commits": scalar("SELECT COUNT(*) FROM item WHERE kind = 'commit'")?,
            "units": scalar("SELECT COUNT(*) FROM unit")?,
            "ciRuns": scalar("SELECT COUNT(*) FROM ci_run")?,
            "rules": scalar("SELECT COUNT(*) FROM rule")?,
            "graphFiles": count_files(&self.root.join("graphs")),
            "dbBytes": db_bytes,
            "repos": repos,
        }))
    }
}

pub fn content_hash(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0u8]);
    }
    hasher.finalize().iter().take(8).map(|byte| format!("{byte:02x}")).collect()
}

fn count_files(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries.flatten().map(|entry| if entry.path().is_dir() { count_files(&entry.path()) } else { 1 }).sum()
}

const SCHEMA: &str = "
PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA busy_timeout=30000;
CREATE TABLE IF NOT EXISTS item(
  id INTEGER PRIMARY KEY, kind TEXT NOT NULL, product TEXT NOT NULL, repo TEXT NOT NULL, ref TEXT NOT NULL,
  title TEXT NOT NULL, body TEXT NOT NULL, path TEXT NOT NULL, ts INTEGER NOT NULL, hash TEXT NOT NULL,
  UNIQUE(kind, repo, ref));
CREATE INDEX IF NOT EXISTS item_kind_product ON item(kind, product);
CREATE VIRTUAL TABLE IF NOT EXISTS item_fts USING fts5(title, body, path, content='item', content_rowid='id', tokenize='porter unicode61 remove_diacritics 2');
CREATE TRIGGER IF NOT EXISTS item_ai AFTER INSERT ON item BEGIN
  INSERT INTO item_fts(rowid, title, body, path) VALUES (new.id, new.title, new.body, new.path); END;
CREATE TRIGGER IF NOT EXISTS item_ad AFTER DELETE ON item BEGIN
  INSERT INTO item_fts(item_fts, rowid, title, body, path) VALUES ('delete', old.id, old.title, old.body, old.path); END;
CREATE TRIGGER IF NOT EXISTS item_au AFTER UPDATE ON item BEGIN
  INSERT INTO item_fts(item_fts, rowid, title, body, path) VALUES ('delete', old.id, old.title, old.body, old.path);
  INSERT INTO item_fts(rowid, title, body, path) VALUES (new.id, new.title, new.body, new.path); END;
CREATE TABLE IF NOT EXISTS commit_area(sha TEXT NOT NULL, repo TEXT NOT NULL, product TEXT NOT NULL, area TEXT NOT NULL, ts INTEGER NOT NULL, fix INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS commit_area_sha ON commit_area(repo, sha);
CREATE INDEX IF NOT EXISTS commit_area_area ON commit_area(product, area);
CREATE TABLE IF NOT EXISTS unit(repo TEXT NOT NULL, product TEXT NOT NULL, dir TEXT NOT NULL, kind TEXT NOT NULL, name TEXT NOT NULL,
  version TEXT NOT NULL, description TEXT NOT NULL, deps TEXT NOT NULL, PRIMARY KEY(repo, dir, kind));
CREATE TABLE IF NOT EXISTS ci_run(repo TEXT NOT NULL, id INTEGER NOT NULL, workflow TEXT NOT NULL, conclusion TEXT NOT NULL,
  branch TEXT NOT NULL, event TEXT NOT NULL, ts INTEGER NOT NULL, PRIMARY KEY(repo, id));
CREATE TABLE IF NOT EXISTS product(repo TEXT NOT NULL, id TEXT NOT NULL, display TEXT NOT NULL, description TEXT NOT NULL, dir TEXT NOT NULL, docs_url TEXT NOT NULL, PRIMARY KEY(repo, id));
CREATE TABLE IF NOT EXISTS rule(product TEXT NOT NULL, section TEXT NOT NULL, ord INTEGER NOT NULL, text TEXT NOT NULL, evidence TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS state(key TEXT PRIMARY KEY, value TEXT NOT NULL);
";
