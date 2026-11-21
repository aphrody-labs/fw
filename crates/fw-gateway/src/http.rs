//! Direct HTTP calls for UTCP tools, one route per OpenAPI operation, plus the protocols fw refuses to run.
//!
//! `UtcpClient` dispatches a call to the provider a manual was registered with, so one provider would call one
//! URL. An OpenAPI document has hundreds of operations; this protocol keeps the route of every tool instead
//! (method, URL template, which arguments are query, header or body) and fills path parameters from the call.

use crate::config::Secret;
use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use rs_utcp::openapi::OpenApiConverter;
use rs_utcp::providers::base::Provider;
use rs_utcp::providers::http::HttpProvider;
use rs_utcp::tools::Tool;
use rs_utcp::transports::ClientTransport;
use rs_utcp::transports::stream::{StreamResult, VecStreamResult};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::sync::RwLock;
use std::time::Duration;

const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;
const PATH_SEGMENT: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

#[derive(Debug, Clone)]
pub struct Route {
    pub method: String,
    pub url: String,
    pub query: HashSet<String>,
    pub headers: Vec<String>,
    pub body_field: Option<String>,
}

pub struct HttpProtocol {
    client: reqwest::Client,
    routes: RwLock<HashMap<String, Route>>,
    secrets: RwLock<HashMap<String, Secret>>,
}

fn read<T>(lock: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn write<T>(lock: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Plain HTTP is accepted for loopback only; every other URL must be https.
pub fn check_url(raw: &str) -> Result<url::Url> {
    let parsed = url::Url::parse(raw).with_context(|| format!("invalid URL {raw:?}"))?;
    let loopback = match parsed.host() {
        Some(url::Host::Domain(domain)) => domain == "localhost" || domain.ends_with(".localhost"),
        Some(url::Host::Ipv4(address)) => address.is_loopback(),
        Some(url::Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    };
    match parsed.scheme() {
        "https" => Ok(parsed),
        "http" if loopback => Ok(parsed),
        scheme => bail!("{raw}: scheme {scheme} is refused, tool calls use https (http only on loopback)"),
    }
}

/// The header a secret goes in and its value, read from the host environment now.
pub fn secret_header(secret: &Secret) -> Result<(String, String)> {
    let value = std::env::var(&secret.env).map_err(|_| anyhow!("environment variable {} is not set on the gateway host", secret.env))?;
    let header = secret.header.clone().unwrap_or_else(|| "Authorization".to_string());
    let scheme = secret.scheme.clone().unwrap_or_else(|| "Bearer".to_string());
    let value = if scheme.is_empty() { value } else { format!("{scheme} {value}") };
    Ok((header, value))
}

impl HttpProtocol {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_default();
        Self { client, routes: RwLock::default(), secrets: RwLock::default() }
    }

    pub fn set_secret(&self, provider: &str, secret: Secret) {
        write(&self.secrets).insert(provider.to_string(), secret);
    }

    pub fn add_route(&self, tool: &str, route: Route) {
        write(&self.routes).insert(tool.to_string(), route);
    }

    pub fn route_count(&self) -> usize {
        read(&self.routes).len()
    }

    fn build(&self, tool: &str, args: &HashMap<String, Value>) -> Result<(reqwest::RequestBuilder, Route)> {
        let route = read(&self.routes).get(tool).cloned().ok_or_else(|| anyhow!("no HTTP route registered for tool {tool}"))?;
        let provider = tool.split_once('.').map_or(tool, |(provider, _)| provider);
        let mut remaining: Map<String, Value> = args.iter().map(|(key, value)| (key.clone(), value.clone())).collect();

        let mut path_url = route.url.clone();
        for key in placeholders(&route.url) {
            let value = remaining.remove(&key).ok_or_else(|| anyhow!("missing path parameter {key}"))?;
            let text = match &value {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            path_url = path_url.replace(&format!("{{{key}}}"), &utf8_percent_encode(&text, PATH_SEGMENT).to_string());
        }
        let mut url = check_url(&path_url)?;

        let mut header_values = Vec::new();
        for name in &route.headers {
            if let Some(value) = remaining.remove(name) {
                header_values.push((name.clone(), value.as_str().map_or_else(|| value.to_string(), str::to_string)));
            }
        }
        let mut query_values = Vec::new();
        for name in &route.query {
            if let Some(value) = remaining.remove(name) {
                query_values.push((name.clone(), value));
            }
        }
        let method = reqwest::Method::from_bytes(route.method.as_bytes())?;
        let has_body = matches!(method.as_str(), "POST" | "PUT" | "PATCH");
        let body = if !has_body {
            for (key, value) in std::mem::take(&mut remaining) {
                query_values.push((key, value));
            }
            None
        } else if let Some(field) = &route.body_field {
            remaining.remove(field)
        } else if remaining.is_empty() {
            None
        } else {
            Some(Value::Object(std::mem::take(&mut remaining)))
        };
        if !query_values.is_empty() {
            let mut pairs = url.query_pairs_mut();
            for (key, value) in &query_values {
                match value {
                    Value::Array(items) => {
                        for item in items {
                            pairs.append_pair(key, item.as_str().map_or_else(|| item.to_string(), str::to_string).as_str());
                        }
                    }
                    other => {
                        pairs.append_pair(key, other.as_str().map_or_else(|| other.to_string(), str::to_string).as_str());
                    }
                }
            }
        }

        let mut request = self.client.request(method, url);
        for (name, value) in header_values {
            request = request.header(name, value);
        }
        if let Some(secret) = read(&self.secrets).get(provider) {
            let (header, value) = secret_header(secret)?;
            request = request.header(header, value);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        Ok((request, route))
    }
}

impl Default for HttpProtocol {
    fn default() -> Self {
        Self::new()
    }
}

fn placeholders(template: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        let Some(end) = rest[start..].find('}') else { break };
        names.push(rest[start + 1..start + end].to_string());
        rest = &rest[start + end + 1..];
    }
    names
}

#[async_trait]
impl ClientTransport for HttpProtocol {
    async fn register_tool_provider(&self, prov: &dyn Provider) -> Result<Vec<Tool>> {
        bail!("provider {} must be registered with the tools of its manual: the gateway does not fetch tool lists itself", prov.name())
    }

    async fn deregister_tool_provider(&self, prov: &dyn Provider) -> Result<()> {
        let prefix = format!("{}.", prov.name());
        write(&self.routes).retain(|name, _| !name.starts_with(&prefix));
        write(&self.secrets).remove(&prov.name());
        Ok(())
    }

    async fn call_tool(&self, tool_name: &str, args: HashMap<String, Value>, _prov: &dyn Provider) -> Result<Value> {
        let (request, _route) = self.build(tool_name, &args)?;
        let response = request.send().await.map_err(|error| anyhow!("request failed: {}", error.without_url()))?;
        let status = response.status();
        let bytes = response.bytes().await.map_err(|error| anyhow!("reading the response failed: {}", error.without_url()))?;
        if bytes.len() > MAX_BODY_BYTES {
            bail!("response of {} bytes exceeds the {MAX_BODY_BYTES} byte limit", bytes.len());
        }
        let body = serde_json::from_slice::<Value>(&bytes).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
        if !status.is_success() {
            let text = body.to_string();
            let cut: String = text.chars().take(2000).collect();
            bail!("HTTP {status}: {cut}");
        }
        Ok(body)
    }

    async fn call_tool_stream(&self, tool_name: &str, args: HashMap<String, Value>, prov: &dyn Provider) -> Result<Box<dyn StreamResult>> {
        let value = self.call_tool(tool_name, args, prov).await?;
        Ok(Box::new(VecStreamResult::new(vec![value], None)))
    }
}

/// Replaces the `cli` and `text` protocols of rs-utcp, which would run local programs or read local files for
/// whatever a manual declares.
pub struct DeniedProtocol(pub &'static str);

#[async_trait]
impl ClientTransport for DeniedProtocol {
    async fn register_tool_provider(&self, _prov: &dyn Provider) -> Result<Vec<Tool>> {
        bail!("the {} protocol is disabled in fw-gateway", self.0)
    }

    async fn deregister_tool_provider(&self, _prov: &dyn Provider) -> Result<()> {
        Ok(())
    }

    async fn call_tool(&self, _tool_name: &str, _args: HashMap<String, Value>, _prov: &dyn Provider) -> Result<Value> {
        bail!("the {} protocol is disabled in fw-gateway", self.0)
    }

    async fn call_tool_stream(&self, _tool_name: &str, _args: HashMap<String, Value>, _prov: &dyn Provider) -> Result<Box<dyn StreamResult>> {
        bail!("the {} protocol is disabled in fw-gateway", self.0)
    }
}

/// Query parameter names per (METHOD, path template), read from the OpenAPI document itself: the converter
/// flattens every parameter into one schema and drops where it goes.
fn query_parameters(spec: &Value) -> HashMap<(String, String), HashSet<String>> {
    let resolve = |parameter: &Value| -> Option<Value> {
        match parameter.get("$ref").and_then(Value::as_str) {
            Some(reference) => spec.pointer(reference.strip_prefix('#')?).cloned(),
            None => Some(parameter.clone()),
        }
    };
    let mut table = HashMap::new();
    let Some(paths) = spec.get("paths").and_then(Value::as_object) else { return table };
    for (path, item) in paths {
        let shared: Vec<Value> = item.get("parameters").and_then(Value::as_array).cloned().unwrap_or_default();
        let Some(item) = item.as_object() else { continue };
        for (method, operation) in item {
            if !matches!(method.as_str(), "get" | "post" | "put" | "delete" | "patch") {
                continue;
            }
            let own = operation.get("parameters").and_then(Value::as_array).cloned().unwrap_or_default();
            let names: HashSet<String> = shared
                .iter()
                .chain(own.iter())
                .filter_map(resolve)
                .filter(|parameter| parameter.get("in").and_then(Value::as_str) == Some("query"))
                .filter_map(|parameter| parameter.get("name").and_then(Value::as_str).map(str::to_string))
                .collect();
            table.insert((method.to_ascii_uppercase(), path.clone()), names);
        }
    }
    table
}

pub struct OpenapiTools {
    pub tools: Vec<Tool>,
    pub routes: Vec<(String, Route)>,
    pub skipped_by_tag: usize,
}

/// References followed in a row before a schema is cut to a bare object.
const REF_DEPTH: usize = 3;

/// `reference` replaced by what it points to, `depth` references deep; a cycle or the depth limit leaves a bare
/// object. rs-utcp inlines references without a limit, which overflows the stack on recursive schemas (the
/// Cloudflare document has them) and grows without bound on the others.
fn dereference(value: &Value, spec: &Value, depth: usize, stack: &mut Vec<String>) -> Value {
    match value {
        Value::Object(map) => {
            if let Some(reference) = map.get("$ref").and_then(Value::as_str) {
                let target = reference.strip_prefix('#').and_then(|pointer| spec.pointer(pointer));
                return match target {
                    Some(target) if depth > 0 && !stack.iter().any(|seen| seen == reference) => {
                        stack.push(reference.to_string());
                        let resolved = dereference(target, spec, depth - 1, stack);
                        stack.pop();
                        resolved
                    }
                    _ => serde_json::json!({ "type": "object" }),
                };
            }
            Value::Object(map.iter().map(|(key, inner)| (key.clone(), dereference(inner, spec, depth, stack))).collect())
        }
        Value::Array(items) => Value::Array(items.iter().map(|item| dereference(item, spec, depth, stack)).collect()),
        other => other.clone(),
    }
}

/// What the converter reads of each operation (parameters and request body) with references resolved under
/// the limit above, the parameters shared by a path copied into its operations, and the responses dropped:
/// a call needs its inputs, and the response schemas are most of the bytes of a large document.
fn prepare_spec(spec: &Value) -> Value {
    let mut prepared = spec.clone();
    let Some(paths) = spec.get("paths").and_then(Value::as_object) else { return prepared };
    let mut resolved_paths = Map::new();
    for (path, item) in paths {
        let Some(item) = item.as_object() else { continue };
        let shared: Vec<Value> = item.get("parameters").and_then(Value::as_array).cloned().unwrap_or_default();
        let mut next = Map::new();
        for (method, operation) in item {
            if !matches!(method.as_str(), "get" | "post" | "put" | "delete" | "patch") {
                continue;
            }
            let Some(operation) = operation.as_object() else { continue };
            let mut parameters = shared.clone();
            parameters.extend(operation.get("parameters").and_then(Value::as_array).cloned().unwrap_or_default());
            let mut rewritten = Map::new();
            for key in ["operationId", "summary", "description", "tags", "security"] {
                if let Some(value) = operation.get(key) {
                    rewritten.insert(key.to_string(), value.clone());
                }
            }
            rewritten.insert("parameters".into(), dereference(&Value::Array(parameters), spec, REF_DEPTH, &mut Vec::new()));
            if let Some(body) = operation.get("requestBody") {
                rewritten.insert("requestBody".into(), dereference(body, spec, REF_DEPTH, &mut Vec::new()));
            }
            next.insert(method.clone(), Value::Object(rewritten));
        }
        resolved_paths.insert(path.clone(), Value::Object(next));
    }
    prepared["paths"] = Value::Object(resolved_paths);
    prepared
}

/// Turn an OpenAPI document into UTCP tools and the routes that call them.
pub fn convert_openapi(spec: Value, spec_location: Option<String>, provider: &str, base_url: Option<&str>, tags: &[String]) -> Result<OpenapiTools> {
    let spec = prepare_spec(&spec);
    let queries = query_parameters(&spec);
    let manual = OpenApiConverter::new(spec, spec_location, Some(provider.to_string())).convert();
    let mut tools = Vec::new();
    let mut routes = Vec::new();
    let mut skipped_by_tag = 0;
    for mut tool in manual.tools {
        if !tags.is_empty() && !tool.tags.iter().any(|tag| tags.contains(tag)) {
            skipped_by_tag += 1;
            continue;
        }
        let Some(value) = tool.provider.take() else { continue };
        let declared: HttpProvider = serde_json::from_value(value).with_context(|| format!("tool {}: provider is not an HTTP provider", tool.name))?;
        let mut url = declared.url.clone();
        let method = declared.http_method.to_ascii_uppercase();
        let query = url
            .match_indices('/')
            .find_map(|(index, _)| queries.get(&(method.clone(), url[index..].to_string())).map(|names| (index, names.clone())));
        if let (Some(base), Some((index, _))) = (base_url, &query) {
            url = format!("{}{}", base.trim_end_matches('/'), &declared.url[*index..]);
        }
        routes.push((
            tool.name.clone(),
            Route { method, url, query: query.map(|(_, names)| names).unwrap_or_default(), headers: declared.header_fields.unwrap_or_default(), body_field: declared.body_field },
        ));
        tools.push(tool);
    }
    Ok(OpenapiTools { tools, routes, skipped_by_tag })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn recursive_schemas_and_shared_parameters_convert() {
        let spec = json!({
            "openapi": "3.0.0",
            "info": { "title": "t", "version": "1" },
            "servers": [{ "url": "https://api.example.com/v1" }],
            "components": { "schemas": { "Node": { "type": "object", "properties": { "child": { "$ref": "#/components/schemas/Node" }, "name": { "type": "string" } } } },
                            "parameters": { "Page": { "name": "page", "in": "query", "schema": { "type": "integer" } } } },
            "paths": { "/nodes/{id}": {
                "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string" } }, { "$ref": "#/components/parameters/Page" }],
                "put": { "operationId": "putNode", "tags": ["nodes"], "requestBody": { "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Node" } } } },
                         "responses": { "200": { "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Node" } } } } } }
            } }
        });
        let converted = convert_openapi(spec, None, "api", None, &[]).expect("converts");
        assert_eq!(converted.tools.len(), 1);
        let properties = converted.tools[0].inputs.properties.clone().unwrap_or_default();
        assert!(properties.contains_key("id") && properties.contains_key("page"), "{properties:?}");
        let (name, route) = &converted.routes[0];
        assert_eq!(name, "putNode");
        assert_eq!(route.url, "https://api.example.com/v1/nodes/{id}");
        assert!(route.query.contains("page"));
    }
}
