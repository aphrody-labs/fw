//! Edge OS: the per-tenant `FwOSAgent` Durable Object (SQLite VFS, Gatekeeper, learn log, exec audit),
//! the Tier-0 POSIX shell and the 3-tier exec router. Port of `FwOSAgent` in `cloudflare/worker.ts`.
//!
//! Everything that is logic lives behind the [`Host`] trait (SQL, R2 blobs, LOADER, SANDBOX, learn sinks) so the same
//! code runs on the Durable Object (`DoHost`, wasm32) and on rusqlite in the native unit tests.

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

/// `package.json` `version` of the TypeScript worker (4.2.0 at the time of the port, checked by the parity test).
pub const FW_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Must equal `compatibility_date` in `wrangler.jsonc`; the Bun parity test enforces it.
pub const COMPATIBILITY_DATE: &str = "2026-11-19";
const R2_SPILL_THRESHOLD_BYTES: usize = 65_536;

const DEFAULT_RULES_JSON: &str = include_str!("../data/gatekeeper_rules.json");

// ---------------------------------------------------------------------------------------------------------------
// Small JS-compatible helpers shared by `os`, `ai` and `proto`.
// ---------------------------------------------------------------------------------------------------------------

/// Milliseconds since the Unix epoch (`Date.now()`).
pub fn now_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::ZERO)
            .as_millis() as f64
    }
}

/// `new Date(ms).toISOString()`.
pub fn iso_from_ms(ms: i64) -> String {
    let days = ms.div_euclid(86_400_000);
    let rem = ms.rem_euclid(86_400_000);
    // Howard Hinnant's civil-from-days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3_600_000,
        rem / 60_000 % 60,
        rem / 1000 % 60,
        rem % 1000
    )
}

pub fn now_iso() -> String {
    iso_from_ms(now_ms() as i64)
}

fn base36(mut n: u64) -> String {
    if n == 0 {
        return "0".into();
    }
    let mut out = Vec::new();
    while n > 0 {
        out.push(b"0123456789abcdefghijklmnopqrstuvwxyz"[(n % 36) as usize]);
        n /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

fn random_uuid() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        let global = js_sys::global();
        let crypto = js_sys::Reflect::get(&global, &"crypto".into()).ok();
        if let Some(crypto) = crypto
            && let Ok(f) = js_sys::Reflect::get(&crypto, &"randomUUID".into())
            && let Ok(f) = f.dyn_into::<js_sys::Function>()
            && let Ok(v) = f.call0(&crypto)
            && let Some(s) = v.as_string()
        {
            return s;
        }
        let r = |n: u32| (0..n).map(|_| format!("{:x}", (js_sys::Math::random() * 16.0) as u32)).collect::<String>();
        format!("{}-{}-{}-{}-{}", r(8), r(4), r(4), r(4), r(12))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0x9e37_79b9_7f4a_7c15);
        let n = COUNTER.fetch_add(0x9e37_79b9_7f4a_7c15, Ordering::Relaxed) ^ (now_ms() as u64).rotate_left(17);
        let h = format!("{:016x}{:016x}", n, n.wrapping_mul(0xbf58_476d_1ce4_e5b9));
        format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
    }
}

/// `${prefix}_${Date.now().toString(36)}_${crypto.randomUUID().slice(0, 12)}`.
pub fn make_id(prefix: &str) -> String {
    let uuid = random_uuid();
    format!("{prefix}_{}_{}", base36(now_ms() as u64), &uuid[..12.min(uuid.len())])
}

pub fn sha256_hex(input: &[u8]) -> String {
    Sha256::digest(input).iter().map(|b| format!("{b:02x}")).collect()
}

/// JavaScript `String(value)` for JSON values.
pub fn js_string(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 1e21 => format!("{}", f as i128),
            _ => n.to_string(),
        },
        Value::String(s) => s.clone(),
        Value::Array(a) => a
            .iter()
            .map(|x| if x.is_null() { String::new() } else { js_string(x) })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".into(),
    }
}

/// JavaScript truthiness for JSON values.
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// A JSON number that prints like JavaScript (integers without a fraction).
pub fn num_value(f: f64) -> Value {
    if f.fract() == 0.0 && f.abs() < 9_007_199_254_740_992.0 {
        Value::from(f as i64)
    } else {
        serde_json::Number::from_f64(f).map_or(Value::Null, Value::Number)
    }
}

/// `Number.parseInt(s, 10)`; `None` is NaN.
pub fn parse_int_js(s: &str) -> Option<i64> {
    let t = s.trim_start();
    let (neg, rest) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let n: i64 = digits.parse().ok()?;
    Some(if neg { -n } else { n })
}

/// A JSON object body, or `{}` when the text is not valid JSON or not an object.
pub fn body_object(text: &str) -> Map<String, Value> {
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(m)) => m,
        _ => Map::new(),
    }
}

/// The TS `json()` helper: 2-space pretty JSON, UTF-8 content type and permissive CORS.
pub fn json_response(data: &Value, status: u16) -> worker::Result<worker::Response> {
    let body = serde_json::to_string_pretty(data).map_err(|e| worker::Error::RustError(e.to_string()))?;
    let mut resp = worker::Response::ok(body)?.with_status(status);
    let headers = resp.headers_mut();
    headers.set("content-type", "application/json; charset=utf-8")?;
    headers.set("access-control-allow-origin", "*")?;
    Ok(resp)
}

// ---------------------------------------------------------------------------------------------------------------
// Paths and the Tier-0 tokenizer.
// ---------------------------------------------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
pub struct NormPath {
    pub path: String,
    pub traversal: bool,
}

/// `normalizeVfsPath(rawPath, cwd)`.
pub fn normalize_path(raw: &str, cwd: &str) -> NormPath {
    let raw = if raw.is_empty() { "/" } else { raw };
    let trimmed = raw.trim().replace('\\', "/");
    let combined = if trimmed.starts_with('/') {
        trimmed
    } else {
        format!("{}/{trimmed}", cwd.trim_end_matches('/'))
    };
    let mut stack: Vec<&str> = Vec::new();
    let mut traversal = false;
    for seg in combined.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                traversal = true;
                stack.pop();
            }
            s => stack.push(s),
        }
    }
    NormPath { path: format!("/{}", stack.join("/")), traversal }
}

/// `parseShellTokens`: whitespace splitting with single and double quotes.
pub fn parse_shell_tokens(command: &str) -> Vec<String> {
    let chars: Vec<char> = command.chars().collect();
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            } else if ch == '\\' && q == '"' && i + 1 < chars.len() {
                i += 1;
                current.push(chars[i]);
            } else {
                current.push(ch);
            }
        } else if ch == '\'' || ch == '"' {
            quote = Some(ch);
        } else if ch.is_whitespace() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
        i += 1;
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn is_js_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// `segment.match(/^(.*?)(>>?)\s*([^\s]+)\s*$/)`: (command, operator, target).
fn split_redirect(segment: &str) -> Option<(String, &'static str, String)> {
    let target_ok = |rest: &str| {
        let t = rest.trim();
        !t.is_empty() && !t.chars().any(char::is_whitespace)
    };
    for (i, c) in segment.char_indices() {
        if c == '>' {
            let after = &segment[i + 1..];
            if let Some(rest) = after.strip_prefix('>')
                && target_ok(rest)
            {
                return Some((segment[..i].to_string(), ">>", rest.trim().to_string()));
            }
            if target_ok(after) {
                return Some((segment[..i].to_string(), ">", after.trim().to_string()));
            }
        }
        if is_js_line_terminator(c) {
            return None;
        }
    }
    None
}

fn split_lines(text: &str) -> Vec<String> {
    let parts: Vec<&str> = text.split('\n').collect();
    let last = parts.len().saturating_sub(1);
    parts
        .iter()
        .enumerate()
        .map(|(i, p)| if i < last { p.strip_suffix('\r').unwrap_or(p).to_string() } else { (*p).to_string() })
        .collect()
}

/// Lines of `text` matching the JavaScript regular expression `pattern`.
fn grep_lines(pattern: &str, text: &str) -> Result<Vec<String>, String> {
    let lines = split_lines(text);
    #[cfg(target_arch = "wasm32")]
    {
        let ctor = js_sys::Reflect::get(&js_sys::global(), &"RegExp".into()).map_err(|_| "RegExp unavailable".to_string())?;
        let args = js_sys::Array::of1(&pattern.into());
        let re = js_sys::Reflect::construct(&ctor.into(), &args).map_err(|_| "invalid regular expression".to_string())?;
        let re: js_sys::RegExp = re.into();
        Ok(lines.into_iter().filter(|l| re.test(l)).collect())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Ok(lines.into_iter().filter(|l| l.contains(pattern)).collect())
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Gatekeeper.
// ---------------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub id: String,
    pub capability: String,
    pub effect: String,
    pub principal: String,
    pub path_prefix: Option<String>,
    pub reason: String,
    pub updated_at: String,
}

impl Rule {
    pub fn to_value(&self) -> Value {
        json!({
            "id": self.id,
            "capability": self.capability,
            "effect": self.effect,
            "principal": self.principal,
            "pathPrefix": self.path_prefix,
            "reason": self.reason,
            "updatedAt": self.updated_at,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    pub allowed: bool,
    pub capability: String,
    pub principal: String,
    pub path: Option<String>,
    pub matched_rule_id: String,
    pub reason: String,
}

impl Decision {
    pub fn effect(&self) -> &'static str {
        if self.allowed { "allow" } else { "deny" }
    }

    pub fn to_value(&self) -> Value {
        let mut m = Map::new();
        m.insert("allowed".into(), self.allowed.into());
        m.insert("capability".into(), self.capability.clone().into());
        m.insert("principal".into(), self.principal.clone().into());
        if let Some(p) = &self.path {
            m.insert("path".into(), p.clone().into());
        }
        m.insert("matchedRuleId".into(), self.matched_rule_id.clone().into());
        m.insert("effect".into(), self.effect().into());
        m.insert("reason".into(), self.reason.clone().into());
        Value::Object(m)
    }
}

/// The 14 seed rules (`defaultGatekeeperRules()` in the TS worker, dumped to `data/gatekeeper_rules.json`).
pub fn default_rules(ts: &str) -> Vec<Rule> {
    let rows: Vec<Value> = serde_json::from_str(DEFAULT_RULES_JSON).unwrap_or_default();
    rows.iter()
        .map(|r| {
            let s = |k: &str| r.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
            Rule {
                id: s("id"),
                capability: s("capability"),
                effect: s("effect"),
                principal: s("principal"),
                path_prefix: r.get("pathPrefix").and_then(Value::as_str).map(String::from),
                reason: s("reason"),
                updated_at: ts.to_string(),
            }
        })
        .collect()
}

/// `gatekeeperCheck`: traversal guard, then first matching deny, then first matching allow, else default deny.
pub fn evaluate(rules: &[Rule], capability: &str, principal: &str, raw_path: Option<&str>) -> Decision {
    let mut normalized: Option<String> = None;
    if let Some(raw) = raw_path {
        let norm = normalize_path(raw, "/");
        if norm.traversal {
            return Decision {
                allowed: false,
                capability: capability.into(),
                principal: principal.into(),
                path: Some(raw.into()),
                matched_rule_id: "builtin-traversal-guard".into(),
                reason: "Path traversal ('..') denied by Gatekeeper".into(),
            };
        }
        normalized = Some(norm.path);
    }
    let matching: Vec<&Rule> = rules
        .iter()
        .filter(|r| {
            let cap_ok = r.capability == "*" || r.capability == capability;
            let principal_ok = r.principal == "*" || r.principal == principal;
            let path_ok = match r.path_prefix.as_deref() {
                None | Some("") => true,
                Some(prefix) => normalized
                    .as_deref()
                    .is_some_and(|np| np == prefix || np.starts_with(&format!("{prefix}/"))),
            };
            cap_ok && principal_ok && path_ok
        })
        .collect();
    let decide = |rule: &Rule, allowed: bool| Decision {
        allowed,
        capability: capability.into(),
        principal: principal.into(),
        path: normalized.clone(),
        matched_rule_id: rule.id.clone(),
        reason: rule.reason.clone(),
    };
    if let Some(r) = matching.iter().find(|r| r.effect == "deny") {
        return decide(r, false);
    }
    if let Some(r) = matching.iter().find(|r| r.effect == "allow") {
        return decide(r, true);
    }
    Decision {
        allowed: false,
        capability: capability.into(),
        principal: principal.into(),
        path: normalized,
        matched_rule_id: "default-deny".into(),
        reason: format!("No Gatekeeper allow rule matched capability '{capability}' for principal '{principal}'"),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Host abstraction.
// ---------------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Null,
    Int(i64),
    Text(String),
}

impl Val {
    fn text(&self) -> String {
        match self {
            Val::Text(s) => s.clone(),
            Val::Int(i) => i.to_string(),
            Val::Null => String::new(),
        }
    }
    fn opt_text(&self) -> Option<String> {
        match self {
            Val::Null => None,
            v => Some(v.text()),
        }
    }
    fn int(&self) -> i64 {
        match self {
            Val::Int(i) => *i,
            Val::Text(s) => s.parse().unwrap_or(0),
            Val::Null => 0,
        }
    }
}

impl From<&str> for Val {
    fn from(s: &str) -> Self {
        Val::Text(s.into())
    }
}
impl From<String> for Val {
    fn from(s: String) -> Self {
        Val::Text(s)
    }
}
impl From<&String> for Val {
    fn from(s: &String) -> Self {
        Val::Text(s.clone())
    }
}
impl From<i64> for Val {
    fn from(i: i64) -> Self {
        Val::Int(i)
    }
}
impl From<Option<&String>> for Val {
    fn from(s: Option<&String>) -> Self {
        s.map_or(Val::Null, |s| Val::Text(s.clone()))
    }
}

/// Which learn sinks are bound (`sinks` in `/api/learn/status`) or were written (`forwarded` in the append reply).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sinks {
    pub k2: bool,
    pub basin: bool,
    pub d1: bool,
}

pub struct LoaderJob {
    pub module_source: String,
    pub allow_network: bool,
    pub env: Value,
    pub input: Value,
}

pub struct LoaderOut {
    pub ok: bool,
    pub text: String,
}

pub struct SandboxJob {
    pub argv: Vec<String>,
    pub cwd: Option<String>,
    pub env: Option<Value>,
    pub timeout_ms: Option<Value>,
}

pub struct SandboxOut {
    pub exit_code: i64,
    pub stdout: String,
    pub stderr: String,
}

/// Everything the Edge OS kernel needs from its runtime.
#[allow(async_fn_in_trait)]
pub trait Host {
    fn sql_exec(&self, query: &str, params: &[Val]) -> Result<(), String>;
    fn sql_query(&self, query: &str, params: &[Val]) -> Result<Vec<Vec<Val>>, String>;
    fn tenant(&self) -> String;
    fn fw_env(&self) -> String;
    fn has_blobs(&self) -> bool;
    async fn blob_get(&self, key: &str) -> Result<Option<String>, String>;
    async fn blob_put(&self, key: &str, content: &str, content_type: &str, sha256: &str, path: &str) -> Result<(), String>;
    /// `None` when the `LOADER` binding is absent.
    async fn run_loader(&self, job: &LoaderJob) -> Option<Result<LoaderOut, String>>;
    /// `None` when the `SANDBOX` binding is absent.
    async fn run_sandbox(&self, job: &SandboxJob) -> Option<Result<SandboxOut, String>>;
    async fn forward_learn(&self, record: &Value) -> Result<Sinks, String>;
    fn sinks(&self) -> Sinks;
}

// ---------------------------------------------------------------------------------------------------------------
// Kernel.
// ---------------------------------------------------------------------------------------------------------------

const DDL: [&str; 4] = [
    "CREATE TABLE IF NOT EXISTS vfs_nodes (
          path TEXT PRIMARY KEY,
          kind TEXT NOT NULL,
          content TEXT NOT NULL,
          content_type TEXT NOT NULL,
          size INTEGER NOT NULL,
          sha256 TEXT NOT NULL,
          blob_key TEXT,
          metadata TEXT NOT NULL,
          updated_at TEXT NOT NULL
        )",
    "CREATE TABLE IF NOT EXISTS gatekeeper_rules (
          id TEXT PRIMARY KEY,
          capability TEXT NOT NULL,
          effect TEXT NOT NULL,
          principal TEXT NOT NULL,
          path_prefix TEXT,
          reason TEXT NOT NULL,
          updated_at TEXT NOT NULL
        )",
    "CREATE TABLE IF NOT EXISTS learn_events (
          id TEXT PRIMARY KEY,
          kind TEXT NOT NULL,
          source TEXT NOT NULL,
          summary TEXT NOT NULL,
          tags TEXT NOT NULL,
          payload TEXT NOT NULL,
          created_at TEXT NOT NULL
        )",
    "CREATE TABLE IF NOT EXISTS exec_audit (
          id TEXT PRIMARY KEY,
          tier INTEGER NOT NULL,
          command TEXT NOT NULL,
          allowed INTEGER NOT NULL,
          exit_code INTEGER NOT NULL,
          created_at TEXT NOT NULL
        )",
];

const NODE_COLS: &str = "path, kind, content, content_type, size, sha256, blob_key, metadata, updated_at";
const INSERT_NODE_IGNORE: &str = "INSERT OR IGNORE INTO vfs_nodes (path, kind, content, content_type, size, sha256, blob_key, metadata, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)";
const INSERT_RULE_IGNORE: &str = "INSERT OR IGNORE INTO gatekeeper_rules (id, capability, effect, principal, path_prefix, reason, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)";

type Res<T> = Result<T, String>;

#[derive(Debug, Clone)]
struct Node {
    path: String,
    kind: String,
    content: String,
    content_type: String,
    size: i64,
    sha256: String,
    blob_key: Option<String>,
    metadata: Value,
    updated_at: String,
}

impl Node {
    fn dir(path: &str, ts: &str) -> Self {
        Node {
            path: path.into(),
            kind: "dir".into(),
            content: String::new(),
            content_type: "inode/directory".into(),
            size: 0,
            sha256: String::new(),
            blob_key: None,
            metadata: json!({}),
            updated_at: ts.into(),
        }
    }

    fn from_row(r: &[Val]) -> Self {
        let metadata_text = r[7].text();
        Node {
            path: r[0].text(),
            kind: r[1].text(),
            content: r[2].text(),
            content_type: r[3].text(),
            size: r[4].int(),
            sha256: r[5].text(),
            blob_key: r[6].opt_text(),
            metadata: serde_json::from_str(if metadata_text.is_empty() { "{}" } else { &metadata_text }).unwrap_or_else(|_| json!({})),
            updated_at: r[8].text(),
        }
    }
}

#[derive(Default)]
pub struct WriteOpts {
    pub content_type: Option<String>,
    pub metadata: Option<Value>,
    pub principal: String,
    pub spill_to_r2: bool,
}

/// The body of `/vfs/exec` and the `vfs_exec` MCP tool.
#[derive(Default)]
pub struct ExecReq {
    pub command: Option<String>,
    pub argv: Option<Vec<String>>,
    pub code: Option<String>,
    pub tier: Option<u8>,
    pub cwd: Option<String>,
    pub env: Map<String, Value>,
    pub principal: String,
    pub allow_network: bool,
    pub timeout_ms: Option<Value>,
    pub input: Option<Value>,
}

impl ExecReq {
    pub fn from_value(body: &Map<String, Value>, principal: &str) -> Self {
        let string = |k: &str| body.get(k).and_then(Value::as_str).map(String::from);
        let tier = body.get("tier").and_then(Value::as_f64).and_then(|f| [0u8, 1, 2].into_iter().find(|t| f64::from(*t) == f));
        ExecReq {
            command: string("command"),
            argv: body.get("argv").and_then(Value::as_array).map(|a| a.iter().map(js_string).collect()),
            code: string("code"),
            tier,
            cwd: string("cwd"),
            env: body.get("env").and_then(Value::as_object).cloned().unwrap_or_default(),
            principal: body.get("principal").and_then(Value::as_str).unwrap_or(principal).to_string(),
            allow_network: body.get("allowNetwork").is_some_and(truthy),
            timeout_ms: body.get("timeoutMs").filter(|v| !v.is_null()).cloned(),
            input: body.get("input").filter(|v| !v.is_null()).cloned(),
        }
    }
}

/// `selectExecTier`.
pub fn select_exec_tier(req: &ExecReq) -> u8 {
    if let Some(t) = req.tier {
        return t;
    }
    if req.code.as_deref().is_some_and(|c| !c.trim().is_empty()) {
        return 1;
    }
    let cmd = req.command.as_deref().unwrap_or("").trim();
    let lower = cmd.to_ascii_lowercase();
    if ["js:", "ts:", "codemode:"].iter().any(|p| lower.starts_with(p)) {
        return 1;
    }
    let first = req
        .argv
        .as_ref()
        .and_then(|a| a.first().cloned())
        .unwrap_or_else(|| cmd.split_whitespace().next().unwrap_or("").to_string());
    const TIER2: [&str; 13] = ["python", "python3", "gcc", "clang", "cargo", "rustc", "go", "git", "docker", "node", "bun", "bash", "/bin/bash"];
    if TIER2.contains(&first.as_str()) || first == "/bin/sh" {
        return 2;
    }
    0
}

fn is_ok(v: &Value) -> bool {
    v.get("ok").and_then(Value::as_bool) == Some(true)
}

fn str_of(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

fn slice_utf16(s: &str, max: usize) -> String {
    let mut used = 0;
    let mut out = String::new();
    for c in s.chars() {
        used += c.len_utf16();
        if used > max {
            break;
        }
        out.push(c);
    }
    out
}

struct Step {
    exit_code: i64,
    stdout: String,
    stderr: String,
    cwd: String,
}

pub struct Kernel<'a, H: Host> {
    pub h: &'a H,
}

impl<'a, H: Host> Kernel<'a, H> {
    pub fn new(h: &'a H) -> Self {
        Kernel { h }
    }

    fn backend(&self) -> &'static str {
        "sqlite"
    }

    /// Schema and seed rows (`ensureInitialized`); idempotent, existing tenant rows are never touched.
    pub fn init(&self) -> Res<()> {
        for ddl in DDL {
            self.h.sql_exec(ddl, &[])?;
        }
        let ts = now_iso();
        let manifest = serde_json::to_string_pretty(&json!({
            "product": "fw-edge",
            "version": FW_VERSION,
            "compatibilityDate": COMPATIBILITY_DATE,
            "tiers": [0, 1, 2],
        }))
        .map_err(|e| e.to_string())?;
        for dir in ["/", "/etc", "/workspace"] {
            self.insert_node_ignore(&Node::dir(dir, &ts))?;
        }
        self.insert_node_ignore(&Node {
            path: "/etc/fw.json".into(),
            kind: "file".into(),
            size: manifest.len() as i64,
            content: manifest,
            content_type: "application/json".into(),
            sha256: "seed".into(),
            blob_key: None,
            metadata: json!({}),
            updated_at: ts.clone(),
        })?;
        for rule in default_rules(&ts) {
            self.h.sql_exec(
                INSERT_RULE_IGNORE,
                &[
                    (&rule.id).into(),
                    (&rule.capability).into(),
                    (&rule.effect).into(),
                    (&rule.principal).into(),
                    rule.path_prefix.as_ref().into(),
                    (&rule.reason).into(),
                    (&rule.updated_at).into(),
                ],
            )?;
        }
        Ok(())
    }

    fn insert_node_ignore(&self, n: &Node) -> Res<()> {
        self.h.sql_exec(
            INSERT_NODE_IGNORE,
            &[
                (&n.path).into(),
                (&n.kind).into(),
                (&n.content).into(),
                (&n.content_type).into(),
                n.size.into(),
                (&n.sha256).into(),
                n.blob_key.as_ref().into(),
                n.metadata.to_string().into(),
                (&n.updated_at).into(),
            ],
        )
    }

    fn read_node(&self, path: &str) -> Res<Option<Node>> {
        let rows = self.h.sql_query(&format!("SELECT {NODE_COLS} FROM vfs_nodes WHERE path = ?"), &[path.into()])?;
        Ok(rows.first().map(|r| Node::from_row(r)))
    }

    fn upsert_node(&self, n: &Node) -> Res<()> {
        self.h.sql_exec(
            "INSERT INTO vfs_nodes (path, kind, content, content_type, size, sha256, blob_key, metadata, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(path) DO UPDATE SET
           kind = excluded.kind,
           content = excluded.content,
           content_type = excluded.content_type,
           size = excluded.size,
           sha256 = excluded.sha256,
           blob_key = excluded.blob_key,
           metadata = excluded.metadata,
           updated_at = excluded.updated_at",
            &[
                (&n.path).into(),
                (&n.kind).into(),
                (&n.content).into(),
                (&n.content_type).into(),
                n.size.into(),
                (&n.sha256).into(),
                n.blob_key.as_ref().into(),
                n.metadata.to_string().into(),
                (&n.updated_at).into(),
            ],
        )
    }

    fn delete_node(&self, path: &str) -> Res<bool> {
        if self.read_node(path)?.is_none() {
            return Ok(false);
        }
        self.h.sql_exec("DELETE FROM vfs_nodes WHERE path = ?", &[path.into()])?;
        Ok(true)
    }

    fn ensure_parent_dirs(&self, path: &str) -> Res<()> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let ts = now_iso();
        let mut acc = String::new();
        for part in parts.iter().take(parts.len().saturating_sub(1)) {
            acc.push('/');
            acc.push_str(part);
            if self.read_node(&acc)?.is_none() {
                self.upsert_node(&Node::dir(&acc, &ts))?;
            }
        }
        Ok(())
    }

    fn list_all_nodes(&self) -> Res<Vec<Node>> {
        let rows = self.h.sql_query(&format!("SELECT {NODE_COLS} FROM vfs_nodes ORDER BY path ASC"), &[])?;
        Ok(rows.iter().map(|r| Node::from_row(r)).collect())
    }

    // ---- Gatekeeper ----

    pub fn rules(&self) -> Res<Vec<Rule>> {
        let rows = self.h.sql_query(
            "SELECT id, capability, effect, principal, path_prefix, reason, updated_at FROM gatekeeper_rules ORDER BY id ASC",
            &[],
        )?;
        Ok(rows
            .iter()
            .map(|r| Rule {
                id: r[0].text(),
                capability: r[1].text(),
                effect: r[2].text(),
                principal: r[3].text(),
                path_prefix: r[4].opt_text(),
                reason: r[5].text(),
                updated_at: r[6].text(),
            })
            .collect())
    }

    pub fn upsert_rule(&self, mut rule: Rule) -> Res<Rule> {
        if rule.id.is_empty() {
            rule.id = make_id("rule");
        }
        if rule.principal.is_empty() {
            rule.principal = "*".into();
        }
        if rule.reason.is_empty() {
            rule.reason = "Custom Gatekeeper rule".into();
        }
        self.h.sql_exec(
            "INSERT INTO gatekeeper_rules (id, capability, effect, principal, path_prefix, reason, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
           capability = excluded.capability,
           effect = excluded.effect,
           principal = excluded.principal,
           path_prefix = excluded.path_prefix,
           reason = excluded.reason,
           updated_at = excluded.updated_at",
            &[
                (&rule.id).into(),
                (&rule.capability).into(),
                (&rule.effect).into(),
                (&rule.principal).into(),
                rule.path_prefix.as_ref().into(),
                (&rule.reason).into(),
                (&rule.updated_at).into(),
            ],
        )?;
        Ok(rule)
    }

    pub fn check(&self, capability: &str, principal: &str, raw_path: Option<&str>) -> Res<Decision> {
        Ok(evaluate(&self.rules()?, capability, principal, raw_path))
    }

    fn denied(decision: &Decision) -> Value {
        json!({ "ok": false, "denied": true, "gatekeeper": decision.to_value() })
    }

    // ---- VFS ----

    pub fn vfs_ls(&self, prefix: &str, principal: &str) -> Res<Value> {
        let decision = self.check("vfs:read", principal, Some(prefix))?;
        if !decision.allowed {
            return Ok(Self::denied(&decision));
        }
        let clean = normalize_path(prefix, "/").path;
        let entries: Vec<Value> = self
            .list_all_nodes()?
            .into_iter()
            .filter(|n| {
                if clean == "/" {
                    n.path != "/"
                } else {
                    n.path == clean || n.path.starts_with(&format!("{clean}/"))
                }
            })
            .map(|n| {
                json!({
                    "path": n.path,
                    "kind": n.kind,
                    "size": n.size,
                    "contentType": n.content_type,
                    "blobKey": n.blob_key,
                    "updatedAt": n.updated_at,
                })
            })
            .collect();
        Ok(json!({
            "ok": true,
            "prefix": clean,
            "storage": self.backend(),
            "count": entries.len(),
            "entries": entries,
        }))
    }

    pub async fn vfs_read(&self, raw_path: &str, principal: &str) -> Res<Value> {
        let decision = self.check("vfs:read", principal, Some(raw_path))?;
        if !decision.allowed {
            return Ok(Self::denied(&decision));
        }
        let path = normalize_path(raw_path, "/").path;
        let Some(node) = self.read_node(&path)? else {
            return Ok(json!({ "ok": false, "error": "ENOENT", "path": path }));
        };
        if node.kind == "dir" {
            return Ok(json!({ "ok": false, "error": "EISDIR", "path": path }));
        }
        let mut content = node.content.clone();
        let mut hydrated_from = self.backend();
        if let Some(key) = &node.blob_key
            && self.h.has_blobs()
            && let Some(text) = self.h.blob_get(key).await?
        {
            content = text;
            hydrated_from = "r2";
        }
        Ok(json!({
            "ok": true,
            "path": node.path,
            "content": content,
            "contentType": node.content_type,
            "size": node.size,
            "sha256": node.sha256,
            "blobKey": node.blob_key,
            "storage": hydrated_from,
            "metadata": node.metadata,
            "updatedAt": node.updated_at,
        }))
    }

    pub async fn vfs_write(&self, raw_path: &str, content: &str, opts: WriteOpts) -> Res<Value> {
        let principal = if opts.principal.is_empty() { "default" } else { &opts.principal };
        let decision = self.check("vfs:write", principal, Some(raw_path))?;
        if !decision.allowed {
            return Ok(Self::denied(&decision));
        }
        let path = normalize_path(raw_path, "/").path;
        if path == "/" {
            return Ok(json!({ "ok": false, "error": "Cannot write file content to root directory '/'" }));
        }
        self.ensure_parent_dirs(&path)?;
        let size = content.len();
        let sha = sha256_hex(content.as_bytes());
        let content_type = opts.content_type.clone().unwrap_or_else(|| {
            if path.ends_with(".json") { "application/json".into() } else { "text/plain; charset=utf-8".into() }
        });
        let ts = now_iso();
        let mut stored = content.to_string();
        let mut blob_key: Option<String> = None;
        let mut storage = self.backend();
        if self.h.has_blobs() && (opts.spill_to_r2 || size > R2_SPILL_THRESHOLD_BYTES) {
            let key = format!("vfs/{}{path}", self.h.tenant());
            self.h.blob_put(&key, content, &content_type, &sha, &path).await?;
            blob_key = Some(key);
            stored = String::new();
            storage = "r2";
        }
        self.upsert_node(&Node {
            path: path.clone(),
            kind: "file".into(),
            content: stored,
            content_type: content_type.clone(),
            size: size as i64,
            sha256: sha.clone(),
            blob_key: blob_key.clone(),
            metadata: opts.metadata.clone().unwrap_or_else(|| json!({})),
            updated_at: ts.clone(),
        })?;
        Ok(json!({
            "ok": true,
            "path": path,
            "size": size,
            "sha256": sha,
            "blobKey": blob_key,
            "contentType": content_type,
            "storage": storage,
            "updatedAt": ts,
        }))
    }

    pub fn vfs_stat(&self, raw_path: &str, principal: &str) -> Res<Value> {
        let decision = self.check("vfs:read", principal, Some(raw_path))?;
        if !decision.allowed {
            return Ok(Self::denied(&decision));
        }
        let path = normalize_path(raw_path, "/").path;
        let Some(node) = self.read_node(&path)? else {
            return Ok(json!({ "ok": true, "exists": false, "path": path }));
        };
        Ok(json!({
            "ok": true,
            "exists": true,
            "path": node.path,
            "kind": node.kind,
            "size": node.size,
            "contentType": node.content_type,
            "sha256": node.sha256,
            "blobKey": node.blob_key,
            "metadata": node.metadata,
            "storage": if node.blob_key.is_some() { "r2" } else { self.backend() },
            "updatedAt": node.updated_at,
        }))
    }

    // ---- exec ----

    fn record_audit(&self, tier: u8, command: &str, allowed: bool, exit_code: i64) -> Res<()> {
        self.h.sql_exec(
            "INSERT INTO exec_audit (id, tier, command, allowed, exit_code, created_at) VALUES (?, ?, ?, ?, ?, ?)",
            &[
                make_id("exec").into(),
                i64::from(tier).into(),
                slice_utf16(command, 512).into(),
                i64::from(allowed).into(),
                exit_code.into(),
                now_iso().into(),
            ],
        )
    }

    async fn run_tier0_single(&self, segment: &str, cwd: &str, principal: &str, env_vars: &Map<String, Value>) -> Res<Step> {
        let (core, redirect) = match split_redirect(segment) {
            Some((core, op, target)) => (core.trim().to_string(), Some((op, target))),
            None => (segment.trim().to_string(), None),
        };
        let tokens = parse_shell_tokens(&core);
        if tokens.is_empty() {
            return Ok(Step { exit_code: 0, stdout: String::new(), stderr: String::new(), cwd: cwd.into() });
        }
        let cmd = tokens[0].as_str();
        let args = &tokens[1..];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut exit_code = 0;
        let mut next_cwd = cwd.to_string();

        match cmd {
            "pwd" => stdout = format!("{cwd}\n"),
            "whoami" => stdout = format!("{principal}\n"),
            "uname" => stdout = format!("FwEdgeOS {FW_VERSION} cloudflare-workers wasm32-v8\n"),
            "echo" => stdout = format!("{}\n", args.join(" ")),
            "env" => {
                let mut merged: Vec<(String, String)> = vec![
                    ("FW_ENV".into(), self.h.fw_env()),
                    ("PWD".into(), cwd.into()),
                    ("USER".into(), principal.into()),
                ];
                for (k, v) in env_vars {
                    let val = js_string(v);
                    match merged.iter_mut().find(|(mk, _)| mk == k) {
                        Some(slot) => slot.1 = val,
                        None => merged.push((k.clone(), val)),
                    }
                }
                stdout = format!("{}\n", merged.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("\n"));
            }
            "cd" => {
                let target = args.first().map_or("/", String::as_str);
                let norm = normalize_path(target, cwd);
                if norm.traversal {
                    exit_code = 1;
                    stderr = "cd: path traversal denied\n".into();
                } else {
                    match self.read_node(&norm.path)? {
                        Some(n) if n.kind == "dir" => next_cwd = norm.path,
                        _ => {
                            exit_code = 1;
                            stderr = format!("cd: {target}: No such directory\n");
                        }
                    }
                }
            }
            "ls" => {
                let target = args.first().map_or(cwd, String::as_str);
                let norm = normalize_path(target, cwd);
                let res = self.vfs_ls(&norm.path, principal)?;
                if is_ok(&res) {
                    stdout = entry_paths(&res);
                } else {
                    exit_code = 126;
                    stderr = "ls: Gatekeeper denied\n".into();
                }
            }
            "cat" => {
                if args.is_empty() {
                    exit_code = 1;
                    stderr = "cat: missing file operand\n".into();
                } else {
                    let mut parts = String::new();
                    for a in args {
                        let r = self.vfs_read(&normalize_path(a, cwd).path, principal).await?;
                        if is_ok(&r) {
                            parts.push_str(&str_of(&r, "content"));
                        } else {
                            exit_code = 1;
                            let why = r.get("error").and_then(Value::as_str).unwrap_or("denied");
                            stderr.push_str(&format!("cat: {a}: {why}\n"));
                        }
                    }
                    stdout = parts;
                }
            }
            "stat" => match args.first() {
                None => {
                    exit_code = 1;
                    stderr = "stat: missing operand\n".into();
                }
                Some(target) => {
                    let s = self.vfs_stat(&normalize_path(target, cwd).path, principal)?;
                    stdout = format!("{s}\n");
                }
            },
            "mkdir" => {
                for a in args.iter().filter(|a| a.as_str() != "-p") {
                    let norm = normalize_path(a, cwd);
                    let dec = self.check("vfs:write", principal, Some(&norm.path))?;
                    if dec.allowed {
                        self.ensure_parent_dirs(&norm.path)?;
                        self.upsert_node(&Node::dir(&norm.path, &now_iso()))?;
                    } else {
                        exit_code = 126;
                        stderr.push_str(&format!("mkdir: {a}: {}\n", dec.reason));
                    }
                }
            }
            "touch" => {
                for a in args {
                    let norm = normalize_path(a, cwd);
                    if self.read_node(&norm.path)?.is_none() {
                        let w = self.vfs_write(&norm.path, "", WriteOpts { principal: principal.into(), ..Default::default() }).await?;
                        if !is_ok(&w) {
                            exit_code = 126;
                            stderr.push_str(&format!("touch: {a}: denied\n"));
                        }
                    }
                }
            }
            "rm" => {
                for a in args.iter().filter(|a| !a.starts_with('-')) {
                    let norm = normalize_path(a, cwd);
                    let dec = self.check("vfs:write", principal, Some(&norm.path))?;
                    if !dec.allowed {
                        exit_code = 126;
                        stderr.push_str(&format!("rm: {a}: {}\n", dec.reason));
                    } else if !self.delete_node(&norm.path)? {
                        exit_code = 1;
                        stderr.push_str(&format!("rm: cannot remove '{a}': No such file\n"));
                    }
                }
            }
            "cp" | "mv" => {
                if args.len() < 2 {
                    exit_code = 1;
                    stderr = format!("{cmd}: missing file operand\n");
                } else {
                    let src = normalize_path(&args[0], cwd);
                    let dst = normalize_path(&args[1], cwd);
                    let r = self.vfs_read(&src.path, principal).await?;
                    if is_ok(&r) {
                        let w = self
                            .vfs_write(&dst.path, &str_of(&r, "content"), WriteOpts { principal: principal.into(), ..Default::default() })
                            .await?;
                        if !is_ok(&w) {
                            exit_code = 126;
                            stderr = format!("{cmd}: {}: denied\n", args[1]);
                        } else if cmd == "mv" {
                            self.delete_node(&src.path)?;
                        }
                    } else {
                        exit_code = 1;
                        let why = r.get("error").and_then(Value::as_str).unwrap_or("failed");
                        stderr = format!("{cmd}: {}: {why}\n", args[0]);
                    }
                }
            }
            "wc" => match args.last() {
                None => {
                    exit_code = 1;
                    stderr = "wc: missing file operand\n".into();
                }
                Some(target) => {
                    let r = self.vfs_read(&normalize_path(target, cwd).path, principal).await?;
                    if is_ok(&r) {
                        let text = str_of(&r, "content");
                        let lines = if text.is_empty() { 0 } else { text.split('\n').count() };
                        let words = text.split_whitespace().count();
                        stdout = format!("{lines} {words} {} {target}\n", text.len());
                    } else {
                        exit_code = 1;
                        stderr = format!("wc: {target}: No such file\n");
                    }
                }
            },
            "head" | "tail" => {
                let mut count: i64 = 10;
                let mut file_arg = args.first();
                if args.first().map(String::as_str) == Some("-n")
                    && let Some(n) = args.get(1)
                {
                    count = parse_int_js(n).filter(|v| *v != 0).unwrap_or(10).max(1);
                    file_arg = args.get(2);
                }
                match file_arg {
                    None => {
                        exit_code = 1;
                        stderr = format!("{cmd}: missing file operand\n");
                    }
                    Some(file) => {
                        let r = self.vfs_read(&normalize_path(file, cwd).path, principal).await?;
                        if is_ok(&r) {
                            let lines = split_lines(&str_of(&r, "content"));
                            let n = count as usize;
                            let sliced: &[String] = if cmd == "head" {
                                &lines[..n.min(lines.len())]
                            } else {
                                &lines[lines.len().saturating_sub(n)..]
                            };
                            stdout = format!("{}\n", sliced.join("\n"));
                        } else {
                            exit_code = 1;
                            stderr = format!("{cmd}: {file}: No such file\n");
                        }
                    }
                }
            }
            "grep" => {
                if args.len() < 2 {
                    exit_code = 1;
                    stderr = "grep: usage: grep <pattern> <path>\n".into();
                } else {
                    let (pattern, file) = (&args[0], &args[1]);
                    let r = self.vfs_read(&normalize_path(file, cwd).path, principal).await?;
                    if is_ok(&r) {
                        match grep_lines(pattern, &str_of(&r, "content")) {
                            Ok(matches) if matches.is_empty() => exit_code = 1,
                            Ok(matches) => stdout = format!("{}\n", matches.join("\n")),
                            Err(e) => {
                                exit_code = 2;
                                stderr = format!("grep: {e}\n");
                            }
                        }
                    } else {
                        exit_code = 1;
                        stderr = format!("grep: {file}: No such file\n");
                    }
                }
            }
            "find" => {
                let prefix = args.first().map_or(cwd, String::as_str);
                let ls = self.vfs_ls(&normalize_path(prefix, cwd).path, principal)?;
                stdout = entry_paths(&ls);
            }
            "help" => {
                stdout = "Fw Tier-0 DO POSIX Shell: pwd ls cat echo stat mkdir touch rm cp mv wc head tail grep find cd env uname whoami help\n".into();
            }
            _ => {
                exit_code = 127;
                stderr = format!("sh: {cmd}: command not found (use tier=1 for CodeMode JS or tier=2 for Sandbox microVM)\n");
            }
        }

        if let Some((op, target)) = redirect
            && exit_code == 0
        {
            let target_norm = normalize_path(&target, &next_cwd);
            let mut final_content = stdout.clone();
            if op == ">>" {
                let prev = self.vfs_read(&target_norm.path, principal).await?;
                if is_ok(&prev) {
                    final_content = format!("{}{stdout}", str_of(&prev, "content"));
                }
            }
            let w = self
                .vfs_write(&target_norm.path, &final_content, WriteOpts { principal: principal.into(), ..Default::default() })
                .await?;
            if !is_ok(&w) {
                return Ok(Step { exit_code: 126, stdout: String::new(), stderr: format!("sh: {target}: Gatekeeper denied\n"), cwd: next_cwd });
            }
            stdout.clear();
        }
        Ok(Step { exit_code, stdout, stderr, cwd: next_cwd })
    }

    pub async fn vfs_exec(&self, req: &ExecReq) -> Res<Value> {
        let principal = if req.principal.is_empty() { "default" } else { &req.principal };
        let tier = select_exec_tier(req);
        let capability = format!("exec:tier{tier}");
        let decision = self.check(&capability, principal, req.cwd.as_deref())?;
        let cmd_label = req
            .command
            .clone()
            .or_else(|| req.argv.as_ref().map(|a| a.join(" ")))
            .unwrap_or_else(|| match &req.code {
                Some(c) if !c.is_empty() => format!("codemode({}B)", utf16_len(c)),
                _ => String::new(),
            });

        if !decision.allowed {
            self.record_audit(tier, &cmd_label, false, 126)?;
            return Ok(json!({
                "ok": false,
                "denied": true,
                "tier": tier,
                "gatekeeper": decision.to_value(),
                "exitCode": 126,
                "stdout": "",
                "stderr": format!("Gatekeeper denied {capability}: {}", decision.reason),
            }));
        }

        if req.allow_network {
            let egress = self.check("net:egress", principal, None)?;
            if !egress.allowed {
                self.record_audit(tier, &cmd_label, false, 126)?;
                return Ok(json!({
                    "ok": false,
                    "denied": true,
                    "tier": tier,
                    "gatekeeper": egress.to_value(),
                    "exitCode": 126,
                    "stdout": "",
                    "stderr": format!("Gatekeeper denied net:egress: {}", egress.reason),
                }));
            }
        }

        if tier == 0 {
            let raw = req.command.clone().or_else(|| req.argv.as_ref().map(|a| a.join(" "))).unwrap_or_default();
            let segments: Vec<String> = raw
                .replace("&&", ";")
                .split(';')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let mut cwd = normalize_path(req.cwd.as_deref().unwrap_or("/workspace"), "/").path;
            let (mut stdout, mut stderr, mut exit_code) = (String::new(), String::new(), 0);
            for seg in &segments {
                let step = self.run_tier0_single(seg, &cwd, principal, &req.env).await?;
                stdout.push_str(&step.stdout);
                stderr.push_str(&step.stderr);
                cwd = step.cwd;
                exit_code = step.exit_code;
                if exit_code != 0 && raw.contains("&&") {
                    break;
                }
            }
            self.record_audit(0, &raw, true, exit_code)?;
            return Ok(json!({
                "ok": exit_code == 0,
                "tier": 0,
                "runtime": "do-posix-shell",
                "storage": self.backend(),
                "cwd": cwd,
                "exitCode": exit_code,
                "stdout": stdout,
                "stderr": stderr,
                "gatekeeper": decision.to_value(),
            }));
        }

        let outbound = if req.allow_network { Value::from("allowed") } else { Value::Null };

        if tier == 1 {
            let raw_code = req.code.clone().unwrap_or_else(|| {
                let cmd = req.command.as_deref().unwrap_or("");
                let lower = cmd.to_ascii_lowercase();
                let stripped = ["js:", "ts:", "codemode:"]
                    .iter()
                    .find(|p| lower.starts_with(*p))
                    .map_or(cmd, |p| &cmd[p.len()..]);
                stripped.trim().to_string()
            });
            // The TS worker builds the function with `new Function("input", code)`, which workerd refuses inside a Dynamic
            // Worker ("Code generation from strings disallowed"); the snippet is embedded as the function body instead.
            let module_source = format!(
                "export default {{\n  async fetch(request) {{\n    const input = await request.json().catch(() => ({{}}));\n    const fn = async function (input) {{\n{raw_code}\n    }};\n    const value = await fn(input);\n    return Response.json({{ ok: true, value }});\n  }}\n}};"
            );
            let job = LoaderJob {
                module_source,
                allow_network: req.allow_network,
                env: Value::Object(req.env.clone()),
                input: req.input.clone().unwrap_or_else(|| json!({})),
            };
            if let Some(out) = self.h.run_loader(&job).await {
                let out = out?;
                let parsed: Value = serde_json::from_str(&out.text).unwrap_or_else(|_| Value::String(out.text.clone()));
                let exit_code = i64::from(!out.ok);
                self.record_audit(1, &cmd_label, true, exit_code)?;
                let stdout = match &parsed {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                return Ok(json!({
                    "ok": out.ok,
                    "tier": 1,
                    "runtime": "dynamic-worker-loader",
                    "isolated": true,
                    "globalOutbound": outbound,
                    "exitCode": exit_code,
                    "stdout": stdout,
                    "stderr": if out.ok { String::new() } else { out.text.clone() },
                    "result": parsed,
                    "gatekeeper": decision.to_value(),
                }));
            }
            self.record_audit(1, &cmd_label, true, 0)?;
            return Ok(json!({
                "ok": true,
                "tier": 1,
                "runtime": "dynamic-worker-loader",
                "simulated": true,
                "bindingRequired": "LOADER",
                "globalOutbound": outbound,
                "exitCode": 0,
                "stdout": "",
                "stderr": "",
                "note": "env.LOADER binding not configured; declare worker_loaders in wrangler.jsonc to execute live CodeMode isolates",
                "gatekeeper": decision.to_value(),
            }));
        }

        // Tier-2: @cloudflare/sandbox 1.0 container microVM (explicit argv contract).
        let argv: Vec<String> = match &req.argv {
            Some(a) if !a.is_empty() => a.clone(),
            _ => vec!["/bin/sh".into(), "-lc".into(), req.command.clone().unwrap_or_else(|| "true".into())],
        };
        let job = SandboxJob {
            argv: argv.clone(),
            cwd: req.cwd.clone(),
            env: if req.env.is_empty() { None } else { Some(Value::Object(req.env.clone())) },
            timeout_ms: req.timeout_ms.clone(),
        };
        if let Some(out) = self.h.run_sandbox(&job).await {
            let out = out?;
            self.record_audit(2, &argv.join(" "), true, out.exit_code)?;
            return Ok(json!({
                "ok": out.exit_code == 0,
                "tier": 2,
                "runtime": "cloudflare-sandbox-microvm",
                "argv": argv,
                "exitCode": out.exit_code,
                "stdout": out.stdout,
                "stderr": out.stderr,
                "gatekeeper": decision.to_value(),
            }));
        }
        self.record_audit(2, &argv.join(" "), true, 0)?;
        Ok(json!({
            "ok": true,
            "tier": 2,
            "runtime": "cloudflare-sandbox-microvm",
            "simulated": true,
            "bindingRequired": "SANDBOX",
            "argv": argv,
            "exitCode": 0,
            "stdout": "",
            "stderr": "",
            "note": "env.SANDBOX binding not configured; attach containers + SANDBOX Durable Object from wrangler.fullstack.jsonc",
            "gatekeeper": decision.to_value(),
        }))
    }

    // ---- learn log ----

    pub async fn learn_record(&self, input: &Map<String, Value>, principal: &str) -> Res<Value> {
        let decision = self.check("learn:write", principal, None)?;
        if !decision.allowed {
            return Ok(Self::denied(&decision));
        }
        let text = |k: &str, default: &str| match input.get(k) {
            Some(v) if truthy(v) => js_string(v),
            _ => default.to_string(),
        };
        let tags: Vec<Value> = input
            .get("tags")
            .and_then(Value::as_array)
            .map(|a| a.iter().map(|t| Value::String(js_string(t))).collect())
            .unwrap_or_default();
        let payload = match input.get("payload") {
            Some(v @ (Value::Object(_) | Value::Array(_))) => v.clone(),
            _ => json!({}),
        };
        let record = json!({
            "id": make_id("learn"),
            "kind": text("kind", "observation"),
            "source": text("source", "fw-edge"),
            "summary": text("summary", ""),
            "tags": tags,
            "payload": payload,
            "createdAt": now_iso(),
        });
        self.h.sql_exec(
            "INSERT INTO learn_events (id, kind, source, summary, tags, payload, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
            &[
                str_of(&record, "id").into(),
                str_of(&record, "kind").into(),
                str_of(&record, "source").into(),
                str_of(&record, "summary").into(),
                record["tags"].to_string().into(),
                record["payload"].to_string().into(),
                str_of(&record, "createdAt").into(),
            ],
        )?;
        let forwarded = self.h.forward_learn(&record).await?;
        Ok(json!({
            "ok": true,
            "event": record,
            "storage": self.backend(),
            "forwarded": { "k2": forwarded.k2, "basinPipeline": forwarded.basin, "d1": forwarded.d1 },
        }))
    }

    fn learn_events(&self, limit: i64) -> Res<Vec<Value>> {
        let rows = self.h.sql_query(
            "SELECT id, kind, source, summary, tags, payload, created_at FROM learn_events ORDER BY created_at DESC LIMIT ?",
            &[limit.into()],
        )?;
        Ok(rows
            .iter()
            .map(|r| {
                let parse = |v: &Val, empty: &str| {
                    let t = v.text();
                    serde_json::from_str::<Value>(if t.is_empty() { empty } else { &t }).unwrap_or(Value::Null)
                };
                json!({
                    "id": r[0].text(),
                    "kind": r[1].text(),
                    "source": r[2].text(),
                    "summary": r[3].text(),
                    "tags": parse(&r[4], "[]"),
                    "payload": parse(&r[5], "{}"),
                    "createdAt": r[6].text(),
                })
            })
            .collect())
    }

    pub fn learn_list(&self, limit: i64) -> Res<Value> {
        let events = self.learn_events(limit.clamp(1, 500))?;
        Ok(json!({ "ok": true, "storage": self.backend(), "count": events.len(), "events": events }))
    }

    pub fn learn_status(&self) -> Res<Value> {
        let all = self.learn_events(500)?;
        let mut by_kind = Map::new();
        for ev in &all {
            let kind = str_of(ev, "kind");
            let n = by_kind.get(&kind).and_then(Value::as_i64).unwrap_or(0);
            by_kind.insert(kind, (n + 1).into());
        }
        let audit = self
            .h
            .sql_query("SELECT COUNT(*) AS cnt FROM exec_audit", &[])?
            .first()
            .and_then(|r| r.first().map(Val::int))
            .unwrap_or(0);
        let sinks = self.h.sinks();
        Ok(json!({
            "ok": true,
            "tenant": self.h.tenant(),
            "storageBackend": self.backend(),
            "totalEvents": all.len(),
            "byKind": by_kind,
            "latestEventAt": all.first().map_or(Value::Null, |e| e["createdAt"].clone()),
            "execAuditCount": audit,
            "sinks": { "sqlite": true, "k2": sinks.k2, "basinPipeline": sinks.basin, "d1": sinks.d1 },
        }))
    }
}

fn entry_paths(listing: &Value) -> String {
    let paths: Vec<String> = listing
        .get("entries")
        .and_then(Value::as_array)
        .map(|a| a.iter().map(|e| str_of(e, "path")).collect())
        .unwrap_or_default();
    if paths.is_empty() { String::new() } else { format!("{}\n", paths.join("\n")) }
}

// ---------------------------------------------------------------------------------------------------------------
// DO request router (the internal `/vfs/*`, `/gatekeeper/*`, `/api/learn/*` paths of `FwOSAgent.fetch`).
// ---------------------------------------------------------------------------------------------------------------

pub struct DoReq {
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub principal: String,
    pub body: Option<String>,
}

impl DoReq {
    fn q(&self, name: &str) -> Option<&str> {
        self.query.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }
    fn is(&self, method: &str, path: &str) -> bool {
        self.method == method && self.path == path
    }
    fn body_map(&self) -> Map<String, Value> {
        if self.method == "POST" { body_object(self.body.as_deref().unwrap_or("")) } else { Map::new() }
    }
}

fn str_field<'m>(m: &'m Map<String, Value>, k: &str) -> Option<&'m str> {
    m.get(k).and_then(Value::as_str)
}

/// `FwOSAgent.fetch`: returns the HTTP status and the JSON body.
pub async fn route_do<H: Host>(h: &H, req: &DoReq) -> (u16, Value) {
    match route_do_inner(h, req).await {
        Ok(r) => r,
        Err(e) => (500, json!({ "ok": false, "error": e })),
    }
}

async fn route_do_inner<H: Host>(h: &H, req: &DoReq) -> Res<(u16, Value)> {
    let k = Kernel::new(h);
    let principal = req.principal.as_str();
    let denied = |v: &Value| v.get("denied").is_some_and(truthy);
    let status_for = |v: &Value, bad: u16| if denied(v) { 403 } else if is_ok(v) { 200 } else { bad };

    match req.path.as_str() {
        "/vfs/ls" => {
            let body = req.body_map();
            let prefix = str_field(&body, "prefix").or_else(|| str_field(&body, "path")).or_else(|| req.q("prefix")).or_else(|| req.q("path")).unwrap_or("/");
            let res = k.vfs_ls(prefix, principal)?;
            return Ok((if denied(&res) { 403 } else { 200 }, res));
        }
        "/vfs/read" => {
            let body = req.body_map();
            let path = str_field(&body, "path").or_else(|| req.q("path")).unwrap_or("");
            if path.is_empty() {
                return Ok((400, json!({ "ok": false, "error": "Missing path" })));
            }
            let res = k.vfs_read(path, principal).await?;
            return Ok((status_for(&res, 404), res));
        }
        "/vfs/write" if req.method == "POST" => {
            let body = req.body_map();
            let (Some(path), Some(content)) = (str_field(&body, "path").filter(|p| !p.is_empty()), str_field(&body, "content")) else {
                return Ok((400, json!({ "ok": false, "error": "Missing path or string content" })));
            };
            let opts = WriteOpts {
                content_type: str_field(&body, "contentType").map(String::from),
                metadata: body.get("metadata").filter(|v| !v.is_null()).cloned(),
                principal: principal.into(),
                spill_to_r2: body.get("spillToR2").is_some_and(truthy),
            };
            let res = k.vfs_write(path, content, opts).await?;
            return Ok((status_for(&res, 400), res));
        }
        "/vfs/stat" => {
            let body = req.body_map();
            let path = str_field(&body, "path").or_else(|| req.q("path")).unwrap_or("/");
            let res = k.vfs_stat(path, principal)?;
            return Ok((if denied(&res) { 403 } else { 200 }, res));
        }
        "/vfs/exec" if req.method == "POST" => {
            let body = body_object(req.body.as_deref().unwrap_or(""));
            let res = k.vfs_exec(&ExecReq::from_value(&body, principal)).await?;
            return Ok((if denied(&res) { 403 } else { 200 }, res));
        }
        _ => {}
    }

    if req.is("GET", "/gatekeeper/policies") {
        let rules: Vec<Value> = k.rules()?.iter().map(Rule::to_value).collect();
        return Ok((200, json!({ "ok": true, "count": rules.len(), "rules": rules })));
    }
    if req.is("POST", "/gatekeeper/policies") {
        let body = body_object(req.body.as_deref().unwrap_or(""));
        let effect = str_field(&body, "effect").filter(|e| *e == "allow" || *e == "deny");
        let capability = str_field(&body, "capability").filter(|c| !c.is_empty());
        let (Some(capability), Some(effect)) = (capability, effect) else {
            return Ok((400, json!({ "ok": false, "error": "Missing capability or valid effect ('allow' | 'deny')" })));
        };
        let rule = k.upsert_rule(Rule {
            id: str_field(&body, "id").map_or_else(|| make_id("rule"), String::from),
            capability: capability.into(),
            effect: effect.into(),
            principal: str_field(&body, "principal").unwrap_or("*").into(),
            path_prefix: str_field(&body, "pathPrefix").map(String::from),
            reason: str_field(&body, "reason").unwrap_or("Updated via Gatekeeper API").into(),
            updated_at: now_iso(),
        })?;
        return Ok((200, json!({ "ok": true, "rule": rule.to_value() })));
    }
    if req.is("POST", "/gatekeeper/check") {
        let body = body_object(req.body.as_deref().unwrap_or(""));
        let Some(capability) = str_field(&body, "capability").filter(|c| !c.is_empty()) else {
            return Ok((400, json!({ "ok": false, "error": "Missing capability" })));
        };
        let who = str_field(&body, "principal").unwrap_or(principal);
        let decision = k.check(capability, who, str_field(&body, "path"))?;
        return Ok((if decision.allowed { 200 } else { 403 }, json!({ "ok": true, "decision": decision.to_value() })));
    }

    if req.is("GET", "/api/learn/events") {
        let limit = parse_int_js(req.q("limit").unwrap_or("50")).filter(|v| *v != 0).unwrap_or(50);
        return Ok((200, k.learn_list(limit)?));
    }
    if req.is("POST", "/api/learn/events") {
        let body = body_object(req.body.as_deref().unwrap_or(""));
        let res = k.learn_record(&body, principal).await?;
        return Ok((if denied(&res) { 403 } else { 201 }, res));
    }
    if req.is("GET", "/api/learn/status") {
        return Ok((200, k.learn_status()?));
    }

    Ok((404, json!({ "ok": false, "error": "DO route not found", "path": req.path })))
}

// ---------------------------------------------------------------------------------------------------------------
// Durable Object (wasm32 runtime).
// ---------------------------------------------------------------------------------------------------------------

mod runtime {
    use std::collections::HashMap;

    use js_sys::{Array, Function, Object, Promise, Reflect};
    use serde_json::Value;
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;
    use worker::{Env, Request, Response, State, durable_object};

    use super::{
        DoReq, Host, LoaderJob, LoaderOut, SandboxJob, SandboxOut, Sinks, Val, route_do,
    };
    use worker::{SqlStorage, SqlStorageValue};
    use worker::DurableObject;

    /// `Boolean(env[name])` for an object binding.
    pub fn binding(env: &Env, name: &str) -> Option<JsValue> {
        Reflect::get(env, &JsValue::from(name)).ok().filter(|v| !v.is_undefined() && !v.is_null())
    }

    pub fn js_message(v: &JsValue) -> String {
        if let Ok(m) = Reflect::get(v, &"message".into())
            && let Some(s) = m.as_string()
        {
            return s;
        }
        v.as_string().unwrap_or_else(|| String::from(js_sys::JsString::from(v.clone())))
    }

    fn err(v: JsValue) -> String {
        js_message(&v)
    }

    /// `await value` for a value that may or may not be a promise.
    pub async fn settle(v: JsValue) -> Result<JsValue, String> {
        match v.dyn_into::<Promise>() {
            Ok(p) => JsFuture::from(p).await.map_err(err),
            Err(v) => Ok(v),
        }
    }

    pub fn method(obj: &JsValue, name: &str) -> Option<Function> {
        Reflect::get(obj, &JsValue::from(name)).ok().and_then(|f| f.dyn_into::<Function>().ok())
    }

    fn call(obj: &JsValue, name: &str, args: &[JsValue]) -> Result<JsValue, String> {
        let f = method(obj, name).ok_or_else(|| format!("{name} is not a function"))?;
        let arr: Array = args.iter().collect();
        f.apply(obj, &arr).map_err(err)
    }

    pub fn to_js(v: &Value) -> JsValue {
        js_sys::JSON::parse(&v.to_string()).unwrap_or(JsValue::NULL)
    }

    pub fn from_js(v: &JsValue) -> Option<Value> {
        let s = js_sys::JSON::stringify(v).ok()?.as_string()?;
        serde_json::from_str(&s).ok()
    }

    fn set(obj: &Object, k: &str, v: &JsValue) {
        let _ = Reflect::set(obj, &JsValue::from(k), v);
    }

    fn sql_val(v: &Val) -> SqlStorageValue {
        match v {
            Val::Null => SqlStorageValue::Null,
            Val::Int(i) => SqlStorageValue::Integer(*i),
            Val::Text(s) => SqlStorageValue::String(s.clone()),
        }
    }

    pub struct DoHost {
        pub sql: SqlStorage,
        pub env: Env,
        pub tenant: String,
    }

    impl DoHost {
        fn bucket(&self) -> Result<worker::Bucket, String> {
            self.env.bucket("VFS_BLOBS").map_err(|e| e.to_string())
        }
    }

    impl Host for DoHost {
        fn sql_exec(&self, query: &str, params: &[Val]) -> Result<(), String> {
            let bound: Vec<SqlStorageValue> = params.iter().map(sql_val).collect();
            self.sql.exec(query, bound).map(|_| ()).map_err(|e| e.to_string())
        }

        fn sql_query(&self, query: &str, params: &[Val]) -> Result<Vec<Vec<Val>>, String> {
            let bound: Vec<SqlStorageValue> = params.iter().map(sql_val).collect();
            let cursor = self.sql.exec(query, bound).map_err(|e| e.to_string())?;
            let mut rows = Vec::new();
            for row in cursor.raw() {
                let row = row.map_err(|e| e.to_string())?;
                rows.push(
                    row.into_iter()
                        .map(|c| match c {
                            SqlStorageValue::Null => Val::Null,
                            SqlStorageValue::Integer(i) => Val::Int(i),
                            SqlStorageValue::Boolean(b) => Val::Int(i64::from(b)),
                            SqlStorageValue::Float(f) => Val::Int(f as i64),
                            SqlStorageValue::String(s) => Val::Text(s),
                            SqlStorageValue::Blob(b) => Val::Text(String::from_utf8_lossy(&b).into_owned()),
                        })
                        .collect(),
                );
            }
            Ok(rows)
        }

        fn tenant(&self) -> String {
            self.tenant.clone()
        }

        fn fw_env(&self) -> String {
            self.env.var("FW_ENV").map_or_else(|_| "production".into(), |v| v.to_string())
        }

        fn has_blobs(&self) -> bool {
            binding(&self.env, "VFS_BLOBS").is_some()
        }

        async fn blob_get(&self, key: &str) -> Result<Option<String>, String> {
            let obj = self.bucket()?.get(key).execute().await.map_err(|e| e.to_string())?;
            match obj.as_ref().and_then(|o| o.body()) {
                Some(body) => Ok(Some(body.text().await.map_err(|e| e.to_string())?)),
                None => Ok(None),
            }
        }

        async fn blob_put(&self, key: &str, content: &str, content_type: &str, sha256: &str, path: &str) -> Result<(), String> {
            let meta = HashMap::from([("sha256".to_string(), sha256.to_string()), ("path".to_string(), path.to_string())]);
            self.bucket()?
                .put(key, content.to_string())
                .http_metadata(worker::HttpMetadata { content_type: Some(content_type.to_string()), ..Default::default() })
                .custom_metadata(meta)
                .execute()
                .await
                .map(|_| ())
                .map_err(|e| e.to_string())
        }

        async fn run_loader(&self, job: &LoaderJob) -> Option<Result<LoaderOut, String>> {
            let loader = binding(&self.env, "LOADER")?;
            Some(loader_run(&loader, job).await)
        }

        async fn run_sandbox(&self, job: &SandboxJob) -> Option<Result<SandboxOut, String>> {
            let ns = binding(&self.env, "SANDBOX")?;
            Some(sandbox_run(&ns, &self.tenant, job).await)
        }

        async fn forward_learn(&self, record: &Value) -> Result<Sinks, String> {
            let mut out = Sinks::default();
            if let Some(k2) = binding(&self.env, "EVENT_LOG") {
                if method(&k2, "produce").is_some() {
                    let entry = Object::new();
                    set(&entry, "key", &JsValue::from(record["id"].as_str().unwrap_or_default()));
                    set(&entry, "value", &JsValue::from(record.to_string()));
                    settle(call(&k2, "produce", &[Array::of1(&entry).into()])?).await?;
                    out.k2 = true;
                } else if method(&k2, "send").is_some() {
                    settle(call(&k2, "send", &[Array::of1(&to_js(record)).into()])?).await?;
                    out.k2 = true;
                }
            }
            if let Some(basin) = binding(&self.env, "TELEMETRY_PIPELINE") {
                let mut with_tenant = record.clone();
                if let Some(m) = with_tenant.as_object_mut() {
                    m.insert("tenant".into(), self.tenant.clone().into());
                }
                settle(call(&basin, "send", &[Array::of1(&to_js(&with_tenant)).into()])?).await?;
                out.basin = true;
            }
            if binding(&self.env, "META_DB").is_some() {
                out.d1 = self.insert_d1(record).await.is_ok();
            }
            Ok(out)
        }

        fn sinks(&self) -> Sinks {
            Sinks {
                k2: binding(&self.env, "EVENT_LOG").is_some(),
                basin: binding(&self.env, "TELEMETRY_PIPELINE").is_some(),
                d1: binding(&self.env, "META_DB").is_some(),
            }
        }
    }

    impl DoHost {
        async fn insert_d1(&self, record: &Value) -> Result<(), String> {
            let db = self.env.d1("META_DB").map_err(|e| e.to_string())?;
            let s = |k: &str| JsValue::from(record[k].as_str().unwrap_or_default());
            db.prepare("INSERT INTO fw_learn_events (id, tenant, kind, source, summary, created_at) VALUES (?, ?, ?, ?, ?, ?)")
                .bind(&[s("id"), JsValue::from(self.tenant.as_str()), s("kind"), s("source"), s("summary"), s("createdAt")])
                .map_err(|e| e.to_string())?
                .run()
                .await
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
    }

    async fn loader_run(loader: &JsValue, job: &LoaderJob) -> Result<LoaderOut, String> {
        let modules = Object::new();
        set(&modules, "index.js", &JsValue::from(job.module_source.as_str()));
        let cfg = Object::new();
        set(&cfg, "compatibilityDate", &JsValue::from(super::COMPATIBILITY_DATE));
        set(&cfg, "mainModule", &JsValue::from("index.js"));
        set(&cfg, "modules", &modules);
        if !job.allow_network {
            set(&cfg, "globalOutbound", &JsValue::NULL);
        }
        set(&cfg, "env", &to_js(&job.env));
        let worker = call(loader, "load", &[cfg.into()])?;
        let entry = call(&worker, "getEntrypoint", &[])?;
        let headers = Object::new();
        set(&headers, "content-type", &JsValue::from("application/json"));
        let init = Object::new();
        set(&init, "method", &JsValue::from("POST"));
        set(&init, "headers", &headers);
        set(&init, "body", &JsValue::from(job.input.to_string()));
        let res = settle(call(&entry, "fetch", &[JsValue::from("http://codemode.internal/run"), init.into()])?).await?;
        let ok = Reflect::get(&res, &"ok".into()).ok().and_then(|v| v.as_bool()).unwrap_or(false);
        let text = settle(call(&res, "text", &[])?).await?.as_string().unwrap_or_default();
        Ok(LoaderOut { ok, text })
    }

    async fn sandbox_run(ns: &JsValue, tenant: &str, job: &SandboxJob) -> Result<SandboxOut, String> {
        let stub = if method(ns, "getByName").is_some() {
            call(ns, "getByName", &[JsValue::from(tenant)])?
        } else {
            let id = call(ns, "idFromName", &[JsValue::from(tenant)])?;
            call(ns, "get", &[id])?
        };
        let argv: Array = job.argv.iter().map(|a| JsValue::from(a.as_str())).collect();
        let opts = Object::new();
        if let Some(cwd) = &job.cwd {
            set(&opts, "cwd", &JsValue::from(cwd.as_str()));
        }
        if let Some(env) = &job.env {
            set(&opts, "env", &to_js(env));
        }
        if let Some(t) = &job.timeout_ms {
            set(&opts, "timeoutMs", &to_js(t));
        }
        let num = |v: &JsValue, k: &str| Reflect::get(v, &k.into()).ok().and_then(|x| x.as_f64());
        let text = |v: &JsValue, k: &str| Reflect::get(v, &k.into()).ok().and_then(|x| x.as_string()).unwrap_or_default();

        if method(&stub, "exec").is_some() {
            let handle = settle(call(&stub, "exec", &[argv.into(), opts.into()])?).await?;
            let out = if method(&handle, "output").is_some() {
                let o = Object::new();
                set(&o, "encoding", &JsValue::from("utf8"));
                settle(call(&handle, "output", &[o.into()])?).await?
            } else {
                handle
            };
            return Ok(SandboxOut { exit_code: num(&out, "exitCode").unwrap_or(0.0) as i64, stdout: text(&out, "stdout"), stderr: text(&out, "stderr") });
        }

        let body = serde_json::json!({ "argv": job.argv, "cwd": job.cwd, "env": job.env, "timeoutMs": job.timeout_ms });
        let headers = Object::new();
        set(&headers, "content-type", &JsValue::from("application/json"));
        let init = Object::new();
        set(&init, "method", &JsValue::from("POST"));
        set(&init, "headers", &headers);
        set(&init, "body", &JsValue::from(body.to_string()));
        let res = settle(call(&stub, "fetch", &[JsValue::from("https://sandbox.internal/exec"), init.into()])?).await?;
        let ok = Reflect::get(&res, &"ok".into()).ok().and_then(|v| v.as_bool()).unwrap_or(false);
        let parsed = match settle(call(&res, "json", &[])?).await {
            Ok(v) => v,
            Err(_) => JsValue::from(Object::new()),
        };
        let exit = num(&parsed, "exitCode").map_or(i64::from(!ok), |f| f as i64);
        Ok(SandboxOut { exit_code: exit, stdout: text(&parsed, "stdout"), stderr: text(&parsed, "stderr") })
    }

    /// The per-tenant Durable Object (same class name and SQLite tables as the TypeScript `FwOSAgent`).
    #[durable_object]
    pub struct FwOSAgent {
        state: State,
        env: Env,
    }

    impl FwOSAgent {
        fn host(&self) -> DoHost {
            let id = self.state.id();
            DoHost { sql: self.state.storage().sql(), env: self.env.clone(), tenant: id.name().unwrap_or_else(|| id.to_string()) }
        }
    }

    impl DurableObject for FwOSAgent {
        fn new(state: State, env: Env) -> Self {
            let agent = FwOSAgent { state, env };
            if let Err(e) = super::Kernel::new(&agent.host()).init() {
                worker::console_error!("FwOSAgent init failed: {e}");
            }
            agent
        }

        async fn fetch(&self, mut req: Request) -> worker::Result<Response> {
            let url = req.url()?;
            let body = if matches!(req.method(), worker::Method::Get | worker::Method::Head) { None } else { req.text().await.ok() };
            let do_req = DoReq {
                method: req.method().to_string(),
                path: url.path().to_string(),
                query: url.query_pairs().map(|(k, v)| (k.into_owned(), v.into_owned())).collect(),
                principal: req.headers().get("x-fw-principal")?.unwrap_or_else(|| "default".into()),
                body,
            };
            let (status, value) = route_do(&self.host(), &do_req).await;
            super::json_response(&value, status)
        }
    }
}

pub use runtime::{FwOSAgent, binding, from_js, js_message, settle, to_js};

/// Tenant routing: header `x-fw-tenant`, else `?tenant=`, else `default`.
pub fn resolve_tenant(header: Option<&str>, query: Option<&str>) -> String {
    [header, query]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|s| !s.is_empty())
        .unwrap_or("default")
        .to_string()
}

#[cfg(test)]
pub(crate) mod testkit {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::future::Future;
    use std::task::{Context, Poll, Waker};

    use rusqlite::types::Value as SqlValue;

    use super::*;

    /// Polls an immediately-ready future (the test host never yields).
    pub(crate) fn block_on<F: Future>(fut: F) -> F::Output {
        let mut fut = Box::pin(fut);
        let mut cx = Context::from_waker(Waker::noop());
        loop {
            if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
                return v;
            }
        }
    }

    pub(crate) struct TestHost {
        pub(crate) db: rusqlite::Connection,
        pub(crate) tenant: String,
        pub(crate) blobs: Option<RefCell<HashMap<String, String>>>,
        pub(crate) loader: bool,
        pub(crate) sandbox: bool,
        pub(crate) sinks: Sinks,
        pub(crate) forwarded: RefCell<Vec<Value>>,
        pub(crate) sandbox_calls: RefCell<Vec<Vec<String>>>,
    }

    impl TestHost {
        pub(crate) fn new(tenant: &str) -> Self {
            TestHost {
                db: rusqlite::Connection::open_in_memory().unwrap(),
                tenant: tenant.into(),
                blobs: None,
                loader: false,
                sandbox: false,
                sinks: Sinks::default(),
                forwarded: RefCell::new(Vec::new()),
                sandbox_calls: RefCell::new(Vec::new()),
            }
        }
    }

    fn to_sql(p: &[Val]) -> Vec<SqlValue> {
        p.iter()
            .map(|v| match v {
                Val::Null => SqlValue::Null,
                Val::Int(i) => SqlValue::Integer(*i),
                Val::Text(s) => SqlValue::Text(s.clone()),
            })
            .collect()
    }

    impl Host for TestHost {
        fn sql_exec(&self, q: &str, p: &[Val]) -> Result<(), String> {
            self.db.execute(q, rusqlite::params_from_iter(to_sql(p))).map(|_| ()).map_err(|e| e.to_string())
        }
        fn sql_query(&self, q: &str, p: &[Val]) -> Result<Vec<Vec<Val>>, String> {
            let mut stmt = self.db.prepare(q).map_err(|e| e.to_string())?;
            let n = stmt.column_count();
            let rows = stmt
                .query_map(rusqlite::params_from_iter(to_sql(p)), |row| {
                    (0..n)
                        .map(|i| {
                            Ok(match row.get::<_, SqlValue>(i)? {
                                SqlValue::Null => Val::Null,
                                SqlValue::Integer(i) => Val::Int(i),
                                SqlValue::Text(s) => Val::Text(s),
                                _ => Val::Null,
                            })
                        })
                        .collect::<rusqlite::Result<Vec<Val>>>()
                })
                .map_err(|e| e.to_string())?;
            rows.collect::<rusqlite::Result<Vec<_>>>().map_err(|e| e.to_string())
        }
        fn tenant(&self) -> String {
            self.tenant.clone()
        }
        fn fw_env(&self) -> String {
            "production".into()
        }
        fn has_blobs(&self) -> bool {
            self.blobs.is_some()
        }
        async fn blob_get(&self, key: &str) -> Result<Option<String>, String> {
            Ok(self.blobs.as_ref().and_then(|b| b.borrow().get(key).cloned()))
        }
        async fn blob_put(&self, key: &str, content: &str, _ct: &str, _sha: &str, _path: &str) -> Result<(), String> {
            self.blobs.as_ref().ok_or("no bucket")?.borrow_mut().insert(key.into(), content.into());
            Ok(())
        }
        async fn run_loader(&self, job: &LoaderJob) -> Option<Result<LoaderOut, String>> {
            self.loader.then(|| {
                assert!(!job.allow_network);
                Ok(LoaderOut { ok: true, text: r#"{"ok":true,"value":42}"#.into() })
            })
        }
        async fn run_sandbox(&self, job: &SandboxJob) -> Option<Result<SandboxOut, String>> {
            self.sandbox.then(|| {
                self.sandbox_calls.borrow_mut().push(job.argv.clone());
                Ok(SandboxOut { exit_code: 0, stdout: format!("sandbox-ran:{}", job.argv.join(" ")), stderr: String::new() })
            })
        }
        async fn forward_learn(&self, record: &Value) -> Result<Sinks, String> {
            self.forwarded.borrow_mut().push(record.clone());
            Ok(self.sinks)
        }
        fn sinks(&self) -> Sinks {
            self.sinks
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::testkit::{TestHost, block_on};
    use super::*;

    fn call(h: &TestHost, method: &str, path: &str, principal: &str, query: &[(&str, &str)], body: Option<Value>) -> (u16, Value) {
        Kernel::new(h).init().unwrap();
        block_on(route_do(
            h,
            &DoReq {
                method: method.into(),
                path: path.into(),
                query: query.iter().map(|(k, v)| ((*k).into(), (*v).into())).collect(),
                principal: principal.into(),
                body: body.map(|b| b.to_string()),
            },
        ))
    }

    fn post(h: &TestHost, path: &str, body: Value) -> (u16, Value) {
        call(h, "POST", path, "default", &[], Some(body))
    }

    #[test]
    fn iso_matches_to_iso_string() {
        assert_eq!(iso_from_ms(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso_from_ms(1_763_726_096_789), "2025-11-21T11:54:56.789Z");
        assert_eq!(iso_from_ms(951_782_400_000), "2000-02-29T00:00:00.000Z");
    }

    #[test]
    fn ids_have_the_ts_shape() {
        let id = make_id("learn");
        let parts: Vec<&str> = id.splitn(3, '_').collect();
        assert_eq!(parts[0], "learn");
        assert_eq!(parts[2].len(), 12);
        assert!(parts[1].chars().all(|c| c.is_ascii_alphanumeric()));
        assert_ne!(make_id("x"), make_id("x"));
    }

    #[test]
    fn normalizes_paths_like_the_ts_worker() {
        let n = |p: &str, cwd: &str| normalize_path(p, cwd);
        assert_eq!(n("/workspace/../../etc/passwd", "/"), NormPath { path: "/etc/passwd".into(), traversal: true });
        assert_eq!(n("a/b/./c", "/workspace/"), NormPath { path: "/workspace/a/b/c".into(), traversal: false });
        assert_eq!(n("", "/x"), NormPath { path: "/".into(), traversal: false });
        assert_eq!(n("   ", "/x"), NormPath { path: "/x".into(), traversal: false });
        assert_eq!(n("\\win\\path", "/"), NormPath { path: "/win/path".into(), traversal: false });
        assert_eq!(n("//a//b/", "/"), NormPath { path: "/a/b".into(), traversal: false });
    }

    #[test]
    fn tokenizes_quotes_and_escapes() {
        assert_eq!(parse_shell_tokens("echo  'a b'  \"c \\\" d\" e"), vec!["echo", "a b", "c \" d", "e"]);
        assert_eq!(parse_shell_tokens("  "), Vec::<String>::new());
        assert_eq!(parse_shell_tokens("a\"b c\"d"), vec!["ab cd"]);
    }

    #[test]
    fn splits_redirects_like_the_regex() {
        assert_eq!(split_redirect("echo hi > /a"), Some(("echo hi ".into(), ">", "/a".into())));
        assert_eq!(split_redirect("echo hi >> /a "), Some(("echo hi ".into(), ">>", "/a".into())));
        assert_eq!(split_redirect("echo hi"), None);
        assert_eq!(split_redirect("echo hi > a b"), None);
        assert_eq!(split_redirect("echo >>"), Some(("echo ".into(), ">", ">".into())));
    }

    #[test]
    fn parse_int_matches_js() {
        assert_eq!(parse_int_js("12abc"), Some(12));
        assert_eq!(parse_int_js(" -3"), Some(-3));
        assert_eq!(parse_int_js("abc"), None);
        assert_eq!(parse_int_js("0"), Some(0));
    }

    #[test]
    fn seeds_fourteen_default_rules_and_the_manifest() {
        let h = TestHost::new("t");
        let k = Kernel::new(&h);
        k.init().unwrap();
        k.init().unwrap();
        assert_eq!(k.rules().unwrap().len(), 14);
        let stat = k.vfs_stat("/etc/fw.json", "default").unwrap();
        assert_eq!(stat["sha256"], "seed");
        let read = block_on(k.vfs_read("/etc/fw.json", "default")).unwrap();
        let manifest: Value = serde_json::from_str(read["content"].as_str().unwrap()).unwrap();
        assert_eq!(manifest["product"], "fw-edge");
        assert_eq!(manifest["compatibilityDate"], COMPATIBILITY_DATE);
        assert_eq!(read["size"], read["content"].as_str().unwrap().len());
        let ids: Vec<String> = k.rules().unwrap().into_iter().map(|r| r.id).collect();
        assert_eq!(ids.first().unwrap(), "allow-admin-all");
        assert!(ids.contains(&"deny-secrets-write".to_string()));
    }

    #[test]
    fn gatekeeper_denials_match_the_ts_scenarios() {
        let h = TestHost::new("t");
        let k = Kernel::new(&h);
        k.init().unwrap();
        let d = k.check("vfs:write", "default", Some("/etc/secrets/token")).unwrap();
        assert!(!d.allowed);
        assert_eq!(d.matched_rule_id, "deny-secrets-write");
        let t = k.check("vfs:read", "default", Some("/workspace/../../etc/passwd")).unwrap();
        assert_eq!(t.matched_rule_id, "builtin-traversal-guard");
        assert_eq!(t.path.as_deref(), Some("/workspace/../../etc/passwd"));
        assert_eq!(k.check("exec:tier2", "guest", None).unwrap().matched_rule_id, "deny-guest-tier2");
        assert_eq!(k.check("net:egress", "default", None).unwrap().matched_rule_id, "deny-default-egress");
        assert!(k.check("exec:tier2", "default", None).unwrap().allowed);
        assert_eq!(k.check("nothing", "stranger", None).unwrap().matched_rule_id, "default-deny");
        // admin is allowed everywhere except under explicit deny rules
        assert!(k.check("exec:tier2", "admin", None).unwrap().allowed);
        assert!(!k.check("vfs:write", "admin", Some("/etc/secrets/x")).unwrap().allowed);
        // the decision JSON omits `path` when no path was checked
        assert!(k.check("ai:infer", "default", None).unwrap().to_value().get("path").is_none());
    }

    #[test]
    fn custom_rule_is_enforced_and_listed() {
        let h = TestHost::new("t");
        let (status, body) = post(
            &h,
            "/gatekeeper/policies",
            json!({"id": "deny-locked-dir", "capability": "vfs:write", "effect": "deny", "principal": "*", "pathPrefix": "/workspace/locked", "reason": "Locked by policy test"}),
        );
        assert_eq!(status, 200);
        assert_eq!(body["rule"]["pathPrefix"], "/workspace/locked");
        let (status, body) = post(&h, "/gatekeeper/check", json!({"capability": "vfs:write", "path": "/workspace/locked/file.txt"}));
        assert_eq!(status, 403);
        assert_eq!(body["decision"]["matchedRuleId"], "deny-locked-dir");
        let (status, body) = call(&h, "GET", "/gatekeeper/policies", "default", &[], None);
        assert_eq!(status, 200);
        assert_eq!(body["count"], 15);
        assert!(body["rules"].as_array().unwrap().iter().any(|r| r["id"] == "deny-locked-dir"));
        assert_eq!(post(&h, "/gatekeeper/policies", json!({"capability": "x", "effect": "maybe"})).0, 400);
        assert_eq!(post(&h, "/gatekeeper/check", json!({})).0, 400);
    }

    #[test]
    fn vfs_write_read_stat_ls_roundtrip() {
        let h = TestHost::new("prod-tenant");
        let (s, w) = post(&h, "/vfs/write", json!({"path": "/workspace/kernel.json", "content": "{\"kernel\":\"sqlite\"}"}));
        assert_eq!(s, 200);
        assert_eq!(w["storage"], "sqlite");
        assert_eq!(w["sha256"].as_str().unwrap().len(), 64);
        assert_eq!(w["contentType"], "application/json");
        let (_, stat) = call(&h, "GET", "/vfs/stat", "default", &[("path", "/workspace/kernel.json")], None);
        assert_eq!((stat["exists"].clone(), stat["storage"].clone()), (json!(true), json!("sqlite")));
        let (_, ls) = call(&h, "GET", "/vfs/ls", "default", &[("prefix", "/workspace")], None);
        assert_eq!(ls["storage"], "sqlite");
        assert!(ls["entries"].as_array().unwrap().iter().any(|e| e["path"] == "/workspace/kernel.json"));
        let (s, r) = call(&h, "GET", "/vfs/read", "default", &[("path", "/workspace/kernel.json")], None);
        assert_eq!((s, r["content"].clone()), (200, json!("{\"kernel\":\"sqlite\"}")));
        assert_eq!(call(&h, "GET", "/vfs/read", "default", &[("path", "/nope")], None).0, 404);
        assert_eq!(call(&h, "GET", "/vfs/read", "default", &[("path", "/workspace")], None).1["error"], "EISDIR");
        assert_eq!(call(&h, "GET", "/vfs/read", "default", &[], None).0, 400);
        assert_eq!(post(&h, "/vfs/write", json!({"path": "/x"})).0, 400);
        assert_eq!(post(&h, "/vfs/write", json!({"path": "/", "content": "x"})).0, 400);
        let (_, missing) = call(&h, "GET", "/vfs/stat", "default", &[("path", "/ghost")], None);
        assert_eq!(missing, json!({"ok": true, "exists": false, "path": "/ghost"}));
        assert_eq!(call(&h, "GET", "/vfs/write", "default", &[], None).0, 404);
    }

    #[test]
    fn vfs_denials_are_403() {
        let h = TestHost::new("t");
        assert_eq!(post(&h, "/vfs/write", json!({"path": "/etc/secrets/token", "content": "bad"})).0, 403);
        assert_eq!(call(&h, "GET", "/vfs/read", "default", &[("path", "/workspace/../../etc/passwd")], None).0, 403);
        let (s, b) = call(&h, "POST", "/vfs/exec", "guest", &[], Some(json!({"tier": 2, "argv": ["python3", "--version"]})));
        assert_eq!(s, 403);
        assert_eq!(b["gatekeeper"]["matchedRuleId"], "deny-guest-tier2");
        let (s, b) = post(&h, "/vfs/exec", json!({"tier": 1, "code": "return 1;", "allowNetwork": true}));
        assert_eq!(s, 403);
        assert_eq!(b["stderr"], "Gatekeeper denied net:egress: Outbound network egress is fail-closed by default (CodeMode globalOutbound: null)");
        let audit = h.sql_query("SELECT COUNT(*) FROM exec_audit WHERE allowed = 0", &[]).unwrap();
        assert_eq!(audit[0][0], Val::Int(2));
    }

    #[test]
    fn r2_spill_roundtrip_and_threshold() {
        let mut h = TestHost::new("tiers-tenant");
        h.blobs = Some(RefCell::new(HashMap::new()));
        let (_, w) = post(&h, "/vfs/write", json!({"path": "/workspace/blob.txt", "content": "spillover-payload", "spillToR2": true}));
        assert_eq!((w["ok"].clone(), w["storage"].clone()), (json!(true), json!("r2")));
        assert_eq!(w["blobKey"], "vfs/tiers-tenant/workspace/blob.txt");
        assert_eq!(h.blobs.as_ref().unwrap().borrow().len(), 1);
        let (_, r) = call(&h, "GET", "/vfs/read", "default", &[("path", "/workspace/blob.txt")], None);
        assert_eq!((r["storage"].clone(), r["content"].clone()), (json!("r2"), json!("spillover-payload")));
        let (_, stat) = call(&h, "GET", "/vfs/stat", "default", &[("path", "/workspace/blob.txt")], None);
        assert_eq!(stat["storage"], "r2");
        let big = "x".repeat(R2_SPILL_THRESHOLD_BYTES + 1);
        assert_eq!(post(&h, "/vfs/write", json!({"path": "/big.txt", "content": big})).1["storage"], "r2");
        assert_eq!(post(&h, "/vfs/write", json!({"path": "/small.txt", "content": "x"})).1["storage"], "sqlite");
        // without a bucket a spill request stays in SQLite
        let plain = TestHost::new("t");
        assert_eq!(post(&plain, "/vfs/write", json!({"path": "/a", "content": "x", "spillToR2": true})).1["storage"], "sqlite");
    }

    #[test]
    fn tier0_shell_redirect_chain_grep_wc() {
        let h = TestHost::new("tiers-tenant");
        let (s, r) = post(
            &h,
            "/vfs/exec",
            json!({"tier": 0, "command": "mkdir -p /workspace/src && echo alpha-beta > /workspace/src/note.txt && grep alpha /workspace/src/note.txt && wc /workspace/src/note.txt"}),
        );
        assert_eq!(s, 200);
        assert_eq!((r["ok"].clone(), r["tier"].clone(), r["runtime"].clone()), (json!(true), json!(0), json!("do-posix-shell")));
        let stdout = r["stdout"].as_str().unwrap();
        assert!(stdout.contains("alpha-beta") && stdout.contains("/workspace/src/note.txt"), "{stdout}");
        assert_eq!(stdout, "alpha-beta\n2 1 11 /workspace/src/note.txt\n");
        assert_eq!(r["cwd"], "/workspace");
    }

    fn sh(h: &TestHost, cmd: &str) -> Value {
        post(h, "/vfs/exec", json!({"command": cmd})).1
    }

    #[test]
    fn tier0_builtins() {
        let h = TestHost::new("t");
        let r = sh(&h, "pwd && whoami && uname && echo 'a  b' c");
        assert_eq!(r["stdout"], format!("/workspace\ndefault\nFwEdgeOS {FW_VERSION} cloudflare-workers wasm32-v8\na  b c\n"));
        assert_eq!(sh(&h, "cd /etc && pwd")["stdout"], "/etc\n");
        assert_eq!(sh(&h, "cd /nope")["stderr"], "cd: /nope: No such directory\n");
        assert_eq!(sh(&h, "cd ../..")["stderr"], "cd: path traversal denied\n");
        assert_eq!(sh(&h, "nonsense")["exitCode"], 127);
        assert!(sh(&h, "help")["stdout"].as_str().unwrap().starts_with("Fw Tier-0 DO POSIX Shell:"));
        assert_eq!(sh(&h, "env")["stdout"], "FW_ENV=production\nPWD=/workspace\nUSER=default\n");
        assert_eq!(sh(&h, "")["ok"], true);

        sh(&h, "echo one > a.txt; echo two >> a.txt; echo three >> a.txt");
        assert_eq!(sh(&h, "cat a.txt")["stdout"], "one\ntwo\nthree\n");
        assert_eq!(sh(&h, "head -n 2 a.txt")["stdout"], "one\ntwo\n");
        assert_eq!(sh(&h, "tail -n 1 a.txt")["stdout"], "\n");
        assert_eq!(sh(&h, "tail -n 2 a.txt")["stdout"], "three\n\n");
        assert_eq!(sh(&h, "grep tw a.txt")["stdout"], "two\n");
        assert_eq!(sh(&h, "grep zzz a.txt")["exitCode"], 1);
        assert_eq!(sh(&h, "grep a.txt")["stderr"], "grep: usage: grep <pattern> <path>\n");
        assert_eq!(sh(&h, "wc a.txt")["stdout"], "4 3 14 a.txt\n");
        assert_eq!(sh(&h, "cp a.txt b.txt && mv b.txt c.txt && cat c.txt")["stdout"], "one\ntwo\nthree\n");
        assert_eq!(sh(&h, "cat b.txt")["stderr"], "cat: b.txt: ENOENT\n");
        assert_eq!(sh(&h, "rm c.txt && rm c.txt")["stderr"], "rm: cannot remove 'c.txt': No such file\n");
        assert!(sh(&h, "touch t.txt && stat t.txt")["stdout"].as_str().unwrap().starts_with("{\"ok\":true,\"exists\":true,\"path\":\"/workspace/t.txt\",\"kind\":\"file\",\"size\":0,"));
        let ls = sh(&h, "ls /workspace");
        assert!(ls["stdout"].as_str().unwrap().contains("/workspace/a.txt\n"));
        assert_eq!(sh(&h, "find /workspace")["stdout"], ls["stdout"]);
        assert_eq!(sh(&h, "echo x > /etc/secrets/k")["exitCode"], 126);
        assert_eq!(sh(&h, "mkdir /etc/secrets/d")["exitCode"], 126);
        assert_eq!(sh(&h, "cat")["stderr"], "cat: missing file operand\n");
        // `;` keeps going after a failure, `&&` stops
        assert_eq!(sh(&h, "cat nope; echo after")["stdout"], "after\n");
        assert_eq!(sh(&h, "cat nope && echo after")["stdout"], "");
        let (_, env) = (0, post(&h, "/vfs/exec", json!({"command": "env", "env": {"FOO": "bar", "PWD": "/x"}, "principal": "agent"})).1);
        assert_eq!(env["stdout"], "FW_ENV=production\nPWD=/x\nUSER=agent\nFOO=bar\n");
    }

    #[test]
    fn exec_tier_selection() {
        let t = |v: Value| select_exec_tier(&ExecReq::from_value(v.as_object().unwrap(), "default"));
        assert_eq!(t(json!({"command": "echo hi"})), 0);
        assert_eq!(t(json!({"tier": "auto", "code": "return 1;"})), 1);
        assert_eq!(t(json!({"command": "JS:return 1"})), 1);
        assert_eq!(t(json!({"tier": "auto", "argv": ["python3", "-c", "print(6*7)"]})), 2);
        assert_eq!(t(json!({"command": "git status"})), 2);
        assert_eq!(t(json!({"command": "/bin/sh -c x"})), 2);
        assert_eq!(t(json!({"tier": 1, "command": "git status"})), 1);
        assert_eq!(t(json!({"code": "  "})), 0);
    }

    #[test]
    fn tier1_loader_and_simulated_fallback() {
        let mut h = TestHost::new("tiers-tenant");
        let sim = post(&h, "/vfs/exec", json!({"tier": 1, "code": "return 1;"})).1;
        assert_eq!((sim["ok"].clone(), sim["simulated"].clone(), sim["bindingRequired"].clone()), (json!(true), json!(true), json!("LOADER")));
        assert_eq!(sim["globalOutbound"], Value::Null);
        h.loader = true;
        let r = post(&h, "/vfs/exec", json!({"tier": "auto", "code": "return 21 * 2;"})).1;
        assert_eq!(r["tier"], 1);
        assert_eq!(r["runtime"], "dynamic-worker-loader");
        assert_eq!(r["isolated"], true);
        assert_eq!(r["globalOutbound"], Value::Null);
        assert_eq!(r["result"]["value"], 42);
        assert_eq!(r["stdout"], "{\"ok\":true,\"value\":42}");
    }

    #[test]
    fn tier2_sandbox_and_simulated_fallback() {
        let mut h = TestHost::new("tiers-tenant");
        let sim = post(&h, "/vfs/exec", json!({"tier": "auto", "argv": ["python3", "-c", "print(6*7)"]})).1;
        assert_eq!((sim["simulated"].clone(), sim["bindingRequired"].clone()), (json!(true), json!("SANDBOX")));
        assert_eq!(post(&h, "/vfs/exec", json!({"tier": 2, "command": "ls -la"})).1["argv"], json!(["/bin/sh", "-lc", "ls -la"]));
        h.sandbox = true;
        let r = post(&h, "/vfs/exec", json!({"tier": "auto", "argv": ["python3", "-c", "print(6*7)"]})).1;
        assert_eq!((r["ok"].clone(), r["tier"].clone(), r["runtime"].clone()), (json!(true), json!(2), json!("cloudflare-sandbox-microvm")));
        assert_eq!(r["stdout"], "sandbox-ran:python3 -c print(6*7)");
        assert_eq!(h.sandbox_calls.borrow().len(), 1);
    }

    #[test]
    fn learn_log_sinks_list_and_status() {
        let mut h = TestHost::new("prod-tenant");
        h.sinks = Sinks { k2: true, basin: true, d1: true };
        let (s, body) = post(
            &h,
            "/api/learn/events",
            json!({"kind": "recovery", "source": "fw-edge-test", "summary": "Promoted verified Cloudflare Edge OS topology", "tags": ["cloudflare", "k2", 7], "payload": {"tier": 0}}),
        );
        assert_eq!(s, 201);
        assert_eq!(body["storage"], "sqlite");
        assert_eq!(body["forwarded"], json!({"k2": true, "basinPipeline": true, "d1": true}));
        assert_eq!(body["event"]["tags"], json!(["cloudflare", "k2", "7"]));
        assert_eq!(h.forwarded.borrow().len(), 1);
        let defaults = post(&h, "/api/learn/events", json!({})).1;
        assert_eq!(defaults["event"]["kind"], "observation");
        assert_eq!(defaults["event"]["source"], "fw-edge");
        assert_eq!(defaults["event"]["summary"], "");

        let (_, list) = call(&h, "GET", "/api/learn/events", "default", &[("limit", "1")], None);
        assert_eq!((list["storage"].clone(), list["count"].clone()), (json!("sqlite"), json!(1)));
        let (_, all) = call(&h, "GET", "/api/learn/events", "default", &[], None);
        assert_eq!(all["count"], 2);
        let summaries: Vec<&str> = all["events"].as_array().unwrap().iter().map(|e| e["summary"].as_str().unwrap()).collect();
        assert!(summaries.iter().any(|s| s.contains("Cloudflare Edge OS")));
        let (_, status) = call(&h, "GET", "/api/learn/status", "default", &[], None);
        assert_eq!((status["ok"].clone(), status["storageBackend"].clone(), status["totalEvents"].clone()), (json!(true), json!("sqlite"), json!(2)));
        assert_eq!(status["byKind"]["recovery"], 1);
        assert_eq!(status["tenant"], "prod-tenant");
        assert_eq!(status["sinks"], json!({"sqlite": true, "k2": true, "basinPipeline": true, "d1": true}));
        assert_eq!(status["execAuditCount"], 0);
    }

    #[test]
    fn learn_write_can_be_denied() {
        let h = TestHost::new("t");
        let (s, _) = post(
            &h,
            "/gatekeeper/policies",
            json!({"id": "deny-learn", "capability": "learn:write", "effect": "deny", "principal": "*"}),
        );
        assert_eq!(s, 200);
        let (s, body) = post(&h, "/api/learn/events", json!({"summary": "x"}));
        assert_eq!((s, body["denied"].clone()), (403, json!(true)));
    }

    #[test]
    fn exec_audit_truncates_long_commands() {
        let h = TestHost::new("t");
        let long = format!("echo {}", "a".repeat(600));
        sh(&h, &long);
        let rows = h.sql_query("SELECT command FROM exec_audit", &[]).unwrap();
        assert_eq!(rows[0][0].text().len(), 512);
    }

    #[test]
    fn unknown_do_route_is_404_json() {
        let h = TestHost::new("t");
        let (s, b) = call(&h, "GET", "/nope", "default", &[], None);
        assert_eq!((s, b), (404, json!({"ok": false, "error": "DO route not found", "path": "/nope"})));
    }

    #[test]
    fn tenant_resolution_prefers_header_then_query() {
        assert_eq!(resolve_tenant(Some(" a "), Some("b")), "a");
        assert_eq!(resolve_tenant(Some("  "), Some("b")), "b");
        assert_eq!(resolve_tenant(None, None), "default");
    }

    #[test]
    fn js_helpers() {
        assert_eq!(js_string(&json!(3)), "3");
        assert_eq!(js_string(&json!(1.5)), "1.5");
        assert_eq!(js_string(&json!(null)), "null");
        assert_eq!(js_string(&json!([1, null, "a"])), "1,,a");
        assert_eq!(js_string(&json!({})), "[object Object]");
        assert!(!truthy(&json!("")) && truthy(&json!("x")) && !truthy(&json!(0)) && truthy(&json!({})));
        assert_eq!(num_value(131072.0), json!(131072));
        assert_eq!(num_value(0.5).to_string(), "0.5");
    }
}
