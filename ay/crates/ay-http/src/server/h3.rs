//! HTTP/3 over QUIC: every request is bridged to the same axum router the TCP listener serves.

use std::io;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use bytes::{Buf, Bytes};
use futures_util::stream;
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

async fn request(resolver: RequestResolver<h3_quinn::Connection, Bytes>, app: Router) -> Result<(), BoxError> {
    let (request, stream) = resolver.resolve_request().await?;
    let (mut send, mut recv) = stream.split();
    let (parts, ()) = request.into_parts();
    // The request body is drained by its own task, so the receive half of the stream lives until the response is sent even when the
    // handler drops the body unread (dropping it earlier sends STOP_SENDING and the peer may see the connection as terminated).
    let (chunks, body_rx) = tokio::sync::mpsc::channel::<Result<Bytes, io::Error>>(8);
    let reader = tokio::spawn(async move {
        loop {
            match recv.recv_data().await {
                Ok(Some(mut chunk)) => {
                    let _ = chunks.send(Ok(chunk.copy_to_bytes(chunk.remaining()))).await;
                }
                Ok(None) => break,
                Err(error) => {
                    let _ = chunks.send(Err(io::Error::other(error))).await;
                    break;
                }
            }
        }
        recv
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
    reader.abort();
    Ok(())
}
