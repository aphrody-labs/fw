//! The Edge OS HTTP surface: every route of the TypeScript `export default { fetch }` except `/drive*`
//! (CORS, static assets, `/health`, A2A card, OpenAPI, `/v1/*`, MCP, ACP, A2A, `/os/*`, `/cluster`, `/claw/*`,
//! `/api/learn/*`, 404). JSON-RPC dispatch is plain functions over the [`Dispatch`] and
//! [`Inference`](crate::ai::Inference) traits so it runs in the native unit tests.

use std::cell::RefCell;
use std::collections::HashMap;

use serde_json::{Map, Value, json};
use wasm_bindgen::JsValue;
use worker::{Context, Env, Headers, Method, Request, RequestInit, Response, Result};

use crate::ai::{self, ChatReply, Inference, WorkersAi};
use crate::os::{COMPATIBILITY_DATE, FW_VERSION, body_object, binding, from_js, js_string, json_response, make_id, normalize_path, now_iso, resolve_tenant};

const CATALOG_JSON: &str = include_str!("../data/catalog.json");
const TIERS_JSON: &str = include_str!("../data/tiers.json");
const MCP_TOOLS_JSON: &str = include_str!("../data/mcp_tools.json");
const OPENAPI_PATHS_JSON: &str = include_str!("../data/openapi_paths.json");
const OPENAPI_DESCRIPTION_JSON: &str = include_str!("../data/openapi_description.json");

const STATIC_WEB_PATHS: [&str; 9] = [
    "/",
    "/index.html",
    "/app.css",
    "/app.js",
    "/robots.txt",
    "/sitemap.xml",
    "/llms.txt",
    "/install.sh",
    "/install.ps1",
];

const BINDINGS: [&str; 10] = ["AI", "FW_OS", "LOADER", "VFS_BLOBS", "META_DB", "MEMORY_VEC", "BROWSER", "EVENT_LOG", "TELEMETRY_PIPELINE", "SANDBOX"];

fn embedded(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or(Value::Null)
}

// ---------------------------------------------------------------------------------------------------------------
// Static documents.
// ---------------------------------------------------------------------------------------------------------------

/// `CLOUDFLARE_EDGE_OS_CATALOG`, optionally filtered by `layer`.
pub fn catalog_items(layer: Option<&str>) -> Vec<Value> {
    let all = embedded(CATALOG_JSON);
    all.as_array()
        .map(|a| a.iter().filter(|i| layer.is_none_or(|l| i["layer"] == l)).cloned().collect())
        .unwrap_or_default()
}

pub fn catalog_response(layer: Option<&str>) -> Value {
    let items = catalog_items(layer);
    json!({
        "ok": true,
        "product": "fw-edge",
        "updated": "2026-11-19",
        "compatibilityDate": COMPATIBILITY_DATE,
        "count": items.len(),
        "items": items,
    })
}

pub fn tiers_response(bindings: Value) -> Value {
    json!({ "ok": true, "product": "fw-edge", "version": FW_VERSION, "tiers": embedded(TIERS_JSON), "bindings": bindings })
}

pub fn health_response(fw_env: &str, deployment: Value, bindings: Value) -> Value {
    json!({
        "ok": true,
        "product": "fw-edge",
        "version": FW_VERSION,
        "runtime": "cloudflare-workers",
        "compatibilityDate": COMPATIBILITY_DATE,
        "env": fw_env,
        "deployment": deployment,
        "bindings": bindings,
    })
}

pub fn agent_card(origin: &str, workers_ai: bool) -> Value {
    json!({
        "name": "Fw Edge",
        "description": "Fw Edge OS Worker with SQLite DO VFS, 3-tier execution, Gatekeeper, and MCP/ACP/A2A gateway",
        "url": format!("{origin}/a2a"),
        "version": FW_VERSION,
        "protocolVersion": "1.0.1",
        "defaultInputModes": ["text/plain", "application/json"],
        "defaultOutputModes": ["text/plain", "application/json"],
        "capabilities": {
            "streaming": false,
            "pushNotifications": false,
            "stateTransitionHistory": true,
            "edge": true,
            "workersAi": workers_ai,
            "durableObjectVfs": true,
            "executionTiers": [0, 1, 2],
        },
        "skills": [
            { "id": "edge-vfs", "name": "Tenant SQLite VFS", "description": "Persistent per-tenant POSIX filesystem with R2 spillover" },
            { "id": "edge-exec", "name": "3-Tier Execution", "description": "Tier-0 DO shell, Tier-1 Dynamic Worker Loader CodeMode, Tier-2 Sandbox microVM" },
            { "id": "gatekeeper", "name": "Gatekeeper Policy", "description": "Fail-closed capability checks per principal and path prefix" },
            { "id": "autonomous-learn", "name": "Autonomous Learning Log", "description": "Durable event capture into SQLite, K2 Streams, and Basin Pipelines" },
        ],
    })
}

pub fn openapi_spec(origin: &str) -> Value {
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Fw Cloudflare Edge OS API",
            "version": FW_VERSION,
            "description": embedded(OPENAPI_DESCRIPTION_JSON),
        },
        "servers": [{ "url": origin }],
        "paths": embedded(OPENAPI_PATHS_JSON),
    })
}

pub fn cluster_response(bindings: Value) -> Value {
    json!({
        "ok": true,
        "runtime": "cloudflare-workers-edge",
        "bindings": bindings,
        "tiers": ["tier-0-do-sqlite", "tier-1-dynamic-worker", "tier-2-sandbox-container"],
    })
}

pub fn claw_status() -> Value {
    json!({ "ok": true, "agent": "fw-edge-hermes-claw", "runtime": "cloudflare-workers", "mcpTools": 4, "sandboxAllowBinaries": 13, "skills": 10 })
}

pub fn claw_ranking() -> Value {
    json!({
        "ok": true,
        "modernRustClis": [
            { "rank": 1, "name": "uv" },
            { "rank": 2, "name": "rg" },
            { "rank": 3, "name": "bat" },
            { "rank": 4, "name": "fd" },
            { "rank": 5, "name": "yazi" },
            { "rank": 6, "name": "nu" },
        ],
        "protocolsAndAgentOs": [
            { "rank": 1, "name": "openclaw" },
            { "rank": 2, "name": "hermes-agent" },
            { "rank": 3, "name": "zeroclaw" },
            { "rank": 4, "name": "agentskills" },
            { "rank": 5, "name": "rmcp" },
        ],
    })
}

pub fn claw_run(goal: Option<&str>) -> Value {
    json!({
        "ok": true,
        "goal": goal.unwrap_or("Audit Cloudflare Edge OS"),
        "channel": "CloudflareEdgeHttp",
        "steps": [
            { "step": 1, "action": "ground-hypothesis-via-edge-do-memory", "verified": true },
            { "step": 2, "action": "verify-mcp-tools-list", "verified": true },
            { "step": 3, "action": "consolidate-edge-telemetry", "verified": true },
        ],
    })
}

// ---------------------------------------------------------------------------------------------------------------
// JSON-RPC: MCP, ACP, A2A.
// ---------------------------------------------------------------------------------------------------------------

/// Calls the tenant Durable Object (`dispatchToTenantDO`) and returns its JSON reply.
#[allow(async_fn_in_trait)]
pub trait Dispatch {
    async fn call(&self, tenant: &str, method: &str, path: &str, principal: &str, body: Option<String>) -> std::result::Result<Value, String>;
}

fn rpc_result(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn rpc_error(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn rpc_id(rpc: &Map<String, Value>) -> Value {
    rpc.get("id").cloned().unwrap_or(Value::Null)
}

/// `rpc.method ?? "ping"`.
fn rpc_method(rpc: &Map<String, Value>) -> Value {
    rpc.get("method").filter(|m| !m.is_null()).cloned().unwrap_or_else(|| "ping".into())
}

/// The keys of `args` that are present, as a JSON object (`JSON.stringify` drops `undefined`).
fn pick(args: &Map<String, Value>, keys: &[&str]) -> Value {
    Value::Object(keys.iter().filter_map(|k| args.get(*k).map(|v| ((*k).to_string(), v.clone()))).collect())
}

/// `handleMcp`: Streamable HTTP MCP JSON-RPC 2.0.
pub async fn handle_mcp<D: Dispatch>(rpc: &Map<String, Value>, tenant: &str, principal: &str, d: &D) -> std::result::Result<Value, String> {
    let id = rpc_id(rpc);
    let method = rpc_method(rpc);
    let method = method.as_str();

    if method == Some("initialize") {
        return Ok(rpc_result(
            &id,
            json!({
                "protocolVersion": "2025-11-25",
                "capabilities": {
                    "tools": { "listChanged": false },
                    "resources": { "subscribe": false, "listChanged": false },
                },
                "serverInfo": { "name": "fw-edge", "version": FW_VERSION },
            }),
        ));
    }
    if method == Some("tools/list") {
        return Ok(rpc_result(&id, json!({ "tools": embedded(MCP_TOOLS_JSON) })));
    }
    if method == Some("tools/call") {
        let empty = Map::new();
        let params = rpc.get("params").and_then(Value::as_object).unwrap_or(&empty);
        let tool = params.get("name").filter(|n| !n.is_null()).map(js_string).unwrap_or_default();
        let args = params.get("arguments").and_then(Value::as_object).unwrap_or(&empty);
        let whole = Value::Object(args.clone());
        let payload = match tool.as_str() {
            "vfs_ls" => {
                let prefix = args.get("prefix").filter(|v| !v.is_null()).cloned().unwrap_or_else(|| "/".into());
                d.call(tenant, "POST", "/vfs/ls", principal, Some(json!({ "prefix": prefix }).to_string())).await?
            }
            "vfs_read" => d.call(tenant, "POST", "/vfs/read", principal, Some(pick(args, &["path"]).to_string())).await?,
            "vfs_write" => d.call(tenant, "POST", "/vfs/write", principal, Some(pick(args, &["path", "content", "contentType"]).to_string())).await?,
            "vfs_stat" => d.call(tenant, "POST", "/vfs/stat", principal, Some(pick(args, &["path"]).to_string())).await?,
            "vfs_exec" => d.call(tenant, "POST", "/vfs/exec", principal, Some(whole.to_string())).await?,
            "gatekeeper_check" => d.call(tenant, "POST", "/gatekeeper/check", principal, Some(whole.to_string())).await?,
            "os_catalog" => {
                let items = catalog_items(args.get("layer").and_then(Value::as_str));
                json!({ "ok": true, "count": items.len(), "items": items })
            }
            "learn_log" => d.call(tenant, "POST", "/api/learn/events", principal, Some(whole.to_string())).await?,
            "learn_status" => d.call(tenant, "GET", "/api/learn/status", principal, None).await?,
            other => return Ok(rpc_error(&id, -32602, &format!("Unknown MCP tool: {other}"))),
        };
        let is_error = payload.get("ok") == Some(&Value::Bool(false));
        return Ok(rpc_result(
            &id,
            json!({
                "content": [{ "type": "text", "text": payload.to_string() }],
                "structuredContent": payload,
                "isError": is_error,
            }),
        ));
    }
    if method == Some("ping") {
        return Ok(rpc_result(&id, json!({ "protocol": "mcp", "method": "ping", "edge": true, "ok": true })));
    }
    Ok(rpc_error(&id, -32601, &format!("Method not found: {}", js_string(&rpc_method(rpc)))))
}

#[derive(Default)]
struct TenantState {
    acp_sessions: HashMap<String, Value>,
    a2a_tasks: HashMap<String, Value>,
}

thread_local! {
    /// ACP sessions and A2A tasks are isolate-local memory per tenant, as in the TypeScript worker.
    static TENANTS: RefCell<HashMap<String, TenantState>> = RefCell::new(HashMap::new());
}

fn with_tenant<R>(tenant: &str, fw_env: &str, f: impl FnOnce(&mut TenantState) -> R) -> R {
    TENANTS.with(|t| f(t.borrow_mut().entry(format!("{tenant}:{fw_env}")).or_default()))
}

fn create_acp_session(tenant: &str, fw_env: &str, cwd: &str) -> Value {
    let session = json!({
        "sessionId": make_id("acp"),
        "tenant": tenant,
        "cwd": normalize_path(cwd, "/").path,
        "createdAt": now_iso(),
        "turns": 0,
    });
    with_tenant(tenant, fw_env, |s| s.acp_sessions.insert(session["sessionId"].as_str().unwrap_or_default().to_string(), session.clone()));
    session
}

fn bump_acp_session(tenant: &str, fw_env: &str, session_id: &str) -> Option<Value> {
    with_tenant(tenant, fw_env, |s| {
        let existing = s.acp_sessions.get_mut(session_id)?;
        let turns = existing["turns"].as_i64().unwrap_or(0) + 1;
        existing["turns"] = turns.into();
        Some(existing.clone())
    })
}

fn inference_error(id: &Value, e: &ai::AiError) -> Value {
    rpc_error(id, -32001, e.message())
}

/// `handleAcp`.
pub async fn handle_acp<I: Inference>(rpc: &Map<String, Value>, tenant: &str, fw_env: &str, ai: Option<&I>) -> Value {
    let id = rpc_id(rpc);
    let method = rpc_method(rpc);
    let params = rpc.get("params").and_then(Value::as_object);

    match method.as_str() {
        Some("initialize") => rpc_result(
            &id,
            json!({
                "protocol": "acp",
                "protocolVersion": "1.33.0",
                "edge": true,
                "ok": true,
                "agentInfo": { "name": "fw-edge", "version": FW_VERSION },
                "agentCapabilities": {
                    "loadSession": true,
                    "promptCapabilities": { "embeddedContext": true },
                    "edgeTiers": [0, 1, 2],
                },
            }),
        ),
        Some("session/new") => {
            let cwd = params.and_then(|p| p.get("cwd")).and_then(Value::as_str).unwrap_or("/workspace");
            let mut session = create_acp_session(tenant, fw_env, cwd);
            if let Some(m) = session.as_object_mut() {
                m.insert("mode".into(), "edge-os".into());
                m.insert("ok".into(), true.into());
            }
            rpc_result(&id, session)
        }
        Some("session/prompt") => {
            let empty = Map::new();
            let params = params.unwrap_or(&empty);
            let session_id = match params.get("sessionId").and_then(Value::as_str) {
                Some(s) => s.to_string(),
                None => create_acp_session(tenant, fw_env, "/workspace")["sessionId"].as_str().unwrap_or_default().to_string(),
            };
            let bumped = bump_acp_session(tenant, fw_env, &session_id).unwrap_or_else(|| create_acp_session(tenant, fw_env, "/workspace"));
            let prompt = match params.get("prompt") {
                Some(Value::String(s)) => s.clone(),
                Some(Value::Array(a)) => a
                    .iter()
                    .map(|p| match p.as_object().filter(|o| o.contains_key("text")) {
                        Some(o) => o.get("text").filter(|t| !t.is_null()).map(js_string).unwrap_or_default(),
                        None => js_string(p),
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                _ => String::new(),
            };
            match ai::run_inference_text(ai, params.get("model"), json!([{ "role": "user", "content": prompt }])).await {
                Err(e) => inference_error(&id, &e),
                Ok((text, model)) => rpc_result(
                    &id,
                    json!({
                        "sessionId": bumped["sessionId"],
                        "turns": bumped["turns"],
                        "stopReason": "end_turn",
                        "model": model,
                        "output": { "role": "assistant", "text": text },
                        "ok": true,
                    }),
                ),
            }
        }
        Some("ping") => rpc_result(&id, json!({ "protocol": "acp", "method": "ping", "edge": true, "ok": true })),
        _ => rpc_error(&id, -32601, &format!("Method not found: {}", js_string(&method))),
    }
}

/// `handleA2A`.
pub async fn handle_a2a<I: Inference>(rpc: &Map<String, Value>, tenant: &str, fw_env: &str, ai: Option<&I>) -> Value {
    let id = rpc_id(rpc);
    let method = rpc_method(rpc);
    let empty = Map::new();
    let params = rpc.get("params").and_then(Value::as_object).unwrap_or(&empty);

    match method.as_str() {
        Some("tasks/send") => {
            let task_id = match params.get("id").and_then(Value::as_str) {
                Some(s) if !s.trim().is_empty() => s.to_string(),
                _ => make_id("task"),
            };
            let mut prompt = params.get("prompt").and_then(Value::as_str).unwrap_or("").to_string();
            if prompt.is_empty()
                && let Some(msg) = params.get("message").and_then(Value::as_object)
            {
                prompt = msg
                    .get("parts")
                    .and_then(Value::as_array)
                    .map(|parts| {
                        parts
                            .iter()
                            .map(|p| p.get("text").filter(|t| !t.is_null()).map(js_string).unwrap_or_default())
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();
            }
            let text = match ai::run_inference_text(ai, params.get("model"), json!([{ "role": "user", "content": prompt }])).await {
                Ok((text, _)) => text,
                Err(e) => return inference_error(&id, &e),
            };
            let task = json!({
                "id": task_id,
                "tenant": tenant,
                "status": { "state": "completed", "timestamp": now_iso() },
                "artifacts": [{ "name": "response", "parts": [{ "type": "text", "text": text }] }],
                "history": [
                    { "role": "user", "parts": [{ "type": "text", "text": prompt }] },
                    { "role": "agent", "parts": [{ "type": "text", "text": text }] },
                ],
            });
            with_tenant(tenant, fw_env, |s| s.a2a_tasks.insert(task_id, task.clone()));
            rpc_result(&id, task)
        }
        Some("tasks/get") => {
            let task_id = params.get("id").and_then(Value::as_str).unwrap_or("");
            match with_tenant(tenant, fw_env, |s| s.a2a_tasks.get(task_id).cloned()) {
                Some(task) => rpc_result(&id, task),
                None => rpc_error(&id, -32001, &format!("Task not found: {task_id}")),
            }
        }
        Some("ping") => rpc_result(&id, json!({ "protocol": "a2a", "method": "ping", "edge": true, "ok": true })),
        _ => rpc_error(&id, -32601, &format!("Method not found: {}", js_string(&method))),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Worker routing.
// ---------------------------------------------------------------------------------------------------------------

fn binding_summary(env: &Env) -> Value {
    Value::Object(BINDINGS.iter().map(|b| ((*b).to_string(), binding(env, b).is_some().into())).collect())
}

fn fw_env_name(env: &Env) -> String {
    env.var("FW_ENV").map_or_else(|_| "production".into(), |v| v.to_string())
}

fn cors_preflight() -> Result<Response> {
    let mut resp = Response::empty()?.with_status(204);
    let h = resp.headers_mut();
    h.set("access-control-allow-origin", "*")?;
    h.set("access-control-allow-methods", "GET, POST, PUT, DELETE, OPTIONS")?;
    h.set("access-control-allow-headers", "Content-Type, Authorization, X-Fw-Tenant, X-Fw-Principal, Mcp-Session-Id")?;
    Ok(resp)
}

fn error_response(message: &str) -> Result<Response> {
    json_response(&json!({ "ok": false, "error": message }), 500)
}

/// Sends a request to the tenant's `FwOSAgent` (`dispatchToTenantDO`).
async fn forward_to_do(env: &Env, tenant: &str, method: &str, path: &str, search: &str, principal: Option<String>, body: Option<String>) -> Result<Response> {
    let stub = env.durable_object("FW_OS")?.get_by_name(tenant)?;
    let headers = Headers::new();
    if let Some(p) = principal {
        headers.set("x-fw-principal", &p)?;
    }
    headers.set("content-type", "application/json")?;
    let mut init = RequestInit::new();
    init.with_method(Method::from(method.to_string())).with_headers(headers);
    if let Some(b) = body.filter(|_| method != "GET" && method != "HEAD") {
        init.with_body(Some(JsValue::from_str(&b)));
    }
    let req = Request::new_with_init(&format!("https://fw-os.internal{path}{search}"), &init)?;
    stub.fetch_with_request(req).await
}

struct EnvDispatch<'a>(&'a Env);

impl Dispatch for EnvDispatch<'_> {
    async fn call(&self, tenant: &str, method: &str, path: &str, principal: &str, body: Option<String>) -> std::result::Result<Value, String> {
        let mut res = forward_to_do(self.0, tenant, method, path, "", Some(principal.to_string()), body).await.map_err(|e| e.to_string())?;
        let text = res.text().await.map_err(|e| e.to_string())?;
        serde_json::from_str(&text).map_err(|e| e.to_string())
    }
}

async fn assets_fetch(env: &Env, req: Request) -> Result<Response> {
    let resp = env.assets("ASSETS")?.fetch_request(req).await?;
    Response::try_from(resp)
}

async fn body_text(req: &mut Request) -> String {
    req.text().await.unwrap_or_default()
}

pub async fn route(mut req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let url = req.url()?;
    let path = url.path().to_string();
    let method = req.method();
    let is_get = method == Method::Get;
    let is_post = method == Method::Post;
    let query = |name: &str| url.query_pairs().find(|(k, _)| k == name).map(|(_, v)| v.into_owned());
    let tenant = resolve_tenant(req.headers().get("x-fw-tenant").ok().flatten().as_deref(), query("tenant").as_deref());
    let principal = req.headers().get("x-fw-principal").ok().flatten();

    if method == Method::Options {
        return cors_preflight();
    }

    if is_get && STATIC_WEB_PATHS.contains(&path.as_str()) && binding(&env, "ASSETS").is_some() {
        return assets_fetch(&env, req).await;
    }
    if is_get && path == "/webgpu" && binding(&env, "ASSETS").is_some() {
        let mut init = RequestInit::new();
        init.with_method(Method::Get).with_headers(req.headers().clone());
        let origin = url.origin().ascii_serialization();
        return assets_fetch(&env, Request::new_with_init(&format!("{origin}/webgpu.html"), &init)?).await;
    }

    let origin = url.origin().ascii_serialization();
    match path.as_str() {
        "/health" => {
            let deployment = binding(&env, "CF_VERSION_METADATA").and_then(|b| from_js(&b)).unwrap_or(Value::Null);
            return json_response(&health_response(&fw_env_name(&env), deployment, binding_summary(&env)), 200);
        }
        "/.well-known/agent.json" => return json_response(&agent_card(&origin, binding(&env, "AI").is_some()), 200),
        "/openapi.json" => return json_response(&openapi_spec(&origin), 200),
        "/v1/models" => {
            let ai = WorkersAi::bind(&env);
            let (status, body) = ai::models_route(ai.as_ref()).await;
            return json_response(&body, status);
        }
        "/v1/chat/completions" if is_post => {
            let text = body_text(&mut req).await;
            let ai = WorkersAi::bind(&env);
            return match ai::chat_route(ai.as_ref(), &text).await {
                ChatReply::Json(status, body) => json_response(&body, status),
                ChatReply::Stream { model, source } => {
                    let mut resp = Response::from_stream(ai::reframe_stream(source, &model))?;
                    let h = resp.headers_mut();
                    h.set("content-type", "text/event-stream; charset=utf-8")?;
                    h.set("cache-control", "no-store")?;
                    h.set("access-control-allow-origin", "*")?;
                    Ok(resp)
                }
            };
        }
        "/v1/embeddings" if is_post => {
            let text = body_text(&mut req).await;
            let ai = WorkersAi::bind(&env);
            let (status, body) = ai::embeddings_route(ai.as_ref(), &text, &tenant).await;
            return json_response(&body, status);
        }
        "/mcp" if is_post => {
            let rpc = body_object(&body_text(&mut req).await);
            let who = principal.unwrap_or_else(|| "default".into());
            return match handle_mcp(&rpc, &tenant, &who, &EnvDispatch(&env)).await {
                Ok(v) => json_response(&v, 200),
                Err(e) => error_response(&e),
            };
        }
        "/acp" if is_post => {
            let rpc = body_object(&body_text(&mut req).await);
            let ai = WorkersAi::bind(&env);
            return json_response(&handle_acp(&rpc, &tenant, &fw_env_name(&env), ai.as_ref()).await, 200);
        }
        "/a2a" if is_post => {
            let rpc = body_object(&body_text(&mut req).await);
            let ai = WorkersAi::bind(&env);
            return json_response(&handle_a2a(&rpc, &tenant, &fw_env_name(&env), ai.as_ref()).await, 200);
        }
        "/os/catalog" if is_get => return json_response(&catalog_response(query("layer").as_deref()), 200),
        "/os/tiers" if is_get => return json_response(&tiers_response(binding_summary(&env)), 200),
        _ => {}
    }

    let search = url.query().map(|q| format!("?{q}")).unwrap_or_default();
    let has_body = !matches!(method, Method::Get | Method::Head);

    if let Some(sub) = path.strip_prefix("/os").filter(|s| s.starts_with("/vfs/")) {
        let body = if has_body { Some(body_text(&mut req).await) } else { None };
        return forward_to_do(&env, &tenant, method.as_ref(), sub, &search, principal, body).await;
    }

    if path == "/os/gatekeeper" {
        if is_get {
            return forward_to_do(&env, &tenant, "GET", "/gatekeeper/policies", "", principal, None).await;
        }
        if is_post {
            let raw = body_text(&mut req).await;
            let parsed = body_object(&raw);
            let upsert = parsed.get("action").and_then(Value::as_str) == Some("upsert") || matches!(parsed.get("effect").and_then(Value::as_str), Some("allow" | "deny"));
            let target = if upsert { "/gatekeeper/policies" } else { "/gatekeeper/check" };
            return forward_to_do(&env, &tenant, "POST", target, "", principal, Some(raw)).await;
        }
    }

    match path.as_str() {
        "/cluster" if is_get => return json_response(&cluster_response(binding_summary(&env)), 200),
        "/claw/status" if is_get => return json_response(&claw_status(), 200),
        "/claw/ranking" if is_get => return json_response(&claw_ranking(), 200),
        "/claw/run" if is_post => {
            let body = body_object(&body_text(&mut req).await);
            return json_response(&claw_run(body.get("goal").and_then(Value::as_str)), 200);
        }
        "/api/learn/events" | "/api/learn/status" => {
            let body = if has_body { Some(body_text(&mut req).await) } else { None };
            return forward_to_do(&env, &tenant, method.as_ref(), &path, &search, principal, body).await;
        }
        _ => {}
    }

    json_response(&json!({ "ok": false, "error": "Not found", "path": path }), 404)
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::rc::Rc;
    use std::task::{Context as TaskContext, Poll, Waker};

    use super::*;
    use crate::os::testkit::TestHost;
    use crate::os::{DoReq, Kernel, route_do};

    fn block_on<F: Future>(fut: F) -> F::Output {
        let mut fut = Box::pin(fut);
        let mut cx = TaskContext::from_waker(Waker::noop());
        loop {
            if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
                return v;
            }
        }
    }

    /// Routes MCP tool calls to in-process per-tenant kernels (the `env.FW_OS` stub of the tests).
    #[derive(Default)]
    struct LocalDispatch {
        tenants: RefCell<HashMap<String, Rc<TestHost>>>,
    }

    impl Dispatch for LocalDispatch {
        async fn call(&self, tenant: &str, method: &str, path: &str, principal: &str, body: Option<String>) -> std::result::Result<Value, String> {
            let host = self
                .tenants
                .borrow_mut()
                .entry(tenant.to_string())
                .or_insert_with(|| {
                    let h = TestHost::new(tenant);
                    Kernel::new(&h).init().unwrap();
                    Rc::new(h)
                })
                .clone();
            let req = DoReq { method: method.into(), path: path.into(), query: vec![], principal: principal.into(), body };
            Ok(route_do(&*host, &req).await.1)
        }
    }

    /// Echoes the last user message, like `mockAi()` in the TS tests.
    struct EchoAi;

    impl Inference for EchoAi {
        async fn models(&self, _task: &str) -> std::result::Result<Option<Vec<ai::RawModel>>, String> {
            Ok(Some(vec![ai::RawModel {
                name: "@cf/test/big-tools".into(),
                description: None,
                properties: vec![("function_calling".into(), "true".into()), ("context_window".into(), "131072".into())],
            }]))
        }
        async fn run(&self, _model: &str, input: &Value) -> std::result::Result<Value, String> {
            let last = input["messages"][0]["content"].as_str().unwrap_or("");
            Ok(json!({ "response": format!("echo:{last}") }))
        }
        async fn run_stream(&self, _m: &str, _i: &Value) -> std::result::Result<Option<worker::ByteStream>, String> {
            Ok(None)
        }
        fn has_vectorize(&self) -> bool {
            false
        }
        async fn vectorize_upsert(&self, _v: &Value) -> std::result::Result<(), String> {
            Ok(())
        }
    }

    fn rpc(v: Value) -> Map<String, Value> {
        v.as_object().cloned().unwrap()
    }

    fn mcp(d: &LocalDispatch, v: Value) -> Value {
        block_on(handle_mcp(&rpc(v), "mcp-tenant", "default", d)).unwrap()
    }

    #[test]
    fn mcp_initialize_and_tools_list() {
        let d = LocalDispatch::default();
        let init = mcp(&d, json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"}));
        assert_eq!(init["result"]["protocolVersion"], "2025-11-25");
        assert_eq!(init["result"]["serverInfo"]["name"], "fw-edge");
        assert_eq!(init["result"]["serverInfo"]["version"], FW_VERSION);
        assert_eq!(init["id"], 1);
        let list = mcp(&d, json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
        let names: Vec<&str> = list["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["vfs_ls", "vfs_read", "vfs_write", "vfs_stat", "vfs_exec", "gatekeeper_check", "os_catalog", "learn_log", "learn_status"]);
    }

    #[test]
    fn mcp_tool_calls_roundtrip_through_the_tenant_do() {
        let d = LocalDispatch::default();
        let call = |id: i64, name: &str, args: Value| mcp(&d, json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": name, "arguments": args}}));
        let write = call(3, "vfs_write", json!({"path": "/workspace/mcp.txt", "content": "mcp-persisted"}));
        assert_eq!(write["result"]["isError"], false);
        assert_eq!(write["result"]["structuredContent"]["ok"], true);
        assert_eq!(write["result"]["content"][0]["type"], "text");
        assert_eq!(write["result"]["content"][0]["text"], write["result"]["structuredContent"].to_string());
        let read = call(4, "vfs_read", json!({"path": "/workspace/mcp.txt"}));
        assert_eq!(read["result"]["structuredContent"]["content"], "mcp-persisted");
        let cat = call(5, "os_catalog", json!({"layer": "data-stream"}));
        assert_eq!(cat["result"]["structuredContent"]["count"], 2);
        let unknown = call(6, "nonexistent_tool", json!({}));
        assert_eq!(unknown["error"]["code"], -32602);
        assert_eq!(unknown["error"]["message"], "Unknown MCP tool: nonexistent_tool");
        // a failing DO reply marks isError
        let missing = call(7, "vfs_read", json!({}));
        assert_eq!(missing["result"]["isError"], true);
        assert_eq!(missing["result"]["structuredContent"]["error"], "Missing path");
        let ls = call(8, "vfs_ls", json!({}));
        assert_eq!(ls["result"]["structuredContent"]["prefix"], "/");
        let exec = call(9, "vfs_exec", json!({"command": "echo hi"}));
        assert_eq!(exec["result"]["structuredContent"]["stdout"], "hi\n");
        let check = call(10, "gatekeeper_check", json!({"capability": "vfs:write", "path": "/etc/secrets/x"}));
        assert_eq!(check["result"]["structuredContent"]["decision"]["allowed"], false);
        assert_eq!(call(11, "learn_log", json!({"kind": "k", "summary": "s"}))["result"]["structuredContent"]["event"]["kind"], "k");
        assert_eq!(call(12, "learn_status", json!({}))["result"]["structuredContent"]["totalEvents"], 1);
        assert_eq!(call(13, "vfs_stat", json!({"path": "/workspace/mcp.txt"}))["result"]["structuredContent"]["exists"], true);
        // tenants are isolated
        let other = block_on(handle_mcp(
            &rpc(json!({"jsonrpc": "2.0", "id": 14, "method": "tools/call", "params": {"name": "vfs_read", "arguments": {"path": "/workspace/mcp.txt"}}})),
            "someone-else",
            "default",
            &d,
        ))
        .unwrap();
        assert_eq!(other["result"]["structuredContent"]["error"], "ENOENT");
    }

    #[test]
    fn mcp_ping_default_method_and_unknown_method() {
        let d = LocalDispatch::default();
        assert_eq!(mcp(&d, json!({"id": "x", "method": "ping"}))["result"], json!({"protocol": "mcp", "method": "ping", "edge": true, "ok": true}));
        assert_eq!(mcp(&d, json!({}))["result"]["method"], "ping");
        assert_eq!(mcp(&d, json!({}))["id"], Value::Null);
        let err = mcp(&d, json!({"id": 1, "method": "resources/read"}));
        assert_eq!((err["error"]["code"].clone(), err["error"]["message"].clone()), (json!(-32601), json!("Method not found: resources/read")));
    }

    fn acp(v: Value, tenant: &str, ai: Option<&EchoAi>) -> Value {
        block_on(handle_acp(&rpc(v), tenant, "production", ai))
    }

    #[test]
    fn acp_lifecycle() {
        let ai = EchoAi;
        let init = acp(json!({"jsonrpc": "2.0", "id": "a1", "method": "initialize"}), "t", Some(&ai));
        assert_eq!(init["result"]["protocolVersion"], "1.33.0");
        assert_eq!(init["result"]["agentCapabilities"]["edgeTiers"], json!([0, 1, 2]));
        let session = acp(json!({"id": "a2", "method": "session/new", "params": {"cwd": "/workspace"}}), "acp-tenant", Some(&ai));
        let sid = session["result"]["sessionId"].as_str().unwrap().to_string();
        assert!(sid.starts_with("acp_"));
        assert_eq!(session["result"]["cwd"], "/workspace");
        assert_eq!((session["result"]["mode"].clone(), session["result"]["ok"].clone(), session["result"]["turns"].clone()), (json!("edge-os"), json!(true), json!(0)));
        let prompt = |sid: &str| acp(json!({"id": "a3", "method": "session/prompt", "params": {"sessionId": sid, "prompt": "summarize edge os"}}), "acp-tenant", Some(&ai));
        let r = prompt(&sid);
        assert_eq!((r["result"]["stopReason"].clone(), r["result"]["turns"].clone()), (json!("end_turn"), json!(1)));
        assert_eq!(r["result"]["output"]["text"], "echo:summarize edge os");
        assert_eq!(r["result"]["model"], "@cf/test/big-tools");
        assert_eq!(prompt(&sid)["result"]["turns"], 2);
        // an unknown session id creates a fresh session with 0 turns (TS behaviour)
        let fresh = prompt("acp_unknown");
        assert_eq!(fresh["result"]["turns"], 0);
        assert_ne!(fresh["result"]["sessionId"], "acp_unknown");
        // sessions are per tenant
        assert_eq!(acp(json!({"method": "session/prompt", "params": {"sessionId": sid, "prompt": "x"}}), "other", Some(&ai))["result"]["turns"], 0);
        // array prompts join text parts, or stringify non-objects
        let arr = acp(json!({"method": "session/prompt", "params": {"prompt": [{"text": "a"}, "b", 7]}}), "t", Some(&ai));
        assert_eq!(arr["result"]["output"]["text"], "echo:a\nb\n7");
        assert_eq!(acp(json!({"id": 1, "method": "ping"}), "t", None)["result"]["protocol"], "acp");
        assert_eq!(acp(json!({"id": 1, "method": "nope"}), "t", None)["error"]["code"], -32601);
        // no AI binding: JSON-RPC error, not an HTTP failure
        let no_ai = acp(json!({"id": 5, "method": "session/prompt", "params": {"prompt": "x"}}), "t", None);
        assert_eq!(no_ai["error"]["code"], -32001);
        assert!(no_ai["error"]["message"].as_str().unwrap().contains("Workers AI binding"));
    }

    fn a2a(v: Value, tenant: &str, ai: Option<&EchoAi>) -> Value {
        block_on(handle_a2a(&rpc(v), tenant, "production", ai))
    }

    #[test]
    fn a2a_lifecycle() {
        let ai = EchoAi;
        let sent = a2a(json!({"id": "t1", "method": "tasks/send", "params": {"id": "task-edge-42", "prompt": "audit worker"}}), "a2a-tenant", Some(&ai));
        assert_eq!(sent["result"]["id"], "task-edge-42");
        assert_eq!(sent["result"]["status"]["state"], "completed");
        assert_eq!(sent["result"]["artifacts"][0]["parts"][0]["text"], "echo:audit worker");
        assert_eq!(sent["result"]["history"][0], json!({"role": "user", "parts": [{"type": "text", "text": "audit worker"}]}));
        assert_eq!(sent["result"]["tenant"], "a2a-tenant");
        let got = a2a(json!({"id": "t2", "method": "tasks/get", "params": {"id": "task-edge-42"}}), "a2a-tenant", Some(&ai));
        assert_eq!((got["result"]["id"].clone(), got["result"]["status"]["state"].clone()), (json!("task-edge-42"), json!("completed")));
        let missing = a2a(json!({"id": "t3", "method": "tasks/get", "params": {"id": "missing"}}), "a2a-tenant", Some(&ai));
        assert_eq!((missing["error"]["code"].clone(), missing["error"]["message"].clone()), (json!(-32001), json!("Task not found: missing")));
        // tasks are per tenant
        assert_eq!(a2a(json!({"method": "tasks/get", "params": {"id": "task-edge-42"}}), "elsewhere", Some(&ai))["error"]["code"], -32001);
        // message parts and generated ids
        let msg = a2a(json!({"method": "tasks/send", "params": {"id": "  ", "message": {"parts": [{"text": "one"}, {"text": "two"}]}}}), "a2a-tenant", Some(&ai));
        assert_eq!(msg["result"]["artifacts"][0]["parts"][0]["text"], "echo:one\ntwo");
        assert!(msg["result"]["id"].as_str().unwrap().starts_with("task_"));
        assert_eq!(a2a(json!({"method": "ping"}), "t", None)["result"]["protocol"], "a2a");
        assert_eq!(a2a(json!({"method": "tasks/send", "params": {"prompt": "x"}}), "t", None)["error"]["code"], -32001);
    }

    #[test]
    fn catalog_matches_the_ts_worker() {
        let all = catalog_response(None);
        assert_eq!(all["ok"], true);
        assert_eq!(all["count"], 15);
        assert_eq!(all["updated"], "2026-11-19");
        let joined: String = all["items"].as_array().unwrap().iter().map(|i| format!("{}@{} | ", i["name"].as_str().unwrap(), i["version"].as_str().unwrap())).collect();
        for needle in [
            "cloudflare/cloudflare-os",
            "cloudflare/computer",
            "@cloudflare/dofs",
            "cloudflare/agents@0.33.0",
            "@cloudflare/codemode@0.5.6",
            "@cloudflare/sandbox@1.0.3",
            "@cloudflare/containers@0.3.7",
            "worker@0.9.0",
            "Browser Run + Kitesurf",
            "cloudflare/vibesdk",
            "cloudflare/moltworker",
            "hono@4.13",
            "just-bash@3.6.1",
            "@modelcontextprotocol/server@2.x",
            "Cloudflare Basin",
            "Cloudflare K2",
        ] {
            assert!(joined.contains(needle), "{needle}");
        }
        assert_eq!(catalog_response(Some("data-stream"))["count"], 2);
        assert_eq!(catalog_response(Some("nope"))["count"], 0);
        assert!(catalog_items(None)[1].get("docsUrl").is_some() && catalog_items(None)[0].get("docsUrl").is_none());
    }

    #[test]
    fn tiers_health_card_and_openapi() {
        let bindings = json!({"AI": false});
        let tiers = tiers_response(bindings.clone());
        assert_eq!(tiers["tiers"].as_array().unwrap().iter().map(|t| t["tier"].as_i64().unwrap()).collect::<Vec<_>>(), [0, 1, 2]);
        assert_eq!(tiers["tiers"][0]["coldStartMs"], 0);
        let health = health_response("staging", Value::Null, bindings);
        assert_eq!((health["ok"].clone(), health["product"].clone(), health["env"].clone()), (json!(true), json!("fw-edge"), json!("staging")));
        assert_eq!(health["compatibilityDate"], COMPATIBILITY_DATE);
        assert_eq!(health["deployment"], Value::Null);
        let card = agent_card("https://edge.fw.test", false);
        assert_eq!(card["url"], "https://edge.fw.test/a2a");
        assert_eq!(card["protocolVersion"], "1.0.1");
        assert_eq!(card["capabilities"]["executionTiers"], json!([0, 1, 2]));
        assert_eq!(card["capabilities"]["workersAi"], false);
        assert_eq!(card["skills"].as_array().unwrap().len(), 4);
        let spec = openapi_spec("https://edge.fw.test");
        assert_eq!(spec["openapi"], "3.1.0");
        assert_eq!(spec["info"]["version"], FW_VERSION);
        for p in ["/v1/embeddings", "/os/catalog", "/os/vfs/exec", "/os/gatekeeper", "/api/learn/events", "/drive/{path}"] {
            assert!(spec["paths"].get(p).is_some(), "{p}");
        }
        assert_eq!(spec["servers"][0]["url"], "https://edge.fw.test");
    }

    #[test]
    fn claw_and_cluster_documents() {
        assert_eq!(claw_status()["mcpTools"], 4);
        assert_eq!(claw_ranking()["modernRustClis"][0], json!({"rank": 1, "name": "uv"}));
        assert_eq!(claw_run(None)["goal"], "Audit Cloudflare Edge OS");
        assert_eq!(claw_run(Some("g"))["goal"], "g");
        assert_eq!(claw_run(None)["steps"].as_array().unwrap().len(), 3);
        assert_eq!(cluster_response(json!({}))["tiers"][2], "tier-2-sandbox-container");
    }
}
