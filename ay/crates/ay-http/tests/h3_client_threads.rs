//! Which side of an h3 exchange tolerates a multi-thread runtime.
//!
//! The server (`ay_http::server`) is exercised on a multi-thread runtime by `server_multi_client_current`. The h3 client of reqwest
//! (with the hyperium/h3 master patched in the workspace) answers some requests with `RemoteTerminate { code: H3_NO_ERROR }` when it
//! runs on a multi-thread runtime while another client churns connections in the same process: `server_current_client_multi` is the
//! reproduction, ignored until reqwest or h3 fix it. Single-thread clients are stable (see `h3_roundtrip`).

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
#[ignore = "reqwest h3 client on a multi-thread runtime: RemoteTerminate(H3_NO_ERROR) under concurrent connection churn"]
fn server_current_client_multi() {
    let env = start_server(false);
    let a = std::thread::scope(|s| {
        let h1 = s.spawn(|| tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build().unwrap().block_on(many(&env)));
        let h2 = s.spawn(|| tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build().unwrap().block_on(churn(&env)));
        (h1.join(), h2.join())
    });
    assert!(a.0.is_ok() && a.1.is_ok(), "{a:?}");
}
