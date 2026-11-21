#![forbid(unsafe_code)]

//! `fw-crates`: crate intelligence over crates.io. The dataset comes from the daily database dump (`sync`), the
//! RustSec advisory-db and the sparse index; `call` is the one entry point the CLI, `fw serve`, MCP and the Bun
//! plugin share. Results are JSON values with an `op` field, rendered to text or MDX by `render`.
//!
//! Ranking: `rank.rs` (documented in `docs/crates/RANKING.md`, generated from the same constants).
//! Benchmarks: `bench.rs`. Project skeletons: `stack.rs` with `stack.toml`.

pub mod awesome;
pub mod bench;
pub mod http;
pub mod model;
pub mod query;
pub mod rank;
pub mod render;
pub mod stack;
pub mod store;
pub mod time;

#[cfg(feature = "sync")]
pub mod advisories;
#[cfg(feature = "sync")]
pub mod sync;

use serde_json::{Value, json};

fn str_arg<'a>(args: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| args.get(*k).and_then(Value::as_str)).map(str::trim).filter(|s| !s.is_empty())
}

fn limit(args: &Value, default: usize) -> usize {
    args.get("limit").and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))).map_or(default, |n| usize::try_from(n).unwrap_or(default)).clamp(1, 200)
}

fn names(args: &Value) -> Vec<String> {
    match args.get("names").or_else(|| args.get("crates")) {
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(str::to_owned).collect(),
        Some(Value::String(s)) => s.split([',', ' ']).filter(|n| !n.is_empty()).map(str::to_owned).collect(),
        _ => Vec::new(),
    }
}

fn flag(args: &Value, key: &str, default: bool) -> bool {
    match args.get(key) {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => matches!(s.as_str(), "1" | "true" | "yes"),
        _ => default,
    }
}

/// The operations exposed over HTTP and MCP: `search`, `info`, `compare`, `rank`, `awesome`, `stack`.
/// `args` keys: `query`, `category`, `limit`, `name`, `names`, `goal`, `live`.
pub fn call(op: &str, args: &Value) -> Result<Value, String> {
    let ds = store::load()?;
    match op {
        "search" => query::search(&ds, str_arg(args, &["query", "q"]).unwrap_or(""), str_arg(args, &["category"]), limit(args, 20)),
        "info" => query::info(&ds, str_arg(args, &["name", "crate"]).ok_or("missing `name`")?, flag(args, "live", false)),
        "compare" => query::compare(&ds, &names(args), flag(args, "live", false)),
        "rank" => query::rank(&ds, str_arg(args, &["category"]), limit(args, 20)),
        "awesome" => query::awesome(&ds, str_arg(args, &["category"]), limit(args, awesome::PER_CATEGORY)),
        "stack" => stack::generate(&ds, str_arg(args, &["goal"]).ok_or("missing `goal`")?),
        other => Err(format!("unknown crates operation `{other}` (search, info, compare, rank, awesome, stack)")),
    }
}

/// MCP tool descriptors for the operations `call` serves: `(name, description, input schema)`.
pub fn mcp_tools() -> Vec<(&'static str, &'static str, Value)> {
    let obj = |props: Value, required: &[&str]| json!({ "type": "object", "properties": props, "required": required, "additionalProperties": false });
    vec![
        ("crates_search", "Search the ranked crates.io dataset by words (name, keywords, categories, description); results ordered by relevance then score.", obj(json!({ "query": { "type": "string" }, "category": { "type": "string", "description": "crates.io category slug or unique part of it" }, "limit": { "type": "integer" } }), &["query"])),
        ("crates_info", "One crate: score with its components, downloads, dependents, releases, MSRV, licence, advisories, rank overall and per category; live=true adds the live sparse-index record.", obj(json!({ "name": { "type": "string" }, "live": { "type": "boolean" } }), &["name"])),
        ("crates_compare", "Compare two or more crates side by side with the best crate per metric.", obj(json!({ "names": { "type": "array", "items": { "type": "string" } }, "live": { "type": "boolean" } }), &["names"])),
        ("crates_awesome", "The ranked best crates of a crates.io category, or the index of every awesome list when no category is given.", obj(json!({ "category": { "type": "string" }, "limit": { "type": "integer" } }), &[])),
        ("crates_stack", "Pick crates from the ranking for a goal in words (for example `web API with Postgres and auth`) and return the choices with reasons and a Cargo project skeleton.", obj(json!({ "goal": { "type": "string" } }), &["goal"])),
    ]
}

/// Runs an MCP tool by name; `None` when the name is not a crates tool.
pub fn call_mcp_tool(name: &str, args: &Value) -> Option<Result<Value, String>> {
    let op = name.strip_prefix("crates_")?;
    matches!(op, "search" | "info" | "compare" | "awesome" | "stack").then(|| call(op, args))
}

/// Serves `GET /v1/crates/<op>[/<arg>]?<query>` for `fw serve`. Returns `(status, body)`.
pub fn http(path_and_query: &str) -> (u16, String) {
    let (path, qs) = path_and_query.split_once('?').unwrap_or((path_and_query, ""));
    let mut parts = path.trim_start_matches("/v1/crates").trim_matches('/').splitn(2, '/');
    let op = parts.next().unwrap_or("");
    let rest = parts.next().unwrap_or("");
    let mut args = serde_json::Map::new();
    for pair in qs.split('&').filter(|p| !p.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        args.insert(url_decode(k), Value::String(url_decode(v)));
    }
    if !rest.is_empty() {
        let key = match op {
            "info" => "name",
            "awesome" => "category",
            "compare" => "names",
            _ => "query",
        };
        args.insert(key.to_owned(), Value::String(url_decode(rest).replace('/', if op == "compare" { "," } else { "/" })));
    }
    match call(op, &Value::Object(args)) {
        Ok(v) => (200, v.to_string()),
        Err(e) => (if e.starts_with("unknown crates operation") || e.starts_with("no crates dataset") { 404 } else { 400 }, json!({ "ok": false, "error": e }).to_string()),
    }
}

fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                if let Some(v) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    out.push(v);
                    i += 2;
                } else {
                    out.push(b'%');
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode() {
        assert_eq!(url_decode("web+API%20with%2Fx"), "web API with/x");
        assert_eq!(url_decode("100%"), "100%");
        assert_eq!(url_decode("%zz"), "%zz");
    }

    #[test]
    fn mcp_names() {
        assert_eq!(mcp_tools().len(), 5);
        assert!(call_mcp_tool("fw_status", &json!({})).is_none());
        assert!(call_mcp_tool("crates_bogus", &json!({})).is_none());
    }
}
