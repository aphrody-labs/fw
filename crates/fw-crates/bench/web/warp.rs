use serde::Serialize;
use warp::Filter;

#[derive(Serialize)]
struct Message {
    message: &'static str,
}

#[tokio::main]
async fn main() {
    let port: u16 = std::env::args().nth(1).and_then(|p| p.parse().ok()).unwrap_or(8080);
    let hello = warp::path("hello").and(warp::path::end()).map(|| "Hello, World!");
    let json = warp::path("json").and(warp::path::end()).map(|| warp::reply::json(&Message { message: "Hello, World!" }));
    warp::serve(hello.or(json)).run(([127, 0, 0, 1], port)).await;
}
