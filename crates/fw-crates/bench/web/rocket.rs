#[macro_use]
extern crate rocket;

use rocket::serde::{Serialize, json::Json};

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
struct Message {
    message: &'static str,
}

#[get("/hello")]
fn hello() -> &'static str {
    "Hello, World!"
}

#[get("/json")]
fn json() -> Json<Message> {
    Json(Message { message: "Hello, World!" })
}

#[launch]
fn rocket() -> _ {
    let port: u16 = std::env::args().nth(1).and_then(|p| p.parse().ok()).unwrap_or(8080);
    let config = rocket::Config { port, address: [127, 0, 0, 1].into(), log_level: rocket::config::LogLevel::Off, ..rocket::Config::release_default() };
    rocket::custom(config).mount("/", routes![hello, json])
}
