//! One in-process tool gateway.
//!
//! UTCP (`rs-utcp`) holds the tool repository, search and direct calls; OpenAPI documents become tools called
//! straight over HTTP; existing MCP servers are consumed through the rmcp client; the whole catalogue is
//! served to agents by [`server::GatewayServer`] as MCP over stdio or MCP over ACP. Secrets are read from the
//! host environment at call time and never appear in a schema, a result or an error.

pub mod acp;
pub mod config;
pub mod http;
pub mod mcp_client;
pub mod server;

use anyhow::{Context, Result, anyhow, bail};
use config::{Config, ProviderConfig, Settings};
use http::{HttpProtocol, Route};
use mcp_client::RmcpProtocol;
use rs_utcp::UtcpClient;
use rs_utcp::UtcpClientInterface;
use rs_utcp::config::UtcpClientConfig;
use rs_utcp::plugins::codemode::{CodeModeArgs, CodeModeResult, CodeModeUtcp};
use rs_utcp::providers::base::Provider;
use rs_utcp::providers::http::HttpProvider;
use rs_utcp::providers::mcp::McpProvider;
use rs_utcp::repository::ToolRepository;
use rs_utcp::repository::in_memory::InMemoryToolRepository;
use rs_utcp::tag::tag_search::TagSearchStrategy;
use rs_utcp::tools::Tool;
use rs_utcp::transports::register_communication_protocol;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

/// `UtcpClient` copies the process-wide protocol registry when it is created, so the protocols are installed
/// once: `http` and `mcp` are ours, `cli` and `text` refuse every call.
struct Protocols {
    http: Arc<HttpProtocol>,
    mcp: Arc<RmcpProtocol>,
}

fn protocols() -> &'static Protocols {
    static PROTOCOLS: OnceLock<Protocols> = OnceLock::new();
    PROTOCOLS.get_or_init(|| {
        let http = Arc::new(HttpProtocol::new());
        let mcp = Arc::new(RmcpProtocol::default());
        register_communication_protocol("http", http.clone());
        register_communication_protocol("mcp", mcp.clone());
        register_communication_protocol("cli", Arc::new(http::DeniedProtocol("cli")));
        register_communication_protocol("text", Arc::new(http::DeniedProtocol("text")));
        Protocols { http, mcp }
    })
}

#[derive(Debug, Clone)]
pub struct ProviderReport {
    pub name: String,
    pub kind: &'static str,
    pub tools: usize,
    pub rejected: usize,
    pub error: Option<String>,
}

pub struct Gateway {
    utcp: Arc<UtcpClient>,
    repo: Arc<InMemoryToolRepository>,
    codemode: Arc<CodeModeUtcp>,
    pub settings: Settings,
    pub report: Vec<ProviderReport>,
}

/// Words of a tool name become tags: the search strategy scores tags and descriptions, not names.
pub(crate) fn with_name_tags(mut tool: Tool, provider: &str) -> Tool {
    let mut tags = std::mem::take(&mut tool.tags);
    tags.push(provider.to_string());
    for word in tool.name.split(|character: char| !character.is_alphanumeric()).filter(|word| word.len() > 1) {
        tags.push(word.to_lowercase());
    }
    tags.sort();
    tags.dedup();
    tool.tags = tags;
    tool
}

fn protocol_only(provider: &mut rs_utcp::providers::base::BaseProvider, key: &str) {
    provider.allowed_communication_protocols = Some(vec![key.to_string()]);
}

async fn load_spec(location: &str) -> Result<(Value, Option<String>)> {
    if location.starts_with("https://") || location.starts_with("http://") {
        http::check_url(location)?;
        let (spec, final_url) = rs_utcp::openapi::load_spec_from_url(location).await?;
        return Ok((spec, Some(final_url)));
    }
    let text = std::fs::read_to_string(location).with_context(|| format!("reading {location}"))?;
    Ok((serde_json::from_str(&text).with_context(|| format!("{location} is not JSON"))?, None))
}

impl Gateway {
    pub async fn from_config(config: Config) -> Result<Self> {
        let protocols = protocols();
        let repo = Arc::new(InMemoryToolRepository::new());
        let strategy = Arc::new(TagSearchStrategy::new(repo.clone() as Arc<dyn ToolRepository>, config.gateway.description_weight));
        let utcp = Arc::new(UtcpClient::new(UtcpClientConfig::new(), repo.clone(), strategy).await?);
        let codemode = Arc::new(CodeModeUtcp::new(utcp.clone() as Arc<dyn UtcpClientInterface>));
        let mut gateway = Self { utcp, repo, codemode, settings: config.gateway.clone(), report: Vec::new() };
        for provider in &config.providers {
            let kind = match provider {
                ProviderConfig::Openapi { .. } => "openapi",
                ProviderConfig::Mcp { .. } => "mcp",
                ProviderConfig::Manual { .. } => "manual",
            };
            let outcome = gateway.register(provider, protocols).await;
            gateway.report.push(match outcome {
                Ok((tools, rejected)) => ProviderReport { name: provider.name().to_string(), kind, tools, rejected, error: None },
                Err(error) => ProviderReport { name: provider.name().to_string(), kind, tools: 0, rejected: 0, error: Some(format!("{error:#}")) },
            });
        }
        Ok(gateway)
    }

    async fn register(&self, provider: &ProviderConfig, protocols: &Protocols) -> Result<(usize, usize)> {
        match provider {
            ProviderConfig::Openapi { name, spec, base_url, auth, tags } => {
                let (document, location) = load_spec(spec).await?;
                let converted = http::convert_openapi(document, location.clone(), name, base_url.as_deref(), tags)?;
                if converted.tools.is_empty() {
                    bail!("the document declares no operation{}", if tags.is_empty() { "" } else { " with the requested tags" });
                }
                let anchor = base_url.clone().or(location).unwrap_or_else(|| "https://invalid.localhost/".to_string());
                self.register_http(name, &anchor, auth.clone(), converted.tools, converted.routes, protocols).await
            }
            ProviderConfig::Manual { name, source, allow, auth } => {
                if let Some(other) = allow.iter().find(|key| key.as_str() != "http") {
                    bail!("manual {name}: protocol {other:?} is not supported, only http");
                }
                let (document, location) = load_spec(source).await?;
                let tools = document.get("tools").and_then(Value::as_array).ok_or_else(|| anyhow!("manual {name}: no `tools` array"))?;
                let mut accepted = Vec::new();
                let mut routes = Vec::new();
                let mut rejected = 0;
                for raw in tools {
                    let Ok(mut tool) = serde_json::from_value::<Tool>(raw.clone()) else {
                        rejected += 1;
                        continue;
                    };
                    let declared = tool.provider.take().and_then(|value| serde_json::from_value::<HttpProvider>(value).ok());
                    let Some(declared) = declared.filter(|declared| http::check_url(&declared.url.replace(['{', '}'], "")).is_ok()) else {
                        rejected += 1;
                        continue;
                    };
                    let method = declared.http_method.to_ascii_uppercase();
                    let query = if matches!(method.as_str(), "GET" | "DELETE" | "HEAD") { tool.inputs.properties.clone().unwrap_or_default().into_keys().collect() } else { Default::default() };
                    routes.push((tool.name.clone(), Route { method, url: declared.url, query, headers: declared.header_fields.unwrap_or_default(), body_field: declared.body_field }));
                    accepted.push(tool);
                }
                if accepted.is_empty() {
                    bail!("manual {name}: none of {} tools is an https call", tools.len());
                }
                let anchor = location.unwrap_or_else(|| "https://invalid.localhost/".to_string());
                let (count, _) = self.register_http(name, &anchor, auth.clone(), accepted, routes, protocols).await?;
                Ok((count, rejected))
            }
            ProviderConfig::Mcp { name, url, command, args, env, auth } => {
                let mut mcp = match (url, command) {
                    (Some(url), _) => McpProvider::new(name.clone(), url.clone(), None),
                    (None, Some(command)) => {
                        let forwarded: HashMap<String, String> = env.iter().filter_map(|key| std::env::var(key).ok().map(|value| (key.clone(), value))).collect();
                        McpProvider::new_stdio(name.clone(), command.clone(), Some(args.clone()), Some(forwarded))
                    }
                    (None, None) => bail!("MCP provider {name} needs a url or a command"),
                };
                protocol_only(&mut mcp.base, "mcp");
                if let Some(secret) = auth {
                    protocols.mcp.set_secret(name, secret.clone());
                }
                let tools = self.utcp.register_tool_provider(Arc::new(mcp) as Arc<dyn Provider>).await?;
                Ok((tools.len(), 0))
            }
        }
    }

    async fn register_http(&self, name: &str, anchor: &str, auth: Option<config::Secret>, tools: Vec<Tool>, routes: Vec<(String, Route)>, protocols: &Protocols) -> Result<(usize, usize)> {
        let mut provider = HttpProvider::new(name.to_string(), anchor.to_string(), "GET".to_string(), None);
        protocol_only(&mut provider.base, "http");
        if let Some(secret) = auth {
            protocols.http.set_secret(name, secret);
        }
        for (tool, route) in routes {
            protocols.http.add_route(&format!("{name}.{tool}"), route);
        }
        let tagged = tools.into_iter().map(|tool| with_name_tags(tool, name)).collect();
        let registered = self.utcp.register_tool_provider_with_tools(Arc::new(provider) as Arc<dyn Provider>, tagged).await?;
        Ok((registered.len(), 0))
    }

    pub async fn all_tools(&self) -> Result<Vec<Tool>> {
        let mut tools = self.repo.get_tools().await?;
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(tools)
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<Tool>> {
        self.utcp.search_tools(query, limit).await
    }

    pub async fn call(&self, name: &str, arguments: Map<String, Value>) -> Result<Value> {
        self.utcp.call_tool(name, arguments.into_iter().collect()).await
    }

    pub async fn codemode(&self, code: String, timeout: Option<u64>) -> Result<CodeModeResult> {
        self.codemode.execute(CodeModeArgs { code, timeout }).await
    }
}
