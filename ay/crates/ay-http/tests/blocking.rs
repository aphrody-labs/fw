//! The blocking facade serves synchronous callers (no runtime of their own) through the shared client.

use std::io::Read;

use axum::Router;
use axum::routing::get;
use ay_http::blocking::Client;
use ay_http::server::{ServeConfig, serve};

fn start() -> (u16, std::sync::mpsc::Sender<()>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let (stop, stopped) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async move {
            let router = Router::new()
                .route("/json", get(|| async { axum::Json(serde_json::json!({"ok": true})) }))
                .route("/blob", get(|| async { vec![7u8; 300_000] }))
                .route("/status", get(|| async { (axum::http::StatusCode::NOT_FOUND, "nope") }));
            let server = serve(router, ServeConfig::plain("127.0.0.1:0".parse().unwrap())).await.unwrap();
            tx.send(server.tcp_addr().port()).unwrap();
            tokio::task::spawn_blocking(move || {
                let _ = stopped.recv();
            })
            .await
            .unwrap();
            server.shutdown().await;
        });
    });
    (rx.recv().unwrap(), stop)
}

#[test]
fn json_status_and_streamed_body() {
    let (port, _stop) = start();
    let client = Client::new();
    let json: serde_json::Value = client.get(format!("http://127.0.0.1:{port}/json")).send().unwrap().json().unwrap();
    assert_eq!(json["ok"], true);

    let missing = client.get(format!("http://127.0.0.1:{port}/status")).send().unwrap();
    assert_eq!(missing.status(), 404, "statuses are data, not errors");
    assert_eq!(missing.bytes().unwrap().as_ref(), b"nope");

    let mut response = client.get(format!("http://127.0.0.1:{port}/blob")).send().unwrap();
    let mut buf = [0u8; 4096];
    let mut total = 0;
    loop {
        let n = response.read(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        assert!(buf[..n].iter().all(|b| *b == 7));
        total += n;
    }
    assert_eq!(total, 300_000);
}
