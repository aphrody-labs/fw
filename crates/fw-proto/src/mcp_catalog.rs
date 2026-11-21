//! The tools `fw mcp` and `fw serve` expose over MCP: crate intelligence, workspace file and search tools,
//! host target probes, the package-manager scan and the CLI tools of the Hermes/Claw runtime.

use crate::mcp::{CallContext, Catalog, Identity, Outcome};
use crate::{dispatch_kernel_mcp_tool, FW_VERSION, KERNEL_MCP_TOOLS};
use fw_apm::unified_pm::scan_unified_workspace;
use fw_fx::tools::{builtin_tool_specs, execute_builtin_tool};
use fw_hermes_claw::HermesClawAgent;
use fw_wayland_wgpu::unified_wgpu_terminal::UnifiedWgpuTerminalApp;
use serde_json::{json, Value};
use std::path::PathBuf;

/// fw-fx built-in tools the in-process executor can run. The others (`web_search`, `subagent`,
/// `read_tool_result`, `ask_user_question`, `install_skill`, `toolbox`) need the agent host, so they are not listed.
const EXECUTABLE_FX_TOOLS: &[&str] = &["read_file", "write_file", "edit_file", "glob_files", "grep_files", "shell", "web_fetch"];

pub struct FwCatalog {
    workspace: PathBuf,
}

impl FwCatalog {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    pub fn from_current_dir() -> Self {
        Self::new(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }
}

fn annotated(mut tool: Value, title: &str, read_only: bool, destructive: bool, open_world: bool) -> Value {
    tool["title"] = json!(title);
    tool["annotations"] = json!({ "readOnlyHint": read_only, "destructiveHint": destructive, "idempotentHint": read_only, "openWorldHint": open_world });
    tool
}

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({ "name": name, "description": description, "inputSchema": input_schema })
}

fn no_arguments() -> Value {
    json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

/// A result carrying the value both as text and, when it is an object, as `structuredContent`.
fn value_outcome(value: &Value, is_error: bool) -> Outcome {
    let mut result = json!({ "content": [{ "type": "text", "text": value.to_string() }], "isError": is_error });
    if value.is_object() {
        result["structuredContent"] = value.clone();
    }
    Outcome::Complete(result)
}

impl Catalog for FwCatalog {
    fn identity(&self) -> Identity {
        Identity {
            name: "fw".to_owned(),
            title: Some("fw".to_owned()),
            version: FW_VERSION.to_owned(),
            instructions: Some("fw: crate intelligence (crates_*), workspace files and search, host target probes, the package-manager scan and CLI tools such as rg_search.".to_owned()),
        }
    }

    fn capabilities(&self) -> Value {
        json!({ "tools": { "listChanged": false } })
    }

    fn tools(&self) -> Vec<Value> {
        let mut tools: Vec<Value> = KERNEL_MCP_TOOLS
            .iter()
            .map(|(name, description)| {
                let schema = json!({ "type": "object", "properties": { "action": { "type": "string", "description": "Probe action; only `probe` exists." } }, "additionalProperties": false });
                annotated(tool(name, description, schema), name, true, false, false)
            })
            .collect();
        tools.push(annotated(tool("fw_pm_scan", "Unified pure-Rust package manager scan (npm + crates.io + APM) of the workspace", no_arguments()), "Package manager scan", true, false, false));
        tools.push(annotated(tool("fw_m3_terminal_frame", "Pure-Rust Material 3 1.5.0 + Rio/Kitty WGPU terminal frame descriptor", no_arguments()), "Terminal frame", true, false, false));
        for (name, description, schema) in fw_crates::mcp_tools() {
            let live = description.contains("live=true");
            tools.push(annotated(tool(name, description, schema), name, true, false, live));
        }
        for spec in builtin_tool_specs().into_iter().filter(|spec| EXECUTABLE_FX_TOOLS.contains(&spec.name)) {
            let (read_only, destructive, open_world) = match spec.name {
                "write_file" | "edit_file" => (false, true, false),
                "shell" => (false, true, true),
                "web_fetch" => (true, false, true),
                _ => (true, false, false),
            };
            tools.push(annotated(tool(spec.name, spec.description, spec.parameters), spec.name, read_only, destructive, open_world));
        }
        let listed = HermesClawAgent::new("fw").mcp.handle_jsonrpc(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }));
        if let Some(cli_tools) = listed.pointer("/result/tools").and_then(Value::as_array) {
            for cli_tool in cli_tools {
                let name = cli_tool.get("name").and_then(Value::as_str).unwrap_or_default();
                let read_only = cli_tool.pointer("/annotations/readOnlyHint").and_then(Value::as_bool).unwrap_or(false);
                let mut cli_tool = cli_tool.clone();
                if let Some(schema) = cli_tool.get_mut("inputSchema").and_then(Value::as_object_mut) {
                    schema.entry("additionalProperties").or_insert(json!(false));
                }
                tools.push(annotated(cli_tool, name, read_only, !read_only, true));
            }
        }
        tools.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        tools
    }

    fn call_tool(&self, name: &str, arguments: &Value, _cx: &CallContext<'_>) -> Outcome {
        if let Some(probe) = dispatch_kernel_mcp_tool(name, arguments) {
            return value_outcome(&probe, false);
        }
        if let Some(answer) = fw_crates::call_mcp_tool(name, arguments) {
            return match answer {
                Ok(value) => value_outcome(&value, false),
                Err(message) => Outcome::tool_error(message),
            };
        }
        match name {
            "fw_pm_scan" => value_outcome(&json!(scan_unified_workspace(&self.workspace)), false),
            "fw_m3_terminal_frame" => value_outcome(&json!(UnifiedWgpuTerminalApp::new(120, 40)), false),
            _ if EXECUTABLE_FX_TOOLS.contains(&name) => match execute_builtin_tool(&self.workspace, name, arguments) {
                Ok(value) => {
                    let failed = value.get("ok").and_then(Value::as_bool) == Some(false);
                    value_outcome(&value, failed)
                }
                Err(message) => Outcome::tool_error(message),
            },
            _ => {
                let answer = HermesClawAgent::new("fw").mcp.handle_jsonrpc(&json!({
                    "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": arguments }
                }));
                match answer.get("result") {
                    Some(result) => Outcome::Complete(result.clone()),
                    None => Outcome::tool_error(answer.pointer("/error/message").and_then(Value::as_str).unwrap_or("the tool failed").to_owned()),
                }
            }
        }
    }
}
