//! The MCP face of the gateway: three meta-tools whose cost does not grow with the catalogue, and optionally
//! every registered tool as a typed first-level tool.

use crate::Gateway;
use crate::config::Expose;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation, ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{ErrorData as McpError, ServerHandler};
use rs_utcp::tools::{Tool as UtcpTool, ToolInputOutputSchema};
use serde_json::{Map, Value, json};
use std::sync::Arc;

pub const SEARCH_TOOLS: &str = "search_tools";
pub const CALL_TOOL: &str = "call_tool";
pub const RUN_CODEMODE: &str = "run_codemode";
const PAGE: usize = 100;

#[derive(Clone)]
pub struct GatewayServer {
    gateway: Arc<Gateway>,
}

fn object(value: Value) -> Arc<Map<String, Value>> {
    match value {
        Value::Object(map) => Arc::new(map),
        _ => Arc::default(),
    }
}

fn meta_tools() -> Vec<rmcp::model::Tool> {
    vec![
        rmcp::model::Tool::new(
            SEARCH_TOOLS,
            "Find tools by what they do. Returns names, short descriptions and input shapes; call a result with call_tool.",
            object(json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "What you want to do, in a few words" },
                    "limit": { "type": "integer", "description": "Most results to return", "minimum": 1 }
                },
                "required": ["query"]
            })),
        ),
        rmcp::model::Tool::new(
            CALL_TOOL,
            "Call a tool found with search_tools by its exact name.",
            object(json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Tool name as returned by search_tools" },
                    "arguments": { "type": "object", "description": "Arguments matching the tool's input shape" }
                },
                "required": ["name"]
            })),
        ),
        rmcp::model::Tool::new(
            RUN_CODEMODE,
            "Run a short Rhai script that calls several tools in one step with call_tool(name, #{..}) and search_tools(query, limit); only the final value comes back.",
            object(json!({
                "type": "object",
                "properties": {
                    "code": { "type": "string", "description": "Rhai code; its last expression is the result" },
                    "timeout_ms": { "type": "integer", "description": "Time limit, 45000 at most", "minimum": 1 }
                },
                "required": ["code"]
            })),
        ),
    ]
}

fn schema_json(schema: &ToolInputOutputSchema) -> Map<String, Value> {
    let mut map = Map::new();
    map.insert("type".into(), Value::String(schema.type_.clone()));
    map.insert("properties".into(), Value::Object(schema.properties.clone().unwrap_or_default().into_iter().collect()));
    if let Some(required) = &schema.required {
        map.insert("required".into(), json!(required));
    }
    map
}

fn typed_tool(tool: &UtcpTool) -> rmcp::model::Tool {
    rmcp::model::Tool::new(tool.name.clone(), tool.description.clone(), Arc::new(schema_json(&tool.inputs)))
}

fn cut(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        Some((index, _)) => format!("{}...", &text[..index]),
        None => text.to_string(),
    }
}

/// What `search_tools` shows per tool: enough to call it, nothing more.
fn summary(tool: &UtcpTool, description_limit: usize) -> Value {
    let properties: Map<String, Value> = tool
        .inputs
        .properties
        .iter()
        .flatten()
        .map(|(name, schema)| {
            let mut entry = Map::new();
            if let Some(kind) = schema.get("type") {
                entry.insert("type".into(), kind.clone());
            }
            if let Some(options) = schema.get("enum") {
                entry.insert("enum".into(), options.clone());
            }
            if let Some(description) = schema.get("description").and_then(Value::as_str) {
                entry.insert("description".into(), Value::String(cut(description, 80)));
            }
            (name.clone(), Value::Object(entry))
        })
        .collect();
    let mut inputs = Map::new();
    inputs.insert("properties".into(), Value::Object(properties));
    if let Some(required) = tool.inputs.required.as_ref().filter(|required| !required.is_empty()) {
        inputs.insert("required".into(), json!(required));
    }
    json!({ "name": tool.name, "description": cut(&tool.description, description_limit), "inputs": inputs })
}

fn limit_text(text: String, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n[cut at {} of {} bytes]", &text[..end], end, text.len())
}

fn success(value: Value, max_bytes: usize) -> CallToolResult {
    let text = match value {
        Value::String(text) => text,
        other => other.to_string(),
    };
    CallToolResult::success(vec![ContentBlock::text(limit_text(text, max_bytes))])
}

/// A failed call is a result the model can read and correct, not a protocol error.
fn failure(error: &anyhow::Error) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(format!("{error:#}"))])
}

impl GatewayServer {
    pub fn new(gateway: Arc<Gateway>) -> Self {
        Self { gateway }
    }

    pub fn gateway(&self) -> &Arc<Gateway> {
        &self.gateway
    }

    /// Size in bytes of the `tools/list` answer an agent would put in its context, per exposure mode.
    pub async fn listing_bytes(&self) -> anyhow::Result<(usize, usize, usize)> {
        let meta = serde_json::to_vec(&meta_tools())?.len();
        let tools = self.gateway.all_tools().await?;
        let typed: Vec<rmcp::model::Tool> = tools.iter().map(typed_tool).collect();
        Ok((meta, typed.len(), serde_json::to_vec(&typed)?.len()))
    }

    async fn visible_tools(&self) -> anyhow::Result<Vec<rmcp::model::Tool>> {
        let mut tools = Vec::new();
        let expose = self.gateway.settings.expose;
        if matches!(expose, Expose::Meta | Expose::Both) {
            tools.extend(meta_tools());
        }
        if matches!(expose, Expose::Typed | Expose::Both) {
            tools.extend(self.gateway.all_tools().await?.iter().map(typed_tool));
        }
        Ok(tools)
    }

    async fn dispatch(&self, name: &str, arguments: Map<String, Value>) -> Result<CallToolResult, McpError> {
        let settings = &self.gateway.settings;
        match name {
            SEARCH_TOOLS => {
                let query = arguments.get("query").and_then(Value::as_str).ok_or_else(|| McpError::invalid_params("search_tools needs a string `query`", None))?;
                let limit = arguments.get("limit").and_then(Value::as_u64).map_or(settings.search_limit, |limit| limit.clamp(1, 50) as usize);
                Ok(match self.gateway.search(query, limit).await {
                    Ok(tools) => {
                        let found: Vec<Value> = tools.iter().map(|tool| summary(tool, settings.max_description_chars)).collect();
                        success(json!({ "tools": found }), settings.max_result_bytes)
                    }
                    Err(error) => failure(&error),
                })
            }
            CALL_TOOL => {
                let target = arguments.get("name").and_then(Value::as_str).ok_or_else(|| McpError::invalid_params("call_tool needs a string `name`", None))?;
                let tool_arguments = match arguments.get("arguments") {
                    None | Some(Value::Null) => Map::new(),
                    Some(Value::Object(map)) => map.clone(),
                    Some(_) => return Err(McpError::invalid_params("`arguments` must be an object", None)),
                };
                Ok(self.call_utcp(target, tool_arguments).await)
            }
            RUN_CODEMODE => {
                let code = arguments.get("code").and_then(Value::as_str).ok_or_else(|| McpError::invalid_params("run_codemode needs a string `code`", None))?;
                let timeout = arguments.get("timeout_ms").and_then(Value::as_u64);
                Ok(match self.gateway.codemode(code.to_string(), timeout).await {
                    Ok(result) => success(json!({ "value": result.value, "stdout": result.stdout, "stderr": result.stderr }), settings.max_result_bytes),
                    Err(error) => failure(&error),
                })
            }
            typed => Ok(self.call_utcp(typed, arguments).await),
        }
    }

    async fn call_utcp(&self, name: &str, arguments: Map<String, Value>) -> CallToolResult {
        match self.gateway.call(name, arguments).await {
            Ok(value) => success(value, self.gateway.settings.max_result_bytes),
            Err(error) => failure(&error),
        }
    }
}

impl ServerHandler for GatewayServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("fw-gateway", env!("CARGO_PKG_VERSION")))
            .with_instructions("Tools are behind search_tools: search by intent, then call_tool by name; run_codemode chains several calls in one step.")
    }

    async fn list_tools(&self, request: Option<PaginatedRequestParams>, _context: RequestContext<RoleServer>) -> Result<ListToolsResult, McpError> {
        let all = self.visible_tools().await.map_err(|error| McpError::internal_error(format!("{error:#}"), None))?;
        let start = match request.and_then(|request| request.cursor) {
            None => 0,
            Some(cursor) => cursor.parse::<usize>().map_err(|_| McpError::invalid_params("invalid cursor", None))?,
        };
        let end = (start + PAGE).min(all.len());
        let page = all.get(start..end).unwrap_or_default().to_vec();
        let mut result = ListToolsResult::with_all_items(page);
        result.next_cursor = (end < all.len()).then(|| end.to_string());
        Ok(result)
    }

    async fn call_tool(&self, request: CallToolRequestParams, _context: RequestContext<RoleServer>) -> Result<CallToolResponse, McpError> {
        let arguments = request.arguments.unwrap_or_default();
        Ok(self.dispatch(&request.name, arguments).await?.into())
    }
}
