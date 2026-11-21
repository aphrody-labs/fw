//! In-memory HTTP transport for tests: a connector that serves every request from a closure over a
//! `tokio::io::duplex` pipe, recording what the generated clients sent. No sockets, no network.

use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::service::service_fn;
use hyper::{HeaderMap, Request, Response, Uri};
use hyper_util::client::legacy::connect::{Connected, Connection};
use hyper_util::rt::TokioIo;
use std::convert::Infallible;
use std::future::{Ready, ready};
use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use tokio::io::DuplexStream;

/// One request as the server saw it.
#[derive(Clone, Debug)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

impl Recorded {
    pub fn param(&self, name: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
    pub fn params(&self, name: &str) -> Vec<&str> {
        self.query
            .iter()
            .filter(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
            .collect()
    }
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).expect("request body is JSON")
    }
    pub fn bearer(&self) -> Option<&str> {
        self.headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
    }
}

/// What the closure answers.
pub struct Reply {
    pub status: u16,
    pub body: Vec<u8>,
}

impl Reply {
    pub fn ok(body: &str) -> Self {
        Self {
            status: 200,
            body: body.as_bytes().to_vec(),
        }
    }
    pub fn status(status: u16, body: &str) -> Self {
        Self {
            status,
            body: body.as_bytes().to_vec(),
        }
    }
}

type Responder = dyn Fn(&Recorded) -> Reply + Send + Sync;

#[derive(Clone)]
pub struct Mock {
    log: Arc<Mutex<Vec<Recorded>>>,
    responder: Arc<Responder>,
}

impl Mock {
    pub fn new(responder: impl Fn(&Recorded) -> Reply + Send + Sync + 'static) -> Self {
        Self {
            log: Arc::default(),
            responder: Arc::new(responder),
        }
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.log.lock().unwrap().clone()
    }
}

/// Decodes `%XX` and `+` (query strings) and `%XX` (paths).
pub fn percent_decode(s: &str, plus_is_space: bool) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() && u8::from_str_radix(&s[i + 1..i + 3], 16).is_ok() => {
                out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap());
                i += 3;
            }
            b'+' if plus_is_space => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub struct MockIo(TokioIo<DuplexStream>);

impl hyper::rt::Read for MockIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: hyper::rt::ReadBufCursor<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_read(cx, buf)
    }
}

impl hyper::rt::Write for MockIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.0).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_shutdown(cx)
    }
}

impl Connection for MockIo {
    fn connected(&self) -> Connected {
        Connected::new()
    }
}

impl tower_service::Service<Uri> for Mock {
    type Response = MockIo;
    type Error = io::Error;
    type Future = Ready<Result<MockIo, io::Error>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _uri: Uri) -> Self::Future {
        let (client_io, server_io) = tokio::io::duplex(1 << 20);
        let log = self.log.clone();
        let responder = self.responder.clone();
        let svc = service_fn(move |req: Request<Incoming>| {
            let log = log.clone();
            let responder = responder.clone();
            async move {
                let (parts, body) = req.into_parts();
                let body = body
                    .collect()
                    .await
                    .map(|c| c.to_bytes().to_vec())
                    .unwrap_or_default();
                let query = parts
                    .uri
                    .query()
                    .map(|q| {
                        q.split('&')
                            .filter(|p| !p.is_empty())
                            .map(|p| {
                                let (k, v) = p.split_once('=').unwrap_or((p, ""));
                                (percent_decode(k, true), percent_decode(v, true))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let rec = Recorded {
                    method: parts.method.to_string(),
                    path: percent_decode(parts.uri.path(), false),
                    query,
                    headers: parts.headers,
                    body,
                };
                let reply = responder(&rec);
                log.lock().unwrap().push(rec);
                Ok::<_, Infallible>(
                    Response::builder()
                        .status(reply.status)
                        .header("content-type", "application/json")
                        .body(Full::new(Bytes::from(reply.body)))
                        .unwrap(),
                )
            }
        });
        tokio::spawn(async move {
            let _ = hyper::server::conn::http1::Builder::new()
                .serve_connection(TokioIo::new(server_io), svc)
                .await;
        });
        ready(Ok(MockIo(TokioIo::new(client_io))))
    }
}
