//! Local backends: rusqlite for metadata (`drive.db`) and plain files for blobs (`blobs/<2 hex>/<blake3>`),
//! so identical uploads are stored once and every blob is an ordinary file.

use crate::store::{BlobSink, BlobStream, Blobs, Row, SCHEMA, Sql, Stmt, Value};
use crate::{Error, Result};
use async_trait::async_trait;
use bytes::Bytes;
use rusqlite::types::{Value as Cell, ValueRef};
use rusqlite::{Connection, params_from_iter};
use std::fs;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Self::Db(e.to_string())
    }
}

pub struct SqliteSql {
    db: Mutex<Connection>,
}

impl SqliteSql {
    /// Opens (or creates) the database file and its schema.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }

    /// A private in-memory database (tests).
    pub fn memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(db: Connection) -> Result<Self> {
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        db.execute_batch("pragma journal_mode = wal; pragma foreign_keys = on;")?;
        for ddl in SCHEMA {
            db.execute_batch(ddl)?;
        }
        Ok(Self { db: Mutex::new(db) })
    }

    fn db(&self) -> MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn cell(v: Value) -> Cell {
    match v {
        Value::Null => Cell::Null,
        Value::Int(i) => Cell::Integer(i),
        Value::Text(t) => Cell::Text(t),
    }
}

fn value(v: ValueRef<'_>) -> Value {
    match v {
        ValueRef::Integer(i) => Value::Int(i),
        ValueRef::Text(t) => Value::Text(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Real(f) => Value::Int(f as i64),
        ValueRef::Null | ValueRef::Blob(_) => Value::Null,
    }
}

#[async_trait]
impl Sql for SqliteSql {
    async fn query(&self, sql: &str, params: Vec<Value>) -> Result<Vec<Row>> {
        let db = self.db();
        let mut stmt = db.prepare_cached(sql)?;
        let cols = stmt.column_count();
        let mut rows = stmt.query(params_from_iter(params.into_iter().map(cell)))?;
        let mut out = Vec::new();
        while let Some(r) = rows.next()? {
            out.push(
                (0..cols)
                    .map(|i| r.get_ref(i).map(value))
                    .collect::<rusqlite::Result<Row>>()?,
            );
        }
        Ok(out)
    }

    async fn batch(&self, stmts: Vec<Stmt>) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        for s in stmts {
            let mut stmt = tx.prepare_cached(&s.sql)?;
            stmt.execute(params_from_iter(s.params.into_iter().map(cell)))?;
        }
        tx.commit()?;
        Ok(())
    }
}

/// Blobs as files under `root/blobs`, uploads staged in `root/tmp`.
pub struct FsBlobs {
    root: PathBuf,
}

impl FsBlobs {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join("blobs"))?;
        fs::create_dir_all(root.join("tmp"))?;
        Ok(Self { root })
    }

    pub fn path(&self, hash: &str) -> PathBuf {
        self.root.join("blobs").join(&hash[..2]).join(hash)
    }

    /// Blob files stored.
    pub fn count(&self) -> usize {
        fs::read_dir(self.root.join("blobs"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|d| fs::read_dir(d.path()).map_or(0, Iterator::count))
            .sum()
    }

    /// Staged uploads not yet committed or discarded.
    pub fn pending(&self) -> usize {
        fs::read_dir(self.root.join("tmp")).map_or(0, Iterator::count)
    }
}

struct FsSink {
    tmp: PathBuf,
    file: Option<fs::File>,
    blobs: PathBuf,
}

impl Drop for FsSink {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.tmp);
    }
}

#[async_trait]
impl BlobSink for FsSink {
    async fn write(&mut self, chunk: &[u8]) -> Result<()> {
        self.file
            .as_mut()
            .ok_or(Error::NotFound)?
            .write_all(chunk)?;
        Ok(())
    }

    async fn commit(mut self: Box<Self>, hash: &str) -> Result<()> {
        if let Some(mut f) = self.file.take() {
            f.flush()?;
            f.sync_all()?;
        }
        let dest = self.blobs.join(&hash[..2]).join(hash);
        if !dest.exists() {
            fs::create_dir_all(dest.parent().ok_or(Error::NotFound)?)?;
            fs::rename(&self.tmp, &dest)?;
        }
        Ok(())
    }
}

#[async_trait]
impl Blobs for FsBlobs {
    async fn begin(&self) -> Result<Box<dyn BlobSink>> {
        let tmp = self.root.join("tmp").join(crate::new_id());
        let file = fs::File::create(&tmp)?;
        Ok(Box::new(FsSink {
            tmp,
            file: Some(file),
            blobs: self.root.join("blobs"),
        }))
    }

    async fn open(&self, hash: &str, range: Option<(u64, u64)>) -> Result<BlobStream> {
        let mut file = fs::File::open(self.path(hash))?;
        let len = file.metadata()?.len();
        let (start, len) = match range {
            Some((s, e)) => (s, e - s + 1),
            None => (0, len),
        };
        if start > 0 {
            file.seek(SeekFrom::Start(start))?;
        }
        Ok(Box::pin(futures_util::stream::unfold(
            (file.take(len), vec![0u8; 256 * 1024]),
            |(mut r, mut buf)| async move {
                match r.read(&mut buf) {
                    Ok(0) => None,
                    Ok(n) => Some((
                        Ok::<_, io::Error>(Bytes::copy_from_slice(&buf[..n])),
                        (r, buf),
                    )),
                    Err(e) => Some((Err(e), (r, buf))),
                }
            },
        )))
    }

    async fn delete(&self, hash: &str) -> Result<()> {
        let _ = fs::remove_file(self.path(hash));
        Ok(())
    }
}
