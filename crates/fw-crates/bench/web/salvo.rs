use salvo::prelude::*;
use serde::Serialize;

#[derive(Serialize)]
struct Message {
    message: &'static str,
}

#[handler]
async fn hello() -> &'static str {
    "Hello, World!"
}

#[handler]
async fn json() -> Json<Message> {
    Json(Message { message: "Hello, World!" })
}

#[tokio::main]
async fn main() {
    let port = std::env::args().nth(1).unwrap_or_else(|| "8080".into());
    let router = Router::new().push(Router::with_path("hello").get(hello)).push(Router::with_path("json").get(json));
    let acceptor = TcpListener::new(format!("127.0.0.1:{port}")).bind().await;
    Server::new(acceptor).serve(router).await;
}
