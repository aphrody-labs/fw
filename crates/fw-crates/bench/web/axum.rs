use axum::{Json, Router, routing::get};
use serde::Serialize;

#[derive(Serialize)]
struct Message {
    message: &'static str,
}

#[tokio::main]
async fn main() {
    let port = std::env::args().nth(1).unwrap_or_else(|| "8080".into());
    let app = Router::new()
        .route("/hello", get(|| async { "Hello, World!" }))
        .route("/json", get(|| async { Json(Message { message: "Hello, World!" }) }));
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}")).await.expect("bind");
    axum::serve(listener, app).await.expect("serve");
}
