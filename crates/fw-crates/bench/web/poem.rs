use poem::{Route, Server, get, handler, listener::TcpListener, web::Json};
use serde::Serialize;

#[derive(Serialize)]
struct Message {
    message: &'static str,
}

#[handler]
fn hello() -> &'static str {
    "Hello, World!"
}

#[handler]
fn json() -> Json<Message> {
    Json(Message { message: "Hello, World!" })
}

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let port = std::env::args().nth(1).unwrap_or_else(|| "8080".into());
    let app = Route::new().at("/hello", get(hello)).at("/json", get(json));
    Server::new(TcpListener::bind(format!("127.0.0.1:{port}"))).run(app).await
}
