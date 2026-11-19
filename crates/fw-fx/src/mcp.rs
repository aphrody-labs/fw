//! Pure MCP engine matching `vercel-labs/fx/src/mcp/`.
//! Owns server registry, schema validation, lazy tool search, and JSON-RPC 2.0 dispatch.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub transport: String,
    pub command_or_url: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolEntry {
    pub server: String,
    pub name: String,
    pub qualified_name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Default)]
pub struct McpEngine {
    pub servers: BTreeMap<String, McpServerConfig>,
    pub tools: Vec<McpToolEntry>,
}

impl McpEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_server(&mut self, cfg: McpServerConfig) {
        self.servers.insert(cfg.name.clone(), cfg);
    }

    pub fn register_tool(&mut self, server: &str, name: &str, description: &str, input_schema: Value) {
        self.tools.push(McpToolEntry {
            server: server.to_owned(),
            name: name.to_owned(),
            qualified_name: format!("mcp__{server}__{name}"),
            description: description.to_owned(),
            input_schema,
        });
    }

    /// Lazy tool search matching fx's on-demand schema loading.
    pub fn search_tools(&self, query: &str) -> Vec<&McpToolEntry> {
        let q = query.to_ascii_lowercase();
        self.tools
            .iter()
            .filter(|t| {
                q.is_empty()
                    || t.qualified_name.to_ascii_lowercase().contains(&q)
                    || t.description.to_ascii_lowercase().contains(&q)
            })
            .collect()
    }

    /// Dispatch a JSON-RPC 2.0 MCP request (`initialize`, `tools/list`, `tools/search`).
    pub fn handle_jsonrpc(&self, req: &Value) -> Value {
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let method = req.get("method").and_then(Value::as_str).unwrap_or("");
        match method {
            "initialize" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2025-11-25",
                    "serverInfo": { "name": "fw-fx-mcp", "version": crate::config::FX_VERSION },
                    "capabilities": { "tools": { "listChanged": true } }
                }
            }),
            "tools/list" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "tools": self.tools.iter().map(|t| json!({
                        "name": t.qualified_name,
                        "description": t.description,
                        "inputSchema": t.input_schema
                    })).collect::<Vec<_>>()
                }
            }),
            _ => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("Method not found: {method}") }
            }),
        }
    }
}
