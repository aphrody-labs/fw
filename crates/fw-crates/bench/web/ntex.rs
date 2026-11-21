use ntex::web::{self, App, HttpResponse};
use ntex::SharedCfg;
use serde::Serialize;

#[derive(Serialize)]
struct Message {
    message: &'static str,
}

async fn hello() -> &'static str {
    "Hello, World!"
}

async fn json() -> HttpResponse {
    HttpResponse::Ok().json(&Message { message: "Hello, World!" })
}

#[ntex::main]
async fn main() -> std::io::Result<()> {
    let port: u16 = std::env::args().nth(1).and_then(|p| p.parse().ok()).unwrap_or(8080);
    web::HttpServer::new(async |_| App::new().route("/hello", web::get().to(hello)).route("/json", web::get().to(json)))
        .bind(("127.0.0.1", port), SharedCfg::new("bench"))?
        .run()
        .await
}
