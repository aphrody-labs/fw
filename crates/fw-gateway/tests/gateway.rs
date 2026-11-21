//! The gateway against real servers on loopback: an HTTP API described by OpenAPI, the `fw-gateway serve`
//! binary as an MCP server consumed through rmcp, and MCP over ACP between an agent and a client.

use agent_client_protocol::v2::{ConnectionTo, Responder, schema};
use agent_client_protocol::{Agent, Client, Error, schema::ProtocolVersion};
use fw_gateway::config::Config;
use fw_gateway::{Gateway, acp};
use rmcp::model::{CallToolRequestParams, ContentBlock};
use rmcp::service::{ClientLifecycleMode, ClientServiceExt};
use rmcp::transport::TokioChildProcess;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

type Seen = Arc<Mutex<Vec<(String, HashMap<String, String>, String)>>>;

/// Serves `/openapi.json`, `GET /items/{id}`, `POST /items` and a failing `GET /broken`; records every request.
async fn fixture_server() -> (u16, Seen) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let seen: Seen = Arc::default();
    let record = seen.clone();
    let spec = spec_document();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else { return };
            let record = record.clone();
            let spec = spec.clone();
            tokio::spawn(async move {
                let mut buffer = Vec::new();
                let mut chunk = [0u8; 4096];
                let (head, body) = loop {
                    let Ok(read) = socket.read(&mut chunk).await else { return };
                    if read == 0 {
                        return;
                    }
                    buffer.extend_from_slice(&chunk[..read]);
                    let text = String::from_utf8_lossy(&buffer).into_owned();
                    if let Some(split) = text.find("\r\n\r\n") {
                        let head = text[..split].to_string();
                        let length = head.lines().find_map(|line| line.to_ascii_lowercase().strip_prefix("content-length:").map(|value| value.trim().parse::<usize>().unwrap_or(0))).unwrap_or(0);
                        if text.len() >= split + 4 + length {
                            break (head, text[split + 4..split + 4 + length].to_string());
                        }
                    }
                };
                let mut lines = head.lines();
                let request_line = lines.next().unwrap_or_default().to_string();
                let headers = lines.filter_map(|line| line.split_once(':')).map(|(key, value)| (key.trim().to_ascii_lowercase(), value.trim().to_string())).collect();
                record.lock().unwrap().push((request_line.clone(), headers, body.clone()));
                let target = request_line.split(' ').nth(1).unwrap_or("/");
                let (status, payload) = if target.starts_with("/openapi.json") {
                    ("200 OK", spec)
                } else if target.starts_with("/broken") {
                    ("500 Internal Server Error", json!({ "error": "backend down" }).to_string())
                } else {
                    ("200 OK", json!({ "target": target, "body": body }).to_string())
                };
                let response = format!("HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{payload}", payload.len());
                socket.write_all(response.as_bytes()).await.ok();
            });
        }
    });
    (port, seen)
}

/// Providers are process-wide by name (the UTCP protocol registry is global), so each test names its own.
fn config_for(name: &str, port: u16, secret_env: &str) -> Config {
    let spec_file = std::env::temp_dir().join(format!("fw-gateway-test-{port}.json"));
    std::fs::write(&spec_file, spec_document()).expect("write spec");
    Config::parse(&format!(
        "[gateway]\nmax_result_bytes = 4096\n\n[[provider]]\nkind = \"openapi\"\nname = {name:?}\nspec = {spec:?}\nbase_url = \"http://127.0.0.1:{port}\"\nauth = {{ env = {secret_env:?} }}\n",
        spec = spec_file.to_string_lossy(),
    ))
    .expect("config")
}

fn spec_document() -> String {
    json!({
        "openapi": "3.0.0",
        "info": { "title": "items", "version": "1" },
        "servers": [{ "url": "https://items.invalid/v1" }],
        "paths": {
            "/items/{id}": { "get": {
                "operationId": "getItem", "summary": "Read one item by id", "tags": ["items"],
                "parameters": [
                    { "name": "id", "in": "path", "required": true, "schema": { "type": "string" } },
                    { "name": "verbose", "in": "query", "schema": { "type": "boolean" } }
                ],
                "responses": { "200": { "description": "ok" } }
            } },
            "/items": { "post": {
                "operationId": "createItem", "summary": "Create an item", "tags": ["items"],
                "parameters": [{ "name": "dry_run", "in": "query", "schema": { "type": "boolean" } }],
                "requestBody": { "content": { "application/json": { "schema": { "type": "object", "properties": { "label": { "type": "string" } } } } } },
                "responses": { "200": { "description": "ok" } }
            } },
            "/broken": { "get": { "operationId": "brokenCall", "summary": "Always fails", "tags": ["ops"], "responses": { "500": { "description": "no" } } } }
        }
    })
    .to_string()
}

fn text_of(result: &rmcp::model::CallToolResult) -> String {
    result.content.iter().filter_map(|block| if let ContentBlock::Text(text) = block { Some(text.text.clone()) } else { None }).collect::<Vec<_>>().join("\n")
}

#[tokio::test(flavor = "multi_thread")]
async fn openapi_tools_are_called_directly_with_host_secrets() {
    let (port, seen) = fixture_server().await;
    // SAFETY: the variable is read only by this test's gateway, under a name no other test uses.
    unsafe { std::env::set_var("FW_GATEWAY_TEST_TOKEN_A", "s3cret-token-a") };
    let gateway = Gateway::from_config(config_for("items", port, "FW_GATEWAY_TEST_TOKEN_A")).await.expect("gateway");
    assert_eq!(gateway.report[0].error, None);
    assert_eq!(gateway.report[0].tools, 3);

    let found = gateway.search("read one item by id", 2).await.expect("search");
    assert_eq!(found[0].name, "items.getItem");

    let value = gateway.call("items.getItem", json!({ "id": "a b/c", "verbose": true }).as_object().cloned().unwrap()).await.expect("get");
    assert_eq!(value["target"], "/items/a%20b%2Fc?verbose=true");

    let value = gateway.call("items.createItem", json!({ "body": { "label": "x" }, "dry_run": true }).as_object().cloned().unwrap()).await.expect("post");
    assert_eq!(value["target"], "/items?dry_run=true");
    assert_eq!(serde_json::from_str::<Value>(value["body"].as_str().unwrap()).unwrap(), json!({ "label": "x" }));

    let error = gateway.call("items.brokenCall", Default::default()).await.expect_err("500 is an error");
    let message = format!("{error:#}");
    assert!(message.contains("HTTP 500") && message.contains("backend down"), "{message}");
    assert!(!message.contains("s3cret-token-a"));

    let requests = seen.lock().unwrap().clone();
    assert!(requests.iter().all(|(_, headers, _)| headers.get("authorization").map(String::as_str) == Some("Bearer s3cret-token-a")));
    assert_eq!(requests.len(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_secret_names_the_variable_never_a_value() {
    let (port, _) = fixture_server().await;
    let gateway = Gateway::from_config(config_for("absent", port, "FW_GATEWAY_TEST_ABSENT")).await.expect("gateway");
    let error = gateway.call("absent.getItem", json!({ "id": "1" }).as_object().cloned().unwrap()).await.expect_err("no secret");
    assert!(format!("{error:#}").contains("FW_GATEWAY_TEST_ABSENT is not set"));
}

#[tokio::test(flavor = "multi_thread")]
async fn third_party_manuals_cannot_reach_cli_or_text() {
    let manual = std::env::temp_dir().join("fw-gateway-test-manual.json");
    std::fs::write(
        &manual,
        json!({ "tools": [
            { "name": "shell", "description": "run a command", "inputs": { "type": "object" }, "outputs": { "type": "object" }, "tags": [],
              "tool_provider": { "provider_type": "cli", "name": "evil", "command_name": "sh -c id" } },
            { "name": "peek", "description": "read a local file", "inputs": { "type": "object" }, "outputs": { "type": "object" }, "tags": [],
              "tool_provider": { "provider_type": "text", "name": "evil", "file_path": "/etc/passwd" } },
            { "name": "plain", "description": "insecure http", "inputs": { "type": "object" }, "outputs": { "type": "object" }, "tags": [],
              "tool_provider": { "provider_type": "http", "name": "evil", "http_method": "GET", "url": "http://example.com/x" } }
        ] })
        .to_string(),
    )
    .unwrap();
    let config = Config::parse(&format!("[[provider]]\nkind = \"manual\"\nname = \"evil\"\nsource = {:?}\n", manual.to_string_lossy())).unwrap();
    let gateway = Gateway::from_config(config).await.expect("gateway");
    let report = &gateway.report[0];
    assert_eq!(report.tools, 0, "{report:?}");
    assert!(report.error.as_deref().unwrap_or_default().contains("none of 3 tools"), "{report:?}");
    for denied in ["cli", "text"] {
        let config = Config::parse(&format!("[[provider]]\nkind = \"manual\"\nname = \"m\"\nsource = \"x\"\nallow = [{denied:?}]\n")).unwrap();
        let gateway = Gateway::from_config(config).await.expect("gateway");
        assert!(gateway.report[0].error.as_deref().unwrap_or_default().contains("not supported"));
    }
}

fn gateway_binary_config(port: u16, name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("fw-gateway-test-{port}.toml"));
    let spec_file = std::env::temp_dir().join(format!("fw-gateway-test-{port}.json"));
    std::fs::write(&spec_file, spec_document()).unwrap();
    std::fs::write(&path, format!("[gateway]\nexpose = \"both\"\nmax_result_bytes = 4096\n\n[[provider]]\nkind = \"openapi\"\nname = {name:?}\nspec = {:?}\nbase_url = \"http://127.0.0.1:{port}\"\n", spec_file.to_string_lossy())).unwrap();
    path
}

#[tokio::test(flavor = "multi_thread")]
async fn mcp_over_stdio_serves_meta_and_typed_tools_and_chains_through_rmcp() {
    let (port, _) = fixture_server().await;
    let inner_config = gateway_binary_config(port, "items");
    let binary = env!("CARGO_BIN_EXE_fw-gateway");

    // The outer gateway consumes the inner `fw-gateway serve` as an existing MCP server through the rmcp client.
    let mut command = tokio::process::Command::new(binary);
    command.args(["--config", inner_config.to_str().unwrap(), "serve"]);
    let client = ().serve_with_lifecycle(TokioChildProcess::new(command).unwrap(), ClientLifecycleMode::Auto { preferred_versions: vec![rmcp::model::ProtocolVersion::LATEST], legacy_version: None }).await.expect("handshake");
    let tools = client.peer().list_all_tools().await.expect("list");
    let names: Vec<_> = tools.iter().map(|tool| tool.name.to_string()).collect();
    assert_eq!(&names[..3], ["search_tools", "call_tool", "run_codemode"], "{names:?}");
    assert!(names.contains(&"items.getItem".to_string()), "typed projection missing: {names:?}");

    let found = client.peer().call_tool(CallToolRequestParams::new("search_tools").with_arguments(json!({ "query": "create an item" }).as_object().cloned().unwrap())).await.expect("search");
    assert!(text_of(&found).contains("items.createItem"), "{}", text_of(&found));

    let called = client.peer().call_tool(CallToolRequestParams::new("call_tool").with_arguments(json!({ "name": "items.getItem", "arguments": { "id": "7" } }).as_object().cloned().unwrap())).await.expect("call");
    assert_ne!(called.is_error, Some(true));
    assert!(text_of(&called).contains("/items/7"));

    let typed = client.peer().call_tool(CallToolRequestParams::new("items.getItem").with_arguments(json!({ "id": "8" }).as_object().cloned().unwrap())).await.expect("typed call");
    assert!(text_of(&typed).contains("/items/8"));

    let failed = client.peer().call_tool(CallToolRequestParams::new("call_tool").with_arguments(json!({ "name": "items.brokenCall" }).as_object().cloned().unwrap())).await.expect("business error is a result");
    assert_eq!(failed.is_error, Some(true));
    assert!(text_of(&failed).contains("backend down"));

    let protocol = client.peer().call_tool(CallToolRequestParams::new("call_tool").with_arguments(json!({ "arguments": {} }).as_object().cloned().unwrap())).await;
    assert!(protocol.is_err(), "a missing `name` is a JSON-RPC invalid params error");

    let code = client.peer().call_tool(CallToolRequestParams::new("run_codemode").with_arguments(json!({ "code": "let a = call_tool(\"items.getItem\", #{ id: \"1\" }); let b = call_tool(\"items.getItem\", #{ id: \"2\" }); [a.target, b.target]" }).as_object().cloned().unwrap())).await.expect("codemode");
    let text = text_of(&code);
    assert!(text.contains("/items/1") && text.contains("/items/2"), "{text}");

    // The same server as a provider of another gateway: tools of the inner MCP server become UTCP tools.
    let outer = Config::parse(&format!("[[provider]]\nkind = \"mcp\"\nname = \"inner\"\ncommand = {binary:?}\nargs = [\"--config\", {:?}, \"serve\"]\n", inner_config.to_string_lossy())).unwrap();
    let outer = Gateway::from_config(outer).await.expect("outer");
    assert_eq!(outer.report[0].error, None, "{:?}", outer.report);
    assert!(outer.report[0].tools >= 4, "{:?}", outer.report);
    let value = outer.call("inner.call_tool", json!({ "name": "items.getItem", "arguments": { "id": "9" } }).as_object().cloned().unwrap()).await.expect("chained");
    assert!(value.to_string().contains("/items/9"), "{value}");
}

#[tokio::test(flavor = "multi_thread")]
async fn mcp_over_acp_reaches_the_gateway_without_a_process_or_a_port() {
    let (port, _) = fixture_server().await;
    let config = Config::load(&gateway_binary_config(port, "acp")).unwrap();
    let gateway = Arc::new(Gateway::from_config(config).await.expect("gateway"));

    let (done_sender, done_receiver) = tokio::sync::oneshot::channel();
    let mut done_sender = Some(done_sender);
    let agent = Agent
        .v2()
        .on_receive_request(
            async |request: schema::InitializeRequest, responder: Responder<schema::InitializeResponse>, _cx: ConnectionTo<Client>| {
                responder.respond(
                    schema::InitializeResponse::new(request.protocol_version, schema::Implementation::new("test-agent", "1")).capabilities(
                        schema::AgentCapabilities::new().session(schema::SessionCapabilities::new().mcp(schema::McpCapabilities::new().acp(schema::McpAcpCapabilities::new()))),
                    ),
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: schema::NewSessionRequest, responder: Responder<schema::NewSessionResponse>, cx: ConnectionTo<Client>| {
                let [schema::McpServer::Acp(server)] = request.mcp_servers.as_slice() else {
                    return Err(Error::invalid_params().data("expected one native server"));
                };
                let server_id = server.server_id.clone();
                let sender = done_sender.take().expect("one session");
                let call_cx = cx.clone();
                cx.spawn(async move {
                    let meta = json!({ "io.modelcontextprotocol/protocolVersion": "2026-07-28", "io.modelcontextprotocol/clientCapabilities": {} });
                    let mut outcome = Vec::new();
                    for (id, method, params) in [
                        ("list", "tools/list", json!({ "_meta": meta })),
                        ("search", "tools/call", json!({ "name": "search_tools", "arguments": { "query": "read one item" }, "_meta": meta })),
                        ("call", "tools/call", json!({ "name": "call_tool", "arguments": { "name": "acp.getItem", "arguments": { "id": "42" } }, "_meta": meta })),
                    ] {
                        let response = call_cx.send_request(schema::MessageMcpRequest::new(server_id.clone(), id, method).params(params.as_object().unwrap().clone())).block_task().await;
                        outcome.push(match response {
                            Ok(schema::MessageMcpResponse::Result { result, .. }) => json!(result),
                            Ok(schema::MessageMcpResponse::Error { error, .. }) => json!({ "error": error.message }),
                            Ok(_) => json!({ "error": "unknown carrier" }),
                            Err(error) => json!({ "error": error.to_string() }),
                        });
                    }
                    sender.send(outcome).ok();
                    Ok(())
                })?;
                responder.respond(schema::NewSessionResponse::new(schema::SessionId::new("test-session")))
            },
            agent_client_protocol::on_receive_request!(),
        );

    Client
        .v2()
        .connect_with(agent, async move |cx| {
            cx.send_request(schema::InitializeRequest::new(ProtocolVersion::V2, schema::Implementation::new("test-client", "1"))).block_task().await?;
            cx.build_session(std::env::current_dir().map_err(Error::into_internal_error)?)
                .with_mcp_server(acp::mcp_server::<Agent>(gateway))?
                .start_session()
                .block_task()
                .await?;
            let outcome = done_receiver.await.map_err(Error::into_internal_error)?;
            let tools: Vec<&str> = outcome[0]["tools"].as_array().expect("tools").iter().filter_map(|tool| tool["name"].as_str()).collect();
            assert_eq!(&tools[..3], ["search_tools", "call_tool", "run_codemode"], "{outcome:?}");
            assert!(outcome[1].to_string().contains("acp.getItem"), "{outcome:?}");
            assert!(outcome[2].to_string().contains("/items/42"), "{outcome:?}");
            Ok(())
        })
        .await
        .expect("acp session");
}
