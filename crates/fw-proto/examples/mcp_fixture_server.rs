//! Serves the MCP conformance fixtures: `cargo run -p fw-proto --example mcp_fixture_server -- [host:port | --stdio]`.
//! Over HTTP the endpoint is `http://<addr>/mcp`; with `--stdio` it speaks newline-delimited JSON-RPC.

use fw_proto::mcp::{serve_stdio, Engine};
use fw_proto::mcp_fixture::FixtureCatalog;
use std::io::{stdin, stdout, BufReader};
use std::net::SocketAddr;
use std::sync::Arc;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let target = std::env::args().nth(1).unwrap_or_else(|| "127.0.0.1:3000".to_owned());
    let catalog = Arc::new(FixtureCatalog::default());
    if target == "--stdio" {
        return serve_stdio(&Arc::new(Engine::new(catalog)), BufReader::new(stdin()), stdout());
    }
    let addr: SocketAddr = target.parse().map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("`{target}` is not a host:port address: {error}")))?;
    eprintln!("fw-proto fixture MCP server on http://{addr}/mcp");
    fw_proto::serve_with(addr, catalog).await
}
