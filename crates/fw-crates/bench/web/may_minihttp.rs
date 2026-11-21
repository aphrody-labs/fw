use may_minihttp::{HttpServer, HttpService, Request, Response};
use serde::Serialize;
use std::io;

#[derive(Serialize)]
struct Message {
    message: &'static str,
}

#[derive(Clone)]
struct Service;

impl HttpService for Service {
    fn call(&mut self, req: Request, rsp: &mut Response) -> io::Result<()> {
        match req.path() {
            "/hello" => {
                rsp.header("Content-Type: text/plain");
                rsp.body("Hello, World!");
            }
            "/json" => {
                rsp.header("Content-Type: application/json");
                let body = serde_json::to_vec(&Message { message: "Hello, World!" }).map_err(io::Error::other)?;
                rsp.body_vec(body);
            }
            _ => {
                rsp.status_code(404, "Not Found");
            }
        }
        Ok(())
    }
}

fn main() {
    let port = std::env::args().nth(1).unwrap_or_else(|| "8080".into());
    let workers = std::thread::available_parallelism().map_or(1, usize::from);
    may::config().set_workers(workers);
    let server = HttpServer(Service).start(format!("127.0.0.1:{port}")).expect("bind");
    server.join().expect("serve");
}
