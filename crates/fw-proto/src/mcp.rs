//! MCP server engine shared by `fw mcp` (stdio) and `fw serve` (`POST /mcp`, Streamable HTTP).
//!
//! The current specification is 2026-07-28: stateless requests carrying their version and client
//! capabilities in `_meta`, `server/discover`, `resultType`, cache hints, multi round-trip requests and
//! `subscriptions/listen`. A request that opens with `initialize` is served with the earlier,
//! session-scoped behaviour (2025-11-25, 2025-06-18, 2025-03-26, 2024-11-05), so clients that have not
//! moved yet keep working. Tools, resources and prompts come from a [`Catalog`]; the engine owns the
//! protocol: validation, error codes, pagination, caching hints, progress, cancellation and streams.

use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

pub const MODERN_VERSION: &str = "2026-07-28";
pub const LEGACY_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const META_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
const META_CAPABILITIES: &str = "io.modelcontextprotocol/clientCapabilities";
const META_LOG_LEVEL: &str = "io.modelcontextprotocol/logLevel";
const META_SERVER_INFO: &str = "io.modelcontextprotocol/serverInfo";
const META_SUBSCRIPTION: &str = "io.modelcontextprotocol/subscriptionId";

/// Largest number of items in one `*/list` page.
const PAGE_SIZE: usize = 200;
const CURSOR_PREFIX: &str = "fwmcp:";
const MAX_SCHEMA_DEPTH: usize = 32;

/// Every protocol version the server answers, newest first.
pub fn supported_versions() -> Vec<&'static str> {
    std::iter::once(MODERN_VERSION).chain(LEGACY_VERSIONS.iter().copied()).collect()
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

// ---- errors and outcomes ------------------------------------------------------------------------------------

/// A JSON-RPC error plus the HTTP status the Streamable HTTP binding answers it with.
#[derive(Debug, Clone)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
    pub status: u16,
}

impl RpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), data: None, status: 200 }
    }

    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }

    pub fn with_status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }

    /// `-32602`: the request is well formed JSON-RPC but its parameters are not acceptable.
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(-32602, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(-32603, message)
    }

    pub fn method_not_found(method: &str) -> Self {
        Self::new(-32601, format!("Method not found: {method}")).with_status(404)
    }

    pub fn header_mismatch(message: impl Into<String>) -> Self {
        Self::new(-32020, message).with_status(400)
    }

    /// `-32021`: `required` is a `ClientCapabilities` object keyed by the missing capability.
    pub fn missing_capability(required: Value) -> Self {
        Self::new(-32021, "Server requires a client capability this request did not declare")
            .with_data(json!({ "requiredCapabilities": required }))
            .with_status(400)
    }

    pub fn unsupported_version(requested: &str) -> Self {
        Self::new(-32022, "Unsupported protocol version")
            .with_data(json!({ "supported": supported_versions(), "requested": requested }))
            .with_status(400)
    }

    fn to_response(&self, id: &Value) -> Value {
        let mut error = json!({ "code": self.code, "message": self.message });
        if let Some(data) = &self.data {
            error["data"] = data.clone();
        }
        json!({ "jsonrpc": "2.0", "id": id, "error": error })
    }
}

/// What a tool call, resource read or prompt lookup produced.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// The method's result object (`CallToolResult`, `ReadResourceResult`, `GetPromptResult`).
    Complete(Value),
    /// Multi round-trip request: the client retries with `inputResponses` and the echoed `requestState`.
    InputRequired { requests: Option<Value>, state: Option<String> },
    Fail(RpcError),
}

impl Outcome {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Complete(json!({ "content": [{ "type": "text", "text": text.into() }] }))
    }

    pub fn tool_error(text: impl Into<String>) -> Self {
        Self::Complete(json!({ "content": [{ "type": "text", "text": text.into() }], "isError": true }))
    }
}

// ---- cancellation, subscriptions, per-call context ----------------------------------------------------------

/// Set when the client abandons a request: `notifications/cancelled` on stdio, a closed response stream on HTTP.
#[derive(Clone, Default, Debug)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    /// Sleeps up to `duration` in short steps and returns true as soon as the request is cancelled.
    pub fn sleep(&self, duration: Duration) -> bool {
        let step = Duration::from_millis(5);
        let mut left = duration;
        while !left.is_zero() {
            if self.is_cancelled() {
                return true;
            }
            let slice = left.min(step);
            std::thread::sleep(slice);
            left -= slice;
        }
        self.is_cancelled()
    }
}

#[derive(Debug, Clone)]
pub enum ListEvent {
    Tools,
    Prompts,
    Resources,
    ResourceUpdated(String),
}

/// Fans list-change and resource-update events out to the open `subscriptions/listen` streams.
#[derive(Default)]
pub struct Bus {
    subscribers: Mutex<Vec<Sender<ListEvent>>>,
}

impl Bus {
    pub fn subscribe(&self) -> Receiver<ListEvent> {
        let (sender, receiver) = mpsc::channel();
        lock(&self.subscribers).push(sender);
        receiver
    }

    pub fn publish(&self, event: ListEvent) {
        lock(&self.subscribers).retain(|sender| sender.send(event.clone()).is_ok());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Debug,
    Info,
    Notice,
    Warning,
    Error,
    Critical,
    Alert,
    Emergency,
}

impl LogLevel {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "debug" => Self::Debug,
            "info" => Self::Info,
            "notice" => Self::Notice,
            "warning" => Self::Warning,
            "error" => Self::Error,
            "critical" => Self::Critical,
            "alert" => Self::Alert,
            "emergency" => Self::Emergency,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Notice => "notice",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Critical => "critical",
            Self::Alert => "alert",
            Self::Emergency => "emergency",
        }
    }
}

/// Everything one request carries into a tool, resource or prompt handler.
pub struct CallContext<'a> {
    /// True for requests served under 2026-07-28 semantics, false for the session-scoped earlier versions.
    pub modern: bool,
    /// The JSON-RPC id of the request being served.
    pub request_id: &'a Value,
    pub client_capabilities: &'a Value,
    pub log_level: Option<LogLevel>,
    pub progress_token: Option<&'a Value>,
    /// `inputResponses` of a multi round-trip retry.
    pub input_responses: Option<&'a Value>,
    /// The opaque `requestState` of a multi round-trip retry, exactly as the server issued it.
    pub request_state: Option<&'a str>,
    pub bus: &'a Bus,
    pub cancel: &'a Cancel,
    sink: &'a dyn Fn(Value),
}

impl CallContext<'_> {
    pub fn notify(&self, method: &str, params: Value) {
        (self.sink)(json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    /// `notifications/progress`, sent only when the request carried a `progressToken`.
    pub fn progress(&self, progress: f64, total: Option<f64>, message: Option<&str>) {
        let Some(token) = self.progress_token else { return };
        let mut params = json!({ "progressToken": token, "progress": progress });
        if let Some(total) = total {
            params["total"] = json!(total);
        }
        if let Some(message) = message {
            params["message"] = json!(message);
        }
        self.notify("notifications/progress", params);
    }

    /// `notifications/message`, sent only when the request asked for logs at or below `level`.
    pub fn log(&self, level: LogLevel, logger: &str, data: Value) {
        if self.log_level.is_some_and(|minimum| level >= minimum) {
            self.notify("notifications/message", json!({ "level": level.name(), "logger": logger, "data": data }));
        }
    }

    pub fn has_capability(&self, name: &str) -> bool {
        self.client_capabilities.get(name).is_some()
    }
}

// ---- the catalog a server plugs into the engine -------------------------------------------------------------

pub struct Identity {
    pub name: String,
    pub title: Option<String>,
    pub version: String,
    pub instructions: Option<String>,
}

pub trait Catalog: Send + Sync {
    fn identity(&self) -> Identity;
    /// Server capabilities; a feature is served exactly when its key is present.
    fn capabilities(&self) -> Value;
    /// Full `Tool` objects in a deterministic order.
    fn tools(&self) -> Vec<Value>;
    fn call_tool(&self, name: &str, arguments: &Value, cx: &CallContext<'_>) -> Outcome;

    fn resources(&self) -> Vec<Value> {
        Vec::new()
    }
    fn resource_templates(&self) -> Vec<Value> {
        Vec::new()
    }
    fn read_resource(&self, uri: &str, _cx: &CallContext<'_>) -> Outcome {
        Outcome::Fail(RpcError::invalid_params(format!("Resource not found: {uri}")).with_data(json!({ "uri": uri })))
    }
    fn prompts(&self) -> Vec<Value> {
        Vec::new()
    }
    fn get_prompt(&self, name: &str, _arguments: &Value, _cx: &CallContext<'_>) -> Outcome {
        Outcome::Fail(RpcError::invalid_params(format!("Unknown prompt: {name}")))
    }
    fn complete(&self, _params: &Value, _cx: &CallContext<'_>) -> Outcome {
        Outcome::Fail(RpcError::method_not_found("completion/complete"))
    }
    /// `(ttlMs, cacheScope)` for a cacheable method.
    fn cache(&self, method: &str) -> (u64, &'static str) {
        (300_000, if method == "resources/read" { "private" } else { "public" })
    }
}

// ---- connection state ---------------------------------------------------------------------------------------

struct Legacy {
    capabilities: Value,
    log_level: Option<LogLevel>,
}

/// State scoped to one stdio process. An HTTP request gets a fresh, empty connection: nothing carries over.
#[derive(Default)]
pub struct Connection {
    legacy: Mutex<Option<Legacy>>,
    in_flight: Mutex<HashMap<String, Cancel>>,
}

impl Connection {
    /// Registers a request before it runs, so a `notifications/cancelled` that follows it always finds it.
    pub fn begin(&self, id: &Value) -> Cancel {
        let cancel = Cancel::default();
        lock(&self.in_flight).insert(id.to_string(), cancel.clone());
        cancel
    }

    pub fn end(&self, id: &Value) {
        lock(&self.in_flight).remove(&id.to_string());
    }

    pub fn cancel(&self, id: &Value) {
        if let Some(cancel) = lock(&self.in_flight).get(&id.to_string()) {
            cancel.cancel();
        }
    }

    pub fn cancel_all(&self) {
        for cancel in lock(&self.in_flight).values() {
            cancel.cancel();
        }
    }
}

/// The request metadata the Streamable HTTP binding mirrors into headers.
#[derive(Debug, Default, Clone)]
pub struct HttpMeta {
    pub protocol_version: Option<String>,
    pub method: Option<String>,
    pub name: Option<String>,
    /// `Mcp-Param-*` headers, lower-case name without the prefix.
    pub params: Vec<(String, String)>,
}

impl HttpMeta {
    pub fn from_headers<'a>(headers: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let mut meta = Self::default();
        for (name, value) in headers {
            let name = name.to_ascii_lowercase();
            match name.as_str() {
                "mcp-protocol-version" => meta.protocol_version = Some(value.to_owned()),
                "mcp-method" => meta.method = Some(value.to_owned()),
                "mcp-name" => meta.name = Some(value.to_owned()),
                other => {
                    if let Some(param) = other.strip_prefix("mcp-param-") {
                        meta.params.push((param.to_owned(), value.to_owned()));
                    }
                }
            }
        }
        meta
    }
}

/// A JSON-RPC response and the HTTP status it travels with; `body` is `None` for a notification or a
/// cancelled request, to which nothing further may be sent.
#[derive(Debug, Clone)]
pub struct Reply {
    pub status: u16,
    pub body: Option<Value>,
}

impl Reply {
    fn error(id: &Value, error: &RpcError) -> Self {
        Self { status: error.status, body: Some(error.to_response(id)) }
    }
}

enum Era {
    Modern,
    Legacy,
}

struct Incoming<'a> {
    id: &'a Value,
    method: &'a str,
    params: &'a Value,
    http: Option<&'a HttpMeta>,
}

/// Request state both eras hand to the shared method handlers.
struct Call<'a> {
    incoming: &'a Incoming<'a>,
    modern: bool,
    capabilities: Value,
    log_level: Option<LogLevel>,
}

struct ModernRequest {
    capabilities: Value,
    log_level: Option<LogLevel>,
}

pub struct Engine {
    catalog: Arc<dyn Catalog>,
    bus: Arc<Bus>,
}

impl Engine {
    pub fn new(catalog: Arc<dyn Catalog>) -> Self {
        Self { catalog, bus: Arc::new(Bus::default()) }
    }

    pub fn bus(&self) -> &Arc<Bus> {
        &self.bus
    }

    pub fn catalog(&self) -> &Arc<dyn Catalog> {
        &self.catalog
    }

    /// Serves one JSON-RPC message. `sink` receives the request-scoped notifications (progress, logs,
    /// subscription events) while the request runs; the returned reply is the final response.
    pub fn handle(&self, conn: &Connection, message: &Value, http: Option<&HttpMeta>, sink: &dyn Fn(Value), cancel: &Cancel) -> Reply {
        let Some(object) = message.as_object() else {
            return Reply::error(&Value::Null, &RpcError::new(-32600, "Invalid Request: a message is one JSON object").with_status(400));
        };
        let method = object.get("method").and_then(Value::as_str);
        let id = object.get("id");
        if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") || method.is_none() {
            let id = id.filter(|id| valid_id(id)).unwrap_or(&Value::Null);
            return Reply::error(id, &RpcError::new(-32600, "Invalid Request: expected a JSON-RPC 2.0 request or notification").with_status(400));
        }
        let method = method.unwrap_or_default();
        let params = object.get("params").unwrap_or(&Value::Null);

        let Some(id) = id else {
            self.notification(conn, method, params);
            return Reply { status: 202, body: None };
        };
        if !valid_id(id) {
            return Reply::error(&Value::Null, &RpcError::new(-32600, "Invalid Request: id is a string or an integer").with_status(400));
        }

        let incoming = Incoming { id, method, params, http };
        let reply = match self.route(conn, &incoming, sink, cancel) {
            Ok(result) => Reply { status: 200, body: Some(json!({ "jsonrpc": "2.0", "id": id, "result": result })) },
            Err(error) => Reply::error(id, &error),
        };
        if cancel.is_cancelled() {
            return Reply { status: reply.status, body: None };
        }
        reply
    }

    fn notification(&self, conn: &Connection, method: &str, params: &Value) {
        if method == "notifications/cancelled"
            && let Some(request_id) = params.get("requestId")
        {
            conn.cancel(request_id);
        }
    }

    fn era(&self, conn: &Connection, incoming: &Incoming<'_>) -> Era {
        if incoming.params.get("_meta").and_then(|meta| meta.get(META_VERSION)).is_some() {
            return Era::Modern;
        }
        match incoming.http {
            Some(meta) => match meta.protocol_version.as_deref() {
                Some(version) if LEGACY_VERSIONS.contains(&version) => Era::Legacy,
                Some(_) => Era::Modern,
                None => Era::Legacy,
            },
            None if incoming.method == "initialize" || lock(&conn.legacy).is_some() => Era::Legacy,
            None => Era::Modern,
        }
    }

    fn route(&self, conn: &Connection, incoming: &Incoming<'_>, sink: &dyn Fn(Value), cancel: &Cancel) -> Result<Value, RpcError> {
        match self.era(conn, incoming) {
            Era::Modern => {
                let request = self.validate_modern(incoming)?;
                let call = Call { incoming, modern: true, capabilities: request.capabilities, log_level: request.log_level };
                self.serve_method(&call, sink, cancel)
            }
            Era::Legacy => self.route_legacy(conn, incoming, sink, cancel),
        }
    }

    fn route_legacy(&self, conn: &Connection, incoming: &Incoming<'_>, sink: &dyn Fn(Value), cancel: &Cancel) -> Result<Value, RpcError> {
        let method = incoming.method;
        let not_found = || RpcError::new(-32601, format!("Method not found: {method}"));
        match method {
            "initialize" => self.initialize(conn, incoming.params),
            "ping" => Ok(json!({})),
            "resources/subscribe" | "resources/unsubscribe" => {
                let subscribable = self.catalog.capabilities().pointer("/resources/subscribe").and_then(Value::as_bool) == Some(true);
                if !subscribable {
                    return Err(not_found());
                }
                if incoming.params.get("uri").and_then(Value::as_str).is_none() {
                    return Err(RpcError::invalid_params(format!("Invalid params for {method}: 'uri' must be a string")));
                }
                Ok(json!({}))
            }
            "logging/setLevel" => {
                if self.catalog.capabilities().get("logging").is_none() {
                    return Err(not_found());
                }
                let name = incoming.params.get("level").and_then(Value::as_str).unwrap_or_default();
                let level = LogLevel::parse(name).ok_or_else(|| RpcError::invalid_params(format!("Unknown log level: {name}")))?;
                if let Some(legacy) = lock(&conn.legacy).as_mut() {
                    legacy.log_level = Some(level);
                }
                Ok(json!({}))
            }
            "tools/list" | "tools/call" | "resources/list" | "resources/templates/list" | "resources/read" | "prompts/list" | "prompts/get" | "completion/complete" => {
                let (capabilities, log_level) = match lock(&conn.legacy).as_ref() {
                    Some(legacy) => (legacy.capabilities.clone(), legacy.log_level),
                    None => (json!({}), None),
                };
                let call = Call { incoming, modern: false, capabilities, log_level: log_level.or(Some(LogLevel::Info)) };
                self.serve_method(&call, sink, cancel).map_err(|error| if error.status == 404 { error.with_status(200) } else { error })
            }
            _ => Err(not_found()),
        }
    }

    fn initialize(&self, conn: &Connection, params: &Value) -> Result<Value, RpcError> {
        let requested = params
            .get("protocolVersion")
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::invalid_params("Invalid params for initialize: protocolVersion is required"))?;
        let version = LEGACY_VERSIONS.iter().find(|known| **known == requested).copied().unwrap_or(LEGACY_VERSIONS[0]);
        let capabilities = params.get("capabilities").filter(|value| value.is_object()).cloned().unwrap_or_else(|| json!({}));
        *lock(&conn.legacy) = Some(Legacy { capabilities, log_level: None });
        let identity = self.catalog.identity();
        let mut result = json!({
            "protocolVersion": version,
            "capabilities": self.catalog.capabilities(),
            "serverInfo": identity_json(&identity),
        });
        if let Some(instructions) = &identity.instructions {
            result["instructions"] = json!(instructions);
        }
        Ok(result)
    }

    /// Header and `_meta` validation for a request served under 2026-07-28.
    fn validate_modern(&self, incoming: &Incoming<'_>) -> Result<ModernRequest, RpcError> {
        let (method, params, http) = (incoming.method, incoming.params, incoming.http);
        if let Some(meta) = http
            && meta.protocol_version.is_none()
        {
            return Err(RpcError::header_mismatch("Missing MCP-Protocol-Version header"));
        }
        let invalid = |message: &str| RpcError::invalid_params(format!("Invalid params: {message}")).with_status(400);
        let meta = params.get("_meta").and_then(Value::as_object).ok_or_else(|| invalid("_meta is required"))?;
        let version = meta.get(META_VERSION).and_then(Value::as_str).ok_or_else(|| invalid("_meta.io.modelcontextprotocol/protocolVersion is required"))?;
        let capabilities = meta.get(META_CAPABILITIES).filter(|value| value.is_object()).ok_or_else(|| invalid("_meta.io.modelcontextprotocol/clientCapabilities is required"))?;
        let log_level = match meta.get(META_LOG_LEVEL) {
            None => None,
            Some(level) => Some(level.as_str().and_then(LogLevel::parse).ok_or_else(|| invalid("_meta.io.modelcontextprotocol/logLevel is not a log level"))?),
        };
        if let Some(mirrored) = http {
            if mirrored.protocol_version.as_deref() != Some(version) {
                return Err(RpcError::header_mismatch(format!(
                    "Header mismatch: MCP-Protocol-Version header value '{}' does not match body value '{version}'",
                    mirrored.protocol_version.as_deref().unwrap_or_default()
                )));
            }
        }
        if version != MODERN_VERSION {
            return Err(RpcError::unsupported_version(version));
        }
        if let Some(mirrored) = http {
            self.validate_headers(method, params, mirrored)?;
        }
        Ok(ModernRequest { capabilities: capabilities.clone(), log_level })
    }

    fn validate_headers(&self, method: &str, params: &Value, http: &HttpMeta) -> Result<(), RpcError> {
        let mirrored = |what: &str, value: Option<&String>| -> Result<String, RpcError> {
            let raw = value.ok_or_else(|| RpcError::header_mismatch(format!("Missing {what} header")))?;
            decode_header(raw).ok_or_else(|| RpcError::header_mismatch(format!("Header mismatch: {what} header holds characters a header cannot carry")))
        };
        let header_method = mirrored("Mcp-Method", http.method.as_ref())?;
        if header_method != method {
            return Err(RpcError::header_mismatch(format!("Header mismatch: Mcp-Method header value '{header_method}' does not match body value '{method}'")));
        }
        let source = match method {
            "tools/call" | "prompts/get" => Some("name"),
            "resources/read" => Some("uri"),
            _ => None,
        };
        if let Some(field) = source {
            let header_name = mirrored("Mcp-Name", http.name.as_ref())?;
            let body_name = params.get(field).and_then(Value::as_str).unwrap_or_default();
            if header_name != body_name {
                return Err(RpcError::header_mismatch(format!("Header mismatch: Mcp-Name header value '{header_name}' does not match body value '{body_name}'")));
            }
        }
        if method == "tools/call" {
            let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
            if let Some(tool) = self.catalog.tools().iter().find(|tool| tool.get("name").and_then(Value::as_str) == Some(name)) {
                self.validate_param_headers(tool, params.get("arguments"), http)?;
            }
        }
        Ok(())
    }

    /// `x-mcp-header` annotations: each annotated argument that is present must arrive as a matching `Mcp-Param-*` header.
    fn validate_param_headers(&self, tool: &Value, arguments: Option<&Value>, http: &HttpMeta) -> Result<(), RpcError> {
        let Some(schema) = tool.get("inputSchema") else { return Ok(()) };
        let mut annotated = Vec::new();
        collect_header_params(schema, &mut Vec::new(), &mut annotated);
        for (header_name, path) in annotated {
            let value = path.iter().fold(arguments, |current, key| current.and_then(|object| object.get(key.as_str()))).filter(|value| !value.is_null());
            let header = http.params.iter().find(|(name, _)| name.eq_ignore_ascii_case(&header_name)).map(|(_, value)| value);
            match (value, header) {
                (None, None) => {}
                (Some(_), None) => return Err(RpcError::header_mismatch(format!("Missing Mcp-Param-{header_name} header"))),
                (None, Some(_)) => return Err(RpcError::header_mismatch(format!("Header mismatch: Mcp-Param-{header_name} is set but the argument is not"))),
                (Some(value), Some(raw)) => {
                    let decoded = decode_header(raw).ok_or_else(|| RpcError::header_mismatch(format!("Header mismatch: Mcp-Param-{header_name} holds characters a header cannot carry")))?;
                    if !header_matches(value, &decoded) {
                        return Err(RpcError::header_mismatch(format!("Header mismatch: Mcp-Param-{header_name} header value '{decoded}' does not match the argument")));
                    }
                }
            }
        }
        Ok(())
    }

    /// Methods shared by both eras. `modern` selects the 2026-07-28 result shape.
    fn serve_method(&self, call: &Call<'_>, sink: &dyn Fn(Value), cancel: &Cancel) -> Result<Value, RpcError> {
        let (method, params, modern) = (call.incoming.method, call.incoming.params, call.modern);
        let server_capabilities = self.catalog.capabilities();
        let required = match method {
            "tools/list" | "tools/call" => Some("tools"),
            "resources/list" | "resources/templates/list" | "resources/read" => Some("resources"),
            "prompts/list" | "prompts/get" => Some("prompts"),
            "completion/complete" => Some("completions"),
            _ => None,
        };
        if let Some(capability) = required
            && server_capabilities.get(capability).is_none()
        {
            return Err(RpcError::method_not_found(method));
        }
        let meta = params.get("_meta");
        if let Some(responses) = params.get("inputResponses")
            && !responses.as_object().is_some_and(|map| map.values().all(Value::is_object))
        {
            return Err(RpcError::invalid_params("Invalid params: inputResponses is an object mapping each request key to the client's result object"));
        }
        let cx = CallContext {
            modern,
            request_id: call.incoming.id,
            client_capabilities: &call.capabilities,
            log_level: call.log_level,
            progress_token: meta.and_then(|meta| meta.get("progressToken")).filter(|token| token.is_string() || token.is_i64() || token.is_u64()),
            input_responses: params.get("inputResponses").filter(|value| value.is_object()),
            request_state: params.get("requestState").and_then(Value::as_str),
            bus: &self.bus,
            cancel,
            sink,
        };
        let body = match method {
            "server/discover" if modern => self.discover(),
            "tools/list" => paginate(self.catalog.tools(), "tools", params)?,
            "resources/list" => paginate(self.catalog.resources(), "resources", params)?,
            "resources/templates/list" => paginate(self.catalog.resource_templates(), "resourceTemplates", params)?,
            "prompts/list" => paginate(self.catalog.prompts(), "prompts", params)?,
            "tools/call" => self.call_tool(params, &cx)?,
            "resources/read" => {
                let uri = params.get("uri").and_then(Value::as_str).ok_or_else(|| RpcError::invalid_params("Invalid params for resources/read: 'uri' must be a string"))?;
                finish_outcome(self.catalog.read_resource(uri, &cx), modern)?
            }
            "prompts/get" => {
                let name = params.get("name").and_then(Value::as_str).ok_or_else(|| RpcError::invalid_params("Invalid params for prompts/get: 'name' must be a string"))?;
                let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
                if !arguments.is_object() {
                    return Err(RpcError::invalid_params("Invalid params for prompts/get: 'arguments' must be an object"));
                }
                finish_outcome(self.catalog.get_prompt(name, &arguments, &cx), modern)?
            }
            "completion/complete" => finish_outcome(self.catalog.complete(params, &cx), modern)?,
            "subscriptions/listen" if modern => self.listen(params, &server_capabilities, &cx)?,
            _ => return Err(RpcError::method_not_found(method)),
        };
        Ok(if modern { self.stamp(method, body) } else { body })
    }

    fn discover(&self) -> Value {
        let identity = self.catalog.identity();
        let mut result = json!({ "supportedVersions": supported_versions(), "capabilities": self.catalog.capabilities() });
        if let Some(instructions) = identity.instructions {
            result["instructions"] = json!(instructions);
        }
        result
    }

    fn call_tool(&self, params: &Value, cx: &CallContext<'_>) -> Result<Value, RpcError> {
        let name = params.get("name").and_then(Value::as_str).ok_or_else(|| RpcError::invalid_params("Invalid params for tools/call: 'name' must be a string"))?;
        let arguments = match params.get("arguments") {
            None | Some(Value::Null) => json!({}),
            Some(arguments) if arguments.is_object() => arguments.clone(),
            Some(_) => return Err(RpcError::invalid_params("Invalid params for tools/call: 'arguments' must be an object")),
        };
        let tools = self.catalog.tools();
        let tool = tools.iter().find(|tool| tool.get("name").and_then(Value::as_str) == Some(name)).ok_or_else(|| RpcError::invalid_params(format!("Unknown tool: {name}")))?;
        if let Some(schema) = tool.get("inputSchema")
            && let Err(problem) = validate_schema(schema, &arguments, schema, 0)
        {
            return Ok(json!({ "content": [{ "type": "text", "text": format!("Invalid arguments for tool {name}: {problem}") }], "isError": true }));
        }
        finish_tool_outcome(name, self.catalog.call_tool(name, &arguments, cx), cx.modern)
    }

    fn listen(&self, params: &Value, capabilities: &Value, cx: &CallContext<'_>) -> Result<Value, RpcError> {
        let requested = params.get("notifications").and_then(Value::as_object).cloned().unwrap_or_default();
        let enabled = |capability: &str, flag: &str| capabilities.get(capability).and_then(|value| value.get(flag)).and_then(Value::as_bool) == Some(true);
        let mut honored = Map::new();
        let wants = |key: &str| requested.get(key).and_then(Value::as_bool) == Some(true);
        for (key, capability) in [("toolsListChanged", "tools"), ("promptsListChanged", "prompts"), ("resourcesListChanged", "resources")] {
            if wants(key) && enabled(capability, "listChanged") {
                honored.insert(key.to_owned(), json!(true));
            }
        }
        let mut watched: Vec<String> = Vec::new();
        if enabled("resources", "subscribe")
            && let Some(uris) = requested.get("resourceSubscriptions").and_then(Value::as_array)
        {
            watched = uris.iter().filter_map(|uri| uri.as_str().map(str::to_owned)).collect();
            honored.insert("resourceSubscriptions".to_owned(), json!(watched));
        }
        let subscription_id = cx.request_id.clone();
        let receiver = self.bus.subscribe();
        cx.notify("notifications/subscriptions/acknowledged", json!({ "_meta": { META_SUBSCRIPTION: subscription_id }, "notifications": honored }));
        while !cx.cancel.is_cancelled() {
            match receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(event) => {
                    let (method, extra) = match event {
                        ListEvent::Tools if honored.contains_key("toolsListChanged") => ("notifications/tools/list_changed", None),
                        ListEvent::Prompts if honored.contains_key("promptsListChanged") => ("notifications/prompts/list_changed", None),
                        ListEvent::Resources if honored.contains_key("resourcesListChanged") => ("notifications/resources/list_changed", None),
                        ListEvent::ResourceUpdated(uri) if watched.contains(&uri) => ("notifications/resources/updated", Some(uri)),
                        _ => continue,
                    };
                    let mut params = json!({ "_meta": { META_SUBSCRIPTION: subscription_id } });
                    if let Some(uri) = extra {
                        params["uri"] = json!(uri);
                    }
                    cx.notify(method, params);
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        Ok(json!({ "_meta": { META_SUBSCRIPTION: subscription_id } }))
    }

    /// `resultType`, cache hints and the server identity every 2026-07-28 result carries.
    fn stamp(&self, method: &str, mut body: Value) -> Value {
        let Some(result) = body.as_object_mut() else { return body };
        let interim = result.get("resultType").and_then(Value::as_str) == Some("input_required");
        result.entry("resultType").or_insert_with(|| json!("complete"));
        if !interim && matches!(method, "server/discover" | "tools/list" | "prompts/list" | "resources/list" | "resources/templates/list" | "resources/read") {
            let (ttl, scope) = self.catalog.cache(method);
            result.entry("ttlMs").or_insert_with(|| json!(ttl));
            result.entry("cacheScope").or_insert_with(|| json!(scope));
        }
        let identity = self.catalog.identity();
        let meta = result.entry("_meta").or_insert_with(|| json!({}));
        if let Some(meta) = meta.as_object_mut() {
            meta.entry(META_SERVER_INFO).or_insert_with(|| json!({ "name": identity.name, "version": identity.version }));
        }
        body
    }
}

fn identity_json(identity: &Identity) -> Value {
    let mut info = json!({ "name": identity.name, "version": identity.version });
    if let Some(title) = &identity.title {
        info["title"] = json!(title);
    }
    info
}

fn valid_id(id: &Value) -> bool {
    id.is_string() || id.is_i64() || id.is_u64()
}

fn finish_outcome(outcome: Outcome, modern: bool) -> Result<Value, RpcError> {
    match outcome {
        Outcome::Complete(value) => Ok(value),
        Outcome::InputRequired { requests, state } if modern => Ok(input_required(requests, state)),
        Outcome::InputRequired { .. } => Err(RpcError::internal("This request needs input from the client, which protocol versions before 2026-07-28 cannot carry")),
        Outcome::Fail(error) => Err(error),
    }
}

fn finish_tool_outcome(name: &str, outcome: Outcome, modern: bool) -> Result<Value, RpcError> {
    match outcome {
        Outcome::InputRequired { .. } if !modern => Ok(json!({
            "content": [{ "type": "text", "text": format!("Tool {name} needs input from the client; connect with protocol version {MODERN_VERSION} to use it") }],
            "isError": true
        })),
        other => finish_outcome(other, modern),
    }
}

fn input_required(requests: Option<Value>, state: Option<String>) -> Value {
    let mut result = json!({ "resultType": "input_required" });
    if let Some(requests) = requests {
        result["inputRequests"] = requests;
    }
    if let Some(state) = state {
        result["requestState"] = json!(state);
    }
    result
}

fn paginate(items: Vec<Value>, key: &str, params: &Value) -> Result<Value, RpcError> {
    let start = match params.get("cursor") {
        None | Some(Value::Null) => 0,
        Some(cursor) => cursor
            .as_str()
            .and_then(|cursor| cursor.strip_prefix(CURSOR_PREFIX))
            .and_then(|offset| offset.parse::<usize>().ok())
            .filter(|offset| *offset <= items.len())
            .ok_or_else(|| RpcError::invalid_params("Invalid cursor"))?,
    };
    let end = (start + PAGE_SIZE).min(items.len());
    let mut result = json!({ key: items.get(start..end).unwrap_or_default() });
    if end < items.len() {
        result["nextCursor"] = json!(format!("{CURSOR_PREFIX}{end}"));
    }
    Ok(result)
}

// ---- header values ------------------------------------------------------------------------------------------

/// Decodes a header value that may use the `=?base64?...?=` sentinel; `None` when it holds characters a header cannot carry.
fn decode_header(raw: &str) -> Option<String> {
    if let Some(encoded) = raw.strip_prefix("=?base64?").and_then(|rest| rest.strip_suffix("?=")) {
        return String::from_utf8(base64_decode(encoded)?).ok();
    }
    raw.chars().all(|c| c == '\t' || (' '..='~').contains(&c)).then(|| raw.to_owned())
}

/// Standard Base64 with mandatory padding; anything else is rejected.
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    let digits = input.trim_end_matches('=');
    if input.len() % 4 != 0 || input.len() - digits.len() > 2 {
        return None;
    }
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let (mut buffer, mut bits) = (0u32, 0u32);
    for byte in digits.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xFF) as u8);
        }
    }
    Some(out)
}

/// `x-mcp-header` annotations reachable through `properties` keys only, as `(header name, property path)`.
fn collect_header_params(schema: &Value, path: &mut Vec<String>, found: &mut Vec<(String, Vec<String>)>) {
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else { return };
    for (key, property) in properties {
        path.push(key.clone());
        if let Some(name) = property.get("x-mcp-header").and_then(Value::as_str) {
            found.push((name.to_owned(), path.clone()));
        }
        collect_header_params(property, path, found);
        path.pop();
    }
}

fn header_matches(value: &Value, decoded: &str) -> bool {
    match value {
        Value::String(text) => text == decoded,
        Value::Bool(flag) => decoded == if *flag { "true" } else { "false" },
        Value::Number(number) => match (number.as_f64(), decoded.parse::<f64>()) {
            (Some(body), Ok(header)) => body == header,
            _ => false,
        },
        _ => false,
    }
}

// ---- JSON Schema (the subset tool arguments use) ------------------------------------------------------------

fn type_matches(kind: &str, value: &Value) -> bool {
    match kind {
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        "number" => value.is_number(),
        "integer" => value.is_i64() || value.is_u64() || value.as_f64().is_some_and(|number| number.fract() == 0.0),
        _ => true,
    }
}

/// Validates `value` against `schema`; `root` resolves local `$ref`s. Network references are never followed.
pub fn validate_schema(schema: &Value, value: &Value, root: &Value, depth: usize) -> Result<(), String> {
    if depth > MAX_SCHEMA_DEPTH {
        return Err("the schema is nested too deeply".to_owned());
    }
    let Some(schema) = schema.as_object() else { return if schema == &Value::Bool(false) { Err("no value is allowed here".to_owned()) } else { Ok(()) } };
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let target = reference.strip_prefix('#').and_then(|pointer| if pointer.is_empty() { Some(root) } else { root.pointer(pointer) });
        match target {
            Some(target) => validate_schema(target, value, root, depth + 1)?,
            None => return Err(format!("cannot resolve $ref {reference}")),
        }
    }
    if let Some(kind) = schema.get("type") {
        let kinds: Vec<&str> = match kind {
            Value::String(kind) => vec![kind.as_str()],
            Value::Array(kinds) => kinds.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        };
        if !kinds.is_empty() && !kinds.iter().any(|kind| type_matches(kind, value)) {
            return Err(format!("expected {} but got {}", kinds.join(" or "), json_kind(value)));
        }
    }
    if let Some(options) = schema.get("enum").and_then(Value::as_array)
        && !options.contains(value)
    {
        return Err(format!("{value} is not one of the allowed values"));
    }
    if let Some(constant) = schema.get("const")
        && constant != value
    {
        return Err(format!("{value} is not the required value {constant}"));
    }
    if let Some(object) = value.as_object() {
        for name in schema.get("required").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            if !object.contains_key(name) {
                return Err(format!("Missing required property '{name}'"));
            }
        }
        let properties = schema.get("properties").and_then(Value::as_object);
        for (key, member) in object {
            match properties.and_then(|properties| properties.get(key)) {
                Some(property) => validate_schema(property, member, root, depth + 1).map_err(|problem| format!("{key}: {problem}"))?,
                None => match schema.get("additionalProperties") {
                    Some(Value::Bool(false)) => return Err(format!("Unexpected property '{key}'")),
                    Some(extra) if extra.is_object() => validate_schema(extra, member, root, depth + 1).map_err(|problem| format!("{key}: {problem}"))?,
                    _ => {}
                },
            }
        }
    }
    if let Some(items) = value.as_array() {
        if let Some(item_schema) = schema.get("items") {
            for (index, item) in items.iter().enumerate() {
                validate_schema(item_schema, item, root, depth + 1).map_err(|problem| format!("[{index}]: {problem}"))?;
            }
        }
        if schema.get("minItems").and_then(Value::as_u64).is_some_and(|minimum| (items.len() as u64) < minimum) {
            return Err("too few items".to_owned());
        }
        if schema.get("maxItems").and_then(Value::as_u64).is_some_and(|maximum| (items.len() as u64) > maximum) {
            return Err("too many items".to_owned());
        }
    }
    if let Some(text) = value.as_str() {
        let length = text.chars().count() as u64;
        if schema.get("minLength").and_then(Value::as_u64).is_some_and(|minimum| length < minimum) {
            return Err("the text is too short".to_owned());
        }
        if schema.get("maxLength").and_then(Value::as_u64).is_some_and(|maximum| length > maximum) {
            return Err("the text is too long".to_owned());
        }
    }
    if let Some(number) = value.as_f64() {
        if schema.get("minimum").and_then(Value::as_f64).is_some_and(|minimum| number < minimum) {
            return Err(format!("{number} is below the minimum"));
        }
        if schema.get("maximum").and_then(Value::as_f64).is_some_and(|maximum| number > maximum) {
            return Err(format!("{number} is above the maximum"));
        }
    }
    for keyword in ["allOf"] {
        for part in schema.get(keyword).and_then(Value::as_array).into_iter().flatten() {
            validate_schema(part, value, root, depth + 1)?;
        }
    }
    if let Some(options) = schema.get("anyOf").and_then(Value::as_array)
        && !options.iter().any(|option| validate_schema(option, value, root, depth + 1).is_ok())
    {
        return Err("matches none of the anyOf alternatives".to_owned());
    }
    if let Some(options) = schema.get("oneOf").and_then(Value::as_array)
        && options.iter().filter(|option| validate_schema(option, value, root, depth + 1).is_ok()).count() != 1
    {
        return Err("does not match exactly one of the oneOf alternatives".to_owned());
    }
    if let Some(condition) = schema.get("if") {
        let branch = if validate_schema(condition, value, root, depth + 1).is_ok() { "then" } else { "else" };
        if let Some(branch) = schema.get(branch) {
            validate_schema(branch, value, root, depth + 1)?;
        }
    }
    Ok(())
}

fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// One message in, one answer out, with the request-scoped notifications dropped: the in-process path
/// that tests and non-streaming callers use. A `subscriptions/listen` ends at once, since nothing can read its stream.
pub fn handle_json(engine: &Engine, http: &HttpMeta, body: &str) -> (u16, String) {
    let message: Value = match serde_json::from_str(body) {
        Ok(message) => message,
        Err(error) => return (400, RpcError::new(-32700, format!("Parse error: {error}")).to_response(&Value::Null).to_string()),
    };
    let cancel = Cancel::default();
    if message.get("method").and_then(Value::as_str) == Some("subscriptions/listen") {
        cancel.cancel();
    }
    let reply = engine.handle(&Connection::default(), &message, Some(http), &|_| {}, &cancel);
    (reply.status, reply.body.map(|body| body.to_string()).unwrap_or_default())
}

// ---- stdio ---------------------------------------------------------------------------------------------------

fn write_line<W: Write>(output: &Mutex<W>, message: &Value) {
    let mut writer = lock(output);
    if let Err(error) = writeln!(writer, "{message}").and_then(|()| writer.flush()) {
        eprintln!("fw mcp: cannot write to stdout: {error}");
    }
}

/// Serves newline-delimited JSON-RPC until `input` ends. Requests run on their own threads so a
/// `notifications/cancelled` that arrives while one runs reaches it; on end of input every request in
/// flight is cancelled and awaited.
pub fn serve_stdio<R: BufRead, W: Write + Send + 'static>(engine: &Arc<Engine>, input: R, output: W) -> std::io::Result<()> {
    let output = Arc::new(Mutex::new(output));
    let connection = Arc::new(Connection::default());
    let mut workers: Vec<std::thread::JoinHandle<()>> = Vec::new();
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let message: Value = match serde_json::from_str(&line) {
            Ok(message) => message,
            Err(error) => {
                write_line(&output, &RpcError::new(-32700, format!("Parse error: {error}")).to_response(&Value::Null));
                continue;
            }
        };
        let id = message.get("id").filter(|id| valid_id(id)).cloned();
        let Some(id) = id else {
            let sink = |_: Value| {};
            let reply = engine.handle(&connection, &message, None, &sink, &Cancel::default());
            if let Some(body) = reply.body {
                write_line(&output, &body);
            }
            continue;
        };
        let cancel = connection.begin(&id);
        let (engine, connection, output) = (engine.clone(), connection.clone(), output.clone());
        workers.retain(|worker| !worker.is_finished());
        workers.push(std::thread::spawn(move || {
            let sink = {
                let output = output.clone();
                move |notification: Value| write_line(&output, &notification)
            };
            let reply = engine.handle(&connection, &message, None, &sink, &cancel);
            connection.end(&id);
            if let Some(body) = reply.body {
                write_line(&output, &body);
            }
        }));
    }
    connection.cancel_all();
    for worker in workers {
        if worker.join().is_err() {
            eprintln!("fw mcp: a request handler panicked");
        }
    }
    Ok(())
}
