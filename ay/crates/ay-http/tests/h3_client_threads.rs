//! h3 on multi-thread runtimes, on either side of the exchange, with a long-lived connection and a connection-churning client sharing
//! one server.
//!
//! This reproduced `RemoteTerminate { code: 0x0 }` errors at the client. Root cause: when the server finished a response while the client's
//! `send.finish()` on the request stream was still waiting, the server's drop of the request stream half sent STOP_SENDING with quinn's
//! default code 0; reqwest only treats H3_NO_ERROR (0x100) as a benign stop (RFC 9114 section 4.1) and surfaced the reset as an error of
//! the exchange. The server now stops reading with H3_NO_ERROR. A multi-thread client made the race frequent: it lets the response beat
//! the finish acknowledgement.

use std::time::Duration;
use axum::{Router, routing::get};
use ay_http::server::{ServeConfig, Tls, serve};
use ay_http::{Client, Version};

fn router() -> Router { Router::new().route("/ping", get(|| async { "pong" })) }

struct Env { port: u16, pem: String, _stop: std::sync::mpsc::Sender<()> }

fn start_server(multi: bool) -> Env {
    let (tx, rx) = std::sync::mpsc::channel();
    let (stop, stopped) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(move || {
        let rt = if multi { tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build().unwrap() } else { tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap() };
        rt.block_on(async move {
            let (tls, pem) = Tls::self_signed(&["localhost"]).unwrap();
            let server = serve(router(), ServeConfig::secure("127.0.0.1:0".parse().unwrap(), tls)).await.unwrap();
            tx.send((server.tcp_addr().port(), pem)).unwrap();
            tokio::task::spawn_blocking(move || { let _ = stopped.recv(); }).await.unwrap();
            server.shutdown().await;
        });
    });
    let (port, pem) = rx.recv().unwrap();
    Env { port, pem, _stop: stop }
}

fn client(env: &Env) -> Client {
    let cert = ay_http::reqwest::Certificate::from_pem(env.pem.as_bytes()).unwrap();
    Client::builder().add_root_certificate(cert).resolve("localhost", format!("127.0.0.1:{}", env.port).parse().unwrap()).connect_timeout(Duration::from_secs(2)).timeout(Duration::from_secs(10)).build().unwrap()
}

async fn many(env: &Env) {
    let url = format!("https://localhost:{}/ping", env.port);
    let c = client(env);
    c.get(&url).send().await.unwrap();
    for i in 0..3000 {
        let r = c.get(&url).send().await.unwrap_or_else(|e| panic!("request {i}: {e:?}"));
        assert_eq!(r.version(), Version::HTTP_3);
        r.text().await.unwrap_or_else(|e| panic!("body {i}: {e:?}"));
    }
}
async fn churn(env: &Env) {
    let url = format!("https://localhost:{}/ping", env.port);
    let origin = ay_http::altsvc::Origin::of_https(&url.parse().unwrap()).unwrap();
    for i in 0..200 {
        let c = client(env);
        c.alt_svc().insert(origin.clone(), Duration::from_secs(60));
        let r = c.get(&url).send().await.unwrap_or_else(|e| panic!("connection {i}: {e:?}"));
        r.text().await.unwrap_or_else(|e| panic!("cbody {i}: {e:?}"));
    }
}

#[test]
fn server_multi_client_current() {
    let env = start_server(true);
    let a = std::thread::scope(|s| {
        let h1 = s.spawn(|| tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(many(&env)));
        let h2 = s.spawn(|| tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(churn(&env)));
        (h1.join(), h2.join())
    });
    assert!(a.0.is_ok() && a.1.is_ok(), "{a:?}");
}
#[test]
fn server_current_client_multi() {
    let env = start_server(false);
    let a = std::thread::scope(|s| {
        let h1 = s.spawn(|| tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build().unwrap().block_on(many(&env)));
        let h2 = s.spawn(|| tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build().unwrap().block_on(churn(&env)));
        (h1.join(), h2.join())
    });
    assert!(a.0.is_ok() && a.1.is_ok(), "{a:?}");
}
