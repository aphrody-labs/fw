//! HTTP/3 over QUIC: every request is bridged to the same axum router the TCP listener serves.

use std::io;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use bytes::{Buf, Bytes};
use futures_util::stream;
use h3::error::Code;
use h3::server::RequestResolver;
use http_body_util::BodyExt;
use tokio::sync::watch;
use tower::ServiceExt;

use super::tls::Tls;

/// The QUIC listener: accepts connections until `shutdown` flips.
pub(crate) fn bind(addr: std::net::SocketAddr, tls: &Tls, early_data: bool) -> io::Result<quinn::Endpoint> {
    let config = tls.server_config(&[b"h3"], early_data)?;
    let quic = quinn::crypto::rustls::QuicServerConfig::try_from(config).map_err(io::Error::other)?;
    quinn::Endpoint::server(quinn::ServerConfig::with_crypto(Arc::new(quic)), addr)
}

pub(crate) async fn accept_loop(endpoint: quinn::Endpoint, app: Router, mut shutdown: watch::Receiver<bool>) {
    loop {
        tokio::select! {
            incoming = endpoint.accept() => {
                let Some(incoming) = incoming else { break };
                let app = app.clone();
                tokio::spawn(async move {
                    match incoming.await {
                        Ok(connection) => connection_loop(connection, app).await,
                        Err(error) => tracing::debug!(%error, "quic handshake failed"),
                    }
                });
            }
            _ = shutdown.changed() => break,
        }
    }
    endpoint.close(0u32.into(), b"shutdown");
    endpoint.wait_idle().await;
}

async fn connection_loop(connection: quinn::Connection, app: Router) {
    let mut h3 = match h3::server::Connection::new(h3_quinn::Connection::new(connection)).await {
        Ok(h3) => h3,
        Err(error) => {
            tracing::debug!(%error, "h3 connection setup failed");
            return;
        }
    };
    loop {
        match h3.accept().await {
            Ok(Some(resolver)) => {
                let app = app.clone();
                tokio::spawn(async move {
                    if let Err(error) = request(resolver, app).await {
                        tracing::debug!(%error, "h3 request failed");
                    }
                });
            }
            Ok(None) => break,
            Err(error) => {
                tracing::debug!(%error, "h3 accept failed");
                break;
            }
        }
    }
}

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Marks a request that arrived as TLS 1.3 early data (0-RTT). The request also carries `Early-Data: 1` (RFC 8470), and the server
/// answers `425 Too Early` to any early request whose method is not safe.
#[derive(Debug, Clone, Copy)]
pub struct EarlyData;

fn is_safe(method: &http::Method) -> bool {
    matches!(*method, http::Method::GET | http::Method::HEAD | http::Method::OPTIONS)
}

async fn request(resolver: RequestResolver<h3_quinn::Connection, Bytes>, app: Router) -> Result<(), BoxError> {
    let (request, stream) = resolver.resolve_request().await?;
    let early = stream.is_0rtt();
    let (mut send, mut recv) = stream.split();
    let (mut parts, ()) = request.into_parts();
    if early {
        if !is_safe(&parts.method) {
            let response = http::Response::builder().status(http::StatusCode::TOO_EARLY).body(())?;
            send.send_response(response).await?;
            recv.stop_sending(Code::H3_NO_ERROR);
            send.finish().await?;
            return Ok(());
        }
        parts.headers.insert("early-data", http::HeaderValue::from_static("1"));
        parts.extensions.insert(EarlyData);
    }
    // The request body is drained by its own task so that the receive half lives until the request ends by itself, even when the handler
    // drops the body unread. Dropping it earlier (or aborting the read while the client's FIN is still in flight) makes quinn send
    // STOP_SENDING with its default code 0, which reqwest's h3 client turns into `RemoteTerminate { code: 0x0 }` and a failed exchange
    // (seanmonstar/reqwest#3147). A request that does not end within a short grace period (an upload nobody reads) is aborted with
    // H3_NO_ERROR, the code RFC 9114 section 4.1 asks for.
    let (chunks, body_rx) = tokio::sync::mpsc::channel::<Result<Bytes, io::Error>>(8);
    let (done, mut done_rx) = tokio::sync::oneshot::channel::<()>();
    let mut reader = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut done_rx => {
                    recv.stop_sending(Code::H3_NO_ERROR);
                    break;
                }
                read = recv.recv_data() => match read {
                    Ok(Some(mut chunk)) => {
                        let _ = chunks.send(Ok(chunk.copy_to_bytes(chunk.remaining()))).await;
                    }
                    Ok(None) => break,
                    Err(error) => {
                        let _ = chunks.send(Err(io::Error::other(error))).await;
                        break;
                    }
                },
            }
        }
    });
    let body = Body::from_stream(stream::unfold(body_rx, |mut rx| async move { rx.recv().await.map(|item| (item, rx)) }));
    let response = match app.oneshot(http::Request::from_parts(parts, body)).await {
        Ok(response) => response,
        Err(never) => match never {},
    };
    let (parts, mut body) = response.into_parts();
    send.send_response(http::Response::from_parts(parts, ())).await?;
    while let Some(frame) = body.frame().await {
        let frame = frame?;
        match frame.into_data() {
            Ok(data) => send.send_data(data).await?,
            Err(frame) => {
                if let Ok(trailers) = frame.into_trailers() {
                    send.send_trailers(trailers).await?;
                }
            }
        }
    }
    send.finish().await?;
    // Let a request that is already complete (a GET carries its FIN right behind the head) end by itself; only then abort reading.
    if tokio::time::timeout(std::time::Duration::from_millis(250), &mut reader).await.is_err() {
        let _ = done.send(());
    }
    Ok(())
}
