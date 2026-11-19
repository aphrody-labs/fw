//! One axum [`Router`] served over TCP (HTTP/1.1 and h2, optionally TLS) and QUIC (h3), with `Alt-Svc` advertised.

mod h3;
mod tls;

use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::time::Duration;

use axum::Router;
use axum::extract::Request;
use axum::http::HeaderValue;
use axum::middleware::{self, Next};
use axum::response::Response;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto;
use hyper_util::service::TowerToHyperService;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tower::ServiceExt;

pub use tls::Tls;

/// How to serve.
pub struct ServeConfig {
    /// Listen address; port 0 picks a free port (the UDP port of h3 is the TCP port).
    pub addr: SocketAddr,
    /// TLS material. Without it the server speaks h1 and cleartext h2 only; h3 needs TLS.
    pub tls: Option<Tls>,
    /// Serve h3 on the same port over UDP and advertise it with `Alt-Svc` (needs `tls`).
    pub h3: bool,
    /// `ma` of the advertised `Alt-Svc`.
    pub alt_svc_max_age: Duration,
    /// Accept TLS 1.3 early data (0-RTT) on QUIC. Early data can be replayed by an attacker, so enable it only for deployments whose
    /// safe-method handlers are idempotent; the [`crate::Client`] sends early data for `GET`, `HEAD` and `OPTIONS` only.
    pub early_data: bool,
}

impl ServeConfig {
    /// Cleartext h1 and h2c on `addr`.
    #[must_use]
    pub fn plain(addr: SocketAddr) -> Self {
        Self { addr, tls: None, h3: false, alt_svc_max_age: Duration::from_secs(86_400), early_data: false }
    }

    /// TLS on `addr`: h1 and h2 over TCP, h3 over UDP on the same port.
    #[must_use]
    pub fn secure(addr: SocketAddr, tls: Tls) -> Self {
        Self { addr, tls: Some(tls), h3: true, alt_svc_max_age: Duration::from_secs(86_400), early_data: false }
    }
}

/// A running server.
pub struct Server {
    tcp: SocketAddr,
    quic: Option<SocketAddr>,
    stop: watch::Sender<bool>,
    tasks: Vec<JoinHandle<()>>,
}

impl Server {
    /// The TCP address (h1, h2).
    #[must_use]
    pub fn tcp_addr(&self) -> SocketAddr {
        self.tcp
    }

    /// The UDP address (h3), when enabled.
    #[must_use]
    pub fn quic_addr(&self) -> Option<SocketAddr> {
        self.quic
    }

    /// Stops accepting, closes the QUIC endpoint and waits for the listeners to finish. Requests in flight are not awaited.
    pub async fn shutdown(self) {
        let _ = self.stop.send(true);
        for task in self.tasks {
            let _ = task.await;
        }
    }

    /// Waits until the listeners end (they only end on [`Server::shutdown`] or a fatal listener error).
    pub async fn wait(self) {
        for task in self.tasks {
            let _ = task.await;
        }
    }
}

/// Binds the listeners and starts serving `router`.
///
/// # Errors
/// Bind failures, invalid TLS material, or `h3` requested without TLS.
pub async fn serve(router: Router, config: ServeConfig) -> io::Result<Server> {
    let listener = TcpListener::bind(config.addr).await?;
    serve_on(listener, router, config).await
}

/// Starts serving `router` on a TCP listener the caller already bound (`config.addr` is ignored; h3 uses the listener's port over UDP).
///
/// # Errors
/// Bind failures of the QUIC socket, invalid TLS material, or `h3` requested without TLS.
pub async fn serve_on(listener: TcpListener, router: Router, config: ServeConfig) -> io::Result<Server> {
    if config.h3 && config.tls.is_none() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "h3 needs TLS material"));
    }
    let tcp = listener.local_addr()?;
    let (stop, shutdown) = watch::channel(false);
    let mut tasks = Vec::new();

    let mut quic = None;
    let mut app = router;
    if config.h3
        && let Some(tls) = &config.tls
    {
        let endpoint = h3::bind(SocketAddr::new(tcp.ip(), tcp.port()), tls, config.early_data)?;
        quic = Some(endpoint.local_addr()?);
        let alt_svc = HeaderValue::from_str(&format!("h3=\":{}\"; ma={}", tcp.port(), config.alt_svc_max_age.as_secs()))
            .map_err(io::Error::other)?;
        let h3_app = app.clone();
        app = app.layer(middleware::from_fn(move |request: Request, next: Next| {
            let alt_svc = alt_svc.clone();
            async move {
                let mut response: Response = next.run(request).await;
                response.headers_mut().insert(axum::http::header::ALT_SVC, alt_svc);
                response
            }
        }));
        tasks.push(tokio::spawn(h3::accept_loop(endpoint, h3_app, shutdown.clone())));
    }

    let acceptor = match &config.tls {
        Some(tls) => Some(tokio_rustls::TlsAcceptor::from(tls.server_config(&[b"h2", b"http/1.1"], false)?)),
        None => None,
    };
    tasks.push(tokio::spawn(accept_loop(listener, acceptor, app, shutdown)));
    Ok(Server { tcp, quic, stop, tasks })
}

/// Serves `router` over cleartext HTTP/1.1 and h2c on `listener` until the listener fails; the replacement of `axum::serve`.
///
/// # Errors
/// Never returns `Ok` while the listener is healthy.
pub async fn serve_listener(listener: TcpListener, router: Router) -> io::Result<()> {
    serve_on(listener, router, ServeConfig::plain(SocketAddr::from(([0, 0, 0, 0], 0)))).await?.wait().await;
    Ok(())
}

/// Like [`serve_listener`], stopping when `shutdown` resolves; the replacement of `axum::serve(..).with_graceful_shutdown(..)`.
///
/// # Errors
/// Listener setup failures.
pub async fn serve_listener_until(listener: TcpListener, router: Router, shutdown: impl Future<Output = ()>) -> io::Result<()> {
    let server = serve_on(listener, router, ServeConfig::plain(SocketAddr::from(([0, 0, 0, 0], 0)))).await?;
    shutdown.await;
    server.shutdown().await;
    Ok(())
}

async fn accept_loop(listener: TcpListener, acceptor: Option<tokio_rustls::TlsAcceptor>, app: Router, mut shutdown: watch::Receiver<bool>) {
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, _peer) = match accepted {
                    Ok(pair) => pair,
                    Err(error) => {
                        tracing::debug!(%error, "tcp accept failed");
                        continue;
                    }
                };
                let _ = stream.set_nodelay(true);
                let app = app.clone();
                let acceptor = acceptor.clone();
                tokio::spawn(async move { connection(stream, acceptor, app).await });
            }
            _ = shutdown.changed() => break,
        }
    }
}

async fn connection(stream: TcpStream, acceptor: Option<tokio_rustls::TlsAcceptor>, app: Router) {
    let service = TowerToHyperService::new(app.map_request(|request: axum::http::Request<hyper::body::Incoming>| request.map(axum::body::Body::new)));
    let builder = auto::Builder::new(TokioExecutor::new());
    let result = match acceptor {
        Some(acceptor) => match acceptor.accept(stream).await {
            Ok(tls) => builder.serve_connection_with_upgrades(TokioIo::new(tls), service).await,
            Err(error) => {
                tracing::debug!(%error, "tls handshake failed");
                return;
            }
        },
        None => builder.serve_connection_with_upgrades(TokioIo::new(stream), service).await,
    };
    if let Err(error) = result {
        tracing::debug!(%error, "connection ended with an error");
    }
}

