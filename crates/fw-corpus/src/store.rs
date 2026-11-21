//! One SQLite database per source: `doc` rows (one per fetched page or file) and `chunk` rows
//! (one per section) indexed by an external-content FTS5 table.

use crate::chunk::Chunk;
use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub struct Store {
    pub id: String,
    pub path: PathBuf,
    conn: Connection,
}

pub struct DocInput<'a> {
    /// Stable key and citation URL.
    pub url: &'a str,
    pub title: &'a str,
    /// Change marker: blob sha, commit sha, lastmod, `etag:..` or `lm:..` validator.
    pub rev: &'a str,
    pub kind: &'a str,
    /// Immutable URL (commit-pinned) when `url` is a floating one.
    pub origin: &'a str,
    pub license: &'a str,
    pub bytes: usize,
}

pub struct Stats {
    pub docs: i64,
    pub chunks: i64,
    pub bytes: i64,
    pub db_bytes: u64,
    pub last_fetch: Option<i64>,
}

pub struct Hit {
    pub source: String,
    pub url: String,
    pub origin: String,
    pub title: String,
    pub heading: String,
    pub anchor: String,
    pub snippet: String,
    pub score: f64,
    pub fetched_at: i64,
    pub license: String,
}

impl Store {
    pub fn open(root: &Path, id: &str) -> Result<Self> {
        std::fs::create_dir_all(root).with_context(|| format!("create {}", root.display()))?;
        let path = root.join(format!("{id}.db"));
        let conn = Connection::open(&path).with_context(|| format!("open {}", path.display()))?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA busy_timeout=30000;
             CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS doc(
               id INTEGER PRIMARY KEY, url TEXT NOT NULL UNIQUE, title TEXT NOT NULL, rev TEXT NOT NULL,
               kind TEXT NOT NULL, origin TEXT NOT NULL, license TEXT NOT NULL, bytes INTEGER NOT NULL,
               fetched_at INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS chunk(
               id INTEGER PRIMARY KEY, doc_id INTEGER NOT NULL REFERENCES doc(id) ON DELETE CASCADE,
               heading TEXT NOT NULL, anchor TEXT NOT NULL, body TEXT NOT NULL);
             CREATE INDEX IF NOT EXISTS chunk_doc ON chunk(doc_id);
             CREATE VIRTUAL TABLE IF NOT EXISTS chunk_fts USING fts5(
               heading, body, content='chunk', content_rowid='id', tokenize='porter unicode61 remove_diacritics 2');
             CREATE TRIGGER IF NOT EXISTS chunk_ai AFTER INSERT ON chunk BEGIN
               INSERT INTO chunk_fts(rowid, heading, body) VALUES (new.id, new.heading, new.body); END;
             CREATE TRIGGER IF NOT EXISTS chunk_ad AFTER DELETE ON chunk BEGIN
               INSERT INTO chunk_fts(chunk_fts, rowid, heading, body) VALUES ('delete', old.id, old.heading, old.body); END;",
        )?;
        conn.pragma_update(None, "foreign_keys", true)?;
        Ok(Self { id: id.to_owned(), path, conn })
    }

    /// `(rev, fetched_at)` of a stored doc.
    pub fn state(&self, url: &str) -> Result<Option<(String, i64)>> {
        Ok(self
            .conn
            .query_row("SELECT rev, fetched_at FROM doc WHERE url=?1", [url], |row| Ok((row.get(0)?, row.get(1)?)))
            .optional()?)
    }

    pub fn touch(&self, url: &str) -> Result<()> {
        self.conn.execute("UPDATE doc SET fetched_at=?2 WHERE url=?1", params![url, crate::now_unix()])?;
        Ok(())
    }

    pub fn put(&mut self, doc: &DocInput, chunks: &[Chunk]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM doc WHERE url=?1", [doc.url])?;
        tx.execute(
            "INSERT INTO doc(url,title,rev,kind,origin,license,bytes,fetched_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![doc.url, doc.title, doc.rev, doc.kind, doc.origin, doc.license, doc.bytes as i64, crate::now_unix()],
        )?;
        let doc_id = tx.last_insert_rowid();
        {
            let mut insert = tx.prepare("INSERT INTO chunk(doc_id,heading,anchor,body) VALUES(?1,?2,?3,?4)")?;
            for chunk in chunks {
                insert.execute(params![doc_id, chunk.heading, chunk.anchor, chunk.body])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Delete docs of a completed full listing that disappeared upstream.
    pub fn prune_except(&mut self, keep: &HashSet<String>) -> Result<usize> {
        let urls: Vec<String> = {
            let mut statement = self.conn.prepare("SELECT url FROM doc")?;
            statement.query_map([], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?
        };
        let tx = self.conn.transaction()?;
        let mut removed = 0;
        for url in urls.iter().filter(|url| !keep.contains(*url)) {
            removed += tx.execute("DELETE FROM doc WHERE url=?1", [url])?;
        }
        tx.commit()?;
        Ok(removed)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn meta(&self, key: &str) -> Result<Option<String>> {
        Ok(self.conn.query_row("SELECT value FROM meta WHERE key=?1", [key], |row| row.get(0)).optional()?)
    }

    pub fn stats(&self) -> Result<Stats> {
        let (docs, bytes, last_fetch): (i64, i64, Option<i64>) = self.conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(bytes),0), MAX(fetched_at) FROM doc",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        let chunks: i64 = self.conn.query_row("SELECT COUNT(*) FROM chunk", [], |row| row.get(0))?;
        let db_bytes = std::fs::metadata(&self.path).map(|meta| meta.len()).unwrap_or(0)
            + std::fs::metadata(self.path.with_extension("db-wal")).map(|meta| meta.len()).unwrap_or(0);
        Ok(Stats { docs, chunks, bytes, db_bytes, last_fetch })
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<Hit>> {
        let terms = terms(query);
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        for joiner in [" ", " OR "] {
            let expression = terms.iter().map(|term| format!("\"{term}\"")).collect::<Vec<_>>().join(joiner);
            let hits = self.run_match(&expression, limit)?;
            if !hits.is_empty() {
                return Ok(hits);
            }
        }
        Ok(Vec::new())
    }

    fn run_match(&self, expression: &str, limit: usize) -> Result<Vec<Hit>> {
        let mut statement = self.conn.prepare(
            "SELECT d.url, d.origin, d.title, c.heading, c.anchor, snippet(chunk_fts, 1, '[', ']', '...', 28),
                    bm25(chunk_fts, 4.0, 1.0), d.fetched_at, d.license
             FROM chunk_fts JOIN chunk c ON c.id = chunk_fts.rowid JOIN doc d ON d.id = c.doc_id
             WHERE chunk_fts MATCH ?1 ORDER BY bm25(chunk_fts, 4.0, 1.0) LIMIT ?2",
        )?;
        let rows = statement.query_map(params![expression, limit as i64], |row| {
            Ok(Hit {
                source: self.id.clone(),
                url: row.get(0)?,
                origin: row.get(1)?,
                title: row.get(2)?,
                heading: row.get(3)?,
                anchor: row.get(4)?,
                snippet: row.get(5)?,
                score: row.get(6)?,
                fetched_at: row.get(7)?,
                license: row.get(8)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Full text of a doc (all chunks in order) by URL or `url` substring match.
    pub fn read(&self, url: &str) -> Result<Option<(String, String, String)>> {
        let found: Option<(i64, String, String)> = self
            .conn
            .query_row(
                "SELECT id, url, origin FROM doc WHERE url=?1 OR url LIKE '%'||?1||'%' ORDER BY length(url) LIMIT 1",
                [url],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((id, url, origin)) = found else { return Ok(None) };
        let mut statement = self.conn.prepare("SELECT heading, body FROM chunk WHERE doc_id=?1 ORDER BY id")?;
        let body = statement
            .query_map([id], |row| Ok(format!("## {}\n\n{}", row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .join("\n\n");
        Ok(Some((url, origin, body)))
    }
}

const STOPWORDS: &[&str] = &[
    "the", "is", "are", "was", "how", "what", "which", "does", "do", "for", "and", "of", "to", "in", "on", "a", "an", "with",
    "when", "why", "can", "i", "it", "that", "this", "from", "by", "as", "at", "be",
];

fn terms(query: &str) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();
    for word in query.split(|character: char| !character.is_alphanumeric() && character != '_') {
        let word = word.to_lowercase();
        if word.chars().count() >= 2 && !STOPWORDS.contains(&word.as_str()) && !terms.contains(&word) {
            terms.push(word);
        }
    }
    terms
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::markdown;

    #[test]
    fn put_search_prune() {
        let dir = std::env::temp_dir().join(format!("fw-corpus-test-{}", std::process::id()));
        let mut store = Store::open(&dir, "t").unwrap();
        let chunks = markdown("Resolver", "# Resolver\n\nFeature unification in resolver 2 is per target.\n", 3000);
        let doc = DocInput { url: "https://x/r", title: "Resolver", rev: "1", kind: "doc", origin: "", license: "MIT", bytes: 10 };
        store.put(&doc, &chunks).unwrap();
        let hits = store.search("how does feature unification work", 5).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].snippet.contains('['));
        assert_eq!(store.prune_except(&HashSet::new()).unwrap(), 1);
        assert!(store.search("unification", 5).unwrap().is_empty());
        std::fs::remove_dir_all(dir).ok();
    }
}
