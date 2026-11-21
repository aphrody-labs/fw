//! The catalog the MCP conformance suite (`@modelcontextprotocol/conformance`) drives: the tool, resource
//! and prompt names its server scenarios call. It is test content, served by
//! `cargo run -p fw-proto --example mcp_fixture_server`; `fw mcp` and `fw serve` never expose it.

use crate::mcp::{CallContext, Catalog, Identity, ListEvent, LogLevel, Outcome, RpcError};
use serde_json::{json, Value};
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

const PNG_PIXEL: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==";
const WAV_SAMPLE: &str = "UklGRiYAAABXQVZFZm10IBAAAAABAAEAQB8AAAB9AAACABAAZGF0YQIAAAA=";

pub struct FixtureCatalog {
    /// Keys the integrity tag of signed `requestState`; random per process, so a state never outlives it.
    mac_key: RandomState,
    nonce: AtomicU64,
}

impl Default for FixtureCatalog {
    fn default() -> Self {
        Self { mac_key: RandomState::new(), nonce: AtomicU64::new(1) }
    }
}

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({ "name": name, "description": description, "inputSchema": input_schema })
}

fn no_arguments() -> Value {
    json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

fn elicit(message: &str, field: &str, kind: &str) -> Value {
    json!({
        "method": "elicitation/create",
        "params": { "message": message, "requestedSchema": { "type": "object", "properties": { field: { "type": kind } }, "required": [field] } }
    })
}

fn sample(text: &str, max_tokens: u32) -> Value {
    json!({ "method": "sampling/createMessage", "params": { "messages": [{ "role": "user", "content": { "type": "text", "text": text } }], "maxTokens": max_tokens } })
}

fn response_text(responses: &Value, key: &str, field: &str) -> String {
    responses.get(key).and_then(|response| response.get("content")).and_then(|content| content.get(field)).and_then(Value::as_str).unwrap_or("unknown").to_owned()
}

impl FixtureCatalog {
    fn fresh_nonce(&self) -> u64 {
        self.nonce.fetch_add(1, Ordering::Relaxed)
    }

    fn sign(&self, payload: &Value) -> String {
        let data = payload.to_string();
        let tag = self.mac_key.hash_one(&data);
        json!({ "data": data, "mac": format!("{tag:016x}") }).to_string()
    }

    fn verify(&self, state: &str) -> Option<Value> {
        let envelope: Value = serde_json::from_str(state).ok()?;
        let data = envelope.get("data")?.as_str()?;
        let mac = envelope.get("mac")?.as_str()?;
        (mac == format!("{:016x}", self.mac_key.hash_one(data))).then(|| serde_json::from_str(data).ok()).flatten()
    }

    fn state(&self, mut payload: Value) -> String {
        payload["nonce"] = json!(self.fresh_nonce());
        payload.to_string()
    }

    fn mrtr(&self, name: &str, cx: &CallContext<'_>) -> Option<Outcome> {
        let responses = cx.input_responses.cloned().unwrap_or_else(|| json!({}));
        let has = |key: &str| responses.get(key).is_some();
        let again = |requests: Value, state: Option<String>| Outcome::InputRequired { requests: Some(requests), state };
        let confirm = || elicit("Please confirm", "ok", "boolean");
        Some(match name {
            "test_input_required_result_elicitation" => {
                if has("user_name") {
                    Outcome::text(format!("Hello, {}!", response_text(&responses, "user_name", "name")))
                } else {
                    again(json!({ "user_name": elicit("What is your name?", "name", "string") }), None)
                }
            }
            "test_input_required_result_sampling" => {
                if let Some(answer) = responses.get("sample_request") {
                    let text = answer.get("content").and_then(|content| content.get("text")).and_then(Value::as_str).unwrap_or("no response");
                    Outcome::text(format!("Sampling result: {text}"))
                } else {
                    again(json!({ "sample_request": sample("What is the capital of France?", 100) }), None)
                }
            }
            "test_input_required_result_list_roots" => match responses.get("roots_request") {
                Some(answer) => Outcome::text(format!("Found {} root(s)", answer.get("roots").and_then(Value::as_array).map_or(0, Vec::len))),
                None => again(json!({ "roots_request": { "method": "roots/list", "params": {} } }), None),
            },
            "test_input_required_result_request_state" => {
                let confirmed = responses.get("confirm").and_then(|answer| answer.get("content")).and_then(|content| content.get("ok")).and_then(Value::as_bool) == Some(true);
                let state_ok = cx.request_state.and_then(|state| serde_json::from_str::<Value>(state).ok()).is_some_and(|state| state.get("kind") == Some(&json!("request-state")));
                if confirmed && state_ok {
                    Outcome::text("state-ok: requestState validated")
                } else {
                    again(json!({ "confirm": confirm() }), Some(self.state(json!({ "kind": "request-state" }))))
                }
            }
            "test_input_required_result_multiple_inputs" => {
                let state_ok = cx.request_state.and_then(|state| serde_json::from_str::<Value>(state).ok()).is_some_and(|state| state.get("kind") == Some(&json!("multiple-inputs")));
                if state_ok && has("user_name") && has("greeting") && has("client_roots") {
                    let greeting = responses.get("greeting").and_then(|answer| answer.get("content")).and_then(|content| content.get("text")).and_then(Value::as_str).unwrap_or("Hello there!");
                    let roots = responses.get("client_roots").and_then(|answer| answer.get("roots")).and_then(Value::as_array).map_or(0, Vec::len);
                    Outcome::text(format!("Name: {}; Greeting: {greeting}; Roots: {roots}", response_text(&responses, "user_name", "name")))
                } else {
                    again(
                        json!({
                            "user_name": elicit("What is your name?", "name", "string"),
                            "greeting": sample("Generate a greeting", 50),
                            "client_roots": { "method": "roots/list", "params": {} }
                        }),
                        Some(self.state(json!({ "kind": "multiple-inputs" }))),
                    )
                }
            }
            "test_input_required_result_multi_round" => {
                let state = cx.request_state.and_then(|state| serde_json::from_str::<Value>(state).ok());
                let round = state.as_ref().and_then(|state| state.get("round")).and_then(Value::as_u64);
                match round {
                    Some(1) if has("step1") => again(
                        json!({ "step2": elicit("Step 2: What is your favorite color?", "color", "string") }),
                        Some(self.state(json!({ "round": 2, "name": response_text(&responses, "step1", "name") }))),
                    ),
                    Some(2) if has("step2") => {
                        let who = state.as_ref().and_then(|state| state.get("name")).and_then(Value::as_str).unwrap_or("friend");
                        Outcome::text(format!("Multi-round complete for {who} who likes {}", response_text(&responses, "step2", "color")))
                    }
                    _ => again(json!({ "step1": elicit("Step 1: What is your name?", "name", "string") }), Some(self.state(json!({ "round": 1 })))),
                }
            }
            "test_input_required_result_tampered_state" => {
                if let Some(state) = cx.request_state {
                    let Some(verified) = self.verify(state) else {
                        return Some(Outcome::Fail(RpcError::invalid_params("requestState integrity check failed")));
                    };
                    if verified.get("kind") == Some(&json!("tamper-test")) && has("confirm") {
                        return Some(Outcome::text("integrity-ok: state verified"));
                    }
                }
                again(json!({ "confirm": confirm() }), Some(self.sign(&json!({ "kind": "tamper-test", "nonce": self.fresh_nonce() }))))
            }
            "test_input_required_result_capabilities" => {
                let mut requests = serde_json::Map::new();
                if cx.has_capability("elicitation") {
                    requests.insert("elicit_input".to_owned(), elicit("Elicitation input", "value", "string"));
                }
                if cx.has_capability("sampling") {
                    requests.insert("sample_input".to_owned(), sample("Sample request", 50));
                }
                match cx.input_responses.filter(|responses| !responses.as_object().is_some_and(serde_json::Map::is_empty)) {
                    Some(responses) => Outcome::text(format!("capabilities-ok: received {}", responses.as_object().map(|map| map.keys().cloned().collect::<Vec<_>>().join(",")).unwrap_or_default())),
                    None if requests.is_empty() => Outcome::text("No supported capabilities declared"),
                    None => again(Value::Object(requests), Some(self.sign(&json!({ "kind": "capabilities-test", "nonce": self.fresh_nonce() })))),
                }
            }
            _ => return None,
        })
    }
}

impl Catalog for FixtureCatalog {
    fn identity(&self) -> Identity {
        Identity { name: "fw-mcp-conformance-fixture".to_owned(), title: None, version: env!("CARGO_PKG_VERSION").to_owned(), instructions: None }
    }

    fn capabilities(&self) -> Value {
        json!({
            "tools": { "listChanged": true },
            "resources": { "subscribe": true, "listChanged": true },
            "prompts": { "listChanged": true },
            "logging": {},
            "completions": {}
        })
    }

    fn tools(&self) -> Vec<Value> {
        let mut tools = vec![
            tool("test_simple_text", "Tests simple text content response", no_arguments()),
            tool("test_image_content", "Tests image content response", no_arguments()),
            tool("test_audio_content", "Tests audio content response", no_arguments()),
            tool("test_embedded_resource", "Tests embedded resource content response", no_arguments()),
            tool("test_multiple_content_types", "Tests response with multiple content types (text, image, resource)", no_arguments()),
            tool("test_tool_with_logging", "Tests tool that emits log messages during execution", no_arguments()),
            tool("test_tool_with_progress", "Tests tool that reports progress notifications", no_arguments()),
            tool("test_error_handling", "Tests error response handling", no_arguments()),
            tool(
                "json_schema_2020_12_tool",
                "Tool with JSON Schema 2020-12 features for conformance testing (SEP-1613)",
                json!({
                    "$schema": "https://json-schema.org/draft/2020-12/schema",
                    "type": "object",
                    "$defs": { "address": { "$anchor": "addressDef", "type": "object", "properties": { "street": { "type": "string" }, "city": { "type": "string" } } } },
                    "properties": {
                        "name": { "type": "string" },
                        "address": { "$ref": "#/$defs/address" },
                        "contactMethod": { "type": "string", "enum": ["phone", "email"] },
                        "phone": { "type": "string" },
                        "email": { "type": "string" }
                    },
                    "allOf": [{ "anyOf": [{ "required": ["phone"] }, { "required": ["email"] }] }],
                    "if": { "properties": { "contactMethod": { "const": "phone" } }, "required": ["contactMethod"] },
                    "then": { "required": ["phone"] },
                    "else": { "required": ["email"] },
                    "additionalProperties": false
                }),
            ),
            tool("test_missing_capability", "Test tool requiring sampling", no_arguments()),
            tool("test_streaming_elicitation", "Diagnostic tool validating response progress streams", no_arguments()),
            tool("test_logging_tool", "Diagnostic logging validator tool", no_arguments()),
            tool("test_trigger_tool_change", "Publishes a tools list change to the open subscriptions", no_arguments()),
            tool("test_trigger_prompt_change", "Publishes a prompts list change to the open subscriptions", no_arguments()),
            tool(
                "test_header_param",
                "Mirrors region and tenant into Mcp-Param headers (x-mcp-header)",
                json!({
                    "type": "object",
                    "properties": {
                        "region": { "type": "string", "x-mcp-header": "Region" },
                        "tenant": { "type": "object", "properties": { "id": { "type": "integer", "x-mcp-header": "TenantId" } } },
                        "query": { "type": "string" }
                    },
                    "required": ["region"]
                }),
            ),
        ];
        for (name, description) in [
            ("test_input_required_result_elicitation", "MRTR: returns InputRequiredResult with elicitation request"),
            ("test_input_required_result_sampling", "MRTR: returns InputRequiredResult with sampling request"),
            ("test_input_required_result_list_roots", "MRTR: returns InputRequiredResult with roots/list request"),
            ("test_input_required_result_request_state", "MRTR: returns InputRequiredResult with requestState"),
            ("test_input_required_result_multiple_inputs", "MRTR: returns InputRequiredResult with multiple input requests"),
            ("test_input_required_result_multi_round", "MRTR: multi-round InputRequiredResult workflow"),
            ("test_input_required_result_tampered_state", "MRTR: HMAC-signed requestState integrity test"),
            ("test_input_required_result_capabilities", "MRTR: respects client capabilities in inputRequests"),
        ] {
            tools.push(tool(name, description, no_arguments()));
        }
        tools
    }

    fn call_tool(&self, name: &str, arguments: &Value, cx: &CallContext<'_>) -> Outcome {
        if let Some(outcome) = self.mrtr(name, cx) {
            return outcome;
        }
        match name {
            "test_simple_text" => Outcome::text("This is a simple text response for testing."),
            "test_image_content" => Outcome::Complete(json!({ "content": [{ "type": "image", "data": PNG_PIXEL, "mimeType": "image/png" }] })),
            "test_audio_content" => Outcome::Complete(json!({ "content": [{ "type": "audio", "data": WAV_SAMPLE, "mimeType": "audio/wav" }] })),
            "test_embedded_resource" => Outcome::Complete(json!({
                "content": [{ "type": "resource", "resource": { "uri": "test://embedded-resource", "mimeType": "text/plain", "text": "This is an embedded resource content." } }]
            })),
            "test_multiple_content_types" => Outcome::Complete(json!({
                "content": [
                    { "type": "text", "text": "Multiple content types test:" },
                    { "type": "image", "data": PNG_PIXEL, "mimeType": "image/png" },
                    { "type": "resource", "resource": { "uri": "test://mixed-content-resource", "mimeType": "application/json", "text": "{\"test\":\"data\",\"value\":123}" } }
                ]
            })),
            "test_tool_with_logging" => {
                for message in ["Tool execution started", "Tool processing data", "Tool execution completed"] {
                    cx.log(LogLevel::Info, "conformance-test-server", json!(message));
                    if cx.cancel.sleep(Duration::from_millis(50)) {
                        break;
                    }
                }
                Outcome::text("Tool with logging executed successfully")
            }
            "test_tool_with_progress" => {
                for step in [0.0, 50.0, 100.0] {
                    cx.progress(step, Some(100.0), Some(&format!("Completed step {step} of 100")));
                    if cx.cancel.sleep(Duration::from_millis(50)) {
                        break;
                    }
                }
                Outcome::text(cx.progress_token.map_or_else(|| "0".to_owned(), |token| token.as_str().map_or_else(|| token.to_string(), str::to_owned)))
            }
            "test_error_handling" => Outcome::tool_error("This tool intentionally returns an error for testing"),
            "json_schema_2020_12_tool" => Outcome::text(format!("JSON Schema 2020-12 tool called with: {arguments}")),
            "test_header_param" => Outcome::text(format!("header params ok: {arguments}")),
            "test_missing_capability" => {
                if cx.has_capability("sampling") {
                    Outcome::text("Success")
                } else {
                    Outcome::Fail(RpcError::missing_capability(json!({ "sampling": {} })))
                }
            }
            "test_streaming_elicitation" => {
                cx.notify("notifications/progress", json!({ "progressToken": cx.progress_token.cloned().unwrap_or_else(|| json!("token-abc")), "progress": 50, "total": 100 }));
                Outcome::text("Streaming complete")
            }
            "test_logging_tool" => {
                cx.log(LogLevel::Info, "conformance-test-server", json!("Diagnostic trace logging activated"));
                Outcome::text("Logging evaluated")
            }
            "test_trigger_tool_change" => {
                cx.bus.publish(ListEvent::Tools);
                Outcome::text("Mutation triggered")
            }
            "test_trigger_prompt_change" => {
                cx.bus.publish(ListEvent::Prompts);
                Outcome::text("Mutation triggered")
            }
            other => Outcome::Fail(RpcError::invalid_params(format!("Unknown tool: {other}"))),
        }
    }

    fn resources(&self) -> Vec<Value> {
        vec![
            json!({ "uri": "test://static-text", "name": "static-text", "title": "Static Text Resource", "description": "A static text resource for testing", "mimeType": "text/plain" }),
            json!({ "uri": "test://static-binary", "name": "static-binary", "title": "Static Binary Resource", "description": "A static binary resource (image) for testing", "mimeType": "image/png" }),
            json!({ "uri": "test://watched-resource", "name": "watched-resource", "title": "Watched Resource", "description": "A resource that reports updates to subscribers", "mimeType": "text/plain" }),
            json!({ "uri": "test://stateless-static-text", "name": "Stateless Static Text", "description": "A static text resource served on the 2026-07-28 path", "mimeType": "text/plain" }),
        ]
    }

    fn resource_templates(&self) -> Vec<Value> {
        vec![json!({ "uriTemplate": "test://template/{id}/data", "name": "template", "title": "Resource Template", "description": "A resource template with parameter substitution", "mimeType": "application/json" })]
    }

    fn read_resource(&self, uri: &str, _cx: &CallContext<'_>) -> Outcome {
        let contents = match uri {
            "test://static-text" => json!({ "uri": uri, "mimeType": "text/plain", "text": "This is the content of the static text resource." }),
            "test://static-binary" => json!({ "uri": uri, "mimeType": "image/png", "blob": PNG_PIXEL }),
            "test://watched-resource" => json!({ "uri": uri, "mimeType": "text/plain", "text": "Watched resource content" }),
            "test://stateless-static-text" => json!({ "uri": uri, "mimeType": "text/plain", "text": "Static text content from the stateless path." }),
            _ => {
                let Some(id) = uri.strip_prefix("test://template/").and_then(|rest| rest.strip_suffix("/data")).filter(|id| !id.is_empty() && !id.contains('/')) else {
                    return Outcome::Fail(RpcError::invalid_params(format!("Resource not found: {uri}")).with_data(json!({ "uri": uri })));
                };
                json!({ "uri": uri, "mimeType": "application/json", "text": json!({ "id": id, "templateTest": true, "data": format!("Data for ID: {id}") }).to_string() })
            }
        };
        Outcome::Complete(json!({ "contents": [contents] }))
    }

    fn prompts(&self) -> Vec<Value> {
        vec![
            json!({ "name": "test_simple_prompt", "title": "Simple Test Prompt", "description": "A simple prompt without arguments" }),
            json!({
                "name": "test_prompt_with_arguments", "title": "Prompt With Arguments", "description": "A prompt with required arguments",
                "arguments": [{ "name": "arg1", "description": "First test argument", "required": true }, { "name": "arg2", "description": "Second test argument", "required": true }]
            }),
            json!({
                "name": "test_prompt_with_embedded_resource", "title": "Prompt With Embedded Resource", "description": "A prompt that includes an embedded resource",
                "arguments": [{ "name": "resourceUri", "description": "URI of the resource to embed", "required": true }]
            }),
            json!({ "name": "test_prompt_with_image", "title": "Prompt With Image", "description": "A prompt that includes image content" }),
            json!({ "name": "test_input_required_result_prompt", "description": "MRTR: prompt that requires elicitation input" }),
        ]
    }

    fn get_prompt(&self, name: &str, arguments: &Value, cx: &CallContext<'_>) -> Outcome {
        let argument = |key: &str| arguments.get(key).and_then(Value::as_str).map(str::to_owned);
        let missing = |key: &str| Outcome::Fail(RpcError::invalid_params(format!("Missing required argument: {key}")));
        let user = |content: Value| json!({ "role": "user", "content": content });
        let messages = match name {
            "test_simple_prompt" => vec![user(json!({ "type": "text", "text": "This is a simple prompt for testing." }))],
            "test_prompt_with_arguments" => {
                let (Some(first), Some(second)) = (argument("arg1"), argument("arg2")) else {
                    return missing("arg1 and arg2");
                };
                vec![user(json!({ "type": "text", "text": format!("Prompt with arguments: arg1='{first}', arg2='{second}'") }))]
            }
            "test_prompt_with_embedded_resource" => {
                let Some(uri) = argument("resourceUri") else { return missing("resourceUri") };
                vec![
                    user(json!({ "type": "resource", "resource": { "uri": uri, "mimeType": "text/plain", "text": "Embedded resource content for testing." } })),
                    user(json!({ "type": "text", "text": "Please process the embedded resource above." })),
                ]
            }
            "test_prompt_with_image" => vec![
                user(json!({ "type": "image", "data": PNG_PIXEL, "mimeType": "image/png" })),
                user(json!({ "type": "text", "text": "Please analyze the image above." })),
            ],
            "test_input_required_result_prompt" => {
                let Some(responses) = cx.input_responses.filter(|responses| responses.get("user_context").is_some()) else {
                    return Outcome::InputRequired { requests: Some(json!({ "user_context": elicit("What context should the prompt use?", "context", "string") })), state: None };
                };
                vec![user(json!({ "type": "text", "text": format!("Prompt with context: {}", response_text(responses, "user_context", "context")) }))]
            }
            other => return Outcome::Fail(RpcError::invalid_params(format!("Unknown prompt: {other}"))),
        };
        Outcome::Complete(json!({ "messages": messages }))
    }

    fn complete(&self, _params: &Value, _cx: &CallContext<'_>) -> Outcome {
        Outcome::Complete(json!({ "completion": { "values": [], "total": 0, "hasMore": false } }))
    }
}
