//! Storage behind the drive rules: an SQL database for metadata and a blob store for content. The rules
//! in `lib.rs` run over these two traits, so the same code serves the local backends (`sqlite`: rusqlite
//! and plain files) and Cloudflare (D1 for metadata, R2 for blobs, implemented in the `fw-edge` Worker).
//!
//! D1 is SQLite, so one SQL dialect and one schema cover both. The only transaction primitive is
//! [`Sql::batch`] (D1's `batch`): statements that must commit together go in one batch, and reads that
//! decide what to write happen before it.

use crate::Result;
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::stream::BoxStream;
use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Mutex};

/// A SQL parameter or result cell. Metadata needs nothing else: ids and names are text, sizes and times integers.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Int(i64),
    Text(String),
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Self::Text(v.to_owned())
    }
}
impl From<String> for Value {
    fn from(v: String) -> Self {
        Self::Text(v)
    }
}
impl From<&String> for Value {
    fn from(v: &String) -> Self {
        Self::Text(v.clone())
    }
}
impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Self::Int(v)
    }
}
impl From<u64> for Value {
    fn from(v: u64) -> Self {
        Self::Int(i64::try_from(v).unwrap_or(i64::MAX))
    }
}
impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(v: Option<T>) -> Self {
        v.map_or(Self::Null, Into::into)
    }
}

/// `params![a, b]` builds a `Vec<Value>`.
#[macro_export]
macro_rules! params {
    ($($v:expr),* $(,)?) => { vec![$($crate::store::Value::from($v)),*] };
}

pub type Row = Vec<Value>;

/// Typed reads of one result row.
pub trait RowExt {
    fn int(&self, i: usize) -> Result<i64>;
    fn text(&self, i: usize) -> Result<String>;
    fn opt_text(&self, i: usize) -> Result<Option<String>>;
}

impl RowExt for Row {
    fn int(&self, i: usize) -> Result<i64> {
        match self.get(i) {
            Some(Value::Int(v)) => Ok(*v),
            other => Err(crate::Error::Db(format!(
                "column {i}: expected an integer, got {other:?}"
            ))),
        }
    }
    fn text(&self, i: usize) -> Result<String> {
        match self.get(i) {
            Some(Value::Text(v)) => Ok(v.clone()),
            other => Err(crate::Error::Db(format!(
                "column {i}: expected text, got {other:?}"
            ))),
        }
    }
    fn opt_text(&self, i: usize) -> Result<Option<String>> {
        match self.get(i) {
            Some(Value::Null) => Ok(None),
            Some(Value::Text(v)) => Ok(Some(v.clone())),
            other => Err(crate::Error::Db(format!(
                "column {i}: expected text or null, got {other:?}"
            ))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub sql: String,
    pub params: Vec<Value>,
}

impl Stmt {
    pub fn new(sql: impl Into<String>, params: Vec<Value>) -> Self {
        Self {
            sql: sql.into(),
            params,
        }
    }
}

/// Metadata database. `?1`, `?2` numbered parameters; SQLite dialect.
#[async_trait]
pub trait Sql: Send + Sync {
    /// Runs one statement and returns its rows (none for writes).
    async fn query(&self, sql: &str, params: Vec<Value>) -> Result<Vec<Row>>;
    /// Runs the statements as one atomic transaction.
    async fn batch(&self, stmts: Vec<Stmt>) -> Result<()>;
}

/// An upload in progress. Dropping it without `commit` discards the bytes.
#[async_trait]
pub trait BlobSink: Send {
    async fn write(&mut self, chunk: &[u8]) -> Result<()>;
    /// Makes the bytes readable as `hash` (a no-op if that content is already stored).
    async fn commit(self: Box<Self>, hash: &str) -> Result<()>;
}

pub type BlobStream = BoxStream<'static, io::Result<Bytes>>;

/// Content-addressed blob store: one object per blake3 hash.
#[async_trait]
pub trait Blobs: Send + Sync {
    async fn begin(&self) -> Result<Box<dyn BlobSink>>;
    /// The bytes `start..=end` of the blob, or all of it for `None`.
    async fn open(&self, hash: &str, range: Option<(u64, u64)>) -> Result<BlobStream>;
    async fn delete(&self, hash: &str) -> Result<()>;
}

/// Portable DDL, run at open. Pragmas (`foreign_keys`, WAL) belong to the backend: D1 always enforces
/// foreign keys, and the SQLite backend turns them on itself.
pub const SCHEMA: &[&str] = &[
    "create table if not exists nodes (
  id text primary key,
  parent text references nodes(id) on delete cascade,
  name text not null,
  kind text not null check (kind in ('file','folder')),
  mime text not null,
  size integer not null default 0,
  blob text,
  owner text not null,
  trashed_at integer,
  trashed_by_ancestor integer not null default 0,
  created_at integer not null,
  modified_at integer not null
)",
    "create index if not exists nodes_parent on nodes(parent, kind, name)",
    "create index if not exists nodes_owner on nodes(owner, modified_at)",
    "create index if not exists nodes_blob on nodes(blob)",
    "create table if not exists shares (
  node text not null references nodes(id) on delete cascade,
  principal text not null,
  role text not null check (role in ('reader','commenter','writer')),
  primary key (node, principal)
)",
    "create index if not exists shares_principal on shares(principal)",
    "create table if not exists stars (
  user text not null,
  node text not null references nodes(id) on delete cascade,
  primary key (user, node)
)",
];

/// In-memory blob store: buffers an upload until `commit`, like an object store that has no rename.
/// R2 behaves this way (an object appears whole, under a key known up front), so this stands in for it in tests.
#[derive(Clone, Default)]
pub struct MemBlobs {
    objects: Arc<Mutex<HashMap<String, Bytes>>>,
}

impl MemBlobs {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        self.lock().len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn get(&self, hash: &str) -> Option<Bytes> {
        self.lock().get(hash).cloned()
    }
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Bytes>> {
        self.objects.lock().unwrap_or_else(|e| e.into_inner())
    }
}

struct MemSink {
    store: MemBlobs,
    buf: Vec<u8>,
}

#[async_trait]
impl BlobSink for MemSink {
    async fn write(&mut self, chunk: &[u8]) -> Result<()> {
        self.buf.extend_from_slice(chunk);
        Ok(())
    }
    async fn commit(self: Box<Self>, hash: &str) -> Result<()> {
        let MemSink { store, buf } = *self;
        store
            .lock()
            .entry(hash.to_owned())
            .or_insert_with(|| Bytes::from(buf));
        Ok(())
    }
}

#[async_trait]
impl Blobs for MemBlobs {
    async fn begin(&self) -> Result<Box<dyn BlobSink>> {
        Ok(Box::new(MemSink {
            store: self.clone(),
            buf: Vec::new(),
        }))
    }
    async fn open(&self, hash: &str, range: Option<(u64, u64)>) -> Result<BlobStream> {
        let all = self.get(hash).ok_or(crate::Error::NotFound)?;
        let part = match range {
            Some((s, e)) => all.slice(s as usize..=(e as usize).min(all.len().saturating_sub(1))),
            None => all,
        };
        Ok(Box::pin(futures_util::stream::once(async move {
            Ok::<_, io::Error>(part)
        })))
    }
    async fn delete(&self, hash: &str) -> Result<()> {
        self.lock().remove(hash);
        Ok(())
    }
}
