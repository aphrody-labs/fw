//! HTTP/1.1 listener for `fw serve`: reads a whole request (head plus a
//! `Content-Length` body), runs the blocking router off the async workers, and
//! answers with one `Connection: close` response.

use crate::dispatch_http;
use crate::mcp::{Cancel, Catalog, Connection, Engine, HttpMeta, Reply, RpcError};
use crate::mcp_catalog::FwCatalog;
use fw_cluster::HybridClusterRouter;
use serde_json::Value;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

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

/// Everything a connection handler shares: the cluster router, the MCP engine and its host policy.
struct Shared {
    router: Arc<Mutex<HybridClusterRouter>>,
    engine: Arc<Engine>,
    hosts: HostPolicy,
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
    let head = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: {content_type}\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: strict-origin-when-cross-origin\r\nContent-Security-Policy: {}\r\nAccess-Control-Allow-Origin: *\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
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

enum McpEvent {
    Notification(Value),
    Final(Reply),
}

async fn write_json_rpc_error(stream: &mut TcpStream, error: &RpcError) {
    let body = serde_json::json!({ "jsonrpc": "2.0", "id": null, "error": { "code": error.code, "message": error.message } }).to_string();
    write_response(stream, error.status, "application/json", MCP_PATH, &body).await;
}

fn sse_event(message: &Value) -> String {
    format!("event: message\ndata: {message}\n\n")
}

/// Streamable HTTP, revision 2026-07-28: one POST per message. The answer is a JSON object, or, as soon as
/// the request emits a notification (progress, logs, a subscription), an event stream that ends with the
/// response. Closing the connection cancels the request.
async fn serve_mcp(stream: &mut TcpStream, req: Request, shared: &Arc<Shared>) {
    if !shared.hosts.permits(&req) {
        write_json_rpc_error(stream, &RpcError::new(-32600, "Forbidden: the Host or Origin header does not name this server").with_status(403)).await;
        return;
    }
    if req.method != "POST" {
        let body = r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"POST a JSON-RPC message"}}"#;
        let head = format!("HTTP/1.1 405 Method Not Allowed\r\nAllow: POST, OPTIONS\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
        let _ = stream.write_all(head.as_bytes()).await;
        let _ = stream.write_all(body.as_bytes()).await;
        return;
    }
    let message: Value = match serde_json::from_str(&req.body) {
        Ok(message) => message,
        Err(error) => {
            write_json_rpc_error(stream, &RpcError::new(-32700, format!("Parse error: {error}")).with_status(400)).await;
            return;
        }
    };
    let http = HttpMeta::from_headers(req.headers.iter().map(|(name, value)| (name.as_str(), value.as_str())));
    let (sender, mut receiver) = mpsc::unbounded_channel::<McpEvent>();
    let cancel = Cancel::default();
    let worker = {
        let (engine, cancel, sender) = (shared.engine.clone(), cancel.clone(), sender);
        tokio::task::spawn_blocking(move || {
            let sink = {
                let sender = sender.clone();
                move |notification: Value| {
                    let _ = sender.send(McpEvent::Notification(notification));
                }
            };
            let reply = engine.handle(&Connection::default(), &message, Some(&http), &sink, &cancel);
            let _ = sender.send(McpEvent::Final(reply));
        })
    };

    let mut streaming = false;
    let mut keep_alive = tokio::time::interval(KEEP_ALIVE);
    keep_alive.tick().await;
    let mut scratch = [0u8; 256];
    loop {
        tokio::select! {
            event = receiver.recv() => match event {
                Some(McpEvent::Notification(notification)) => {
                    if !streaming {
                        streaming = true;
                        let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache, no-transform\r\nX-Accel-Buffering: no\r\nX-Content-Type-Options: nosniff\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n";
                        if stream.write_all(head.as_bytes()).await.is_err() {
                            cancel.cancel();
                            break;
                        }
                    }
                    if stream.write_all(sse_event(&notification).as_bytes()).await.is_err() {
                        cancel.cancel();
                        break;
                    }
                }
                Some(McpEvent::Final(reply)) => {
                    match (streaming, reply.body) {
                        (true, Some(body)) => {
                            let _ = stream.write_all(sse_event(&body).as_bytes()).await;
                        }
                        (false, Some(body)) => write_response(stream, reply.status, "application/json", MCP_PATH, &body.to_string()).await,
                        (false, None) if reply.status == 202 => write_response(stream, 202, "application/json", MCP_PATH, "").await,
                        _ => {}
                    }
                    let _ = stream.shutdown().await;
                    break;
                }
                None => break,
            },
            // The client closing its end of the connection is the cancellation signal.
            read = stream.read(&mut scratch) => if matches!(read, Ok(0) | Err(_)) {
                cancel.cancel();
                break;
            },
            _ = keep_alive.tick(), if streaming => {
                if stream.write_all(b": keep-alive\n\n").await.is_err() {
                    cancel.cancel();
                    break;
                }
            }
        }
    }
    cancel.cancel();
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
    let shared = Arc::new(Shared { router: Arc::new(Mutex::new(HybridClusterRouter::default_hybrid_cluster())), engine: Arc::new(Engine::new(catalog)), hosts });
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
