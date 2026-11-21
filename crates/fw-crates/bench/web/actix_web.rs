use actix_web::{App, HttpServer, web};
use serde::Serialize;

#[derive(Serialize)]
struct Message {
    message: &'static str,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let port: u16 = std::env::args().nth(1).and_then(|p| p.parse().ok()).unwrap_or(8080);
    HttpServer::new(|| {
        App::new()
            .route("/hello", web::get().to(|| async { "Hello, World!" }))
            .route("/json", web::get().to(|| async { web::Json(Message { message: "Hello, World!" }) }))
    })
    .bind(("127.0.0.1", port))?
    .run()
    .await
}
