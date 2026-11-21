//! HTTP/1.1 listener for `fw serve`: reads a whole request (head plus a
//! `Content-Length` body), runs the blocking router off the async workers, and
//! answers with one `Connection: close` response.

use crate::dispatch_http;
use crate::mcp::{self, Cancel, Catalog, Connection, Engine, HttpMeta, RpcError, StreamControl};
use crate::mcp_catalog::FwCatalog;
use fw_cluster::HybridClusterRouter;
use serde_json::Value;
use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;

/// The MCP endpoint of the control plane.
const MCP_PATH: &str = "/mcp";
/// Idle time after which an open event stream gets an SSE comment line, so proxies do not drop it.
const KEEP_ALIVE: Duration = Duration::from_secs(15);

/// Largest request head (request line plus headers).
const MAX_HEAD_BYTES: usize = 64 * 1024;
/// Largest request body; long chat histories with code stay far below it.
const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
    }
}

/// Most sessions of the earlier protocol versions kept at once; opening one more drops the oldest.
const MAX_SESSIONS: usize = 1024;
/// Reconnection delay a resumable stream asks the client to wait, in milliseconds.
const STREAM_RETRY_MS: u32 = 1000;

/// Everything a connection handler shares: the cluster router, the MCP engine, its host policy and its sessions.
struct Shared {
    router: Arc<Mutex<HybridClusterRouter>>,
    engine: Arc<Engine>,
    hosts: HostPolicy,
    sessions: Sessions,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One session of the earlier protocol versions (`Mcp-Session-Id`): the connection state `initialize` set up and the
/// response streams of its requests, kept until they are delivered so a client that lost one can resume it.
struct Session {
    connection: Arc<Connection>,
    streams: Mutex<HashMap<String, Arc<StreamLog>>>,
    next_stream: AtomicU64,
    opened: u64,
}

#[derive(Default)]
struct Sessions {
    open: Mutex<HashMap<String, Arc<Session>>>,
    counter: AtomicU64,
}

impl Sessions {
    fn fresh_id(&self) -> (String, u64) {
        let serial = self.counter.fetch_add(1, Ordering::SeqCst);
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_nanos());
        let id = format!("{:016x}{:016x}", RandomState::new().hash_one((nanos, serial, 1u8)), RandomState::new().hash_one((nanos, serial, 2u8)));
        (id, serial)
    }

    fn insert(&self, id: String, opened: u64, connection: Arc<Connection>) {
        let mut open = lock(&self.open);
        if open.len() >= MAX_SESSIONS
            && let Some(oldest) = open.iter().min_by_key(|(_, session)| session.opened).map(|(id, _)| id.clone())
        {
            open.remove(&oldest);
        }
        open.insert(id, Arc::new(Session { connection, streams: Mutex::default(), next_stream: AtomicU64::new(1), opened }));
    }

    fn get(&self, id: &str) -> Option<Arc<Session>> {
        lock(&self.open).get(id).cloned()
    }

    fn close(&self, id: &str) -> bool {
        lock(&self.open).remove(id).is_some()
    }
}

/// Everything one request sends to its client, in order, until its answer: the notifications and server requests,
/// then the response. A stream writer reads it from any index, which is what resumption with `Last-Event-ID` needs.
#[derive(Default)]
struct StreamLog {
    state: Mutex<LogState>,
    changed: Notify,
}

#[derive(Default)]
struct LogState {
    messages: Vec<Value>,
    /// Messages before the response.
    notifications: usize,
    status: u16,
    done: bool,
    close_requested: bool,
}

impl StreamLog {
    fn push(&self, message: Value) {
        let mut state = lock(&self.state);
        state.messages.push(message);
        state.notifications += 1;
        drop(state);
        self.changed.notify_one();
    }

    fn finish(&self, reply: mcp::Reply) {
        let mut state = lock(&self.state);
        state.status = reply.status;
        state.messages.extend(reply.body);
        state.done = true;
        drop(state);
        self.changed.notify_one();
    }
}

impl StreamControl for StreamLog {
    fn close(&self) {
        lock(&self.state).close_requested = true;
        self.changed.notify_one();
    }
}

/// DNS-rebinding defence for `/mcp`: the `Host` and `Origin` headers must name this server.
struct HostPolicy {
    /// Lower-case host names a request may use; `None` allows every name (a non-loopback bind with no list configured).
    allowed: Option<Vec<String>>,
}

impl HostPolicy {
    fn for_bind(addr: SocketAddr) -> Self {
        if addr.ip().is_loopback() {
            return Self { allowed: Some(["localhost", "127.0.0.1", "[::1]"].map(str::to_owned).to_vec()) };
        }
        let configured = std::env::var("FW_MCP_ALLOWED_HOSTS").unwrap_or_default();
        let mut names: Vec<String> = configured.split(',').map(|name| name.trim().to_ascii_lowercase()).filter(|name| !name.is_empty()).collect();
        if names.is_empty() {
            return Self { allowed: None };
        }
        if !addr.ip().is_unspecified() {
            names.push(host_label(addr.ip()));
        }
        Self { allowed: Some(names) }
    }

    fn permits(&self, request: &Request) -> bool {
        let Some(allowed) = &self.allowed else { return true };
        let name_of = |value: &str| {
            let authority = value.split("://").last().unwrap_or(value).split('/').next().unwrap_or_default();
            let host = if authority.starts_with('[') { authority.split_once(']').map_or(authority, |(head, _)| &authority[..=head.len()]) } else { authority.split(':').next().unwrap_or_default() };
            host.to_ascii_lowercase()
        };
        [request.header("host"), request.header("origin")].into_iter().flatten().all(|value| allowed.contains(&name_of(value)))
    }
}

fn host_label(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => format!("[{v6}]"),
    }
}

enum ReadError {
    Closed,
    Reject(u16, &'static str),
}

fn find_head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
}

async fn read_request(stream: &mut TcpStream) -> Result<Request, ReadError> {
    let mut buf = Vec::with_capacity(8192);
    let mut chunk = [0u8; 8192];
    let head_end = loop {
        if let Some(end) = find_head_end(&buf) {
            break end;
        }
        if buf.len() > MAX_HEAD_BYTES {
            return Err(ReadError::Reject(431, "request head larger than 64 KiB"));
        }
        match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => return Err(ReadError::Closed),
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    };

    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next().unwrap_or_default().split_whitespace();
    let (Some(method), Some(target)) = (first.next(), first.next()) else {
        return Err(ReadError::Reject(400, "malformed request line"));
    };
    // The query string stays: `/v1/crates/*` reads its arguments from it.
    let path = target.to_owned();

    let mut content_length = 0usize;
    let mut headers = Vec::new();
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { continue };
        headers.push((name.trim().to_owned(), value.trim().to_owned()));
        if name.eq_ignore_ascii_case("content-length") {
            content_length = value
                .trim()
                .parse()
                .map_err(|_| ReadError::Reject(400, "invalid Content-Length"))?;
        } else if name.eq_ignore_ascii_case("transfer-encoding") && !value.trim().eq_ignore_ascii_case("identity") {
            return Err(ReadError::Reject(411, "chunked request bodies are not supported; send Content-Length"));
        }
    }
    if content_length > MAX_BODY_BYTES {
        return Err(ReadError::Reject(413, "request body larger than 16 MiB"));
    }

    let mut body = buf.split_off(head_end);
    while body.len() < content_length {
        match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => return Err(ReadError::Closed),
            Ok(n) => body.extend_from_slice(&chunk[..n]),
        }
    }
    body.truncate(content_length);
    let body = String::from_utf8(body).map_err(|_| ReadError::Reject(400, "request body is not UTF-8"))?;
    Ok(Request { method: method.to_owned(), path, headers, body })
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        202 => "Accepted",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        411 => "Length Required",
        413 => "Content Too Large",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "Status",
    }
}

/// The UI loads its fonts (Google Sans Flex and Code, tools/material-cdn) from the Aphrody CDN; everything else is same-origin.
fn content_security_policy(path: &str) -> &'static str {
    let path = path.split('?').next().unwrap_or(path);
    if path == "/webgpu" {
        "default-src 'self'; script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'"
    } else {
        "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline' https://material.aphrody.com; font-src 'self' https://material.aphrody.com; img-src 'self' data:"
    }
}

async fn write_response(stream: &mut TcpStream, status: u16, content_type: &str, path: &str, payload: &str) {
    write_response_with(stream, status, content_type, path, payload, "").await;
}

/// `extra` holds whole header lines, each ending in CRLF.
async fn write_response_with(stream: &mut TcpStream, status: u16, content_type: &str, path: &str, payload: &str, extra: &str) {
    let head = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: {content_type}\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: strict-origin-when-cross-origin\r\nContent-Security-Policy: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Expose-Headers: Mcp-Session-Id, MCP-Protocol-Version\r\nCache-Control: no-store\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n",
        reason(status),
        content_security_policy(path),
        payload.len()
    );
    let _ = stream.write_all(head.as_bytes()).await;
    let _ = stream.write_all(payload.as_bytes()).await;
    let _ = stream.shutdown().await;
}

async fn handle_conn(mut stream: TcpStream, shared: Arc<Shared>) {
    let req = match read_request(&mut stream).await {
        Ok(req) => req,
        Err(ReadError::Closed) => return,
        Err(ReadError::Reject(status, message)) => {
            let body = serde_json::json!({ "ok": false, "error": message }).to_string();
            write_response(&mut stream, status, "application/json", "", &body).await;
            return;
        }
    };
    if req.method == "OPTIONS" {
        let head = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type, Accept, Authorization, MCP-Protocol-Version, Mcp-Method, Mcp-Name, Mcp-Session-Id, Last-Event-ID, *\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let _ = stream.write_all(head.as_bytes()).await;
        return;
    }
    if req.path.split('?').next() == Some(MCP_PATH) {
        serve_mcp(&mut stream, req, &shared).await;
        return;
    }
    // Routes call model gateways and scan the workspace with blocking I/O.
    let path = req.path.clone();
    let router = shared.router.clone();
    let result = tokio::task::spawn_blocking(move || dispatch_http(&router, &req.method, &req.path, &req.body)).await;
    let (status, content_type, payload) = result.unwrap_or_else(|e| {
        (500, "application/json", serde_json::json!({ "ok": false, "error": format!("route panicked: {e}") }).to_string())
    });
    write_response(&mut stream, status, content_type, &path, &payload).await;
}

async fn write_json_rpc_error(stream: &mut TcpStream, error: &RpcError) {
    let body = serde_json::json!({ "jsonrpc": "2.0", "id": null, "error": { "code": error.code, "message": error.message } }).to_string();
    write_response(stream, error.status, "application/json", MCP_PATH, &body).await;
}

fn unknown_session() -> RpcError {
    RpcError::new(-32001, "Session not found: it ended or never existed; send initialize again").with_status(404)
}

fn sse_event(id: Option<&str>, message: &Value) -> String {
    match id {
        Some(id) => format!("id: {id}\nevent: message\ndata: {message}\n\n"),
        None => format!("event: message\ndata: {message}\n\n"),
    }
}

async fn write_method_not_allowed(stream: &mut TcpStream) {
    let body = r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"POST a JSON-RPC message"}}"#;
    let head = format!("HTTP/1.1 405 Method Not Allowed\r\nAllow: POST, OPTIONS\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
    if stream.write_all(head.as_bytes()).await.is_ok() {
        // The client may already be gone; there is nobody to tell.
        stream.write_all(body.as_bytes()).await.ok();
    }
}

/// How a stream writer ended.
enum Pumped {
    /// The response was written.
    Finished,
    /// The stream was closed on purpose before the response; the client resumes it.
    Detached,
    /// The client went away.
    Gone,
}

/// What a stream writer needs to know about the stream it serves.
struct Pump<'a> {
    log: &'a StreamLog,
    /// Index of the next message to write.
    from: usize,
    /// Event ids are `<stream>-<n>` for streams a client can resume; `None` for the others.
    ids: Option<&'a str>,
    /// `Mcp-Session-Id` to add to a successful plain JSON answer (the response to `initialize`).
    session_header: Option<&'a str>,
    /// Cancelled when the client closes the connection; `None` when the request outlives its stream.
    cancel_on_gone: Option<&'a Cancel>,
    /// A resumed stream is always an event stream.
    resumed: bool,
}

/// Writes the messages of one request to its connection: a plain JSON answer when the request produced nothing but
/// its response, otherwise an event stream that ends with the response (or earlier, when the request closes it).
async fn pump(stream: &mut TcpStream, pump: Pump<'_>) -> Pumped {
    let Pump { log, from, ids, session_header, cancel_on_gone, resumed } = pump;
    let mut sent = from;
    let mut streaming = false;
    let mut keep_alive = tokio::time::interval(KEEP_ALIVE);
    keep_alive.tick().await;
    let mut scratch = [0u8; 256];
    let gone = || {
        if let Some(cancel) = cancel_on_gone {
            cancel.cancel();
        }
        Pumped::Gone
    };
    loop {
        let (batch, done, status, notifications, close) = {
            let mut state = lock(&log.state);
            (state.messages.get(sent..).unwrap_or_default().to_vec(), state.done, state.status, state.notifications, std::mem::take(&mut state.close_requested))
        };
        if !streaming && !resumed && done && notifications == 0 {
            match batch.first() {
                Some(body) => {
                    let extra = session_header.filter(|_| status == 200 && body.get("result").is_some()).map(|id| format!("Mcp-Session-Id: {id}\r\n")).unwrap_or_default();
                    write_response_with(stream, status, "application/json", MCP_PATH, &body.to_string(), &extra).await;
                }
                None if status == 202 => write_response(stream, 202, "application/json", MCP_PATH, "").await,
                None => {}
            }
            stream.shutdown().await.ok();
            return Pumped::Finished;
        }
        if !streaming && (resumed || !batch.is_empty() || close) {
            streaming = true;
            let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache, no-transform\r\nX-Accel-Buffering: no\r\nX-Content-Type-Options: nosniff\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n";
            if stream.write_all(head.as_bytes()).await.is_err() {
                return gone();
            }
            if let Some(id) = ids.filter(|_| !resumed) {
                let priming = format!("id: {id}-0\nretry: {STREAM_RETRY_MS}\ndata: \n\n");
                if stream.write_all(priming.as_bytes()).await.is_err() {
                    return gone();
                }
            }
        }
        if streaming {
            for message in &batch {
                sent += 1;
                let event = sse_event(ids.map(|id| format!("{id}-{sent}")).as_deref(), message);
                if stream.write_all(event.as_bytes()).await.is_err() {
                    return gone();
                }
            }
            if done {
                stream.shutdown().await.ok();
                return Pumped::Finished;
            }
            if close {
                stream.shutdown().await.ok();
                return Pumped::Detached;
            }
        }
        tokio::select! {
            () = log.changed.notified() => {}
            // The client closing its end of the connection is the cancellation signal.
            read = stream.read(&mut scratch) => if matches!(read, Ok(0) | Err(_)) {
                return gone();
            },
            _ = keep_alive.tick(), if streaming => {
                if stream.write_all(b": keep-alive\n\n").await.is_err() {
                    return gone();
                }
            }
        }
    }
}

/// Streamable HTTP. Under 2026-07-28 every POST is self-contained; under the earlier versions `initialize` opens a
/// session (`Mcp-Session-Id`) whose requests may send the client requests of their own (sampling, elicitation) on
/// the response stream, and whose streams a client resumes with `Last-Event-ID`.
async fn serve_mcp(stream: &mut TcpStream, req: Request, shared: &Arc<Shared>) {
    if !shared.hosts.permits(&req) {
        write_json_rpc_error(stream, &RpcError::new(-32600, "Forbidden: the Host or Origin header does not name this server").with_status(403)).await;
        return;
    }
    let http = HttpMeta::from_headers(req.headers.iter().map(|(name, value)| (name.as_str(), value.as_str())));
    match req.method.as_str() {
        "POST" => post_mcp(stream, &req.body, http, shared).await,
        "GET" if mcp::http_is_legacy(&Value::Null, &http) && http.session_id.is_some() && http.last_event_id.is_some() => resume_stream(stream, &http, shared).await,
        "DELETE" if mcp::http_is_legacy(&Value::Null, &http) && http.session_id.is_some() => {
            let ended = http.session_id.as_deref().is_some_and(|id| shared.sessions.close(id));
            if ended {
                write_response(stream, 204, "application/json", MCP_PATH, "").await;
            } else {
                write_json_rpc_error(stream, &unknown_session()).await;
            }
        }
        _ => write_method_not_allowed(stream).await,
    }
}

/// `GET` with `Last-Event-ID`: continues the stream the id names from the event after it.
async fn resume_stream(stream: &mut TcpStream, http: &HttpMeta, shared: &Arc<Shared>) {
    let Some(session) = http.session_id.as_deref().and_then(|id| shared.sessions.get(id)) else {
        write_json_rpc_error(stream, &unknown_session()).await;
        return;
    };
    let last = http.last_event_id.as_deref().unwrap_or_default();
    let Some((stream_id, position)) = last.rsplit_once('-').and_then(|(stream_id, position)| Some((stream_id, position.parse::<usize>().ok()?))) else {
        write_json_rpc_error(stream, &RpcError::invalid_params("Last-Event-ID is not an event id of this server").with_status(400)).await;
        return;
    };
    let Some(log) = lock(&session.streams).get(stream_id).cloned() else {
        write_json_rpc_error(stream, &RpcError::new(-32001, "Stream not found: its events were already delivered").with_status(404)).await;
        return;
    };
    let pumped = pump(stream, Pump { log: &log, from: position, ids: Some(stream_id), session_header: None, cancel_on_gone: None, resumed: true }).await;
    if matches!(pumped, Pumped::Finished) {
        lock(&session.streams).remove(stream_id);
    }
}

/// `POST`: one message in, answered as JSON or as an event stream that ends with the response.
async fn post_mcp(stream: &mut TcpStream, body: &str, http: HttpMeta, shared: &Arc<Shared>) {
    let message: Value = match serde_json::from_str(body) {
        Ok(message) => message,
        Err(error) => {
            write_json_rpc_error(stream, &RpcError::new(-32700, format!("Parse error: {error}")).with_status(400)).await;
            return;
        }
    };
    let legacy = mcp::http_is_legacy(message.get("params").unwrap_or(&Value::Null), &http);
    let session = if legacy { http.session_id.as_deref().map(|id| shared.sessions.get(id).ok_or_else(unknown_session)) } else { None };
    let session = match session.transpose() {
        Ok(session) => session,
        Err(error) => {
            write_json_rpc_error(stream, &error).await;
            return;
        }
    };
    if legacy && mcp::is_response(&message) {
        match session {
            Some(session) => {
                session.connection.deliver(&message);
                write_response(stream, 202, "application/json", MCP_PATH, "").await;
            }
            None => write_json_rpc_error(stream, &RpcError::invalid_params("A response to a server request names its session in Mcp-Session-Id").with_status(400)).await,
        }
        return;
    }

    let opens_session = legacy && session.is_none() && message.get("method").and_then(Value::as_str) == Some("initialize");
    let new_session = opens_session.then(|| shared.sessions.fresh_id());
    let connection = match (&session, opens_session) {
        (Some(session), _) => session.connection.clone(),
        (None, true) => Arc::new(Connection::interactive()),
        (None, false) => Arc::new(Connection::default()),
    };
    let request_id = message.get("id").cloned();
    let log = Arc::new(StreamLog::default());
    // A request of a session outlives its stream, so only `notifications/cancelled` (not a closed socket) cancels it.
    let (cancel, stream_name) = match (&session, &request_id) {
        (Some(session), Some(id)) => {
            let name = session.next_stream.fetch_add(1, Ordering::SeqCst).to_string();
            lock(&session.streams).insert(name.clone(), log.clone());
            (connection.begin(id), Some(name))
        }
        _ => (Cancel::default(), None),
    };
    let in_session = session.is_some();
    let worker = {
        let (shared, connection, cancel, log, request_id) = (shared.clone(), connection.clone(), cancel.clone(), log.clone(), request_id.clone());
        let session_id = new_session.as_ref().map(|(id, _)| id.clone());
        let opened = new_session.as_ref().map_or(0, |(_, serial)| *serial);
        tokio::task::spawn_blocking(move || {
            let sink = {
                let log = log.clone();
                move |notification: Value| log.push(notification)
            };
            let reply = shared.engine.handle_on_stream(&connection, &message, Some(&http), &sink, &cancel, Some(log.as_ref()));
            if let Some(id) = &request_id.filter(|_| in_session) {
                connection.end(id);
            }
            if let Some(id) = session_id
                && reply.status == 200
                && reply.body.as_ref().is_some_and(|body| body.get("result").is_some())
            {
                shared.sessions.insert(id, opened, connection.clone());
            }
            log.finish(reply);
        })
    };

    let ids = stream_name.as_deref();
    let session_header = new_session.as_ref().map(|(id, _)| id.as_str());
    let cancel_on_gone = session.is_none().then_some(&cancel);
    let pumped = pump(stream, Pump { log: &log, from: 0, ids, session_header, cancel_on_gone, resumed: false }).await;
    // A stream the client lost or the request closed stays in place for a resumption; a delivered one is dropped.
    if let (Some(session), Some(name), Pumped::Finished) = (&session, ids, &pumped) {
        lock(&session.streams).remove(name);
    }
    if session.is_none() {
        cancel.cancel();
    }
    if let Err(error) = worker.await {
        eprintln!("fw serve: an MCP request handler panicked: {error}");
    }
}

/// Serves the control plane and web UI on `addr` until the listener fails, with `fw`'s own tools on `/mcp`.
pub async fn serve(addr: SocketAddr) -> std::io::Result<()> {
    serve_with(addr, Arc::new(FwCatalog::from_current_dir())).await
}

/// Like [`serve`], with the MCP endpoint backed by `catalog`.
pub async fn serve_with(addr: SocketAddr, catalog: Arc<dyn Catalog>) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    serve_on(listener, catalog).await
}

/// Serves on an already bound listener; the caller reads the port it got from the operating system.
pub async fn serve_on(listener: TcpListener, catalog: Arc<dyn Catalog>) -> std::io::Result<()> {
    let hosts = HostPolicy::for_bind(listener.local_addr()?);
    let shared = Arc::new(Shared { router: Arc::new(Mutex::new(HybridClusterRouter::default_hybrid_cluster())), engine: Arc::new(Engine::new(catalog)), hosts, sessions: Sessions::default() });
    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(handle_conn(stream, shared.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn roundtrip(raw: Vec<u8>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hosts = HostPolicy::for_bind(addr);
        let shared = Arc::new(Shared {
            router: Arc::new(Mutex::new(HybridClusterRouter::default_hybrid_cluster())),
            engine: Arc::new(Engine::new(Arc::new(FwCatalog::from_current_dir()))),
            hosts,
            sessions: Sessions::default(),
        });
        let server = tokio::spawn(async move {
            let (s, _) = listener.accept().await.unwrap();
            handle_conn(s, shared).await;
        });
        let mut client = TcpStream::connect(addr).await.unwrap();
        // Split writes: the head and the body arrive in separate segments.
        let split = find_head_end(&raw).unwrap_or(raw.len());
        client.write_all(&raw[..split]).await.unwrap();
        client.flush().await.unwrap();
        client.write_all(&raw[split..]).await.unwrap();
        let mut out = String::new();
        client.read_to_string(&mut out).await.unwrap();
        server.await.unwrap();
        out
    }

    #[tokio::test]
    async fn reads_bodies_that_arrive_after_the_head() {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
        let raw = format!("POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n\r\n{body}", body.len());
        let out = roundtrip(raw.into_bytes()).await;
        assert!(out.starts_with("HTTP/1.1 200 OK"), "{out}");
        assert!(out.contains("linux_kernel_mcp"));
    }

    #[tokio::test]
    async fn rejects_oversized_and_chunked_bodies() {
        let raw = format!("POST /mcp HTTP/1.1\r\nContent-Length: {}\r\n\r\n", MAX_BODY_BYTES + 1);
        assert!(roundtrip(raw.into_bytes()).await.starts_with("HTTP/1.1 413"));
        let raw = "POST /mcp HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n";
        assert!(roundtrip(raw.as_bytes().to_vec()).await.starts_with("HTTP/1.1 411"));
    }

    #[tokio::test]
    async fn serves_the_embedded_ui() {
        let out = roundtrip(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n".to_vec()).await;
        assert!(out.contains("text/html"));
        assert!(out.contains("/app.js"));
    }
}
