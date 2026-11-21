//! Existing MCP servers as UTCP providers, through the rmcp client (`server/discover` first, `initialize`
//! as the fallback, so MCP 2026-07-28 and 2025-11-25 servers both work).

use crate::config::Secret;
use crate::http::{check_url, secret_header};
use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use rmcp::model::{CallToolRequestParams, ContentBlock, ProtocolVersion};
use rmcp::service::{ClientInitializeError, ClientLifecycleMode, ClientServiceExt, RoleClient, RunningService};
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::{StreamableHttpClientTransport, TokioChildProcess};
use rs_utcp::providers::base::Provider;
use rs_utcp::providers::mcp::McpProvider;
use rs_utcp::tools::{Tool, ToolInputOutputSchema};
use rs_utcp::transports::ClientTransport;
use rs_utcp::transports::stream::{StreamResult, VecStreamResult};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

type Session = RunningService<RoleClient, ()>;

#[derive(Default)]
pub struct RmcpProtocol {
    sessions: RwLock<HashMap<String, Arc<Session>>>,
    secrets: std::sync::RwLock<HashMap<String, Secret>>,
}

fn lifecycle() -> ClientLifecycleMode {
    ClientLifecycleMode::Auto { preferred_versions: vec![ProtocolVersion::LATEST], legacy_version: None }
}

impl RmcpProtocol {
    pub fn set_secret(&self, provider: &str, secret: Secret) {
        self.secrets.write().unwrap_or_else(std::sync::PoisonError::into_inner).insert(provider.to_string(), secret);
    }

    fn secret(&self, provider: &str) -> Option<Secret> {
        self.secrets.read().unwrap_or_else(std::sync::PoisonError::into_inner).get(provider).cloned()
    }

    async fn connect(&self, provider: &McpProvider) -> Result<Session> {
        let name = provider.base.name.clone();
        let describe = |error: ClientInitializeError| anyhow!("MCP server {name}: {error}");
        if let Some(url) = &provider.url {
            let url = check_url(url)?;
            let mut config = StreamableHttpClientTransportConfig::with_uri(url.as_str());
            if let Some(secret) = self.secret(&name) {
                let (header, value) = secret_header(&secret)?;
                let token = match (header.eq_ignore_ascii_case("authorization"), value.strip_prefix("Bearer ")) {
                    (true, Some(token)) => token.to_string(),
                    _ => {
                        let mut headers = HashMap::new();
                        headers.insert(http::HeaderName::from_bytes(header.as_bytes())?, http::HeaderValue::from_str(&value)?);
                        config = config.custom_headers(headers);
                        String::new()
                    }
                };
                if !token.is_empty() {
                    config = config.auth_header(token);
                }
            }
            let transport = StreamableHttpClientTransport::with_client(reqwest::Client::default(), config);
            return ().serve_with_lifecycle(transport, lifecycle()).await.map_err(describe);
        }
        let command = provider.command.clone().ok_or_else(|| anyhow!("MCP provider {name} has neither a url nor a command"))?;
        let mut process = tokio::process::Command::new(command);
        process.args(provider.args.clone().unwrap_or_default());
        process.env_clear();
        for key in ["PATH", "HOME", "USERPROFILE", "SYSTEMROOT", "TEMP", "TMP"] {
            if let Ok(value) = std::env::var(key) {
                process.env(key, value);
            }
        }
        for (key, value) in provider.env_vars.clone().unwrap_or_default() {
            process.env(key, value);
        }
        let transport = TokioChildProcess::new(process).with_context(|| format!("starting MCP server {name}"))?;
        ().serve_with_lifecycle(transport, lifecycle()).await.map_err(describe)
    }

    async fn session(&self, provider: &str) -> Result<Arc<Session>> {
        self.sessions.read().await.get(provider).cloned().ok_or_else(|| anyhow!("MCP provider {provider} is not connected"))
    }
}

fn utcp_tool(tool: &rmcp::model::Tool) -> Tool {
    let schema = Value::Object((*tool.input_schema).clone());
    let properties = schema.get("properties").and_then(Value::as_object).map(|map| map.iter().map(|(key, value)| (key.clone(), value.clone())).collect());
    let required = schema.get("required").and_then(Value::as_array).map(|items| items.iter().filter_map(|item| item.as_str().map(str::to_string)).collect());
    Tool {
        name: tool.name.to_string(),
        description: tool.description.as_deref().unwrap_or_default().to_string(),
        inputs: ToolInputOutputSchema {
            type_: schema.get("type").and_then(Value::as_str).unwrap_or("object").to_string(),
            properties,
            required,
            description: None,
            title: None,
            items: None,
            enum_: None,
            minimum: None,
            maximum: None,
            format: None,
        },
        outputs: ToolInputOutputSchema { type_: "object".into(), properties: None, required: None, description: None, title: None, items: None, enum_: None, minimum: None, maximum: None, format: None },
        tags: Vec::new(),
        average_response_size: None,
        provider: None,
    }
}

/// An MCP result as one JSON value: the structured content when there is some, the text otherwise.
fn result_value(result: rmcp::model::CallToolResult) -> Result<Value> {
    let mut texts = Vec::new();
    let mut other = Vec::new();
    for block in &result.content {
        match block {
            ContentBlock::Text(text) => texts.push(text.text.clone()),
            block => other.push(serde_json::to_value(block)?),
        }
    }
    if result.is_error == Some(true) {
        bail!("{}", if texts.is_empty() { "the MCP server reported an error without a message".to_string() } else { texts.join("\n") });
    }
    if let Some(structured) = result.structured_content {
        return Ok(structured);
    }
    if other.is_empty() {
        let joined = texts.join("\n");
        return Ok(serde_json::from_str(&joined).unwrap_or(Value::String(joined)));
    }
    Ok(Value::Array(texts.into_iter().map(Value::String).chain(other).collect()))
}

#[async_trait]
impl ClientTransport for RmcpProtocol {
    async fn register_tool_provider(&self, prov: &dyn Provider) -> Result<Vec<Tool>> {
        let provider = prov.as_any().downcast_ref::<McpProvider>().ok_or_else(|| anyhow!("provider {} is not an MCP provider", prov.name()))?;
        let session = Arc::new(self.connect(provider).await?);
        let tools = session.peer().list_all_tools().await.map_err(|error| anyhow!("MCP server {}: tools/list: {error}", provider.base.name))?;
        self.sessions.write().await.insert(provider.base.name.clone(), session);
        Ok(tools.iter().map(|tool| crate::with_name_tags(utcp_tool(tool), &provider.base.name)).collect())
    }

    async fn deregister_tool_provider(&self, prov: &dyn Provider) -> Result<()> {
        self.sessions.write().await.remove(&prov.name());
        Ok(())
    }

    async fn call_tool(&self, tool_name: &str, args: HashMap<String, Value>, prov: &dyn Provider) -> Result<Value> {
        let session = self.session(&prov.name()).await?;
        let arguments: Map<String, Value> = args.into_iter().collect();
        let params = CallToolRequestParams::new(tool_name.to_string()).with_arguments(arguments);
        let result = session.peer().call_tool(params).await.map_err(|error| anyhow!("MCP server {}: {error}", prov.name()))?;
        result_value(result)
    }

    async fn call_tool_stream(&self, tool_name: &str, args: HashMap<String, Value>, prov: &dyn Provider) -> Result<Box<dyn StreamResult>> {
        let value = self.call_tool(tool_name, args, prov).await?;
        Ok(Box::new(VecStreamResult::new(vec![value], None)))
    }
}
