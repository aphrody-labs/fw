//! `fw-drive`: a Google Drive style file service. Metadata lives in one SQLite file (`drive.db`); file
//! content lives in content-addressed blobs (`blobs/<2 hex>/<blake3>`), so identical uploads are stored
//! once and every blob is a plain file. Each operation checks the caller's role, inherited down the tree.

#![forbid(unsafe_code)]

use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Serialize;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(feature = "server")]
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
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
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

pub struct Drive {
    root: PathBuf,
    db: Mutex<Connection>,
    quota_limit: Option<u64>,
}

const SCHEMA: &str = "
pragma journal_mode = wal;
pragma foreign_keys = on;
create table if not exists nodes (
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
);
create index if not exists nodes_parent on nodes(parent, kind, name);
create index if not exists nodes_owner on nodes(owner, modified_at);
create index if not exists nodes_blob on nodes(blob);
create table if not exists shares (
  node text not null references nodes(id) on delete cascade,
  principal text not null,
  role text not null check (role in ('reader','commenter','writer')),
  primary key (node, principal)
);
create index if not exists shares_principal on shares(principal);
create table if not exists stars (
  user text not null,
  node text not null references nodes(id) on delete cascade,
  primary key (user, node)
);
";

const COLS: &str = "n.id, n.parent, n.name, n.kind, n.mime, n.size, n.blob, n.owner, n.trashed_at is not null or n.trashed_by_ancestor, n.created_at, n.modified_at, exists(select 1 from stars s where s.user = ?1 and s.node = n.id)";

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

fn row_node(r: &Row<'_>) -> rusqlite::Result<Node> {
    let kind: String = r.get(3)?;
    Ok(Node {
        id: r.get(0)?,
        parent: r.get(1)?,
        name: r.get(2)?,
        kind: if kind == "folder" {
            Kind::Folder
        } else {
            Kind::File
        },
        mime: r.get(4)?,
        size: r.get::<_, i64>(5)? as u64,
        hash: r.get(6)?,
        owner: r.get(7)?,
        trashed: r.get(8)?,
        created_at: r.get(9)?,
        modified_at: r.get(10)?,
        starred: r.get(11)?,
    })
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

/// Blob being written: content is hashed while it streams to a temporary file.
pub struct BlobWriter {
    tmp: PathBuf,
    file: Option<fs::File>,
    hasher: blake3::Hasher,
    size: u64,
    limit: Option<u64>,
}

impl BlobWriter {
    pub fn write_chunk(&mut self, chunk: &[u8]) -> Result<()> {
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
        self.file
            .as_mut()
            .ok_or(Error::NotFound)?
            .write_all(chunk)?;
        Ok(())
    }
}

impl Drop for BlobWriter {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.tmp);
    }
}

impl Drive {
    /// Opens (or creates) a drive in `root`: `drive.db`, `blobs/` and `tmp/`.
    pub fn open(root: impl AsRef<Path>, quota_limit: Option<u64>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join("blobs"))?;
        fs::create_dir_all(root.join("tmp"))?;
        let db = Connection::open(root.join("drive.db"))?;
        db.execute_batch(SCHEMA)?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        Ok(Self {
            root,
            db: Mutex::new(db),
            quota_limit,
        })
    }

    fn db(&self) -> MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn blob_path(&self, hash: &str) -> PathBuf {
        self.root.join("blobs").join(&hash[..2]).join(hash)
    }

    /// The user's "My Drive" folder, created on first use. Its id is stable per email.
    pub fn my_drive(&self, user: &User) -> Result<Node> {
        let id = format!(
            "root-{}",
            &blake3::hash(user.email().as_bytes()).to_hex()[..20]
        );
        let db = self.db();
        let t = now_ms();
        db.execute(
            "insert or ignore into nodes (id, parent, name, kind, mime, owner, created_at, modified_at) values (?1, null, 'My Drive', 'folder', ?2, ?3, ?4, ?4)",
            params![id, FOLDER_MIME, user.email(), t],
        )?;
        Self::fetch(&db, user, &id)?.ok_or(Error::NotFound)
    }

    fn fetch(db: &Connection, user: &User, id: &str) -> Result<Option<Node>> {
        Ok(db
            .query_row(
                &format!("select {COLS} from nodes n where n.id = ?2"),
                params![user.email(), id],
                row_node,
            )
            .optional()?)
    }

    /// The caller's role on a node: owner of the node, or the highest share on it or an ancestor.
    fn role(db: &Connection, user: &User, id: &str) -> Result<Option<Role>> {
        let mut stmt = db.prepare_cached(
            "with recursive up(id, parent, owner, depth) as (
               select id, parent, owner, 0 from nodes where id = ?1
               union all select n.id, n.parent, n.owner, up.depth + 1 from nodes n join up on n.id = up.parent where up.depth < 64)
             select (select count(*) from up where depth = 0 and owner = ?2),
                    (select max(case s.role when 'writer' then 3 when 'commenter' then 2 else 1 end)
                       from shares s join up on s.node = up.id where s.principal in (?2, '*'))",
        )?;
        let (owns, share): (i64, Option<i64>) =
            stmt.query_row(params![id, user.email()], |r| Ok((r.get(0)?, r.get(1)?)))?;
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

    fn require(db: &Connection, user: &User, id: &str, need: Role) -> Result<Node> {
        let node = Self::fetch(db, user, id)?.ok_or(Error::NotFound)?;
        match Self::role(db, user, id)? {
            None => Err(Error::NotFound),
            Some(r) if r < need => Err(Error::Forbidden(need)),
            Some(_) => Ok(node),
        }
    }

    pub fn role_of(&self, user: &User, id: &str) -> Result<Option<Role>> {
        Self::role(&self.db(), user, id)
    }

    pub fn get(&self, user: &User, id: &str) -> Result<Node> {
        Self::require(&self.db(), user, id, Role::Reader)
    }

    /// Ancestors from "My Drive" (or the highest folder the user can read) down to the node.
    pub fn path(&self, user: &User, id: &str) -> Result<Vec<Node>> {
        let db = self.db();
        let mut out = vec![Self::require(&db, user, id, Role::Reader)?];
        while let Some(parent) = out.last().and_then(|n| n.parent.clone()) {
            if out.len() > 64 || Self::role(&db, user, &parent)?.is_none() {
                break;
            }
            out.push(Self::fetch(&db, user, &parent)?.ok_or(Error::NotFound)?);
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

    pub fn list(&self, user: &User, parent: &str, sort: Sort, desc: bool) -> Result<Vec<Node>> {
        let db = self.db();
        Self::require(&db, user, parent, Role::Reader)?;
        let mut stmt = db.prepare(&format!(
            "select {COLS} from nodes n where n.parent = ?2 and n.trashed_at is null and n.trashed_by_ancestor = 0 order by {}",
            Self::order(sort, desc)
        ))?;
        let rows = stmt
            .query_map(params![user.email(), parent], row_node)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Every node the user can reach: owned, shared with them, and everything below those.
    const ACCESSIBLE: &str = "with recursive acc(id) as (
        select id from nodes where owner = ?1
        union select node from shares where principal = ?1
        union select n.id from nodes n join acc on n.parent = acc.id)";

    pub fn search(&self, user: &User, q: &Search, sort: Sort, desc: bool) -> Result<Vec<Node>> {
        let db = self.db();
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
        let mut stmt = db.prepare(&format!(
            "{} select {COLS} from nodes n where n.id in acc and n.parent is not null and n.trashed_at is null and n.trashed_by_ancestor = 0
               and n.name like ?2 escape '\\' and {kind_sql} and n.modified_at >= ?4 order by {} limit 500",
            Self::ACCESSIBLE,
            Self::order(sort, desc)
        ))?;
        let rows = stmt
            .query_map(
                params![
                    user.email(),
                    pattern,
                    kind_arg,
                    q.modified_after.unwrap_or(0)
                ],
                row_node,
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn recent(&self, user: &User) -> Result<Vec<Node>> {
        let db = self.db();
        let mut stmt = db.prepare(&format!(
            "{} select {COLS} from nodes n where n.id in acc and n.kind = 'file' and n.trashed_at is null and n.trashed_by_ancestor = 0 order by n.modified_at desc limit 100",
            Self::ACCESSIBLE
        ))?;
        let rows = stmt
            .query_map(params![user.email()], row_node)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn starred(&self, user: &User) -> Result<Vec<Node>> {
        let db = self.db();
        let mut stmt = db.prepare(&format!(
            "{} select {COLS} from nodes n join stars s on s.node = n.id and s.user = ?1 where n.id in acc and n.trashed_at is null and n.trashed_by_ancestor = 0 order by {}",
            Self::ACCESSIBLE,
            Self::order(Sort::Name, false)
        ))?;
        let rows = stmt
            .query_map(params![user.email()], row_node)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Items shared directly with the user by someone else.
    pub fn shared_with_me(&self, user: &User) -> Result<Vec<Node>> {
        let db = self.db();
        let mut stmt = db.prepare(&format!(
            "select {COLS} from nodes n join shares sh on sh.node = n.id and sh.principal = ?1 where n.owner != ?1 and n.trashed_at is null and n.trashed_by_ancestor = 0 order by n.modified_at desc"
        ))?;
        let rows = stmt
            .query_map(params![user.email()], row_node)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Items the user trashed (the top of each trashed subtree).
    pub fn trash(&self, user: &User) -> Result<Vec<Node>> {
        let db = self.db();
        let mut stmt = db.prepare(&format!("select {COLS} from nodes n where n.owner = ?1 and n.trashed_at is not null order by n.trashed_at desc"))?;
        let rows = stmt
            .query_map(params![user.email()], row_node)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn create_folder(&self, user: &User, parent: &str, name: &str) -> Result<Node> {
        let name = check_name(name)?;
        let db = self.db();
        let p = Self::require(&db, user, parent, Role::Writer)?;
        if p.kind != Kind::Folder || p.trashed {
            return Err(Error::Invalid("the parent is not an open folder".into()));
        }
        let id = new_id();
        let t = now_ms();
        db.execute(
            "insert into nodes (id, parent, name, kind, mime, owner, created_at, modified_at) values (?1, ?2, ?3, 'folder', ?4, ?5, ?6, ?6)",
            params![id, parent, name, FOLDER_MIME, p.owner, t],
        )?;
        Self::fetch(&db, user, &id)?.ok_or(Error::NotFound)
    }

    pub fn quota_used(&self, owner: &str) -> Result<u64> {
        let used: i64 = self.db().query_row(
            "select coalesce(sum(size), 0) from nodes where owner = ?1 and kind = 'file'",
            params![owner],
            |r| r.get(0),
        )?;
        Ok(used as u64)
    }

    /// Starts an upload into `parent`; the parent's owner is charged, as in Drive.
    pub fn begin_upload(&self, user: &User, parent: &str) -> Result<BlobWriter> {
        let p = {
            let db = self.db();
            Self::require(&db, user, parent, Role::Writer)?
        };
        if p.kind != Kind::Folder || p.trashed {
            return Err(Error::Invalid("the parent is not an open folder".into()));
        }
        let limit = match self.quota_limit {
            Some(limit) => Some(limit.saturating_sub(self.quota_used(&p.owner)?)),
            None => None,
        };
        let tmp = self.root.join("tmp").join(new_id());
        let file = fs::File::create(&tmp)?;
        Ok(BlobWriter {
            tmp,
            file: Some(file),
            hasher: blake3::Hasher::new(),
            size: 0,
            limit,
        })
    }

    /// Moves the finished blob into place (once per content) and records the file.
    pub fn finish_upload(
        &self,
        user: &User,
        parent: &str,
        name: &str,
        mime: Option<&str>,
        mut w: BlobWriter,
    ) -> Result<Node> {
        let name = check_name(name)?;
        if let Some(mut f) = w.file.take() {
            f.flush()?;
            f.sync_all()?;
        }
        let hash = w.hasher.finalize().to_hex().to_string();
        let dest = self.blob_path(&hash);
        if !dest.exists() {
            fs::create_dir_all(dest.parent().ok_or(Error::NotFound)?)?;
            fs::rename(&w.tmp, &dest)?;
        }
        let mime = mime
            .filter(|m| !m.is_empty() && *m != "application/octet-stream")
            .unwrap_or_else(|| mime_for(&name));
        let db = self.db();
        let p = Self::require(&db, user, parent, Role::Writer)?;
        let id = new_id();
        let t = now_ms();
        db.execute(
            "insert into nodes (id, parent, name, kind, mime, size, blob, owner, created_at, modified_at) values (?1, ?2, ?3, 'file', ?4, ?5, ?6, ?7, ?8, ?8)",
            params![id, parent, name, mime, w.size as i64, hash, p.owner, t],
        )?;
        Self::fetch(&db, user, &id)?.ok_or(Error::NotFound)
    }

    /// Whole upload from a reader (CLI and tests).
    pub fn upload(
        &self,
        user: &User,
        parent: &str,
        name: &str,
        mime: Option<&str>,
        mut content: impl Read,
    ) -> Result<Node> {
        let mut w = self.begin_upload(user, parent)?;
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            let n = content.read(&mut buf)?;
            if n == 0 {
                break;
            }
            w.write_chunk(&buf[..n])?;
        }
        self.finish_upload(user, parent, name, mime, w)
    }

    /// The node and its blob path, for downloads.
    pub fn content(&self, user: &User, id: &str) -> Result<(Node, PathBuf)> {
        let node = self.get(user, id)?;
        let hash = node
            .hash
            .clone()
            .ok_or_else(|| Error::Invalid("folders have no content".into()))?;
        let path = self.blob_path(&hash);
        Ok((node, path))
    }

    pub fn rename(&self, user: &User, id: &str, name: &str) -> Result<Node> {
        let name = check_name(name)?;
        let db = self.db();
        let n = Self::require(&db, user, id, Role::Writer)?;
        if n.parent.is_none() {
            return Err(Error::Invalid("My Drive cannot be renamed".into()));
        }
        db.execute(
            "update nodes set name = ?2, modified_at = ?3 where id = ?1",
            params![id, name, now_ms()],
        )?;
        Self::fetch(&db, user, id)?.ok_or(Error::NotFound)
    }

    pub fn move_to(&self, user: &User, id: &str, new_parent: &str) -> Result<Node> {
        let db = self.db();
        let n = Self::require(&db, user, id, Role::Writer)?;
        let target = Self::require(&db, user, new_parent, Role::Writer)?;
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
            cursor = db.query_row("select parent from nodes where id = ?1", params![c], |r| {
                r.get(0)
            })?;
        }
        db.execute(
            "update nodes set parent = ?2, modified_at = ?3 where id = ?1",
            params![id, new_parent, now_ms()],
        )?;
        Self::fetch(&db, user, id)?.ok_or(Error::NotFound)
    }

    pub fn copy(&self, user: &User, id: &str, parent: &str) -> Result<Node> {
        let db = self.db();
        let n = Self::require(&db, user, id, Role::Reader)?;
        let p = Self::require(&db, user, parent, Role::Writer)?;
        if n.kind != Kind::File {
            return Err(Error::Invalid("only files can be copied".into()));
        }
        let new = new_id();
        let t = now_ms();
        db.execute(
            "insert into nodes (id, parent, name, kind, mime, size, blob, owner, created_at, modified_at) values (?1, ?2, ?3, 'file', ?4, ?5, ?6, ?7, ?8, ?8)",
            params![new, parent, format!("Copy of {}", n.name), n.mime, n.size as i64, n.hash, p.owner, t],
        )?;
        Self::fetch(&db, user, &new)?.ok_or(Error::NotFound)
    }

    pub fn set_starred(&self, user: &User, id: &str, starred: bool) -> Result<Node> {
        let db = self.db();
        Self::require(&db, user, id, Role::Reader)?;
        if starred {
            db.execute(
                "insert or ignore into stars (user, node) values (?1, ?2)",
                params![user.email(), id],
            )?;
        } else {
            db.execute(
                "delete from stars where user = ?1 and node = ?2",
                params![user.email(), id],
            )?;
        }
        Self::fetch(&db, user, id)?.ok_or(Error::NotFound)
    }

    const SUBTREE: &str = "with recursive sub(id) as (select id from nodes where parent = ?1 union all select n.id from nodes n join sub on n.parent = sub.id)";

    pub fn trash_node(&self, user: &User, id: &str) -> Result<Node> {
        let mut db = self.db();
        let n = Self::require(&db, user, id, Role::Writer)?;
        if n.parent.is_none() {
            return Err(Error::Invalid("My Drive cannot be trashed".into()));
        }
        let tx = db.transaction()?;
        tx.execute(
            "update nodes set trashed_at = ?2 where id = ?1 and trashed_at is null",
            params![id, now_ms()],
        )?;
        tx.execute(
            &format!(
                "{} update nodes set trashed_by_ancestor = 1 where id in sub",
                Self::SUBTREE
            ),
            params![id],
        )?;
        tx.commit()?;
        Self::fetch(&db, user, id)?.ok_or(Error::NotFound)
    }

    /// Restores a trashed item; if its old folder is gone or trashed, it returns to the owner's My Drive.
    pub fn restore(&self, user: &User, id: &str) -> Result<Node> {
        let root = self.my_drive(&User::new(&self.get(user, id)?.owner)?)?;
        let mut db = self.db();
        let n = Self::require(&db, user, id, Role::Writer)?;
        let tx = db.transaction()?;
        // The node comes back either under an open parent or into My Drive, so no ancestor of it is trashed.
        tx.execute(
            "update nodes set trashed_at = null, trashed_by_ancestor = 0 where id = ?1",
            params![id],
        )?;
        tx.execute(&format!("{} update nodes set trashed_by_ancestor = 0 where id in sub and trashed_at is null", Self::SUBTREE), params![id])?;
        let parent_open: bool = match &n.parent {
            Some(p) => tx
                .query_row("select trashed_at is null and trashed_by_ancestor = 0 from nodes where id = ?1", params![p], |r| r.get(0))
                .optional()?
                .unwrap_or(false),
            None => true,
        };
        if !parent_open {
            tx.execute(
                "update nodes set parent = ?2 where id = ?1",
                params![id, root.id],
            )?;
        }
        tx.commit()?;
        Self::fetch(&db, user, id)?.ok_or(Error::NotFound)
    }

    /// Deletes a trashed item and its subtree for good, then removes blobs nothing references.
    pub fn delete_forever(&self, user: &User, id: &str) -> Result<()> {
        let mut db = self.db();
        let n = Self::require(&db, user, id, Role::Owner)?;
        if n.parent.is_none() {
            return Err(Error::Invalid("My Drive cannot be deleted".into()));
        }
        if !n.trashed {
            return Err(Error::Invalid("move the item to the trash first".into()));
        }
        let tx = db.transaction()?;
        let blobs: Vec<String> = {
            let mut stmt = tx.prepare(
                "with recursive sub(id) as (select ?1 union all select n.id from nodes n join sub on n.parent = sub.id) select distinct blob from nodes where id in sub and blob is not null",
            )?;
            stmt.query_map(params![id], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        tx.execute("delete from nodes where id = ?1", params![id])?;
        let mut orphans = Vec::new();
        for b in blobs {
            let refs: i64 = tx.query_row(
                "select count(*) from nodes where blob = ?1",
                params![b],
                |r| r.get(0),
            )?;
            if refs == 0 {
                orphans.push(b);
            }
        }
        tx.commit()?;
        drop(db);
        for b in orphans {
            let _ = fs::remove_file(self.blob_path(&b));
        }
        Ok(())
    }

    pub fn empty_trash(&self, user: &User) -> Result<usize> {
        let items = self.trash(user)?;
        for n in &items {
            self.delete_forever(user, &n.id)?;
        }
        Ok(items.len())
    }

    pub fn shares(&self, user: &User, id: &str) -> Result<Vec<Share>> {
        let db = self.db();
        Self::require(&db, user, id, Role::Reader)?;
        let mut stmt =
            db.prepare("select principal, role from shares where node = ?1 order by principal")?;
        let rows = stmt
            .query_map(params![id], |r| {
                let role: String = r.get(1)?;
                Ok(Share {
                    principal: r.get(0)?,
                    role: Role::parse(&role).unwrap_or(Role::Reader),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Grants `role` to an email, or to anyone with the link when `principal` is `*`. Owner only.
    pub fn share(&self, user: &User, id: &str, principal: &str, role: Role) -> Result<Vec<Share>> {
        if role == Role::Owner {
            return Err(Error::Invalid("ownership cannot be shared".into()));
        }
        let principal = if principal.trim() == "*" {
            "*".to_owned()
        } else {
            User::new(principal)?.0
        };
        {
            let db = self.db();
            let n = Self::require(&db, user, id, Role::Owner)?;
            if principal == n.owner {
                return Err(Error::Invalid("the owner already has access".into()));
            }
            db.execute(
                "insert into shares (node, principal, role) values (?1, ?2, ?3) on conflict (node, principal) do update set role = excluded.role",
                params![id, principal, role.as_str()],
            )?;
        }
        self.shares(user, id)
    }

    pub fn unshare(&self, user: &User, id: &str, principal: &str) -> Result<Vec<Share>> {
        {
            let db = self.db();
            Self::require(&db, user, id, Role::Owner)?;
            db.execute(
                "delete from shares where node = ?1 and principal = ?2",
                params![id, principal.trim().to_ascii_lowercase()],
            )?;
        }
        self.shares(user, id)
    }

    pub fn quota(&self, user: &User) -> Result<Quota> {
        let used = self.quota_used(user.email())?;
        let db = self.db();
        let mut stmt = db.prepare(&format!("select {COLS} from nodes n where n.owner = ?1 and n.kind = 'file' order by n.size desc limit 50"))?;
        let largest = stmt
            .query_map(params![user.email()], row_node)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Quota {
            used,
            limit: self.quota_limit,
            largest,
        })
    }
}

#[cfg(test)]
mod tests;
