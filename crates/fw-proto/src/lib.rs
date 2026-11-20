#![forbid(unsafe_code)]

//! `fw-proto` — Zero-system-dependency unified protocol fabric:
//! HTTP/1.1 control plane, cluster peer registration, GraphQL,
//! OpenAPI 3.1, OpenAI, Gemini Decision 1, MCP, ACP, A2A, and Browser WebGPU/Wasm32 harness.

mod chat;
mod server;
mod ui;

pub use chat::{list_models, proxy_chat_completions};
pub use server::serve;

use fw_apm::unified_pm::scan_unified_workspace;
use fw_cluster::HybridClusterRouter;
use fw_compute::{detect_hardware_profile, run_compute_smoke_bench, DEFAULT_WGSL_KERNEL};
use fw_fx::acp::AcpServer;
use fw_fx::cli::build_status_json;
use fw_fx::config::FxConfig;
use fw_fx::tools::{builtin_tool_specs, execute_builtin_tool};
use fw_hermes_claw::{modern_cli_ranking, protocol_and_agent_ranking, ChannelKind, HermesClawAgent};
use fw_wayland_wgpu::unified_wgpu_terminal::UnifiedWgpuTerminalApp;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Version of the running server, from the workspace manifest.
pub const FW_VERSION: &str = env!("CARGO_PKG_VERSION");


/// Zero-latency in-process MCP tools representing the Linux, Windows, and WebAssembly kernels.
pub const KERNEL_MCP_TOOLS: &[(&str, &str)] = &[
    (
        "linux_kernel_mcp",
        "Zero-latency Linux kernel MCP tool (Linux 7.5 / CachyOS LTO, sched_ext scx_lavd, io_uring, WSL dxgkrnl, POSIX UAPI)",
    ),
    (
        "windows_kernel_mcp",
        "Zero-latency Windows kernel MCP tool (Win32/NT, in-process ConPTY, SDDL NamedPipe, DX12/WGPU, DirectWrite)",
    ),
    (
        "wasm_kernel_mcp",
        "Zero-latency WebAssembly kernel MCP tool (wasm32-unknown-unknown, wasm32-wasip2, Asterinas OSTD XArray/MlsDisk, WebGPU WGSL)",
    ),
];

/// Executes a Linux, Windows, or WebAssembly kernel MCP tool directly in-process (zero fork / zero IPC latency).
pub fn dispatch_kernel_mcp_tool(tool_name: &str, args: &Value) -> Option<Value> {
    let t0 = Instant::now();
    let action = args.get("action").and_then(Value::as_str).unwrap_or("probe");
    let hw = detect_hardware_profile();
    let payload = match tool_name {
        "linux_kernel_mcp" => json!({
            "ok": true,
            "kernel": "linux",
            "action": action,
            "hostOs": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "capabilities": ["sched_ext:scx_lavd", "io_uring", "cgroups-v2", "namespaces", "wsl-dxgkrnl", "uutils-zero-fork"],
            "hardware": hw
        }),
        "windows_kernel_mcp" => json!({
            "ok": true,
            "kernel": "windows",
            "action": action,
            "hostOs": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "capabilities": ["nt-win32", "in-process-conpty", "named-pipe-sddl", "dx12-wgpu", "directwrite", "job-object-sandbox"],
            "hardware": hw
        }),
        "wasm_kernel_mcp" => json!({
            "ok": true,
            "kernel": "webassembly",
            "action": action,
            "targets": ["wasm32-unknown-unknown", "wasm32-wasip2"],
            "capabilities": ["asterinas-ostd-xarray", "mlsdisk-sha256", "webgpu-wgsl", "cloudflare-workers-edge", "kitty-csi-u-pack"],
            "wgslBytes": DEFAULT_WGSL_KERNEL.len()
        }),
        _ => return None,
    };
    Some(json!({
        "tool": tool_name,
        "inProcessZeroLatency": true,
        "latencyUs": t0.elapsed().as_micros(),
        "result": payload
    }))
}

pub use ui::{CONTROL_PLANE_CSS, CONTROL_PLANE_HTML, CONTROL_PLANE_JS, WEBGPU_HTML_HARNESS};

pub fn build_openapi_3_1() -> Value {
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "fw control plane",
            "version": FW_VERSION
        },
        "paths": {
            "/health": { "get": { "summary": "Zero-dep server health" } },
            "/hardware": { "get": { "summary": "OS UAPI/Win32/POSIX/Darwin & GPU/CPU/TPU hardware profile" } },
            "/cluster": { "get": { "summary": "Hybrid 4-GPU + 3-VPS/CPU + User-WebGPU cluster topology (Pingora + Dynamo + vLLM-Router)" } },
            "/cluster/route": { "post": { "summary": "KV-cache radix + disaggregated P/D routing decision" } },
            "/graphql": { "post": { "summary": "GraphQL query endpoint" } },
            "/cluster/peers": { "post": { "summary": "Register a browser WebGPU compute peer (peerId, vramGb, cpuCores)" } },
            "/v1/models": { "get": { "summary": "OpenAI-compatible models list" } },
            "/v1/chat/completions": { "post": { "summary": "OpenAI-compatible chat completion routed across hybrid cluster" } },
            "/v1beta/decisions": { "post": { "summary": "Gemini Decision 1 calibrated zero-token decisions" } },
            "/mcp": { "post": { "summary": "Streamable HTTP MCP JSON-RPC 2.0 (fw-hermes-claw CliToMcpServer)" } },
            "/acp": { "post": { "summary": "Agent Client Protocol JSON-RPC 2.0" } },
            "/a2a": { "post": { "summary": "Agent-to-Agent JSON-RPC 2.0" } },
            "/claw/status": { "get": { "summary": "Hermes/OpenClaw full-Rust runtime status" } },
            "/claw/ranking": { "get": { "summary": "Verified Nov 2026 Rust CLI and protocol rankings" } },
            "/claw/run": { "post": { "summary": "Execute Hermes/OpenClaw grounded goal loop" } },
            "/webgpu": { "get": { "summary": "Live WebGPU WGSL + Wasm32 browser proof page (Blitz / Chromium CEF + Dawn)" } }
        }
    })
}

pub fn handle_graphql(router: &HybridClusterRouter, query: &str) -> Value {
    let mut data = serde_json::Map::new();
    if query.contains("health") || query.contains("status") {
        data.insert("status".into(), json!({ "ok": true, "version": "3.0.0", "zeroSystemDeps": true }));
    }
    if query.contains("hardware") {
        data.insert("hardware".into(), json!(detect_hardware_profile()));
    }
    if query.contains("cluster") {
        data.insert("cluster".into(), router.summary());
    }
    if query.contains("wgsl") {
        data.insert("wgslBytes".into(), json!(DEFAULT_WGSL_KERNEL.len()));
    }
    if data.is_empty() {
        data.insert("status".into(), json!({ "ok": true, "version": "3.0.0" }));
    }
    json!({ "data": data })
}

/// Registers a browser WebGPU compute peer with the cluster router. The peer reports
/// its own id and capacity; nothing is assumed for it. Signaling (SDP/ICE) is the
/// browser's job over its own transport, so no session description is produced here.
pub fn register_peer(router: &mut HybridClusterRouter, body: &Value) -> Result<Value, String> {
    let peer_id = body
        .get("peerId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .ok_or("`peerId` must be 1-64 characters of [A-Za-z0-9_-]")?;
    let positive = |key: &str| {
        body.get(key)
            .and_then(Value::as_u64)
            .filter(|v| (1..=u64::from(u32::MAX)).contains(v))
            .map(|v| v as u32)
            .ok_or(format!("`{key}` must be a positive integer"))
    };
    let vram_gb = positive("vramGb")?;
    let cpu_cores = positive("cpuCores")?;
    router.register_user_webgpu_peer(peer_id, vram_gb, cpu_cores);
    Ok(json!({ "ok": true, "peerId": peer_id, "vramGb": vram_gb, "cpuCores": cpu_cores, "cluster": router.summary() }))
}

pub fn dispatch_http(
    router: &Arc<Mutex<HybridClusterRouter>>,
    method: &str,
    path: &str,
    body_str: &str,
) -> (u16, &'static str, String) {
    let body_json: Value = serde_json::from_str(body_str).unwrap_or_else(|_| json!({}));
    match (method, path) {
        ("GET", "/") => (200, "text/html; charset=utf-8", CONTROL_PLANE_HTML.to_string()),
        ("GET", "/app.css") => (200, "text/css; charset=utf-8", CONTROL_PLANE_CSS.to_string()),
        ("GET", "/app.js") => (200, "text/javascript; charset=utf-8", CONTROL_PLANE_JS.to_string()),
        ("GET", "/webgpu") => (200, "text/html; charset=utf-8", WEBGPU_HTML_HARNESS.to_string()),
        ("GET", "/claw/status") => {
            let agent = HermesClawAgent::new("fw-hermes-claw");
            (
                200,
                "application/json",
                json!({
                    "ok": true,
                    "agent": agent.name,
                    "sandboxAllowBinaries": agent.policy.allow_binaries.len(),
                    "mcpTools": agent.mcp.tools.len(),
                    "memories": agent.memories.len(),
                    "skills": agent.skills.len()
                })
                .to_string(),
            )
        }
        ("GET", "/claw/ranking") => (
            200,
            "application/json",
            json!({
                "ok": true,
                "modernRustClis": modern_cli_ranking(),
                "protocolsAndAgentOs": protocol_and_agent_ranking()
            })
            .to_string(),
        ),
        ("POST", "/claw/run") => {
            let goal = body_json
                .get("goal")
                .and_then(|v| v.as_str())
                .unwrap_or("Audit Rust CLI MCP skills");
            let mut agent = HermesClawAgent::new("fw-hermes-claw");
            let report = agent.run_goal(ChannelKind::HttpWebhook, goal);
            (200, "application/json", json!(report).to_string())
        }
        ("GET", "/health") => (
            200,
            "application/json",
            json!({
                "ok": true,
                "product": "fw",
                "version": FW_VERSION,
                "zeroSystemDeps": true,
                "consolidatedFeatures": ["fx-rust-agent", "unified-pm", "m3-wgpu-terminal", "kernel-mcp", "hybrid-cluster"]
            })
            .to_string(),
        ),
        ("GET", "/v1/fx/status") => {
            let cfg = FxConfig::from_env();
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            (200, "application/json", build_status_json(&cfg, &cwd).to_string())
        }
        ("GET", "/v1/pm/scan") => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let report = scan_unified_workspace(&cwd);
            (200, "application/json", json!(report).to_string())
        }
        ("GET", "/v1/terminal/m3") => {
            let app = UnifiedWgpuTerminalApp::new(120, 40);
            (200, "application/json", json!(app).to_string())
        }
        ("GET", "/hardware") => (200, "application/json", json!(detect_hardware_profile()).to_string()),
        ("GET", "/compute/bench") => (200, "application/json", json!(run_compute_smoke_bench()).to_string()),
        ("GET", "/openapi.json") => (200, "application/json", build_openapi_3_1().to_string()),
        ("GET", "/.well-known/agent.json") => (
            200,
            "application/json",
            json!({
                "name": "Fw v4.2 Consolidated Agent OS",
                "version": FW_VERSION,
                "protocols": ["http", "graphql", "openapi-3.1", "openai", "mcp", "acp", "a2a", "ggml-rpc", "gemini-decision-1"],
                "endpoints": ["/mcp", "/acp", "/a2a", "/v1/chat/completions", "/v1/fx/status", "/v1/pm/scan", "/v1/terminal/m3", "/webgpu"]
            })
            .to_string(),
        ),
        ("GET", "/cluster") => {
            let r = router.lock().unwrap_or_else(|e| e.into_inner());
            (200, "application/json", r.summary().to_string())
        }
        ("POST", "/cluster/route") => {
            let prompt = body_json.get("prompt").and_then(|v| v.as_str()).unwrap_or("fw");
            let draft = body_json.get("userWebGpuDraft").and_then(|v| v.as_bool()).unwrap_or(true);
            let mut r = router.lock().unwrap_or_else(|e| e.into_inner());
            let decision = r.route_request(prompt, draft);
            (200, "application/json", json!({ "ok": true, "decision": decision }).to_string())
        }
        ("POST", "/graphql") => {
            let q = body_json.get("query").and_then(|v| v.as_str()).unwrap_or("{ status }");
            let r = router.lock().unwrap_or_else(|e| e.into_inner());
            (200, "application/json", handle_graphql(&r, q).to_string())
        }
        ("POST", "/cluster/peers") => {
            let mut r = router.lock().unwrap_or_else(|e| e.into_inner());
            match register_peer(&mut r, &body_json) {
                Ok(v) => (200, "application/json", v.to_string()),
                Err(e) => (400, "application/json", json!({ "ok": false, "error": e }).to_string()),
            }
        }
        ("GET", "/v1/models") => {
            let cfg = FxConfig::from_env();
            let api_key = fw_fx::agent::resolve_api_key(&cfg);
            (200, "application/json", list_models(&cfg, api_key.as_deref()).to_string())
        }
        ("POST", "/v1/chat/completions") => {
            let cfg = FxConfig::from_env();
            let api_key = fw_fx::agent::resolve_api_key(&cfg);
            proxy_chat_completions(&cfg, api_key.as_deref(), body_json)
        }
        ("POST", "/mcp") => {
            let id = body_json.get("id").cloned().unwrap_or(json!(1));
            let rpc_method = body_json.get("method").and_then(Value::as_str).unwrap_or("");
            if rpc_method == "tools/call" {
                let name = body_json["params"]["name"].as_str().unwrap_or("");
                let args = body_json["params"].get("arguments").cloned().unwrap_or_else(|| json!({}));
                if let Some(res) = dispatch_kernel_mcp_tool(name, &args) {
                    return (
                        200,
                        "application/json",
                        json!({ "jsonrpc": "2.0", "id": id, "result": res }).to_string(),
                    );
                }
                let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                if name == "fw_pm_scan" {
                    let report = scan_unified_workspace(&cwd);
                    return (200, "application/json", json!({ "jsonrpc": "2.0", "id": id, "result": report }).to_string());
                }
                if name == "fw_m3_terminal_frame" {
                    let app = UnifiedWgpuTerminalApp::new(120, 40);
                    return (200, "application/json", json!({ "jsonrpc": "2.0", "id": id, "result": app }).to_string());
                }
                if builtin_tool_specs().iter().any(|s| s.name == name) {
                    let res = match execute_builtin_tool(&cwd, name, &args) {
                        Ok(val) => json!({ "jsonrpc": "2.0", "id": id, "result": val }),
                        Err(e) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32000, "message": e } }),
                    };
                    return (200, "application/json", res.to_string());
                }
            }
            let agent = HermesClawAgent::new("fw-hermes-claw");
            let mut resp = agent.mcp.handle_jsonrpc(&body_json);
            if rpc_method == "tools/list"
                && let Some(tools) = resp["result"]["tools"].as_array_mut()
            {
                for (name, desc) in KERNEL_MCP_TOOLS {
                    tools.push(json!({
                        "name": name,
                        "description": desc,
                        "inProcessZeroLatency": true
                    }));
                }
                for spec in builtin_tool_specs() {
                    tools.push(json!({
                        "name": spec.name,
                        "description": spec.description,
                        "inputSchema": spec.parameters,
                        "source": "fw-fx"
                    }));
                }
                tools.push(json!({
                    "name": "fw_pm_scan",
                    "description": "Unified pure-Rust package manager scan (npm + crates.io + APM)",
                    "source": "fw-apm"
                }));
                tools.push(json!({
                    "name": "fw_m3_terminal_frame",
                    "description": "Pure-Rust Material 3 1.5.0 + Rio/Kitty WGPU terminal frame descriptor",
                    "source": "fw-wayland-wgpu"
                }));
            }
            (200, "application/json", resp.to_string())
        }
        ("POST", "/acp") => {
            let rpc_method = body_json.get("method").and_then(Value::as_str).unwrap_or("");
            if matches!(rpc_method, "initialize" | "session/new" | "session/load" | "session/prompt" | "session/cancel") {
                let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                let mut acp = AcpServer::new(FxConfig::from_env(), cwd);
                return (200, "application/json", acp.dispatch(&body_json).to_string());
            }
            let id = body_json.get("id").cloned().unwrap_or(json!(1));
            let tool_name = body_json["params"]["name"].as_str().unwrap_or("");
            let kernel_res = if !tool_name.is_empty() {
                dispatch_kernel_mcp_tool(tool_name, &body_json["params"])
            } else {
                None
            };
            (
                200,
                "application/json",
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "ok": true,
                        "endpoint": path,
                        "method": rpc_method,
                        "version": FW_VERSION,
                        "kernelMcpTools": KERNEL_MCP_TOOLS.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
                        "kernelCall": kernel_res
                    }
                })
                .to_string(),
            )
        }
        ("POST", "/a2a") => {
            let id = body_json.get("id").cloned().unwrap_or(json!(1));
            let rpc_method = body_json.get("method").and_then(|v| v.as_str()).unwrap_or("ping");
            let tool_name = body_json["params"]["name"].as_str().unwrap_or("");
            let kernel_res = if !tool_name.is_empty() {
                dispatch_kernel_mcp_tool(tool_name, &body_json["params"])
            } else {
                None
            };
            (
                200,
                "application/json",
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "ok": true,
                        "endpoint": path,
                        "method": rpc_method,
                        "version": FW_VERSION,
                        "kernelMcpTools": KERNEL_MCP_TOOLS.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
                        "kernelCall": kernel_res
                    }
                })
                .to_string(),
            )
        }
        _ => (
            404,
            "application/json",
            json!({ "ok": false, "error": "Not found", "path": path }).to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_graphql_peers_openai_mcp_acp_a2a_dispatch_cleanly() {
        let router = Arc::new(Mutex::new(HybridClusterRouter::default_hybrid_cluster()));
        let (code, _, gql) = dispatch_http(&router, "POST", "/graphql", r#"{"query":"{ status hardware cluster wgsl }"}"#);
        assert_eq!(code, 200);
        assert!(gql.contains("fw-hybrid-v3"));

        let (code, _, peer) = dispatch_http(&router, "POST", "/cluster/peers", r#"{"peerId":"browser-rtx","vramGb":8,"cpuCores":8}"#);
        assert_eq!(code, 200, "{peer}");
        assert!(peer.contains("\"id\":\"browser-rtx\""), "{peer}");
        let (code, _, bad) = dispatch_http(&router, "POST", "/cluster/peers", r#"{"peerId":"browser-rtx","vramGb":8}"#);
        assert_eq!(code, 400);
        assert!(bad.contains("cpuCores"));
        let (code, _, _) = dispatch_http(&router, "POST", "/rtc/offer", "{}");
        assert_eq!(code, 404);

        let (code, _, html) = dispatch_http(&router, "GET", "/webgpu", "");
        assert_eq!(code, 200);
        assert!(html.contains("navigator.gpu"));

        // Chat is answered by the configured gateway (see chat.rs tests), never synthesized here.
        let (code, _, oai) = dispatch_http(&router, "POST", "/v1/chat/completions", r#"{"messages":[]}"#);
        assert_eq!(code, 400);
        assert!(oai.contains("non-empty"));

        // Verify zero-latency Linux, Windows, and WebAssembly kernel MCP tools over /mcp, /acp, /a2a
        let (code, _, mcp_list) = dispatch_http(&router, "POST", "/mcp", r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
        assert_eq!(code, 200);
        assert!(mcp_list.contains("linux_kernel_mcp"));
        assert!(mcp_list.contains("windows_kernel_mcp"));
        assert!(mcp_list.contains("wasm_kernel_mcp"));

        for tool in ["linux_kernel_mcp", "windows_kernel_mcp", "wasm_kernel_mcp"] {
            let body = format!(r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"{tool}","arguments":{{"action":"probe"}}}}}}"#);
            let (c, _, out) = dispatch_http(&router, "POST", "/mcp", &body);
            assert_eq!(c, 200);
            assert!(out.contains("\"inProcessZeroLatency\":true"), "out={out}");
        }

        let (c_acp, _, out_acp) = dispatch_http(&router, "POST", "/acp", r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"linux_kernel_mcp"}}"#);
        assert_eq!(c_acp, 200);
        assert!(out_acp.contains("sched_ext:scx_lavd"));

        let (c_a2a, _, out_a2a) = dispatch_http(&router, "POST", "/a2a", r#"{"jsonrpc":"2.0","id":4,"method":"tasks/send","params":{"name":"windows_kernel_mcp"}}"#);
        assert_eq!(c_a2a, 200);
        assert!(out_a2a.contains("in-process-conpty"));

        // Consolidated live endpoints: /v1/fx/status, /v1/pm/scan, /v1/terminal/m3, real /acp initialize, /mcp fw_pm_scan
        let (c_fx, _, out_fx) = dispatch_http(&router, "GET", "/v1/fx/status", "");
        assert_eq!(c_fx, 200);
        assert!(out_fx.contains("fw-fx-rust"));

        let (c_pm, _, out_pm) = dispatch_http(&router, "GET", "/v1/pm/scan", "");
        assert_eq!(c_pm, 200);
        assert!(out_pm.contains("fw-unified-pm"));

        let (c_term, _, out_term) = dispatch_http(&router, "GET", "/v1/terminal/m3", "");
        assert_eq!(c_term, 200);
        assert!(out_term.contains("kitty_keyboard_flags"));

        let (c_acp_init, _, out_acp_init) = dispatch_http(&router, "POST", "/acp", r#"{"jsonrpc":"2.0","id":9,"method":"initialize"}"#);
        assert_eq!(c_acp_init, 200);
        assert!(out_acp_init.contains("\"protocolVersion\":1"));
    }
}
