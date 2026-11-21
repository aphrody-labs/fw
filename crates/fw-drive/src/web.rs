//! HTTP surface of fw-drive: a JSON API under `/api/v1` for agents and apps, and HTML pages rendered
//! in Rust for people. Pages work without JavaScript; a short script only adds drag-and-drop upload.
//! Identity comes from Cloudflare Access (`Cf-Access-Authenticated-User-Email`, or the service-token
//! `common_name` in `Cf-Access-Jwt-Assertion`) when the server sits behind an Access tunnel, or from a
//! fixed owner for a local single-user drive.

use crate::{Drive, Error, Kind, Node, Role, Search, Sort, User};
use axum::body::Body;
use axum::extract::FromRequestParts;
use axum::extract::{Multipart, Path, Query, Request, State};
use axum::http::request::Parts;
use axum::http::{Extensions, HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::json;
use std::fmt::Write as _;
use std::sync::{Arc, LazyLock};

/// How callers are identified.
#[derive(Debug, Clone)]
pub enum Auth {
    /// Single-user drive: every request is this user. Bind to localhost only.
    Owner(User),
    /// Behind a Cloudflare Access application reached only through a Cloudflare Tunnel: trust the
    /// identity headers Access adds. Requests without them are refused.
    Access,
    /// Identity already verified by the host (the Worker checks the Cloudflare Access JWT signature) and
    /// placed in the request extensions as a [`Verified`]. Requests without one are refused.
    Verified,
}

/// Request headers plus extensions: what identification needs.
pub struct Ctx {
    headers: HeaderMap,
    ext: Extensions,
}

impl std::ops::Deref for Ctx {
    type Target = HeaderMap;
    fn deref(&self) -> &HeaderMap {
        &self.headers
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Ctx {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        Ok(Self {
            headers: parts.headers.clone(),
            ext: parts.extensions.clone(),
        })
    }
}

/// The caller, verified by the host before the request reaches the router.
#[derive(Debug, Clone)]
pub struct Verified(pub User);

pub struct App {
    pub drive: Drive,
    pub auth: Auth,
}

pub fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/", get(home))
        .route("/folders/{id}", get(folder_page))
        .route("/recent", get(recent_page))
        .route("/starred", get(starred_page))
        .route("/shared", get(shared_page))
        .route("/trash", get(trash_page))
        .route("/quota", get(quota_page))
        .route("/search", get(search_page))
        .route("/files/{id}", get(content_inline))
        .route("/files/{id}/download", get(content_download))
        .route("/files/{id}/details", get(details_page))
        .route("/drive.css", get(css))
        .route("/drive.js", get(js))
        .route("/folders/{id}/new-folder", post(form_new_folder))
        .route("/folders/{id}/upload", post(form_upload))
        .route("/files/{id}/action", post(form_action))
        .route("/trash/empty", post(form_empty_trash))
        .route("/api/v1/me", get(api_me))
        .route("/api/v1/files", get(api_list).post(api_upload))
        .route(
            "/api/v1/files/{id}",
            get(api_get).patch(api_patch).delete(api_delete),
        )
        .route("/api/v1/files/{id}/content", get(content_inline))
        .route("/api/v1/files/{id}/trash", post(api_trash))
        .route("/api/v1/files/{id}/restore", post(api_restore))
        .route("/api/v1/files/{id}/copy", post(api_copy))
        .route(
            "/api/v1/files/{id}/shares",
            get(api_shares).put(api_share).delete(api_unshare),
        )
        .route("/api/v1/folders", post(api_new_folder))
        .route("/api/v1/trash/empty", post(api_empty_trash))
        .route("/api/v1/quota", get(api_quota))
        .layer(middleware::from_fn(same_origin_writes))
        .with_state(app)
}

/// The same router mounted under `base` (for example `/files`): routes move under the prefix, and the
/// root-absolute links, form actions and redirects the pages emit follow them.
pub fn router_at(app: Arc<App>, base: &str) -> Router {
    let base: Arc<str> = base.trim_end_matches('/').into();
    Router::new()
        .nest(&base, router(app))
        .layer(middleware::from_fn_with_state(base, rebase))
}

async fn rebase(State(base): State<Arc<str>>, req: Request, next: Next) -> Response {
    let res = next.run(req).await;
    let (mut parts, body) = res.into_parts();
    if let Some(loc) = parts
        .headers
        .get(header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .filter(|l| l.starts_with('/') && !l.starts_with("//"))
        .and_then(|l| HeaderValue::from_str(&format!("{base}{l}")).ok())
    {
        parts.headers.insert(header::LOCATION, loc);
    }
    let html = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|t| t.starts_with("text/html"));
    if !html {
        return Response::from_parts(parts, body);
    }
    let Ok(bytes) = axum::body::to_bytes(body, 8 << 20).await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let text = String::from_utf8_lossy(&bytes);
    let mut out = String::with_capacity(text.len() + 256);
    let mut rest: &str = &text;
    while let Some((at, attr)) = ["href=\"/", "action=\"/", "src=\"/", "data-drop=\"/"]
        .iter()
        .filter_map(|a| rest.find(a).map(|i| (i, *a)))
        .min_by_key(|(i, _)| *i)
    {
        let end = at + attr.len();
        out.push_str(&rest[..end - 1]);
        // `//host/path` is protocol-relative, not a path of this site.
        if !rest[end..].starts_with('/') {
            out.push_str(&base);
        }
        out.push('/');
        rest = &rest[end..];
    }
    out.push_str(rest);
    parts.headers.remove(header::CONTENT_LENGTH);
    Response::from_parts(parts, Body::from(out))
}

/// Browser writes must come from this origin (Access cookies would otherwise allow cross-site forms).
/// Requests without `Origin` and `Sec-Fetch-Site` are non-browser clients (agents, curl) and pass.
async fn same_origin_writes(req: Request, next: Next) -> Response {
    if !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS) {
        let h = req.headers();
        let site = h.get("sec-fetch-site").and_then(|v| v.to_str().ok());
        let cross_site = matches!(site, Some("cross-site" | "same-site"));
        let origin_mismatch = match (
            h.get(header::ORIGIN).and_then(|v| v.to_str().ok()),
            h.get(header::HOST).and_then(|v| v.to_str().ok()),
        ) {
            (Some(origin), Some(host)) => {
                origin.split_once("://").map(|(_, rest)| rest) != Some(host)
            }
            _ => false,
        };
        if cross_site || origin_mismatch {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "ok": false, "error": "cross-site write refused" })),
            )
                .into_response();
        }
    }
    next.run(req).await
}

#[allow(
    clippy::result_large_err,
    reason = "the error is the HTTP response itself, returned at once by user_or_return!"
)]
fn who(app: &App, headers: &Ctx) -> Result<User, Response> {
    match &app.auth {
        Auth::Owner(u) => Ok(u.clone()),
        Auth::Verified => headers
            .ext
            .get::<Verified>()
            .map(|v| v.0.clone())
            .ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "ok": false, "error": "sign in through Cloudflare Access" })),
                )
                    .into_response()
            }),
        Auth::Access => {
            if let Some(email) = headers
                .get("cf-access-authenticated-user-email")
                .and_then(|v| v.to_str().ok())
            {
                return User::new(email).map_err(|e| failure(&e));
            }
            // Service tokens carry no email; their client id (`common_name`) names the agent.
            let claims = headers
                .get("cf-access-jwt-assertion")
                .and_then(|v| v.to_str().ok())
                .and_then(|jwt| jwt.split('.').nth(1))
                .and_then(decode_b64url)
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
            match claims
                .as_ref()
                .and_then(|c| c.get("common_name"))
                .and_then(|v| v.as_str())
            {
                Some(cn) if !cn.is_empty() => {
                    User::new(&format!("{cn}@service.access")).map_err(|e| failure(&e))
                }
                _ => Err((
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "ok": false, "error": "sign in through Cloudflare Access" })),
                )
                    .into_response()),
            }
        }
    }
}

fn decode_b64url(s: &str) -> Option<Vec<u8>> {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes().filter(|&c| c != b'=') {
        let v = T.iter().position(|&t| t == c)? as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

fn status_of(e: &Error) -> StatusCode {
    match e {
        Error::NotFound => StatusCode::NOT_FOUND,
        Error::Forbidden(_) => StatusCode::FORBIDDEN,
        Error::Invalid(_) => StatusCode::BAD_REQUEST,
        Error::QuotaExceeded { .. } => StatusCode::INSUFFICIENT_STORAGE,
        Error::Io(_) | Error::Db(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn failure(e: &Error) -> Response {
    (
        status_of(e),
        Json(json!({ "ok": false, "error": e.to_string() })),
    )
        .into_response()
}

fn page_failure(e: &Error) -> Response {
    let body = format!(
        "<h1>{}</h1><p><a href=\"/\">Back to My Drive</a></p>",
        esc(&e.to_string())
    );
    (status_of(e), Html(shell("Error", "", &body, None))).into_response()
}

macro_rules! user_or_return {
    ($app:expr, $headers:expr) => {
        match who(&$app, &$headers) {
            Ok(u) => u,
            Err(r) => return r,
        }
    };
}

// ---------- JSON API ----------

#[derive(Deserialize, Default)]
struct ListQuery {
    parent: Option<String>,
    view: Option<String>,
    q: Option<String>,
    r#type: Option<String>,
    sort: Option<String>,
    desc: Option<bool>,
}

async fn api_me(State(app): State<Arc<App>>, headers: Ctx) -> Response {
    let user = user_or_return!(app, headers);
    match app.drive.my_drive(&user).await {
        Ok(root) => Json(json!({ "ok": true, "user": user.email(), "root": root })).into_response(),
        Err(e) => failure(&e),
    }
}

async fn parent_or_root(app: &App, user: &User, parent: Option<String>) -> crate::Result<String> {
    match parent {
        Some(p) => Ok(p),
        None => Ok(app.drive.my_drive(user).await?.id),
    }
}

async fn listing(app: &App, user: &User, q: &ListQuery) -> crate::Result<Vec<Node>> {
    let sort = q.sort.as_deref().and_then(Sort::parse).unwrap_or_default();
    let desc = q.desc.unwrap_or(false);
    match q.view.as_deref() {
        Some("recent") => app.drive.recent(user).await,
        Some("starred") => app.drive.starred(user).await,
        Some("shared") => app.drive.shared_with_me(user).await,
        Some("trash") => app.drive.trash(user).await,
        Some("search") | None if q.q.is_some() || q.r#type.is_some() => {
            app.drive
                .search(
                    user,
                    &Search {
                        text: q.q.clone().unwrap_or_default(),
                        kind: q.r#type.clone().filter(|t| !t.is_empty()),
                        modified_after: None,
                    },
                    sort,
                    desc,
                )
                .await
        }
        Some(other) if other != "folder" => Err(Error::Invalid(format!("unknown view {other:?}"))),
        _ => {
            let parent = match &q.parent {
                Some(p) => p.clone(),
                None => app.drive.my_drive(user).await?.id,
            };
            app.drive.list(user, &parent, sort, desc).await
        }
    }
}

async fn api_list(
    State(app): State<Arc<App>>,
    headers: Ctx,
    Query(q): Query<ListQuery>,
) -> Response {
    let user = user_or_return!(app, headers);
    match listing(&app, &user, &q).await {
        Ok(files) => Json(json!({ "ok": true, "files": files })).into_response(),
        Err(e) => failure(&e),
    }
}

async fn api_get(State(app): State<Arc<App>>, headers: Ctx, Path(id): Path<String>) -> Response {
    let user = user_or_return!(app, headers);
    let found = async {
        let file = app.drive.get(&user, &id).await?;
        Ok::<_, Error>((app.drive.role_of(&user, &id).await?, file))
    }
    .await;
    match found {
        Ok((role, file)) => Json(json!({ "ok": true, "file": file, "role": role })).into_response(),
        Err(e) => failure(&e),
    }
}

#[derive(Deserialize)]
struct UploadQuery {
    parent: Option<String>,
    name: String,
}

/// `POST /api/v1/files?parent=<id>&name=<name>` with the raw bytes as the body (streamed to disk).
async fn api_upload(
    State(app): State<Arc<App>>,
    headers: Ctx,
    Query(q): Query<UploadQuery>,
    body: Body,
) -> Response {
    let user = user_or_return!(app, headers);
    let parent = match parent_or_root(&app, &user, q.parent).await {
        Ok(p) => p,
        Err(e) => return failure(&e),
    };
    let mime = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let mut w = match app.drive.begin_upload(&user, &parent).await {
        Ok(w) => w,
        Err(e) => return failure(&e),
    };
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => return failure(&Error::Invalid(format!("upload interrupted: {e}"))),
        };
        if let Err(e) = w.write_chunk(&chunk).await {
            return failure(&e);
        }
    }
    match app
        .drive
        .finish_upload(&user, &parent, &q.name, mime.as_deref(), w)
        .await
    {
        Ok(file) => (
            StatusCode::CREATED,
            Json(json!({ "ok": true, "file": file })),
        )
            .into_response(),
        Err(e) => failure(&e),
    }
}

#[derive(Deserialize)]
struct Patch {
    name: Option<String>,
    parent: Option<String>,
    starred: Option<bool>,
}

async fn api_patch(
    State(app): State<Arc<App>>,
    headers: Ctx,
    Path(id): Path<String>,
    Json(p): Json<Patch>,
) -> Response {
    let user = user_or_return!(app, headers);
    let result = async {
        let mut file = app.drive.get(&user, &id).await?;
        if let Some(name) = &p.name {
            file = app.drive.rename(&user, &id, name).await?;
        }
        if let Some(parent) = &p.parent {
            file = app.drive.move_to(&user, &id, parent).await?;
        }
        if let Some(starred) = p.starred {
            file = app.drive.set_starred(&user, &id, starred).await?;
        }
        Ok::<_, Error>(file)
    }
    .await;
    match result {
        Ok(file) => Json(json!({ "ok": true, "file": file })).into_response(),
        Err(e) => failure(&e),
    }
}

async fn api_delete(State(app): State<Arc<App>>, headers: Ctx, Path(id): Path<String>) -> Response {
    let user = user_or_return!(app, headers);
    match app.drive.delete_forever(&user, &id).await {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(e) => failure(&e),
    }
}

async fn api_trash(State(app): State<Arc<App>>, headers: Ctx, Path(id): Path<String>) -> Response {
    let user = user_or_return!(app, headers);
    match app.drive.trash_node(&user, &id).await {
        Ok(file) => Json(json!({ "ok": true, "file": file })).into_response(),
        Err(e) => failure(&e),
    }
}

async fn api_restore(
    State(app): State<Arc<App>>,
    headers: Ctx,
    Path(id): Path<String>,
) -> Response {
    let user = user_or_return!(app, headers);
    match app.drive.restore(&user, &id).await {
        Ok(file) => Json(json!({ "ok": true, "file": file })).into_response(),
        Err(e) => failure(&e),
    }
}

#[derive(Deserialize)]
struct CopyBody {
    parent: String,
}

async fn api_copy(
    State(app): State<Arc<App>>,
    headers: Ctx,
    Path(id): Path<String>,
    Json(b): Json<CopyBody>,
) -> Response {
    let user = user_or_return!(app, headers);
    match app.drive.copy(&user, &id, &b.parent).await {
        Ok(file) => (
            StatusCode::CREATED,
            Json(json!({ "ok": true, "file": file })),
        )
            .into_response(),
        Err(e) => failure(&e),
    }
}

#[derive(Deserialize)]
struct ShareBody {
    principal: String,
    role: Option<String>,
}

async fn api_shares(State(app): State<Arc<App>>, headers: Ctx, Path(id): Path<String>) -> Response {
    let user = user_or_return!(app, headers);
    match app.drive.shares(&user, &id).await {
        Ok(shares) => Json(json!({ "ok": true, "shares": shares })).into_response(),
        Err(e) => failure(&e),
    }
}

async fn api_share(
    State(app): State<Arc<App>>,
    headers: Ctx,
    Path(id): Path<String>,
    Json(b): Json<ShareBody>,
) -> Response {
    let user = user_or_return!(app, headers);
    let Some(role) = Role::parse(b.role.as_deref().unwrap_or("reader")) else {
        return failure(&Error::Invalid(
            "role must be reader, commenter or writer".into(),
        ));
    };
    match app.drive.share(&user, &id, &b.principal, role).await {
        Ok(shares) => Json(json!({ "ok": true, "shares": shares })).into_response(),
        Err(e) => failure(&e),
    }
}

async fn api_unshare(
    State(app): State<Arc<App>>,
    headers: Ctx,
    Path(id): Path<String>,
    Json(b): Json<ShareBody>,
) -> Response {
    let user = user_or_return!(app, headers);
    match app.drive.unshare(&user, &id, &b.principal).await {
        Ok(shares) => Json(json!({ "ok": true, "shares": shares })).into_response(),
        Err(e) => failure(&e),
    }
}

#[derive(Deserialize)]
struct NewFolder {
    parent: Option<String>,
    name: String,
}

async fn api_new_folder(
    State(app): State<Arc<App>>,
    headers: Ctx,
    Json(b): Json<NewFolder>,
) -> Response {
    let user = user_or_return!(app, headers);
    let parent = match parent_or_root(&app, &user, b.parent).await {
        Ok(p) => p,
        Err(e) => return failure(&e),
    };
    match app.drive.create_folder(&user, &parent, &b.name).await {
        Ok(folder) => (
            StatusCode::CREATED,
            Json(json!({ "ok": true, "file": folder })),
        )
            .into_response(),
        Err(e) => failure(&e),
    }
}

async fn api_empty_trash(State(app): State<Arc<App>>, headers: Ctx) -> Response {
    let user = user_or_return!(app, headers);
    match app.drive.empty_trash(&user).await {
        Ok(n) => Json(json!({ "ok": true, "deleted": n })).into_response(),
        Err(e) => failure(&e),
    }
}

async fn api_quota(State(app): State<Arc<App>>, headers: Ctx) -> Response {
    let user = user_or_return!(app, headers);
    match app.drive.quota(&user).await {
        Ok(q) => Json(json!({ "ok": true, "quota": q })).into_response(),
        Err(e) => failure(&e),
    }
}

// ---------- Content (Range, ETag) ----------

async fn content_inline(state: State<Arc<App>>, headers: Ctx, id: Path<String>) -> Response {
    content(state, headers, id, false).await
}

async fn content_download(state: State<Arc<App>>, headers: Ctx, id: Path<String>) -> Response {
    content(state, headers, id, true).await
}

/// Parses a single `bytes=a-b`, `bytes=a-` or `bytes=-n` range against `size`.
pub fn parse_range(value: &str, size: u64) -> Option<(u64, u64)> {
    let spec = value.strip_prefix("bytes=")?;
    if spec.contains(',') || size == 0 {
        return None;
    }
    let (a, b) = spec.split_once('-')?;
    let (start, end) = if a.is_empty() {
        let n: u64 = b.parse().ok()?;
        (size.saturating_sub(n), size - 1)
    } else {
        let start: u64 = a.parse().ok()?;
        let end = if b.is_empty() {
            size - 1
        } else {
            b.parse::<u64>().ok()?.min(size - 1)
        };
        (start, end)
    };
    (start <= end && start < size).then_some((start, end))
}

async fn content(
    State(app): State<Arc<App>>,
    headers: Ctx,
    Path(id): Path<String>,
    attachment: bool,
) -> Response {
    let user = user_or_return!(app, headers);
    let node = match app.drive.content(&user, &id).await {
        Ok(v) => v,
        Err(e) => return failure(&e),
    };
    let etag = format!("\"{}\"", node.hash.clone().unwrap_or_default());
    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        == Some(etag.as_str())
    {
        return (StatusCode::NOT_MODIFIED, [(header::ETAG, etag)]).into_response();
    }
    let size = node.size;
    let range = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .map(|r| parse_range(r, size));
    let (status, start, len) = match range {
        Some(None) => {
            return (
                StatusCode::RANGE_NOT_SATISFIABLE,
                [(header::CONTENT_RANGE, format!("bytes */{size}"))],
            )
                .into_response();
        }
        Some(Some((s, e))) => (StatusCode::PARTIAL_CONTENT, s, e - s + 1),
        None => (StatusCode::OK, 0, size),
    };
    let byte_range = (status == StatusCode::PARTIAL_CONTENT).then(|| (start, start + len - 1));
    let stream = match app.drive.read(&node, byte_range).await {
        Ok(s) => s,
        Err(e) => return failure(&e),
    };
    let disposition = format!(
        "{}; filename*=UTF-8''{}",
        if attachment { "attachment" } else { "inline" },
        pct(&node.name)
    );
    let mut res = Response::new(Body::from_stream(stream));
    *res.status_mut() = status;
    let h = res.headers_mut();
    let set = |h: &mut HeaderMap, k: header::HeaderName, v: String| {
        if let Ok(v) = HeaderValue::from_str(&v) {
            h.insert(k, v);
        }
    };
    set(h, header::CONTENT_TYPE, node.mime.clone());
    set(h, header::CONTENT_LENGTH, len.to_string());
    set(h, header::ETAG, etag);
    set(h, header::ACCEPT_RANGES, "bytes".into());
    set(h, header::CONTENT_DISPOSITION, disposition);
    set(h, header::CACHE_CONTROL, "private, no-cache".into());
    set(h, header::X_CONTENT_TYPE_OPTIONS, "nosniff".into());
    // Uploaded HTML or SVG must not run script on the drive's origin.
    set(h, header::CONTENT_SECURITY_POLICY, "sandbox; default-src 'none'; img-src data: blob:; media-src data: blob:; style-src 'unsafe-inline'".into());
    if status == StatusCode::PARTIAL_CONTENT {
        set(
            h,
            header::CONTENT_RANGE,
            format!("bytes {}-{}/{size}", start, start + len - 1),
        );
    }
    res
}

fn pct(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

// ---------- HTML ----------

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// fw brand tokens (colour roles from `fw-m3`, shape, motion, fonts): the generated head of `app.css`.
static TOKENS: LazyLock<&'static str> = LazyLock::new(|| {
    const APP_CSS: &str = include_str!("../../fw-proto/ui/app.css");
    APP_CSS
        .split("/* ---------- Base")
        .next()
        .unwrap_or(APP_CSS)
});

const DRIVE_CSS: &str = include_str!("drive.css");
const DRIVE_JS: &str = include_str!("drive.js");

async fn css() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=300"),
        ],
        format!("{}\n{DRIVE_CSS}", *TOKENS),
    )
        .into_response()
}

async fn js() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=300"),
        ],
        DRIVE_JS,
    )
        .into_response()
}

fn human_size(n: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", U[i])
    }
}

fn human_date(ms: i64) -> String {
    // Civil date from Unix days (Howard Hinnant's algorithm), UTC.
    let days = ms.div_euclid(86_400_000);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    const M: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!("{} {d}, {y}", M[(m - 1) as usize])
}

fn icon(n: &Node) -> &'static str {
    if n.kind == Kind::Folder {
        return "folder";
    }
    let top = n.mime.split('/').next().unwrap_or("");
    match (top, n.mime.as_str()) {
        (_, "application/pdf") => "picture_as_pdf",
        ("image", _) => "image",
        ("video", _) => "movie",
        ("audio", _) => "audio_file",
        ("text", _) => "description",
        (_, "application/zip" | "application/gzip") => "folder_zip",
        _ => "draft",
    }
}

struct Nav<'a> {
    active: &'a str,
    root: &'a str,
    user: &'a str,
    quota: Option<(u64, Option<u64>)>,
    upload_to: Option<&'a str>,
}

fn shell(title: &str, search: &str, main: &str, nav: Option<&Nav<'_>>) -> String {
    let mut side = String::new();
    if let Some(n) = nav {
        if let Some(folder) = n.upload_to {
            let _ = write!(
                side,
                r#"<details class="new"><summary class="fab"><span class="ms" aria-hidden="true">add</span>New</summary><div class="menu" role="menu">
<form method="post" action="/folders/{f}/new-folder" class="menu-form"><label for="nf">New folder</label><input id="nf" name="name" required maxlength="255" placeholder="Untitled folder"><button>Create</button></form>
<form method="post" action="/folders/{f}/upload" enctype="multipart/form-data" class="menu-form" data-upload><label for="up">File upload</label><input id="up" type="file" name="file" multiple required><button>Upload</button></form>
</div></details>"#,
                f = esc(folder)
            );
        }
        side.push_str("<ul class=\"nav\">");
        for (key, href, label, ic) in [
            (
                "drive",
                format!("/folders/{}", n.root),
                "My Drive",
                "folder_open",
            ),
            ("shared", "/shared".into(), "Shared with me", "group"),
            ("recent", "/recent".into(), "Recent", "schedule"),
            ("starred", "/starred".into(), "Starred", "star"),
            ("trash", "/trash".into(), "Trash", "delete"),
            ("quota", "/quota".into(), "Storage", "cloud"),
        ] {
            let current = if key == n.active {
                " aria-current=\"page\""
            } else {
                ""
            };
            let _ = write!(
                side,
                r#"<li><a href="{}"{current}><span class="ms" aria-hidden="true">{ic}</span>{label}</a></li>"#,
                esc(&href)
            );
        }
        side.push_str("</ul>");
        if let Some((used, limit)) = n.quota {
            match limit {
                Some(l) if l > 0 => {
                    let pct = ((used as f64 / l as f64) * 100.0).min(100.0);
                    let _ = write!(
                        side,
                        r#"<div class="quota"><progress max="100" value="{pct:.0}">{pct:.0}%</progress><p>{} of {} used</p></div>"#,
                        human_size(used),
                        human_size(l)
                    );
                }
                _ => {
                    let _ = write!(
                        side,
                        r#"<div class="quota"><p>{} used</p></div>"#,
                        human_size(used)
                    );
                }
            }
        }
        let _ = write!(side, r#"<p class="who">{}</p>"#, esc(n.user));
    }
    format!(
        r##"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{t} - Drive</title>
<link rel="preconnect" href="https://fonts.googleapis.com"><link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Google+Sans+Code:wght@400..600&family=Google+Sans+Flex:opsz,wdth,wght,ROND@6..144,25..151,1..1000,0..100&family=Material+Symbols+Rounded:opsz,wght,FILL,GRAD@20..48,100..700,0..1,-50..200&display=swap">
<link rel="stylesheet" href="/drive.css"><script src="/drive.js" defer></script></head>
<body><a class="skip" href="#main">Skip to files</a><header class="top"><a class="brand" href="/"><span class="ms" aria-hidden="true">hard_drive</span>Drive</a>
<form class="search" action="/search" role="search"><span class="ms" aria-hidden="true">search</span><label class="sr" for="q">Search in Drive</label><input id="q" name="q" value="{s}" placeholder="Search in Drive" autocomplete="off">
<label class="sr" for="type">Type</label><select id="type" name="type"><option value="">Any type</option><option value="folder">Folders</option><option value="document">Documents</option><option value="image">Images</option><option value="video">Videos</option><option value="audio">Audio</option><option value="pdf">PDFs</option></select></form></header>
<div class="layout"><nav class="side" aria-label="Drive">{side}</nav><main id="main">{main}</main></div></body></html>"##,
        t = esc(title),
        s = esc(search),
    )
}

fn table(nodes: &[Node], show_owner: bool, back: &str, trash_view: bool, empty: &str) -> String {
    if nodes.is_empty() {
        return format!(
            r#"<div class="empty"><span class="ms" aria-hidden="true">inbox</span><p>{empty}</p></div>"#
        );
    }
    let mut t = String::from(r#"<table class="files"><thead><tr><th scope="col">Name</th>"#);
    if show_owner {
        t.push_str(r#"<th scope="col">Owner</th>"#);
    }
    t.push_str(r#"<th scope="col">Modified</th><th scope="col" class="num">Size</th><th scope="col"><span class="sr">Actions</span></th></tr></thead><tbody>"#);
    for n in nodes {
        let href = if n.kind == Kind::Folder {
            format!("/folders/{}", n.id)
        } else {
            format!("/files/{}", n.id)
        };
        let star = if n.starred {
            r#"<span class="ms star" aria-label="Starred">star</span>"#
        } else {
            ""
        };
        let _ = write!(
            t,
            r#"<tr><td class="name"><span class="ms" aria-hidden="true">{ic}</span><a href="{href}"{target}>{name}</a>{star}</td>"#,
            ic = icon(n),
            href = esc(&href),
            target = if n.kind == Kind::File {
                r#" target="_blank" rel="noopener""#
            } else {
                ""
            },
            name = esc(&n.name),
        );
        if show_owner {
            let _ = write!(t, "<td>{}</td>", esc(&n.owner));
        }
        let size = if n.kind == Kind::Folder {
            "—".to_owned()
        } else {
            human_size(n.size)
        };
        let _ = write!(
            t,
            r#"<td>{}</td><td class="num">{size}</td><td class="act">"#,
            human_date(n.modified_at)
        );
        let id = esc(&n.id);
        let back = esc(back);
        if trash_view {
            let _ = write!(
                t,
                r#"<form method="post" action="/files/{id}/action"><input type="hidden" name="back" value="{back}"><button name="op" value="restore">Restore</button><button name="op" value="delete" class="danger">Delete forever</button></form>"#
            );
        } else {
            let (star_op, star_label) = if n.starred {
                ("unstar", "Remove from Starred")
            } else {
                ("star", "Add to Starred")
            };
            let _ = write!(
                t,
                r#"<details class="row-menu"><summary aria-label="Actions for {name}"><span class="ms" aria-hidden="true">more_vert</span></summary><div class="menu">
{dl}<a href="/files/{id}/details">Details and sharing</a>
<form method="post" action="/files/{id}/action"><input type="hidden" name="back" value="{back}"><button name="op" value="{star_op}">{star_label}</button></form>
<form method="post" action="/files/{id}/action" class="menu-form"><input type="hidden" name="back" value="{back}"><label for="rn-{id}">Rename</label><input id="rn-{id}" name="name" value="{name}" required maxlength="255"><button name="op" value="rename">Rename</button></form>
<form method="post" action="/files/{id}/action"><input type="hidden" name="back" value="{back}"><button name="op" value="trash">Move to trash</button></form>
</div></details>"#,
                name = esc(&n.name),
                dl = if n.kind == Kind::File {
                    format!(r#"<a href="/files/{id}/download">Download</a>"#)
                } else {
                    String::new()
                },
            );
        }
        t.push_str("</td></tr>");
    }
    t.push_str("</tbody></table>");
    t
}

async fn nav_for<'a>(
    app: &App,
    user: &'a User,
    root: &'a str,
    active: &'a str,
    upload_to: Option<&'a str>,
) -> Nav<'a> {
    let used = app.drive.quota_used(user.email()).await.unwrap_or(0);
    Nav {
        active,
        root,
        user: user.email(),
        quota: Some((used, app.drive.quota_limit())),
        upload_to,
    }
}

async fn home(State(app): State<Arc<App>>, headers: Ctx) -> Response {
    let user = user_or_return!(app, headers);
    match app.drive.my_drive(&user).await {
        Ok(root) => Redirect::to(&format!("/folders/{}", root.id)).into_response(),
        Err(e) => page_failure(&e),
    }
}

#[derive(Deserialize, Default)]
struct SortQuery {
    sort: Option<String>,
    desc: Option<bool>,
}

fn sort_links(base: &str, sort: Sort, desc: bool) -> String {
    let mut s = String::from(r#"<div class="sort" role="group" aria-label="Sort by">"#);
    for (key, label, k) in [
        ("name", "Name", Sort::Name),
        ("modified", "Modified", Sort::Modified),
        ("size", "Size", Sort::Size),
    ] {
        let active = k == sort;
        let next_desc = active && !desc;
        let arrow = if active {
            if desc { " ↓" } else { " ↑" }
        } else {
            ""
        };
        let _ = write!(
            s,
            r#"<a href="{base}?sort={key}&desc={next_desc}"{}>{label}{arrow}</a>"#,
            if active {
                r#" aria-current="true""#
            } else {
                ""
            }
        );
    }
    s.push_str("</div>");
    s
}

async fn folder_page(
    State(app): State<Arc<App>>,
    headers: Ctx,
    Path(id): Path<String>,
    Query(sq): Query<SortQuery>,
) -> Response {
    let user = user_or_return!(app, headers);
    let root = match app.drive.my_drive(&user).await {
        Ok(r) => r,
        Err(e) => return page_failure(&e),
    };
    let sort = sq.sort.as_deref().and_then(Sort::parse).unwrap_or_default();
    let desc = sq.desc.unwrap_or(false);
    let (path, files, role) = match (
        app.drive.path(&user, &id).await,
        app.drive.list(&user, &id, sort, desc).await,
        app.drive.role_of(&user, &id).await,
    ) {
        (Ok(p), Ok(f), Ok(r)) => (p, f, r),
        (Err(e), ..) | (_, Err(e), _) | (.., Err(e)) => return page_failure(&e),
    };
    let folder = path.last().cloned().unwrap_or(root.clone());
    let mut crumbs = String::from(r#"<nav class="crumbs" aria-label="Folder path"><ol>"#);
    for (i, n) in path.iter().enumerate() {
        if i + 1 == path.len() {
            let _ = write!(crumbs, r#"<li><h1>{}</h1></li>"#, esc(&n.name));
        } else {
            let _ = write!(
                crumbs,
                r#"<li><a href="/folders/{}">{}</a></li>"#,
                esc(&n.id),
                esc(&n.name)
            );
        }
    }
    crumbs.push_str("</ol></nav>");
    let back = format!("/folders/{}", folder.id);
    let writable = role.is_some_and(|r| r >= Role::Writer);
    let main = format!(
        r#"{crumbs}{sort}<section class="drop" data-drop="/folders/{fid}/upload" aria-label="Files in {fname}">{table}</section>"#,
        sort = sort_links(&back, sort, desc),
        fid = esc(&folder.id),
        fname = esc(&folder.name),
        table = table(
            &files,
            folder.owner != user.email(),
            &back,
            false,
            "Drop files here, or use New to add a file or a folder."
        ),
    );
    let active = if folder.owner == user.email() {
        "drive"
    } else {
        "shared"
    };
    Html(shell(
        &folder.name,
        "",
        &main,
        Some(
            &nav_for(
                &app,
                &user,
                &root.id,
                active,
                writable.then_some(folder.id.as_str()),
            )
            .await,
        ),
    ))
    .into_response()
}

async fn view_page(
    app: Arc<App>,
    headers: Ctx,
    active: &str,
    title: &str,
    empty: &str,
) -> Response {
    let user = user_or_return!(app, headers);
    let root = match app.drive.my_drive(&user).await {
        Ok(r) => r,
        Err(e) => return page_failure(&e),
    };
    let files = match active {
        "recent" => app.drive.recent(&user).await,
        "starred" => app.drive.starred(&user).await,
        "shared" => app.drive.shared_with_me(&user).await,
        _ => app.drive.trash(&user).await,
    };
    let files = match files {
        Ok(f) => f,
        Err(e) => return page_failure(&e),
    };
    let back = format!("/{active}");
    let mut main = format!("<h1>{}</h1>", esc(title));
    if active == "trash" && !files.is_empty() {
        main.push_str(r#"<form method="post" action="/trash/empty" class="bar"><p>Items in the trash count toward your storage until you delete them.</p><button class="danger">Empty trash</button></form>"#);
    }
    main.push_str(&table(
        &files,
        active != "trash",
        &back,
        active == "trash",
        empty,
    ));
    Html(shell(
        title,
        "",
        &main,
        Some(&nav_for(&app, &user, &root.id, active, None).await),
    ))
    .into_response()
}

async fn recent_page(State(app): State<Arc<App>>, headers: Ctx) -> Response {
    view_page(
        app,
        headers,
        "recent",
        "Recent",
        "Files you add or change appear here.",
    )
    .await
}
async fn starred_page(State(app): State<Arc<App>>, headers: Ctx) -> Response {
    view_page(
        app,
        headers,
        "starred",
        "Starred",
        "Star files and folders to find them here.",
    )
    .await
}
async fn shared_page(State(app): State<Arc<App>>, headers: Ctx) -> Response {
    view_page(
        app,
        headers,
        "shared",
        "Shared with me",
        "Files and folders others share with you appear here.",
    )
    .await
}
async fn trash_page(State(app): State<Arc<App>>, headers: Ctx) -> Response {
    view_page(app, headers, "trash", "Trash", "The trash is empty.").await
}

async fn quota_page(State(app): State<Arc<App>>, headers: Ctx) -> Response {
    let user = user_or_return!(app, headers);
    let (root, q) = match (
        app.drive.my_drive(&user).await,
        app.drive.quota(&user).await,
    ) {
        (Ok(r), Ok(q)) => (r, q),
        (Err(e), _) | (_, Err(e)) => return page_failure(&e),
    };
    let summary = match q.limit {
        Some(l) => format!("{} of {} used", human_size(q.used), human_size(l)),
        None => format!("{} used", human_size(q.used)),
    };
    let main = format!(
        "<h1>Storage</h1><p class=\"lead\">{summary}</p><h2>Largest files</h2>{}",
        table(&q.largest, false, "/quota", false, "No files yet.")
    );
    Html(shell(
        "Storage",
        "",
        &main,
        Some(&nav_for(&app, &user, &root.id, "quota", None).await),
    ))
    .into_response()
}

#[derive(Deserialize, Default)]
struct SearchQuery {
    q: Option<String>,
    r#type: Option<String>,
}

async fn search_page(
    State(app): State<Arc<App>>,
    headers: Ctx,
    Query(sq): Query<SearchQuery>,
) -> Response {
    let user = user_or_return!(app, headers);
    let root = match app.drive.my_drive(&user).await {
        Ok(r) => r,
        Err(e) => return page_failure(&e),
    };
    let text = sq.q.unwrap_or_default();
    let kind = sq.r#type.filter(|t| !t.is_empty());
    let files = match app
        .drive
        .search(
            &user,
            &Search {
                text: text.clone(),
                kind,
                modified_after: None,
            },
            Sort::Name,
            false,
        )
        .await
    {
        Ok(f) => f,
        Err(e) => return page_failure(&e),
    };
    let main = format!(
        "<h1>Search results</h1>{}",
        table(
            &files,
            true,
            "/search",
            false,
            "No files match. Check the spelling or try another type."
        )
    );
    Html(shell(
        "Search",
        &text,
        &main,
        Some(&nav_for(&app, &user, &root.id, "", None).await),
    ))
    .into_response()
}

async fn details_page(
    State(app): State<Arc<App>>,
    headers: Ctx,
    Path(id): Path<String>,
) -> Response {
    let user = user_or_return!(app, headers);
    let root = match app.drive.my_drive(&user).await {
        Ok(r) => r,
        Err(e) => return page_failure(&e),
    };
    let (node, shares, role) = match (
        app.drive.get(&user, &id).await,
        app.drive.shares(&user, &id).await,
        app.drive.role_of(&user, &id).await,
    ) {
        (Ok(n), Ok(s), Ok(r)) => (n, s, r),
        (Err(e), ..) | (_, Err(e), _) | (.., Err(e)) => return page_failure(&e),
    };
    let owner = role == Some(Role::Owner);
    let back = format!("/files/{}/details", node.id);
    let mut people = format!(
        r#"<li><span>{}</span><span class="role">Owner</span></li>"#,
        esc(&node.owner)
    );
    for s in &shares {
        let who = if s.principal == "*" {
            "Anyone with the link".to_owned()
        } else {
            esc(&s.principal)
        };
        let remove = if owner {
            format!(
                r#"<form method="post" action="/files/{id}/action"><input type="hidden" name="back" value="{b}"><input type="hidden" name="principal" value="{p}"><button name="op" value="unshare" aria-label="Remove {who}">Remove</button></form>"#,
                id = esc(&node.id),
                b = esc(&back),
                p = esc(&s.principal),
            )
        } else {
            String::new()
        };
        let _ = write!(
            people,
            r#"<li><span>{who}</span><span class="role">{}</span>{remove}</li>"#,
            s.role
        );
    }
    let share_form = if owner {
        format!(
            r#"<form method="post" action="/files/{id}/action" class="share"><input type="hidden" name="back" value="{b}"><label for="principal">Add people or <code>*</code> for anyone with the link</label><input id="principal" name="principal" required placeholder="name@example.com"><label for="role" class="sr">Role</label><select id="role" name="role"><option value="reader">Viewer</option><option value="commenter">Commenter</option><option value="writer">Editor</option></select><button name="op" value="share">Share</button></form>"#,
            id = esc(&node.id),
            b = esc(&back),
        )
    } else {
        "<p>Only the owner can change who has access.</p>".to_owned()
    };
    let open = if node.kind == Kind::Folder {
        format!("/folders/{}", node.id)
    } else {
        format!("/files/{}", node.id)
    };
    let main = format!(
        r#"<h1>{name}</h1><dl class="meta"><dt>Type</dt><dd>{mime}</dd><dt>Size</dt><dd>{size}</dd><dt>Modified</dt><dd>{modified}</dd><dt>Created</dt><dd>{created}</dd><dt>Content hash</dt><dd><code>{hash}</code></dd></dl>
<p><a class="button" href="{open}">Open</a></p><h2>Who has access</h2><ul class="people">{people}</ul>{share_form}"#,
        name = esc(&node.name),
        mime = esc(&node.mime),
        size = if node.kind == Kind::Folder {
            "—".into()
        } else {
            human_size(node.size)
        },
        modified = human_date(node.modified_at),
        created = human_date(node.created_at),
        hash = esc(node.hash.as_deref().unwrap_or("—")),
        open = esc(&open),
    );
    Html(shell(
        &node.name,
        "",
        &main,
        Some(&nav_for(&app, &user, &root.id, "", None).await),
    ))
    .into_response()
}

#[derive(Deserialize)]
struct NameForm {
    name: String,
}

async fn form_new_folder(
    State(app): State<Arc<App>>,
    headers: Ctx,
    Path(id): Path<String>,
    Form(f): Form<NameForm>,
) -> Response {
    let user = user_or_return!(app, headers);
    match app.drive.create_folder(&user, &id, &f.name).await {
        Ok(_) => Redirect::to(&format!("/folders/{id}")).into_response(),
        Err(e) => page_failure(&e),
    }
}

async fn form_upload(
    State(app): State<Arc<App>>,
    headers: Ctx,
    Path(id): Path<String>,
    mut mp: Multipart,
) -> Response {
    let user = user_or_return!(app, headers);
    loop {
        let mut field = match mp.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => return page_failure(&Error::Invalid(format!("upload: {e}"))),
        };
        let Some(name) = field
            .file_name()
            .map(str::to_owned)
            .filter(|n| !n.is_empty())
        else {
            continue;
        };
        let mime = field.content_type().map(str::to_owned);
        let mut w = match app.drive.begin_upload(&user, &id).await {
            Ok(w) => w,
            Err(e) => return page_failure(&e),
        };
        loop {
            match field.chunk().await {
                Ok(Some(c)) => {
                    if let Err(e) = w.write_chunk(&c).await {
                        return page_failure(&e);
                    }
                }
                Ok(None) => break,
                Err(e) => return page_failure(&Error::Invalid(format!("upload: {e}"))),
            }
        }
        if let Err(e) = app
            .drive
            .finish_upload(&user, &id, &name, mime.as_deref(), w)
            .await
        {
            return page_failure(&e);
        }
    }
    Redirect::to(&format!("/folders/{id}")).into_response()
}

#[derive(Deserialize)]
struct ActionForm {
    op: String,
    back: Option<String>,
    name: Option<String>,
    principal: Option<String>,
    role: Option<String>,
}

async fn form_action(
    State(app): State<Arc<App>>,
    headers: Ctx,
    Path(id): Path<String>,
    Form(f): Form<ActionForm>,
) -> Response {
    let user = user_or_return!(app, headers);
    let d = &app.drive;
    let result = match f.op.as_str() {
        "rename" => d
            .rename(&user, &id, f.name.as_deref().unwrap_or(""))
            .await
            .map(drop),
        "star" => d.set_starred(&user, &id, true).await.map(drop),
        "unstar" => d.set_starred(&user, &id, false).await.map(drop),
        "trash" => d.trash_node(&user, &id).await.map(drop),
        "restore" => d.restore(&user, &id).await.map(drop),
        "delete" => d.delete_forever(&user, &id).await,
        "share" => match Role::parse(f.role.as_deref().unwrap_or("reader")) {
            Some(role) => d
                .share(&user, &id, f.principal.as_deref().unwrap_or(""), role)
                .await
                .map(drop),
            None => Err(Error::Invalid("unknown role".into())),
        },
        "unshare" => d
            .unshare(&user, &id, f.principal.as_deref().unwrap_or(""))
            .await
            .map(drop),
        other => Err(Error::Invalid(format!("unknown action {other:?}"))),
    };
    match result {
        Ok(()) => Redirect::to(safe_back(f.back.as_deref())).into_response(),
        Err(e) => page_failure(&e),
    }
}

async fn form_empty_trash(State(app): State<Arc<App>>, headers: Ctx) -> Response {
    let user = user_or_return!(app, headers);
    match app.drive.empty_trash(&user).await {
        Ok(_) => Redirect::to("/trash").into_response(),
        Err(e) => page_failure(&e),
    }
}

/// Only same-site paths are followed after a form post.
fn safe_back(back: Option<&str>) -> &str {
    match back {
        Some(b) if b.starts_with('/') && !b.starts_with("//") && !b.contains('\\') => b,
        _ => "/",
    }
}
