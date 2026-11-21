//! Protocol tests for the MCP engine over both transports, driven by the conformance fixture catalog
//! (the official suite runs against the same catalog in CI: tools/mcp-conformance/run.ts).

use fw_proto::mcp::{serve_stdio, Engine};
use fw_proto::mcp_catalog::FwCatalog;
use fw_proto::mcp_fixture::FixtureCatalog;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const META: &str = "io.modelcontextprotocol";

fn modern_meta(extra: Value) -> Value {
    let mut meta = json!({
        format!("{META}/protocolVersion"): "2026-07-28",
        format!("{META}/clientInfo"): { "name": "test", "version": "0" },
        format!("{META}/clientCapabilities"): {}
    });
    if let (Some(target), Some(extra)) = (meta.as_object_mut(), extra.as_object()) {
        target.extend(extra.clone());
    }
    meta
}

fn modern(id: u64, method: &str, mut params: Value, extra_meta: Value) -> Value {
    params["_meta"] = modern_meta(extra_meta);
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

// ---- stdio ----------------------------------------------------------------------------------------------------

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Write for Capture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Capture {
    fn messages(&self) -> Vec<Value> {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap().lines().map(|line| serde_json::from_str(line).unwrap()).collect()
    }
}

fn stdio_session(lines: &[Value]) -> Vec<Value> {
    let mut input = lines.iter().map(Value::to_string).collect::<Vec<_>>().join("\n");
    input.push('\n');
    let output = Capture::default();
    let engine = Arc::new(Engine::new(Arc::new(FixtureCatalog::default())));
    serve_stdio(&engine, std::io::Cursor::new(input), output.clone()).unwrap();
    output.messages()
}

fn reply_for(messages: &[Value], id: u64) -> &Value {
    messages.iter().find(|message| message.get("id") == Some(&json!(id))).unwrap_or_else(|| panic!("no reply for id {id}: {messages:?}"))
}

#[test]
fn stdio_discover_and_modern_calls_carry_result_type_cache_hints_and_server_info() {
    let out = stdio_session(&[modern(1, "server/discover", json!({}), json!({})), modern(2, "tools/list", json!({}), json!({})), modern(3, "tools/call", json!({ "name": "test_simple_text" }), json!({}))]);
    let discover = &reply_for(&out, 1)["result"];
    assert_eq!(discover["supportedVersions"][0], "2026-07-28");
    assert_eq!(discover["resultType"], "complete");
    assert!(discover["ttlMs"].as_u64().is_some() && discover["cacheScope"] == "public");
    assert_eq!(discover["_meta"][format!("{META}/serverInfo")]["name"], "fw-mcp-conformance-fixture");
    let list = &reply_for(&out, 2)["result"];
    assert!(list["tools"].as_array().unwrap().iter().all(|tool| tool["inputSchema"]["type"] == "object"));
    assert_eq!(reply_for(&out, 3)["result"]["content"][0]["text"], "This is a simple text response for testing.");
}

#[test]
fn stdio_rejects_what_the_specification_rejects() {
    let no_meta = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {} });
    let wrong_version = modern(2, "tools/list", json!({}), json!({ format!("{META}/protocolVersion"): "1900-01-01" }));
    let removed = modern(3, "ping", json!({}), json!({}));
    let unknown_tool = modern(4, "tools/call", json!({ "name": "nope" }), json!({}));
    let bad_args = modern(5, "tools/call", json!({ "name": "test_simple_text", "arguments": [] }), json!({}));
    let bad_cursor = modern(6, "tools/list", json!({ "cursor": "x" }), json!({}));
    let bad_log = modern(7, "tools/list", json!({}), json!({ format!("{META}/logLevel"): "loud" }));
    let missing_capability = modern(8, "tools/call", json!({ "name": "test_missing_capability" }), json!({}));
    let out = stdio_session(&[no_meta, wrong_version, removed, unknown_tool, bad_args, bad_cursor, bad_log, missing_capability]);
    assert_eq!(reply_for(&out, 1)["error"]["code"], -32602);
    let unsupported = &reply_for(&out, 2)["error"];
    assert_eq!(unsupported["code"], -32022);
    assert_eq!(unsupported["data"]["requested"], "1900-01-01");
    assert_eq!(unsupported["data"]["supported"][0], "2026-07-28");
    assert_eq!(reply_for(&out, 3)["error"]["code"], -32601, "ping was removed in 2026-07-28");
    assert_eq!(reply_for(&out, 4)["error"]["code"], -32602);
    assert_eq!(reply_for(&out, 5)["error"]["code"], -32602);
    assert_eq!(reply_for(&out, 6)["error"]["code"], -32602);
    assert_eq!(reply_for(&out, 7)["error"]["code"], -32602);
    let capability = &reply_for(&out, 8)["error"];
    assert_eq!(capability["code"], -32021);
    assert_eq!(capability["data"]["requiredCapabilities"], json!({ "sampling": {} }));
}

#[test]
fn stdio_malformed_input_gets_parse_and_invalid_request_errors() {
    let output = Capture::default();
    let engine = Arc::new(Engine::new(Arc::new(FixtureCatalog::default())));
    let input = "{not json\n{\"jsonrpc\":\"2.0\",\"id\":null,\"method\":\"x\"}\n[1]\n";
    serve_stdio(&engine, std::io::Cursor::new(input), output.clone()).unwrap();
    let out = output.messages();
    assert_eq!(out.len(), 3);
    assert_eq!(out[0]["error"]["code"], -32700);
    assert_eq!(out[1]["error"]["code"], -32600);
    assert_eq!(out[2]["error"]["code"], -32600);
    assert!(out.iter().all(|message| message["id"].is_null()));
}

#[test]
fn stdio_legacy_era_starts_with_initialize_and_keeps_session_state() {
    let out = stdio_session(&[
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "old", "version": "1" } } }),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "ping" }),
        json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list" }),
        json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "test_simple_text" } }),
        json!({ "jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": { "name": "test_input_required_result_elicitation" } }),
        json!({ "jsonrpc": "2.0", "id": 6, "method": "server/discover" }),
    ]);
    assert_eq!(reply_for(&out, 1)["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(reply_for(&out, 2)["result"], json!({}));
    let list = &reply_for(&out, 3)["result"];
    assert!(list.get("resultType").is_none(), "earlier versions carry no resultType");
    assert!(!list["tools"].as_array().unwrap().is_empty());
    assert_eq!(reply_for(&out, 4)["result"]["isError"], Value::Null);
    assert_eq!(reply_for(&out, 5)["result"]["isError"], true, "a tool that needs MRTR reports it as a tool error to an earlier-version client");
    assert_eq!(reply_for(&out, 6)["error"]["code"], -32601);
    assert_eq!(out.len(), 6, "the notification gets no reply");
}

#[test]
fn stdio_progress_and_logs_are_request_scoped_and_opt_in() {
    let with = modern(1, "tools/call", json!({ "name": "test_tool_with_progress" }), json!({ "progressToken": "tok" }));
    let without = modern(2, "tools/call", json!({ "name": "test_tool_with_progress" }), json!({}));
    let logs = modern(3, "tools/call", json!({ "name": "test_tool_with_logging" }), json!({ format!("{META}/logLevel"): "info" }));
    let silent = modern(4, "tools/call", json!({ "name": "test_tool_with_logging" }), json!({}));
    let out = stdio_session(&[with, without, logs, silent]);
    let progress: Vec<f64> = out.iter().filter(|m| m["method"] == "notifications/progress").map(|m| m["params"]["progress"].as_f64().unwrap()).collect();
    assert_eq!(progress, vec![0.0, 50.0, 100.0], "only the request with a progressToken gets progress, increasing");
    assert!(out.iter().filter(|m| m["method"] == "notifications/progress").all(|m| m["params"]["progressToken"] == "tok"));
    let messages = out.iter().filter(|m| m["method"] == "notifications/message").count();
    assert_eq!(messages, 3, "only the request that set logLevel gets logs");
}

#[test]
fn stdio_cancel_stops_work_and_sends_nothing_for_the_request() {
    let (read_end, mut write_end) = std::io::pipe().unwrap();
    let output = Capture::default();
    let engine = Arc::new(Engine::new(Arc::new(FixtureCatalog::default())));
    let sink = output.clone();
    let server = std::thread::spawn(move || serve_stdio(&engine, BufReader::new(read_end), sink).unwrap());
    let call = modern(1, "tools/call", json!({ "name": "test_tool_with_progress" }), json!({ "progressToken": 1 }));
    writeln!(write_end, "{call}").unwrap();
    // Wait for the first progress notification, then cancel.
    let started = std::time::Instant::now();
    while !output.messages().iter().any(|m| m["method"] == "notifications/progress") {
        assert!(started.elapsed() < Duration::from_secs(5), "no progress seen");
        std::thread::sleep(Duration::from_millis(5));
    }
    writeln!(write_end, "{}", json!({ "jsonrpc": "2.0", "method": "notifications/cancelled", "params": { "requestId": 1 } })).unwrap();
    writeln!(write_end, "{}", modern(2, "tools/call", json!({ "name": "test_simple_text" }), json!({}))).unwrap();
    drop(write_end);
    server.join().unwrap();
    let out = output.messages();
    assert!(out.iter().all(|m| m.get("id") != Some(&json!(1))), "a cancelled request gets no response: {out:?}");
    assert!(reply_for(&out, 2).get("result").is_some(), "other requests are unaffected");
    let progress = out.iter().filter(|m| m["method"] == "notifications/progress").count();
    assert!(progress < 3, "work stopped after the cancel ({progress} progress messages)");
}

#[test]
fn stdio_subscriptions_listen_acknowledges_first_and_delivers_only_what_was_asked() {
    let (read_end, mut write_end) = std::io::pipe().unwrap();
    let output = Capture::default();
    let engine = Arc::new(Engine::new(Arc::new(FixtureCatalog::default())));
    let sink = output.clone();
    let server = std::thread::spawn(move || serve_stdio(&engine, BufReader::new(read_end), sink).unwrap());
    writeln!(write_end, "{}", modern(7, "subscriptions/listen", json!({ "notifications": { "toolsListChanged": true } }), json!({}))).unwrap();
    let wait_for = |count: usize| {
        let started = std::time::Instant::now();
        while output.messages().len() < count {
            assert!(started.elapsed() < Duration::from_secs(5), "expected {count} messages, got {:?}", output.messages());
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    wait_for(1);
    writeln!(write_end, "{}", modern(8, "tools/call", json!({ "name": "test_trigger_prompt_change" }), json!({}))).unwrap();
    writeln!(write_end, "{}", modern(9, "tools/call", json!({ "name": "test_trigger_tool_change" }), json!({}))).unwrap();
    wait_for(4);
    writeln!(write_end, "{}", json!({ "jsonrpc": "2.0", "method": "notifications/cancelled", "params": { "requestId": 7 } })).unwrap();
    drop(write_end);
    server.join().unwrap();
    let out = output.messages();
    assert_eq!(out[0]["method"], "notifications/subscriptions/acknowledged");
    assert_eq!(out[0]["params"]["_meta"][format!("{META}/subscriptionId")], 7);
    let changes: Vec<&str> = out.iter().filter_map(|m| m["method"].as_str()).filter(|method| method.ends_with("list_changed")).collect();
    assert_eq!(changes, vec!["notifications/tools/list_changed"], "the prompts change was not subscribed to");
    assert!(out.iter().all(|m| m.get("id") != Some(&json!(7))), "a cancelled listen sends no response");
}

#[test]
fn input_required_state_is_integrity_protected() {
    let first = stdio_session(&[modern(1, "tools/call", json!({ "name": "test_input_required_result_tampered_state" }), json!({}))]);
    let result = &reply_for(&first, 1)["result"];
    assert_eq!(result["resultType"], "input_required");
    let state = result["requestState"].as_str().unwrap().to_owned();
    let retry = |state: &str, id: u64| modern(id, "tools/call", json!({ "name": "test_input_required_result_tampered_state", "requestState": state, "inputResponses": { "confirm": { "action": "accept", "content": { "ok": true } } } }), json!({}));
    let tampered = state.replace("tamper-test", "tamper-tesx");
    let out = stdio_session(&[retry(&tampered, 2)]);
    assert_eq!(reply_for(&out, 2)["error"]["code"], -32602);
    let bad_responses = stdio_session(&[modern(3, "tools/call", json!({ "name": "test_input_required_result_elicitation", "inputResponses": { "user_name": 12345 } }), json!({}))]);
    assert_eq!(reply_for(&bad_responses, 3)["error"]["code"], -32602);
}

// ---- the real catalog ---------------------------------------------------------------------------------------

#[test]
fn fw_catalog_lists_unique_documented_tools_with_object_schemas() {
    use fw_proto::mcp::Catalog;
    let tools = FwCatalog::new(std::env::temp_dir()).tools();
    let names: Vec<&str> = tools.iter().map(|tool| tool["name"].as_str().unwrap()).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), names.len(), "tool names are unique");
    assert_eq!(names, {
        let mut again = names.clone();
        again.sort_unstable();
        again
    }, "the list order is deterministic");
    for tool in &tools {
        assert!(tool["description"].as_str().is_some_and(|text| !text.is_empty()), "{tool}");
        assert_eq!(tool["inputSchema"]["type"], "object", "{tool}");
        assert!(tool["annotations"]["readOnlyHint"].is_boolean(), "{tool}");
    }
    for unrunnable in ["web_search", "subagent", "read_tool_result", "ask_user_question", "install_skill", "toolbox"] {
        assert!(!names.contains(&unrunnable), "{unrunnable} needs the agent host and must not be listed");
    }
}

#[test]
fn fw_catalog_tool_errors_are_results_not_protocol_errors() {
    let engine = Engine::new(Arc::new(FwCatalog::new(std::env::temp_dir())));
    let call = |name: &str, arguments: Value| {
        let headers = fw_proto::mcp::HttpMeta::from_headers([("MCP-Protocol-Version", "2026-07-28"), ("Mcp-Method", "tools/call"), ("Mcp-Name", name)]);
        let (status, body) = fw_proto::mcp::handle_json(&engine, &headers, &modern(1, "tools/call", json!({ "name": name, "arguments": arguments }), json!({})).to_string());
        (status, serde_json::from_str::<Value>(&body).unwrap())
    };
    let (status, missing) = call("crates_info", json!({}));
    assert_eq!(status, 200);
    assert_eq!(missing["result"]["isError"], true, "{missing}");
    let (_, probe) = call("linux_kernel_mcp", json!({ "action": "probe" }));
    assert_eq!(probe["result"]["structuredContent"]["result"]["kernel"], "linux");
    let (_, wrong_type) = call("read_file", json!({ "path": 5 }));
    assert_eq!(wrong_type["result"]["isError"], true);
    assert!(wrong_type["result"]["content"][0]["text"].as_str().unwrap().contains("path"), "{wrong_type}");
}

// ---- Streamable HTTP ------------------------------------------------------------------------------------------

async fn start_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(fw_proto::serve_on(listener, Arc::new(FixtureCatalog::default())));
    addr.to_string()
}

async fn post(addr: &str, headers: &[(&str, &str)], body: &Value) -> (u16, String, String) {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let body = body.to_string();
    let mut request = format!("POST /mcp HTTP/1.1\r\nHost: {addr}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\n", body.len());
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    request.push_str(&body);
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).await.unwrap();
    let (head, payload) = raw.split_once("\r\n\r\n").unwrap();
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, head.to_owned(), payload.to_owned())
}

fn standard_headers(method: &str, name: Option<&str>) -> Vec<(String, String)> {
    let mut headers = vec![("MCP-Protocol-Version".to_owned(), "2026-07-28".to_owned()), ("Mcp-Method".to_owned(), method.to_owned())];
    if let Some(name) = name {
        headers.push(("Mcp-Name".to_owned(), name.to_owned()));
    }
    headers
}

async fn post_modern(addr: &str, body: &Value, name: Option<&str>) -> (u16, String, String) {
    let method = body["method"].as_str().unwrap().to_owned();
    let headers = standard_headers(&method, name);
    let refs: Vec<(&str, &str)> = headers.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    post(addr, &refs, body).await
}

#[tokio::test]
async fn http_answers_json_and_validates_mirrored_headers() {
    let addr = start_server().await;
    let list = modern(1, "tools/list", json!({}), json!({}));
    let (status, head, payload) = post_modern(&addr, &list, None).await;
    assert_eq!(status, 200);
    assert!(head.to_ascii_lowercase().contains("content-type: application/json"));
    assert_eq!(serde_json::from_str::<Value>(&payload).unwrap()["result"]["resultType"], "complete");

    let call = modern(2, "tools/call", json!({ "name": "test_simple_text" }), json!({}));
    let (status, _, payload) = post_modern(&addr, &call, Some("other_tool")).await;
    assert_eq!(status, 400);
    assert_eq!(serde_json::from_str::<Value>(&payload).unwrap()["error"]["code"], -32020);
    let (status, _, payload) = post(&addr, &[("Mcp-Method", "tools/call"), ("Mcp-Name", "test_simple_text")], &call).await;
    assert_eq!((status, serde_json::from_str::<Value>(&payload).unwrap()["error"]["code"].as_i64()), (400, Some(-32020)), "the version header is required");
    let (status, _, payload) = post(&addr, &[("MCP-Protocol-Version", "2026-07-28"), ("Mcp-Method", "tools/list")], &call).await;
    assert_eq!((status, serde_json::from_str::<Value>(&payload).unwrap()["error"]["code"].as_i64()), (400, Some(-32020)), "Mcp-Method must match the body");
    let (status, _, _) = post_modern(&addr, &modern(3, "no/such", json!({}), json!({})), None).await;
    assert_eq!(status, 404);
    let (status, _, payload) = post(&addr, &[("MCP-Protocol-Version", "2026-07-28"), ("Mcp-Method", "tools/list")], &json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/list", "params": {} })).await;
    assert_eq!((status, serde_json::from_str::<Value>(&payload).unwrap()["error"]["code"].as_i64()), (400, Some(-32602)));
    let old = modern(5, "tools/list", json!({}), json!({ format!("{META}/protocolVersion"): "1900-01-01" }));
    let (status, _, payload) = post(&addr, &[("MCP-Protocol-Version", "1900-01-01"), ("Mcp-Method", "tools/list")], &old).await;
    assert_eq!(status, 400);
    assert_eq!(serde_json::from_str::<Value>(&payload).unwrap()["error"]["code"], -32022);
}

#[tokio::test]
async fn http_mirrors_x_mcp_header_parameters_and_decodes_base64() {
    let addr = start_server().await;
    let call = |region: &str, tenant: u64| modern(1, "tools/call", json!({ "name": "test_header_param", "arguments": { "region": region, "tenant": { "id": tenant } } }), json!({}));
    let send = |region: &str, headers: Vec<(&'static str, String)>| {
        let addr = addr.clone();
        let body = call(region, 42);
        async move {
            let mut all = standard_headers("tools/call", Some("test_header_param"));
            all.extend(headers.into_iter().map(|(a, b)| (a.to_owned(), b)));
            let refs: Vec<(&str, &str)> = all.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
            post(&addr, &refs, &body).await.0
        }
    };
    assert_eq!(send("us-west1", vec![("Mcp-Param-Region", "us-west1".into()), ("Mcp-Param-TenantId", "42".into())]).await, 200);
    assert_eq!(send("Hello, 世界", vec![("Mcp-Param-Region", "=?base64?SGVsbG8sIOS4lueVjA==?=".into()), ("Mcp-Param-TenantId", "42.0".into())]).await, 200, "base64 values decode and integers compare numerically");
    assert_eq!(send("us-west1", vec![("Mcp-Param-Region", "eu".into()), ("Mcp-Param-TenantId", "42".into())]).await, 400);
    assert_eq!(send("us-west1", vec![("Mcp-Param-TenantId", "42".into())]).await, 400, "a mirrored argument without its header is rejected");
    assert_eq!(send("Hello", vec![("Mcp-Param-Region", "=?base64?SGVsbG8?=".into()), ("Mcp-Param-TenantId", "42".into())]).await, 400, "unpadded base64 is invalid");
}

#[tokio::test]
async fn http_progress_arrives_on_an_event_stream_that_ends_with_the_response() {
    let addr = start_server().await;
    let call = modern(1, "tools/call", json!({ "name": "test_tool_with_progress" }), json!({ "progressToken": "t" }));
    let (status, head, payload) = post_modern(&addr, &call, Some("test_tool_with_progress")).await;
    assert_eq!(status, 200);
    assert!(head.to_ascii_lowercase().contains("content-type: text/event-stream"), "{head}");
    assert!(head.to_ascii_lowercase().contains("x-accel-buffering: no"));
    let events: Vec<Value> = payload.lines().filter_map(|line| line.strip_prefix("data: ")).map(|data| serde_json::from_str(data).unwrap()).collect();
    assert_eq!(events.len(), 4, "three progress notifications and the response: {payload}");
    assert!(events[..3].iter().all(|event| event["method"] == "notifications/progress"));
    assert_eq!(events[3]["id"], 1);
}

#[tokio::test]
async fn http_refuses_foreign_origins_and_non_post_methods() {
    let addr = start_server().await;
    let list = modern(1, "tools/list", json!({}), json!({}));
    let (status, _, _) = post(&addr, &[("Origin", "http://evil.example.com"), ("MCP-Protocol-Version", "2026-07-28"), ("Mcp-Method", "tools/list")], &list).await;
    assert_eq!(status, 403);
    let (status, _, _) = post(&addr, &[("Origin", &format!("http://{addr}")), ("MCP-Protocol-Version", "2026-07-28"), ("Mcp-Method", "tools/list")], &list).await;
    assert_eq!(status, 200);
    let mut stream = TcpStream::connect(&addr).await.unwrap();
    stream.write_all(format!("GET /mcp HTTP/1.1\r\nHost: {addr}\r\n\r\n").as_bytes()).await.unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).await.unwrap();
    assert!(raw.starts_with("HTTP/1.1 405"), "{raw}");
    assert!(raw.contains("Allow: POST"));
}

#[tokio::test]
async fn http_notifications_are_accepted_and_garbage_is_a_parse_error() {
    let addr = start_server().await;
    let (status, _, payload) = post(&addr, &[], &json!({ "jsonrpc": "2.0", "method": "notifications/cancelled", "params": { "requestId": 1 } })).await;
    assert_eq!((status, payload.as_str()), (202, ""));
    let mut stream = TcpStream::connect(&addr).await.unwrap();
    let body = "{oops";
    stream.write_all(format!("POST /mcp HTTP/1.1\r\nHost: {addr}\r\nContent-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).await.unwrap();
    assert!(raw.starts_with("HTTP/1.1 400"), "{raw}");
    assert!(raw.contains("-32700"));
}

#[tokio::test]
async fn http_closing_the_stream_cancels_the_request() {
    let addr = start_server().await;
    let listen = modern(1, "subscriptions/listen", json!({ "notifications": { "toolsListChanged": true } }), json!({}));
    let headers = standard_headers("subscriptions/listen", None);
    let mut stream = TcpStream::connect(&addr).await.unwrap();
    let body = listen.to_string();
    let mut request = format!("POST /mcp HTTP/1.1\r\nHost: {addr}\r\nContent-Length: {}\r\n", body.len());
    for (name, value) in &headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    request.push_str(&body);
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut buffer = vec![0u8; 4096];
    let mut seen = String::new();
    while !seen.contains("acknowledged") {
        let read = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buffer)).await.expect("no acknowledgement").unwrap();
        assert!(read > 0);
        seen.push_str(&String::from_utf8_lossy(&buffer[..read]));
    }
    assert!(seen.contains("text/event-stream"));
    // A change published by another request reaches the open stream.
    let trigger = modern(2, "tools/call", json!({ "name": "test_trigger_tool_change" }), json!({}));
    assert_eq!(post_modern(&addr, &trigger, Some("test_trigger_tool_change")).await.0, 200);
    while !seen.contains("tools/list_changed") {
        let read = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buffer)).await.expect("no list_changed").unwrap();
        assert!(read > 0);
        seen.push_str(&String::from_utf8_lossy(&buffer[..read]));
    }
    drop(stream);
    // The server stays healthy after the client left.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(post_modern(&addr, &modern(3, "tools/list", json!({}), json!({})), None).await.0, 200);
}

#[test]
fn stdio_server_reads_lines_incrementally() {
    // A request is answered while the pipe is still open: no waiting for end of input.
    let (read_end, mut write_end) = std::io::pipe().unwrap();
    let (mut answer_read, answer_write) = std::io::pipe().unwrap();
    let engine = Arc::new(Engine::new(Arc::new(FixtureCatalog::default())));
    let server = std::thread::spawn(move || serve_stdio(&engine, BufReader::new(read_end), answer_write).unwrap());
    writeln!(write_end, "{}", modern(1, "server/discover", json!({}), json!({}))).unwrap();
    let mut line = String::new();
    BufReader::new(&mut answer_read).read_line(&mut line).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&line).unwrap()["id"], 1);
    drop(write_end);
    server.join().unwrap();
}

// ---- earlier versions: sessions, server-initiated requests, resumption ---------------------------------------

async fn exchange(addr: &str, method: &str, headers: &[(&str, &str)], body: Option<&Value>) -> (u16, String, String) {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream.write_all(request_bytes(addr, method, headers, body).as_bytes()).await.unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).await.unwrap();
    let (head, payload) = raw.split_once("\r\n\r\n").unwrap();
    (head.split_whitespace().nth(1).unwrap().parse().unwrap(), head.to_owned(), payload.to_owned())
}

fn request_bytes(addr: &str, method: &str, headers: &[(&str, &str)], body: Option<&Value>) -> String {
    let payload = body.map(Value::to_string).unwrap_or_default();
    let mut request = format!("{method} /mcp HTTP/1.1\r\nHost: {addr}\r\nAccept: application/json, text/event-stream\r\nContent-Length: {}\r\n", payload.len());
    if body.is_some() {
        request.push_str("Content-Type: application/json\r\n");
    }
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    request.push_str(&payload);
    request
}

fn header_value(head: &str, name: &str) -> Option<String> {
    head.lines().find_map(|line| line.split_once(':').filter(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.trim().to_owned()))
}

fn event_messages(payload: &str) -> Vec<Value> {
    payload.lines().filter_map(|line| line.strip_prefix("data: ")).filter(|data| !data.is_empty()).map(|data| serde_json::from_str(data).unwrap()).collect()
}

async fn open_session(addr: &str, capabilities: Value) -> String {
    let initialize = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18", "capabilities": capabilities, "clientInfo": { "name": "old", "version": "1" } } });
    let (status, head, _) = exchange(addr, "POST", &[], Some(&initialize)).await;
    assert_eq!(status, 200);
    header_value(&head, "mcp-session-id").expect("initialize issues a session id")
}

fn legacy_call(id: u64, name: &str, arguments: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": arguments } })
}

#[tokio::test]
async fn http_sessions_open_at_initialize_and_end_with_delete() {
    let addr = start_server().await;
    let session = open_session(&addr, json!({})).await;
    let with_session = [("Mcp-Session-Id", session.as_str()), ("MCP-Protocol-Version", "2025-06-18")];
    let list = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" });
    assert_eq!(exchange(&addr, "POST", &with_session, Some(&list)).await.0, 200);
    assert_eq!(exchange(&addr, "GET", &with_session, None).await.0, 405, "no standalone stream is offered");
    assert_eq!(exchange(&addr, "POST", &[("Mcp-Session-Id", "no-such-session")], Some(&list)).await.0, 404);

    let modern_list = modern(3, "tools/list", json!({}), json!({}));
    let modern_headers = [("Mcp-Session-Id", "ignored"), ("MCP-Protocol-Version", "2026-07-28"), ("Mcp-Method", "tools/list")];
    assert_eq!(exchange(&addr, "POST", &modern_headers, Some(&modern_list)).await.0, 200, "2026-07-28 requests ignore the session header");
    assert_eq!(exchange(&addr, "DELETE", &[("Mcp-Session-Id", &session), ("MCP-Protocol-Version", "2026-07-28")], None).await.0, 405);

    assert_eq!(exchange(&addr, "DELETE", &with_session, None).await.0, 204);
    assert_eq!(exchange(&addr, "DELETE", &with_session, None).await.0, 404);
    assert_eq!(exchange(&addr, "POST", &with_session, Some(&list)).await.0, 404, "a session that ended is unknown");
}

#[tokio::test]
async fn http_tool_asks_the_client_for_sampling_on_its_response_stream() {
    let addr = start_server().await;
    let session = open_session(&addr, json!({ "sampling": {} })).await;
    let headers = [("Mcp-Session-Id", session.as_str()), ("MCP-Protocol-Version", "2025-06-18")];
    let mut stream = TcpStream::connect(&addr).await.unwrap();
    stream.write_all(request_bytes(&addr, "POST", &headers, Some(&legacy_call(7, "test_sampling", json!({ "prompt": "Capital of France?" })))).as_bytes()).await.unwrap();
    let mut seen = String::new();
    let mut buffer = vec![0u8; 4096];
    while !seen.contains("sampling/createMessage") {
        let read = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buffer)).await.expect("no sampling request").unwrap();
        assert!(read > 0, "{seen}");
        seen.push_str(&String::from_utf8_lossy(&buffer[..read]));
    }
    assert!(seen.contains("text/event-stream"), "{seen}");
    assert!(seen.contains("retry: "), "a resumable stream starts with a priming event: {seen}");
    let request = event_messages(seen.split_once("\r\n\r\n").unwrap().1).into_iter().find(|message| message["method"] == "sampling/createMessage").unwrap();
    assert_eq!(request["params"]["messages"][0]["content"]["text"], "Capital of France?");

    let answer = json!({ "jsonrpc": "2.0", "id": request["id"], "result": { "role": "assistant", "content": { "type": "text", "text": "Paris" }, "model": "test" } });
    assert_eq!(exchange(&addr, "POST", &headers, Some(&answer)).await.0, 202);
    let mut rest = String::new();
    stream.read_to_string(&mut rest).await.unwrap();
    let response = event_messages(&rest).pop().expect("the stream ends with the response");
    assert_eq!(response["id"], 7);
    assert_eq!(response["result"]["content"][0]["text"], "LLM response: Paris");

    let declined = open_session(&addr, json!({})).await;
    let (_, head, payload) = exchange(&addr, "POST", &[("Mcp-Session-Id", &declined)], Some(&legacy_call(8, "test_sampling", json!({ "prompt": "x" })))).await;
    assert!(head.to_ascii_lowercase().contains("application/json"), "{head}");
    assert_eq!(serde_json::from_str::<Value>(&payload).unwrap()["result"]["isError"], true, "a client that did not declare sampling is not asked");
}

#[tokio::test]
async fn http_a_closed_stream_resumes_with_last_event_id() {
    let addr = start_server().await;
    let session = open_session(&addr, json!({})).await;
    let headers = [("Mcp-Session-Id", session.as_str()), ("MCP-Protocol-Version", "2025-06-18")];
    let (status, head, payload) = exchange(&addr, "POST", &headers, Some(&legacy_call(9, "test_reconnection", json!({})))).await;
    assert_eq!(status, 200);
    assert!(head.to_ascii_lowercase().contains("text/event-stream"), "{head}");
    assert!(event_messages(&payload).is_empty(), "the stream closed before the response: {payload}");
    let priming = payload.lines().find_map(|line| line.strip_prefix("id: ")).expect("priming event id").to_owned();

    let resume = [("Mcp-Session-Id", session.as_str()), ("MCP-Protocol-Version", "2025-06-18"), ("Last-Event-ID", priming.as_str())];
    let (status, _, payload) = exchange(&addr, "GET", &resume, None).await;
    assert_eq!(status, 200);
    let response = event_messages(&payload).pop().expect("the resumed stream carries the response");
    assert_eq!(response["id"], 9);
    assert!(response["result"]["content"][0]["text"].as_str().unwrap().contains("after the stream was closed"), "{response}");
    assert_eq!(exchange(&addr, "GET", &resume, None).await.0, 404, "a delivered stream is gone");
}

#[test]
fn stdio_tool_asks_the_client_for_elicitation_and_reads_the_answer() {
    let (read_end, mut write_end) = std::io::pipe().unwrap();
    let (answer_read, answer_write) = std::io::pipe().unwrap();
    let engine = Arc::new(Engine::new(Arc::new(FixtureCatalog::default())));
    let server = std::thread::spawn(move || serve_stdio(&engine, BufReader::new(read_end), answer_write).unwrap());
    let mut lines = BufReader::new(answer_read);
    let mut next = || {
        let mut line = String::new();
        lines.read_line(&mut line).unwrap();
        serde_json::from_str::<Value>(&line).unwrap()
    };
    writeln!(write_end, "{}", json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18", "capabilities": { "elicitation": {} }, "clientInfo": { "name": "old", "version": "1" } } })).unwrap();
    assert_eq!(next()["id"], 1);
    writeln!(write_end, "{}", legacy_call(2, "test_elicitation", json!({ "message": "Who are you?" }))).unwrap();
    let request = next();
    assert_eq!(request["method"], "elicitation/create");
    assert_eq!(request["params"]["message"], "Who are you?");
    writeln!(write_end, "{}", json!({ "jsonrpc": "2.0", "id": request["id"], "result": { "action": "accept", "content": { "username": "ada", "email": "ada@example.com" } } })).unwrap();
    let response = next();
    assert_eq!(response["id"], 2);
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.starts_with("User response: action=accept") && text.contains("ada@example.com"), "{text}");
    drop(write_end);
    server.join().unwrap();
}
