//! `fw-gateway`: serve the tool gateway as MCP over stdio or MCP over ACP, or call it once from the shell.

use agent_client_protocol::{ByteStreams, Proxy};
use anyhow::{Context, Result, bail};
use fw_gateway::config::Config;
use fw_gateway::{Gateway, acp, server::GatewayServer};
use rmcp::ServiceExt;
use serde_json::{Map, Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

const USAGE: &str = "usage: fw-gateway [--config FILE] <command>
  serve               MCP over stdio (2026-07-28 stateless, 2025-11-25 and earlier through initialize)
  acp                 ACP proxy: attaches the gateway as an MCP-over-ACP server to the session
  tools [QUERY]       print providers and the tools found (all of them, or the best matches for QUERY)
  call NAME [JSON]    call one tool with JSON arguments and print the result
  info                print the bytes `tools/list` costs an agent: meta-tools against every tool typed
  bench NAME [JSON] [RUNS]       time RUNS calls (default 20) of one tool through the gateway
  bench-http METHOD URL [RUNS]   time RUNS plain HTTPS requests to the same API, secret from $BENCH_TOKEN
  bench-mcp URL TOOL [JSON] [RUNS]  time RUNS calls of a tool of an MCP server through a plain rmcp client, secret from $BENCH_TOKEN
The config file is FILE, $FW_GATEWAY_CONFIG, or ./gateway.toml.";

fn timing(millis: &[f64], bytes: usize) -> Value {
    let mut sorted = millis.to_vec();
    sorted.sort_by(f64::total_cmp);
    let at = |fraction: f64| sorted.get(((sorted.len() as f64 - 1.0) * fraction).round() as usize).copied().unwrap_or(0.0);
    json!({ "runs": sorted.len(), "p50_ms": at(0.5), "p95_ms": at(0.95), "min_ms": at(0.0), "result_bytes": bytes })
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut config_path = std::env::var_os("FW_GATEWAY_CONFIG").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("gateway.toml"));
    if let Some(position) = args.iter().position(|argument| argument == "--config") {
        if position + 1 >= args.len() {
            bail!("--config needs a file\n{USAGE}");
        }
        config_path = PathBuf::from(args.remove(position + 1));
        args.remove(position);
    }
    let Some(command) = args.first().cloned() else { bail!("{USAGE}") };
    if matches!(command.as_str(), "help" | "--help" | "-h") {
        println!("{USAGE}");
        return Ok(());
    }
    let config = Config::load(&config_path)?;
    let gateway = Arc::new(Gateway::from_config(config).await?);
    for provider in &gateway.report {
        match &provider.error {
            None => eprintln!("fw-gateway: {} ({}): {} tools{}", provider.name, provider.kind, provider.tools, if provider.rejected > 0 { format!(", {} rejected", provider.rejected) } else { String::new() }),
            Some(error) => eprintln!("fw-gateway: {} ({}): failed: {error}", provider.name, provider.kind),
        }
    }
    match command.as_str() {
        "serve" => {
            let service = GatewayServer::new(gateway).serve(rmcp::transport::stdio()).await.context("starting the MCP server")?;
            service.waiting().await?;
        }
        "acp" => {
            Proxy
                .v1()
                .name("fw-gateway")
                .with_mcp_server(acp::mcp_server(gateway))
                .connect_to(ByteStreams::new(tokio::io::stdout().compat_write(), tokio::io::stdin().compat()))
                .await
                .map_err(|error| anyhow::anyhow!("ACP: {error}"))?;
        }
        "tools" => {
            let tools = match args.get(1) {
                Some(query) => gateway.search(query, gateway.settings.search_limit).await?,
                None => gateway.all_tools().await?,
            };
            let listing: Vec<Value> = tools.iter().map(|tool| json!({ "name": tool.name, "description": tool.description })).collect();
            println!("{}", serde_json::to_string_pretty(&json!({ "count": listing.len(), "tools": listing }))?);
        }
        "call" => {
            let name = args.get(1).context("call needs a tool name")?;
            let arguments: Map<String, Value> = match args.get(2) {
                Some(text) => serde_json::from_str(text).context("arguments must be a JSON object")?,
                None => Map::new(),
            };
            let value = gateway.call(name, arguments).await?;
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        "info" => {
            let (meta, typed_count, typed) = GatewayServer::new(gateway).listing_bytes().await?;
            println!("{}", json!({ "meta_tools": { "tools": 3, "bytes": meta }, "typed_tools": { "tools": typed_count, "bytes": typed } }));
        }
        "bench" => {
            let name = args.get(1).context("bench needs a tool name")?;
            let arguments: Map<String, Value> = match args.get(2) {
                Some(text) => serde_json::from_str(text).context("arguments must be a JSON object")?,
                None => Map::new(),
            };
            let runs: usize = args.get(3).map_or(Ok(20), |text| text.parse()).context("RUNS must be a number")?;
            let mut millis = Vec::new();
            let mut bytes = 0;
            for _ in 0..runs {
                let started = std::time::Instant::now();
                let value = gateway.call(name, arguments.clone()).await?;
                millis.push(started.elapsed().as_secs_f64() * 1000.0);
                bytes = value.to_string().len();
            }
            println!("{}", timing(&millis, bytes));
        }
        "bench-http" => {
            let method = args.get(1).context("bench-http needs a method")?;
            let url = args.get(2).context("bench-http needs a URL")?;
            let runs: usize = args.get(3).map_or(Ok(20), |text| text.parse()).context("RUNS must be a number")?;
            let client = reqwest::Client::new();
            let token = std::env::var("BENCH_TOKEN").ok();
            let mut millis = Vec::new();
            let mut bytes = 0;
            for _ in 0..runs {
                let mut request = client.request(method.parse()?, url);
                if let Some(token) = &token {
                    request = request.bearer_auth(token);
                }
                let started = std::time::Instant::now();
                let body = request.send().await?.bytes().await?;
                millis.push(started.elapsed().as_secs_f64() * 1000.0);
                bytes = body.len();
            }
            println!("{}", timing(&millis, bytes));
        }
        "bench-mcp" => {
            let url = args.get(1).context("bench-mcp needs a URL")?;
            let tool = args.get(2).context("bench-mcp needs a tool name")?;
            let arguments: Map<String, Value> = match args.get(3) {
                Some(text) => serde_json::from_str(text).context("arguments must be a JSON object")?,
                None => Map::new(),
            };
            let runs: usize = args.get(4).map_or(Ok(20), |text| text.parse()).context("RUNS must be a number")?;
            let mut transport = rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig::with_uri(url.as_str());
            if let Ok(token) = std::env::var("BENCH_TOKEN") {
                transport = transport.auth_header(token);
            }
            let transport = rmcp::transport::StreamableHttpClientTransport::with_client(reqwest::Client::default(), transport);
            let lifecycle = rmcp::service::ClientLifecycleMode::Auto { preferred_versions: vec![rmcp::model::ProtocolVersion::LATEST], legacy_version: None };
            let session = rmcp::service::ClientServiceExt::serve_with_lifecycle((), transport, lifecycle).await.context("MCP handshake")?;
            let listed = session.peer().list_all_tools().await?;
            let listing_bytes = serde_json::to_vec(&listed)?.len();
            let mut millis = Vec::new();
            let mut bytes = 0;
            for _ in 0..runs {
                let params = rmcp::model::CallToolRequestParams::new(tool.clone()).with_arguments(arguments.clone());
                let started = std::time::Instant::now();
                let result = session.peer().call_tool(params).await?;
                if result.is_error == Some(true) {
                    bail!("the server answered an error: {}", serde_json::to_string(&result.content)?.chars().take(300).collect::<String>());
                }
                millis.push(started.elapsed().as_secs_f64() * 1000.0);
                bytes = serde_json::to_vec(&result)?.len();
            }
            let mut report = timing(&millis, bytes);
            report["tools_list"] = json!({ "tools": listed.len(), "bytes": listing_bytes });
            println!("{report}");
        }
        other => bail!("unknown command {other:?}\n{USAGE}"),
    }
    Ok(())
}
