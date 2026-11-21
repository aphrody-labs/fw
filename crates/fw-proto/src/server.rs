//! HTTP/1.1 listener for `fw serve`: reads a whole request (head plus a
//! `Content-Length` body), runs the blocking router off the async workers, and
//! answers with one `Connection: close` response.

use crate::dispatch_http;
use fw_cluster::HybridClusterRouter;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Largest request head (request line plus headers).
const MAX_HEAD_BYTES: usize = 64 * 1024;
/// Largest request body; long chat histories with code stay far below it.
const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

struct Request {
    method: String,
    path: String,
    body: String,
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
    let path = target.split('?').next().unwrap_or("/").to_owned();

    let mut content_length = 0usize;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { continue };
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
    Ok(Request { method: method.to_owned(), path, body })
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
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

async fn handle_conn(mut stream: TcpStream, router: Arc<Mutex<HybridClusterRouter>>) {
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
        let head = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type, Authorization, Mcp-Session-Id\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let _ = stream.write_all(head.as_bytes()).await;
        return;
    }
    // Routes call model gateways and scan the workspace with blocking I/O.
    let path = req.path.clone();
    let result = tokio::task::spawn_blocking(move || dispatch_http(&router, &req.method, &req.path, &req.body)).await;
    let (status, content_type, payload) = result.unwrap_or_else(|e| {
        (500, "application/json", serde_json::json!({ "ok": false, "error": format!("route panicked: {e}") }).to_string())
    });
    write_response(&mut stream, status, content_type, &path, &payload).await;
}

/// Serves the control plane and web UI on `addr` until the listener fails.
pub async fn serve(addr: SocketAddr) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    let router = Arc::new(Mutex::new(HybridClusterRouter::default_hybrid_cluster()));
    loop {
        let (stream, _) = listener.accept().await?;
        let r = router.clone();
        tokio::spawn(handle_conn(stream, r));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn roundtrip(raw: Vec<u8>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let router = Arc::new(Mutex::new(HybridClusterRouter::default_hybrid_cluster()));
        let server = tokio::spawn(async move {
            let (s, _) = listener.accept().await.unwrap();
            handle_conn(s, router).await;
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
        let raw = format!("POST /mcp HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\n\r\n{body}", body.len());
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
