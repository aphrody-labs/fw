//! `fw-drive`: a Google Drive style file service. Metadata lives in a SQLite-dialect database and file
//! content in content-addressed blobs (one per blake3 hash), so identical uploads are stored once. Each
//! operation checks the caller's role, inherited down the tree. The rules run over the [`store::Sql`] and
//! [`store::Blobs`] traits: the `sqlite` feature provides a SQLite file with plain-file blobs, and the
//! `fw-edge` Worker provides Cloudflare D1 and R2.

#![forbid(unsafe_code)]

use serde::Serialize;
use std::io::{self, Read};
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::time::{SystemTime, UNIX_EPOCH};
use store::{BlobSink, BlobStream, Blobs, RowExt, Sql, Stmt, Value};

#[cfg(feature = "sqlite")]
pub mod sqlite;
pub mod store;
#[cfg(feature = "web")]
pub mod web;

pub const FOLDER_MIME: &str = "application/vnd.fw.folder";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not found")]
    NotFound,
    #[error("you need {0} access")]
    Forbidden(Role),
    #[error("{0}")]
    Invalid(String),
    #[error("storage is full: {used} of {limit} bytes used")]
    QuotaExceeded { used: u64, limit: u64 },
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("database: {0}")]
    Db(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Reader,
    Commenter,
    Writer,
    Owner,
}

impl Role {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "reader" => Some(Self::Reader),
            "commenter" => Some(Self::Commenter),
            "writer" => Some(Self::Writer),
            "owner" => Some(Self::Owner),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reader => "reader",
            Self::Commenter => "commenter",
            Self::Writer => "writer",
            Self::Owner => "owner",
        }
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    File,
    Folder,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Node {
    pub id: String,
    pub parent: Option<String>,
    pub name: String,
    pub kind: Kind,
    pub mime: String,
    pub size: u64,
    /// blake3 of the content, also the ETag. None for folders.
    pub hash: Option<String>,
    pub owner: String,
    pub starred: bool,
    pub trashed: bool,
    pub created_at: i64,
    pub modified_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    #[default]
    Name,
    Modified,
    Size,
}

impl Sort {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "name" => Some(Self::Name),
            "modified" => Some(Self::Modified),
            "size" => Some(Self::Size),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Search {
    pub text: String,
    /// `folder`, `image`, `video`, `audio`, `pdf`, `text`, `document` or a MIME prefix.
    pub kind: Option<String>,
    pub modified_after: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Share {
    pub principal: String,
    pub role: Role,
}

#[derive(Debug, Clone, Serialize)]
pub struct Quota {
    pub used: u64,
    pub limit: Option<u64>,
    pub largest: Vec<Node>,
}

/// Who is asking. Emails are compared lower-cased; `*` is "anyone with the link".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User(String);

impl User {
    pub fn new(email: &str) -> Result<Self> {
        let e = email.trim().to_ascii_lowercase();
        if e.is_empty() || e == "*" || e.contains(char::is_whitespace) {
            return Err(Error::Invalid(format!("not a user: {email:?}")));
        }
        Ok(Self(e))
    }
    pub fn email(&self) -> &str {
        &self.0
    }
}

/// Cloudflare Workers have no `SystemTime`; `Date.now()` stands in.
#[cfg(target_arch = "wasm32")]
fn now_ms() -> i64 {
    js_sys::Date::now() as i64
}

#[cfg(not(target_arch = "wasm32"))]
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// 128 random bits from `crypto.getRandomValues`, as 32 hex digits.
#[cfg(target_arch = "wasm32")]
pub(crate) fn new_id() -> String {
    use std::fmt::Write as _;
    let crypto = js_sys::Reflect::get(&js_sys::global(), &"crypto".into())
        .expect("a Workers global has `crypto`");
    let fill: js_sys::Function = js_sys::Reflect::get(&crypto, &"getRandomValues".into())
        .expect("crypto.getRandomValues")
        .into();
    let bytes = js_sys::Uint8Array::new_with_length(16);
    fill.call1(&crypto, &bytes)
        .expect("crypto.getRandomValues fills the array");
    let mut out = String::with_capacity(32);
    for b in bytes.to_vec() {
        let _ = write!(out, "{b:02x}");
    }
    out
}

const COLS: &str = "n.id, n.parent, n.name, n.kind, n.mime, n.size, n.blob, n.owner, n.trashed_at is not null or n.trashed_by_ancestor, n.created_at, n.modified_at, exists(select 1 from stars s where s.user = ?1 and s.node = n.id)";

fn node_of(r: &store::Row) -> Result<Node> {
    Ok(Node {
        id: r.text(0)?,
        parent: r.opt_text(1)?,
        name: r.text(2)?,
        kind: if r.text(3)? == "folder" {
            Kind::Folder
        } else {
            Kind::File
        },
        mime: r.text(4)?,
        size: r.int(5)?.max(0) as u64,
        hash: r.opt_text(6)?,
        owner: r.text(7)?,
        trashed: r.int(8)? != 0,
        created_at: r.int(9)?,
        modified_at: r.int(10)?,
        starred: r.int(11)? != 0,
    })
}

fn nodes_of(rows: &[store::Row]) -> Result<Vec<Node>> {
    rows.iter().map(node_of).collect()
}

fn check_name(name: &str) -> Result<String> {
    let n = name.trim();
    if n.is_empty() || n.len() > 255 || n.contains(['/', '\0']) {
        return Err(Error::Invalid(format!(
            "invalid name {name:?}: 1 to 255 bytes, no `/`"
        )));
    }
    Ok(n.to_owned())
}

/// MIME type from the file extension for the common cases; anything else is `application/octet-stream`.
pub fn mime_for(name: &str) -> &'static str {
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "txt" | "log" => "text/plain",
        "md" => "text/markdown",
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "mjs" => "text/javascript",
        "json" => "application/json",
        "toml" => "application/toml",
        "rs" => "text/x-rust",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "ogg" | "opus" => "audio/ogg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "zip" => "application/zip",
        "gz" | "tgz" => "application/gzip",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

fn kind_filter(kind: &str) -> (String, String) {
    match kind {
        "folder" => ("n.kind = 'folder'".into(), String::new()),
        "image" | "video" | "audio" | "text" => ("n.mime like ?3".into(), format!("{kind}/%")),
        "pdf" => ("n.mime = ?3".into(), "application/pdf".into()),
        "document" => (
            "(n.mime like 'text/%' or n.mime = ?3)".into(),
            "application/pdf".into(),
        ),
        other => (
            "n.mime like ?3".into(),
            format!("{}%", other.replace(['%', '_'], "")),
        ),
    }
}

/// Blob being written: content is hashed while it streams into the blob store.
pub struct BlobWriter {
    sink: Box<dyn BlobSink>,
    hasher: blake3::Hasher,
    size: u64,
    limit: Option<u64>,
}

impl BlobWriter {
    pub async fn write_chunk(&mut self, chunk: &[u8]) -> Result<()> {
        self.size += chunk.len() as u64;
        if let Some(limit) = self.limit
            && self.size > limit
        {
            return Err(Error::QuotaExceeded {
                used: self.size,
                limit,
            });
        }
        self.hasher.update(chunk);
        self.sink.write(chunk).await
    }
}

#[derive(Clone)]
pub struct Drive {
    sql: Arc<dyn Sql>,
    blobs: Arc<dyn Blobs>,
    quota_limit: Option<u64>,
}

impl Drive {
    pub fn new(sql: Arc<dyn Sql>, blobs: Arc<dyn Blobs>, quota_limit: Option<u64>) -> Self {
        Self {
            sql,
            blobs,
            quota_limit,
        }
    }

    /// Opens (or creates) a local drive in `root`: `drive.db`, `blobs/` and `tmp/`.
    #[cfg(feature = "sqlite")]
    pub fn open(root: impl AsRef<std::path::Path>, quota_limit: Option<u64>) -> Result<Self> {
        let root = root.as_ref();
        std::fs::create_dir_all(root)?;
        Ok(Self::new(
            Arc::new(sqlite::SqliteSql::open(root.join("drive.db"))?),
            Arc::new(sqlite::FsBlobs::open(root)?),
            quota_limit,
        ))
    }

    pub fn quota_limit(&self) -> Option<u64> {
        self.quota_limit
    }

    /// The user's "My Drive" folder, created on first use. Its id is stable per email.
    pub async fn my_drive(&self, user: &User) -> Result<Node> {
        let id = format!(
            "root-{}",
            &blake3::hash(user.email().as_bytes()).to_hex()[..20]
        );
        let t = now_ms();
        self.sql
            .query(
                "insert or ignore into nodes (id, parent, name, kind, mime, owner, created_at, modified_at) values (?1, null, 'My Drive', 'folder', ?2, ?3, ?4, ?4)",
                params![id.clone(), FOLDER_MIME, user.email(), t],
            )
            .await?;
        self.fetch(user, &id).await?.ok_or(Error::NotFound)
    }

    async fn fetch(&self, user: &User, id: &str) -> Result<Option<Node>> {
        let rows = self
            .sql
            .query(
                &format!("select {COLS} from nodes n where n.id = ?2"),
                params![user.email(), id],
            )
            .await?;
        rows.first().map(node_of).transpose()
    }

    async fn nodes(&self, sql: String, params: Vec<Value>) -> Result<Vec<Node>> {
        nodes_of(&self.sql.query(&sql, params).await?)
    }

    /// The caller's role on a node: owner of the node, or the highest share on it or an ancestor.
    async fn role(&self, user: &User, id: &str) -> Result<Option<Role>> {
        let rows = self
            .sql
            .query(
                "with recursive up(id, parent, owner, depth) as (
               select id, parent, owner, 0 from nodes where id = ?1
               union all select n.id, n.parent, n.owner, up.depth + 1 from nodes n join up on n.id = up.parent where up.depth < 64)
             select (select count(*) from up where depth = 0 and owner = ?2),
                    (select max(case s.role when 'writer' then 3 when 'commenter' then 2 else 1 end)
                       from shares s join up on s.node = up.id where s.principal in (?2, '*'))",
                params![id, user.email()],
            )
            .await?;
        let row = rows.first().ok_or(Error::NotFound)?;
        let owns = row.int(0)?;
        let share = match row.get(1) {
            Some(Value::Int(s)) => Some(*s),
            _ => None,
        };
        Ok(if owns > 0 {
            Some(Role::Owner)
        } else {
            share.map(|s| match s {
                3 => Role::Writer,
                2 => Role::Commenter,
                _ => Role::Reader,
            })
        })
    }

    async fn require(&self, user: &User, id: &str, need: Role) -> Result<Node> {
        let node = self.fetch(user, id).await?.ok_or(Error::NotFound)?;
        match self.role(user, id).await? {
            None => Err(Error::NotFound),
            Some(r) if r < need => Err(Error::Forbidden(need)),
            Some(_) => Ok(node),
        }
    }

    pub async fn role_of(&self, user: &User, id: &str) -> Result<Option<Role>> {
        self.role(user, id).await
    }

    pub async fn get(&self, user: &User, id: &str) -> Result<Node> {
        self.require(user, id, Role::Reader).await
    }

    /// Ancestors from "My Drive" (or the highest folder the user can read) down to the node.
    pub async fn path(&self, user: &User, id: &str) -> Result<Vec<Node>> {
        let mut out = vec![self.require(user, id, Role::Reader).await?];
        while let Some(parent) = out.last().and_then(|n| n.parent.clone()) {
            if out.len() > 64 || self.role(user, &parent).await?.is_none() {
                break;
            }
            out.push(self.fetch(user, &parent).await?.ok_or(Error::NotFound)?);
        }
        out.reverse();
        Ok(out)
    }

    fn order(sort: Sort, desc: bool) -> String {
        let dir = if desc { "desc" } else { "asc" };
        match sort {
            Sort::Name => format!("n.kind = 'file', n.name collate nocase {dir}"),
            Sort::Modified => format!("n.kind = 'file', n.modified_at {dir}"),
            Sort::Size => format!("n.kind = 'file', n.size {dir}, n.name collate nocase"),
        }
    }

    pub async fn list(
        &self,
        user: &User,
        parent: &str,
        sort: Sort,
        desc: bool,
    ) -> Result<Vec<Node>> {
        self.require(user, parent, Role::Reader).await?;
        self.nodes(
            format!(
                "select {COLS} from nodes n where n.parent = ?2 and n.trashed_at is null and n.trashed_by_ancestor = 0 order by {}",
                Self::order(sort, desc)
            ),
            params![user.email(), parent],
        )
        .await
    }

    /// Every node the user can reach: owned, shared with them, and everything below those.
    const ACCESSIBLE: &str = "with recursive acc(id) as (
        select id from nodes where owner = ?1
        union select node from shares where principal = ?1
        union select n.id from nodes n join acc on n.parent = acc.id)";

    pub async fn search(
        &self,
        user: &User,
        q: &Search,
        sort: Sort,
        desc: bool,
    ) -> Result<Vec<Node>> {
        let pattern = format!(
            "%{}%",
            q.text
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        let (kind_sql, kind_arg) = q
            .kind
            .as_deref()
            .map(kind_filter)
            .unwrap_or_else(|| ("1".into(), String::new()));
        self.nodes(
            format!(
                "{} select {COLS} from nodes n where n.id in acc and n.parent is not null and n.trashed_at is null and n.trashed_by_ancestor = 0
               and n.name like ?2 escape '\\' and {kind_sql} and n.modified_at >= ?4 order by {} limit 500",
                Self::ACCESSIBLE,
                Self::order(sort, desc)
            ),
            params![user.email(), pattern, kind_arg, q.modified_after.unwrap_or(0)],
        )
        .await
    }

    pub async fn recent(&self, user: &User) -> Result<Vec<Node>> {
        self.nodes(
            format!(
                "{} select {COLS} from nodes n where n.id in acc and n.kind = 'file' and n.trashed_at is null and n.trashed_by_ancestor = 0 order by n.modified_at desc limit 100",
                Self::ACCESSIBLE
            ),
            params![user.email()],
        )
        .await
    }

    pub async fn starred(&self, user: &User) -> Result<Vec<Node>> {
        self.nodes(
            format!(
                "{} select {COLS} from nodes n join stars s on s.node = n.id and s.user = ?1 where n.id in acc and n.trashed_at is null and n.trashed_by_ancestor = 0 order by {}",
                Self::ACCESSIBLE,
                Self::order(Sort::Name, false)
            ),
            params![user.email()],
        )
        .await
    }

    /// Items shared directly with the user by someone else.
    pub async fn shared_with_me(&self, user: &User) -> Result<Vec<Node>> {
        self.nodes(
            format!("select {COLS} from nodes n join shares sh on sh.node = n.id and sh.principal = ?1 where n.owner != ?1 and n.trashed_at is null and n.trashed_by_ancestor = 0 order by n.modified_at desc"),
            params![user.email()],
        )
        .await
    }

    /// Items the user trashed (the top of each trashed subtree).
    pub async fn trash(&self, user: &User) -> Result<Vec<Node>> {
        self.nodes(
            format!("select {COLS} from nodes n where n.owner = ?1 and n.trashed_at is not null order by n.trashed_at desc"),
            params![user.email()],
        )
        .await
    }

    pub async fn create_folder(&self, user: &User, parent: &str, name: &str) -> Result<Node> {
        let name = check_name(name)?;
        let p = self.require(user, parent, Role::Writer).await?;
        if p.kind != Kind::Folder || p.trashed {
            return Err(Error::Invalid("the parent is not an open folder".into()));
        }
        let id = new_id();
        let t = now_ms();
        self.sql
            .query(
                "insert into nodes (id, parent, name, kind, mime, owner, created_at, modified_at) values (?1, ?2, ?3, 'folder', ?4, ?5, ?6, ?6)",
                params![id.clone(), parent, name, FOLDER_MIME, p.owner, t],
            )
            .await?;
        self.fetch(user, &id).await?.ok_or(Error::NotFound)
    }

    pub async fn quota_used(&self, owner: &str) -> Result<u64> {
        let rows = self
            .sql
            .query(
                "select coalesce(sum(size), 0) from nodes where owner = ?1 and kind = 'file'",
                params![owner],
            )
            .await?;
        Ok(rows.first().map_or(Ok(0), |r| r.int(0))?.max(0) as u64)
    }

    /// Starts an upload into `parent`; the parent's owner is charged, as in Drive.
    pub async fn begin_upload(&self, user: &User, parent: &str) -> Result<BlobWriter> {
        let p = self.require(user, parent, Role::Writer).await?;
        if p.kind != Kind::Folder || p.trashed {
            return Err(Error::Invalid("the parent is not an open folder".into()));
        }
        let limit = match self.quota_limit {
            Some(limit) => Some(limit.saturating_sub(self.quota_used(&p.owner).await?)),
            None => None,
        };
        Ok(BlobWriter {
            sink: self.blobs.begin().await?,
            hasher: blake3::Hasher::new(),
            size: 0,
            limit,
        })
    }

    /// Stores the finished blob (once per content) and records the file.
    pub async fn finish_upload(
        &self,
        user: &User,
        parent: &str,
        name: &str,
        mime: Option<&str>,
        w: BlobWriter,
    ) -> Result<Node> {
        let name = check_name(name)?;
        let p = self.require(user, parent, Role::Writer).await?;
        let hash = w.hasher.finalize().to_hex().to_string();
        w.sink.commit(&hash).await?;
        let mime = mime
            .filter(|m| !m.is_empty() && *m != "application/octet-stream")
            .unwrap_or_else(|| mime_for(&name));
        let id = new_id();
        let t = now_ms();
        self.sql
            .query(
                "insert into nodes (id, parent, name, kind, mime, size, blob, owner, created_at, modified_at) values (?1, ?2, ?3, 'file', ?4, ?5, ?6, ?7, ?8, ?8)",
                params![id.clone(), parent, name, mime, w.size, hash, p.owner, t],
            )
            .await?;
        self.fetch(user, &id).await?.ok_or(Error::NotFound)
    }

    /// Whole upload from a reader (CLI and tests).
    pub async fn upload(
        &self,
        user: &User,
        parent: &str,
        name: &str,
        mime: Option<&str>,
        mut content: impl Read + Send,
    ) -> Result<Node> {
        let mut w = self.begin_upload(user, parent).await?;
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            let n = content.read(&mut buf)?;
            if n == 0 {
                break;
            }
            w.write_chunk(&buf[..n]).await?;
        }
        self.finish_upload(user, parent, name, mime, w).await
    }

    /// The file node, for downloads (folders have no content).
    pub async fn content(&self, user: &User, id: &str) -> Result<Node> {
        let node = self.get(user, id).await?;
        if node.hash.is_none() {
            return Err(Error::Invalid("folders have no content".into()));
        }
        Ok(node)
    }

    /// The bytes `start..=end` of a file's blob, or all of it.
    pub async fn read(&self, node: &Node, range: Option<(u64, u64)>) -> Result<BlobStream> {
        let hash = node
            .hash
            .as_deref()
            .ok_or_else(|| Error::Invalid("folders have no content".into()))?;
        self.blobs.open(hash, range).await
    }

    pub async fn rename(&self, user: &User, id: &str, name: &str) -> Result<Node> {
        let name = check_name(name)?;
        let n = self.require(user, id, Role::Writer).await?;
        if n.parent.is_none() {
            return Err(Error::Invalid("My Drive cannot be renamed".into()));
        }
        self.sql
            .query(
                "update nodes set name = ?2, modified_at = ?3 where id = ?1",
                params![id, name, now_ms()],
            )
            .await?;
        self.fetch(user, id).await?.ok_or(Error::NotFound)
    }

    pub async fn move_to(&self, user: &User, id: &str, new_parent: &str) -> Result<Node> {
        let n = self.require(user, id, Role::Writer).await?;
        let target = self.require(user, new_parent, Role::Writer).await?;
        if n.parent.is_none() {
            return Err(Error::Invalid("My Drive cannot be moved".into()));
        }
        if target.kind != Kind::Folder || target.trashed {
            return Err(Error::Invalid("the target is not an open folder".into()));
        }
        let mut cursor = Some(target.id.clone());
        while let Some(c) = cursor {
            if c == n.id {
                return Err(Error::Invalid("a folder cannot move into itself".into()));
            }
            let rows = self
                .sql
                .query("select parent from nodes where id = ?1", params![c])
                .await?;
            cursor = match rows.first() {
                Some(r) => r.opt_text(0)?,
                None => None,
            };
        }
        self.sql
            .query(
                "update nodes set parent = ?2, modified_at = ?3 where id = ?1",
                params![id, new_parent, now_ms()],
            )
            .await?;
        self.fetch(user, id).await?.ok_or(Error::NotFound)
    }

    pub async fn copy(&self, user: &User, id: &str, parent: &str) -> Result<Node> {
        let n = self.require(user, id, Role::Reader).await?;
        let p = self.require(user, parent, Role::Writer).await?;
        if n.kind != Kind::File {
            return Err(Error::Invalid("only files can be copied".into()));
        }
        let new = new_id();
        let t = now_ms();
        self.sql
            .query(
                "insert into nodes (id, parent, name, kind, mime, size, blob, owner, created_at, modified_at) values (?1, ?2, ?3, 'file', ?4, ?5, ?6, ?7, ?8, ?8)",
                params![new.clone(), parent, format!("Copy of {}", n.name), n.mime, n.size, n.hash, p.owner, t],
            )
            .await?;
        self.fetch(user, &new).await?.ok_or(Error::NotFound)
    }

    pub async fn set_starred(&self, user: &User, id: &str, starred: bool) -> Result<Node> {
        self.require(user, id, Role::Reader).await?;
        if starred {
            self.sql
                .query(
                    "insert or ignore into stars (user, node) values (?1, ?2)",
                    params![user.email(), id],
                )
                .await?;
        } else {
            self.sql
                .query(
                    "delete from stars where user = ?1 and node = ?2",
                    params![user.email(), id],
                )
                .await?;
        }
        self.fetch(user, id).await?.ok_or(Error::NotFound)
    }

    const SUBTREE: &str = "with recursive sub(id) as (select id from nodes where parent = ?1 union all select n.id from nodes n join sub on n.parent = sub.id)";

    pub async fn trash_node(&self, user: &User, id: &str) -> Result<Node> {
        let n = self.require(user, id, Role::Writer).await?;
        if n.parent.is_none() {
            return Err(Error::Invalid("My Drive cannot be trashed".into()));
        }
        self.sql
            .batch(vec![
                Stmt::new(
                    "update nodes set trashed_at = ?2 where id = ?1 and trashed_at is null",
                    params![id, now_ms()],
                ),
                Stmt::new(
                    format!(
                        "{} update nodes set trashed_by_ancestor = 1 where id in sub",
                        Self::SUBTREE
                    ),
                    params![id],
                ),
            ])
            .await?;
        self.fetch(user, id).await?.ok_or(Error::NotFound)
    }

    /// Restores a trashed item; if its old folder is gone or trashed, it returns to the owner's My Drive.
    pub async fn restore(&self, user: &User, id: &str) -> Result<Node> {
        let root = self
            .my_drive(&User::new(&self.get(user, id).await?.owner)?)
            .await?;
        let n = self.require(user, id, Role::Writer).await?;
        // The node comes back either under an open parent or into My Drive, so no ancestor of it is trashed.
        let parent_open = match &n.parent {
            Some(p) => {
                let rows = self
                    .sql
                    .query(
                        "select trashed_at is null and trashed_by_ancestor = 0 from nodes where id = ?1",
                        params![p],
                    )
                    .await?;
                match rows.first() {
                    Some(r) => r.int(0)? != 0,
                    None => false,
                }
            }
            None => true,
        };
        let mut stmts = vec![
            Stmt::new(
                "update nodes set trashed_at = null, trashed_by_ancestor = 0 where id = ?1",
                params![id],
            ),
            Stmt::new(
                format!(
                    "{} update nodes set trashed_by_ancestor = 0 where id in sub and trashed_at is null",
                    Self::SUBTREE
                ),
                params![id],
            ),
        ];
        if !parent_open {
            stmts.push(Stmt::new(
                "update nodes set parent = ?2 where id = ?1",
                params![id, root.id],
            ));
        }
        self.sql.batch(stmts).await?;
        self.fetch(user, id).await?.ok_or(Error::NotFound)
    }

    /// Deletes a trashed item and its subtree for good, then removes blobs nothing references.
    pub async fn delete_forever(&self, user: &User, id: &str) -> Result<()> {
        let n = self.require(user, id, Role::Owner).await?;
        if n.parent.is_none() {
            return Err(Error::Invalid("My Drive cannot be deleted".into()));
        }
        if !n.trashed {
            return Err(Error::Invalid("move the item to the trash first".into()));
        }
        let blobs: Vec<String> = self
            .sql
            .query(
                "with recursive sub(id) as (select ?1 union all select n.id from nodes n join sub on n.parent = sub.id) select distinct blob from nodes where id in sub and blob is not null",
                params![id],
            )
            .await?
            .iter()
            .map(|r| r.text(0))
            .collect::<Result<_>>()?;
        self.sql
            .batch(vec![Stmt::new(
                "delete from nodes where id = ?1",
                params![id],
            )])
            .await?;
        for b in blobs {
            let refs = self
                .sql
                .query(
                    "select count(*) from nodes where blob = ?1",
                    params![b.clone()],
                )
                .await?
                .first()
                .map_or(Ok(0), |r| r.int(0))?;
            if refs == 0 {
                self.blobs.delete(&b).await?;
            }
        }
        Ok(())
    }

    pub async fn empty_trash(&self, user: &User) -> Result<usize> {
        let items = self.trash(user).await?;
        for n in &items {
            self.delete_forever(user, &n.id).await?;
        }
        Ok(items.len())
    }

    pub async fn shares(&self, user: &User, id: &str) -> Result<Vec<Share>> {
        self.require(user, id, Role::Reader).await?;
        self.sql
            .query(
                "select principal, role from shares where node = ?1 order by principal",
                params![id],
            )
            .await?
            .iter()
            .map(|r| {
                Ok(Share {
                    principal: r.text(0)?,
                    role: Role::parse(&r.text(1)?).unwrap_or(Role::Reader),
                })
            })
            .collect()
    }

    /// Grants `role` to an email, or to anyone with the link when `principal` is `*`. Owner only.
    pub async fn share(
        &self,
        user: &User,
        id: &str,
        principal: &str,
        role: Role,
    ) -> Result<Vec<Share>> {
        if role == Role::Owner {
            return Err(Error::Invalid("ownership cannot be shared".into()));
        }
        let principal = if principal.trim() == "*" {
            "*".to_owned()
        } else {
            User::new(principal)?.0
        };
        let n = self.require(user, id, Role::Owner).await?;
        if principal == n.owner {
            return Err(Error::Invalid("the owner already has access".into()));
        }
        self.sql
            .query(
                "insert into shares (node, principal, role) values (?1, ?2, ?3) on conflict (node, principal) do update set role = excluded.role",
                params![id, principal, role.as_str()],
            )
            .await?;
        self.shares(user, id).await
    }

    pub async fn unshare(&self, user: &User, id: &str, principal: &str) -> Result<Vec<Share>> {
        self.require(user, id, Role::Owner).await?;
        self.sql
            .query(
                "delete from shares where node = ?1 and principal = ?2",
                params![id, principal.trim().to_ascii_lowercase()],
            )
            .await?;
        self.shares(user, id).await
    }

    pub async fn quota(&self, user: &User) -> Result<Quota> {
        let used = self.quota_used(user.email()).await?;
        let largest = self
            .nodes(
                format!("select {COLS} from nodes n where n.owner = ?1 and n.kind = 'file' order by n.size desc limit 50"),
                params![user.email()],
            )
            .await?;
        Ok(Quota {
            used,
            limit: self.quota_limit,
            largest,
        })
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod tests;
