//! fw-drive on Cloudflare, mounted at `/files`: metadata in D1, blobs in R2, identity from the Cloudflare
//! Access JWT (people through Google sign-in, agents through the `aphrody-agents` service token), verified
//! in Rust against the team certs before the request reaches the router.

use crate::access::{self, Kind};
use async_trait::async_trait;
use bytes::Bytes;
use fw_drive::store::{BlobSink, BlobStream, Blobs, Row, SCHEMA, Sql, Stmt, Value};
use fw_drive::web::{App, Auth, Verified, router_at};
use fw_drive::{Drive, Error, Result, User};
use futures_util::Stream;
use std::cell::Cell;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};
use tower_service::Service;
use wasm_bindgen::JsValue;
use worker::send::{SendFuture, SendWrapper};
use worker::{
    Bucket, ByteStream, Context, D1Database, D1PreparedStatement, Env, HttpRequest, Range, Request,
    Response,
};

/// R2 has no rename and needs the key before the bytes, so an upload is buffered to learn its blake3
/// hash. Isolate memory is 128 MB, and the final copy doubles the buffer.
pub const MAX_UPLOAD_BYTES: usize = 50 * 1024 * 1024;

const MOUNT: &str = "/files";

fn db_err(e: impl std::fmt::Display) -> Error {
    Error::Db(e.to_string())
}

/// D1 as the drive's metadata database (D1 is SQLite, so the schema and the SQL are the local ones).
pub struct D1Sql {
    db: SendWrapper<D1Database>,
}

impl D1Sql {
    pub fn new(db: D1Database) -> Self {
        Self {
            db: SendWrapper::new(db),
        }
    }

    fn statement(&self, sql: &str, params: Vec<Value>) -> Result<D1PreparedStatement> {
        let args: Vec<JsValue> = params
            .into_iter()
            .map(|v| match v {
                Value::Null => JsValue::NULL,
                Value::Int(i) => JsValue::from_f64(i as f64),
                Value::Text(t) => JsValue::from_str(&t),
            })
            .collect();
        self.db.prepare(sql).bind(&args).map_err(db_err)
    }
}

fn cell(v: serde_json::Value) -> Value {
    match v {
        serde_json::Value::Number(n) => {
            Value::Int(n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)).unwrap_or(0))
        }
        serde_json::Value::String(s) => Value::Text(s),
        serde_json::Value::Bool(b) => Value::Int(i64::from(b)),
        _ => Value::Null,
    }
}

#[async_trait]
impl Sql for D1Sql {
    async fn query(&self, sql: &str, params: Vec<Value>) -> Result<Vec<Row>> {
        SendFuture::new(async {
            let rows: Vec<Vec<serde_json::Value>> = self
                .statement(sql, params)?
                .raw()
                .await
                .map_err(db_err)?;
            Ok(rows
                .into_iter()
                .map(|r| r.into_iter().map(cell).collect())
                .collect())
        })
        .await
    }

    async fn batch(&self, stmts: Vec<Stmt>) -> Result<()> {
        SendFuture::new(async {
            let prepared = stmts
                .into_iter()
                .map(|s| self.statement(&s.sql, s.params))
                .collect::<Result<Vec<_>>>()?;
            self.db.batch(prepared).await.map_err(db_err)?;
            Ok(())
        })
        .await
    }
}

thread_local! {
    static SCHEMA_READY: Cell<bool> = const { Cell::new(false) };
}

/// Creates the tables on first use in this isolate (every statement is `if not exists`).
async fn ensure_schema(sql: &D1Sql) -> Result<()> {
    if SCHEMA_READY.get() {
        return Ok(());
    }
    sql.batch(SCHEMA.iter().map(|ddl| Stmt::new(*ddl, Vec::new())).collect())
        .await?;
    SCHEMA_READY.set(true);
    Ok(())
}

/// R2 as the blob store: one object per content hash under `blobs/`.
pub struct R2Blobs {
    bucket: Arc<SendWrapper<Bucket>>,
}

impl R2Blobs {
    pub fn new(bucket: Bucket) -> Self {
        Self {
            bucket: Arc::new(SendWrapper::new(bucket)),
        }
    }
}

fn key(hash: &str) -> String {
    format!("blobs/{hash}")
}

struct R2Sink {
    bucket: Arc<SendWrapper<Bucket>>,
    chunks: Vec<Bytes>,
    size: usize,
}

#[async_trait]
impl BlobSink for R2Sink {
    async fn write(&mut self, chunk: &[u8]) -> Result<()> {
        self.size += chunk.len();
        if self.size > MAX_UPLOAD_BYTES {
            return Err(Error::Invalid(format!(
                "uploads through the Worker are limited to {} MiB",
                MAX_UPLOAD_BYTES >> 20
            )));
        }
        self.chunks.push(Bytes::copy_from_slice(chunk));
        Ok(())
    }

    async fn commit(self: Box<Self>, hash: &str) -> Result<()> {
        let Self {
            bucket,
            chunks,
            size,
        } = *self;
        SendFuture::new(async {
            if bucket.head(key(hash)).await.map_err(db_err)?.is_some() {
                return Ok(());
            }
            let mut data = Vec::with_capacity(size);
            for c in &chunks {
                data.extend_from_slice(c);
            }
            bucket
                .put(key(hash), data)
                .execute()
                .await
                .map_err(db_err)?;
            Ok(())
        })
        .await
    }
}

/// The JavaScript stream of an R2 body, asserted `Send` (a Worker isolate runs one thread).
struct R2Body(Pin<Box<SendWrapper<ByteStream>>>);

impl Stream for R2Body {
    type Item = io::Result<Bytes>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        self.0.as_mut().get_mut().0.poll_next_unpin_compat(cx)
    }
}

trait PollCompat {
    fn poll_next_unpin_compat(&mut self, cx: &mut TaskContext<'_>) -> Poll<Option<io::Result<Bytes>>>;
}

impl PollCompat for ByteStream {
    fn poll_next_unpin_compat(&mut self, cx: &mut TaskContext<'_>) -> Poll<Option<io::Result<Bytes>>> {
        Pin::new(self).poll_next(cx).map(|item| {
            item.map(|r| r.map(Bytes::from).map_err(|e| io::Error::other(e.to_string())))
        })
    }
}

#[async_trait]
impl Blobs for R2Blobs {
    async fn begin(&self) -> Result<Box<dyn BlobSink>> {
        Ok(Box::new(R2Sink {
            bucket: self.bucket.clone(),
            chunks: Vec::new(),
            size: 0,
        }))
    }

    async fn open(&self, hash: &str, range: Option<(u64, u64)>) -> Result<BlobStream> {
        SendFuture::new(async {
            let mut get = self.bucket.get(key(hash));
            if let Some((start, end)) = range {
                get = get.range(Range::OffsetWithLength {
                    offset: start,
                    length: end - start + 1,
                });
            }
            let object = get
                .execute()
                .await
                .map_err(db_err)?
                .ok_or(Error::NotFound)?;
            let stream = object
                .body()
                .ok_or(Error::NotFound)?
                .stream()
                .map_err(db_err)?;
            let body: BlobStream = Box::pin(R2Body(Box::pin(SendWrapper::new(stream))));
            Ok(body)
        })
        .await
    }

    async fn delete(&self, hash: &str) -> Result<()> {
        SendFuture::new(async { self.bucket.delete(key(hash)).await.map_err(db_err) }).await
    }
}

/// `GET|POST|... /files/*`.
pub async fn handle(req: Request, env: Env, _ctx: Context) -> worker::Result<Response> {
    let who = match access::verify(&req, &env, "files").await {
        Ok(identity) => identity,
        Err(refusal) => return Ok(refusal),
    };
    let email = match who.kind {
        Kind::User => who.id,
        Kind::Service => format!("{}@service.access", who.id),
    };
    let user = match User::new(&email) {
        Ok(u) => u,
        Err(e) => return Response::error(e.to_string(), 403),
    };

    let sql = D1Sql::new(env.d1("FW_DRIVE_DB")?);
    if let Err(e) = ensure_schema(&sql).await {
        return Response::error(format!("files: database not ready: {e}"), 503);
    }
    let quota = env
        .var("FW_DRIVE_QUOTA_GB")
        .ok()
        .and_then(|v| v.to_string().parse::<f64>().ok())
        .map(|gb| (gb * 1024.0 * 1024.0 * 1024.0) as u64);
    let drive = Drive::new(
        Arc::new(sql),
        Arc::new(R2Blobs::new(env.bucket("FW_DRIVE_BLOBS")?)),
        quota,
    );
    let app = Arc::new(App {
        drive,
        auth: Auth::Verified,
    });

    let mut http_req: HttpRequest = req.try_into()?;
    http_req.extensions_mut().insert(Verified(user));
    if http_req.uri().path() == "/files/" {
        let mut parts = http_req.uri().clone().into_parts();
        parts.path_and_query = Some(
            match http_req.uri().query() {
                Some(q) => format!("{MOUNT}?{q}"),
                None => MOUNT.to_owned(),
            }
            .parse()
            .map_err(|e: http::uri::InvalidUri| worker::Error::RustError(e.to_string()))?,
        );
        *http_req.uri_mut() = http::Uri::from_parts(parts)
            .map_err(|e| worker::Error::RustError(e.to_string()))?;
    }
    let res = router_at(app, MOUNT)
        .call(http_req)
        .await
        .map_err(|never| match never {})?;
    Response::try_from(res)
}
