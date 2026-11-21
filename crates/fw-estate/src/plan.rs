use crate::cloudflare::{Cloudflare, Live, ZoneLive};
use crate::model::{normalize_content, parse_route, Estate, Record};
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

const MANAGED_KINDS: [&str; 7] = ["A", "AAAA", "CNAME", "TXT", "MX", "SRV", "CAA"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Action {
    pub op: String,
    pub kind: String,
    pub scope: String,
    pub target: String,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub schema: u32,
    pub estate_sha256: String,
    pub actions: Vec<Action>,
}

fn action(op: &str, kind: &str, scope: &str, target: &str, detail: String, id: Option<String>) -> Action {
    Action { op: op.into(), kind: kind.into(), scope: scope.into(), target: target.into(), detail, id }
}

pub fn estate_sha256(text: &str) -> String {
    Sha256::digest(text.as_bytes()).iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn compute(estate: &Estate, estate_text: &str, live: &Live) -> Result<Plan> {
    let mut actions = Vec::new();
    for (zone_name, declared) in &estate.zone {
        let zone = live.zones.iter().find(|zone| &zone.name == zone_name).ok_or_else(|| anyhow!("zone {zone_name} is declared but not in the account"))?;
        dns_actions(zone_name, &declared.record, zone, live, &mut actions)?;
        route_actions(zone_name, &declared.route, zone, &mut actions)?;
        domain_actions(zone_name, &declared.domain, zone, live, &mut actions)?;
        setting_actions(zone_name, &estate.zone_settings(zone_name), zone, &mut actions);
        if declared.dnssec && zone.dnssec == "disabled" {
            actions.push(action("update", "dnssec", zone_name, zone_name, "disabled -> active".into(), Some(zone.id.clone())));
        }
    }
    for (name, reason) in &estate.retired.worker {
        if let Some(settings) = live.worker_settings.get(name) {
            let holds_objects = settings["bindings"].as_array().into_iter().flatten().any(|binding| binding["type"] == "durable_object_namespace");
            if holds_objects {
                bail!("retired worker {name} owns a Durable Object namespace: deleting it would delete the objects; list them in the plan first");
            }
            actions.push(action("delete", "worker", "account", name, reason.clone(), None));
        }
    }
    let live_buckets: BTreeSet<&str> = live.buckets.iter().filter_map(|bucket| bucket["name"].as_str()).collect();
    for (name, bucket) in &estate.bucket {
        if !live_buckets.contains(name.as_str()) {
            actions.push(action("create", "bucket", "account", name, format!("tier {}", bucket.tier), None));
            continue;
        }
        if let Some(days) = bucket.abort_multipart_days {
            let has_abort = live.bucket_lifecycle.get(name).and_then(|lifecycle| lifecycle["rules"].as_array()).is_some_and(|rules| rules.iter().any(|rule| rule.get("abortMultipartUploadsTransition").is_some()));
            if !has_abort {
                actions.push(action("update", "bucket-lifecycle", "account", name, format!("abort incomplete multipart uploads after {days} days"), None));
            }
        }
    }
    let rank = |op: &str| match op {
        "delete" => 0,
        "update" => 1,
        _ => 2,
    };
    actions.sort_by(|left, right| (rank(&left.op), &left.kind, &left.scope, &left.target, &left.detail).cmp(&(rank(&right.op), &right.kind, &right.scope, &right.target, &right.detail)));
    Ok(Plan { schema: 1, estate_sha256: estate_sha256(estate_text), actions })
}

pub fn live_record(row: &Value) -> Option<(String, Record)> {
    let kind = row["type"].as_str()?.to_string();
    if !MANAGED_KINDS.contains(&kind.as_str()) {
        return None;
    }
    let name = row["name"].as_str()?.to_string();
    let record = Record { kind, name, content: row["content"].as_str()?.to_string(), proxied: row["proxied"].as_bool().unwrap_or(false), ttl: row["ttl"].as_u64().unwrap_or(1) as u32, priority: row["priority"].as_u64().map(|value| value as u16) };
    Some((row["id"].as_str()?.to_string(), record))
}

/// `tunnel:<name>` becomes `<id>.cfargotunnel.com`, `worker` becomes the placeholder address a Worker-only hostname points at.
fn resolve_content(record: &mut Record, live: &Live) -> Result<()> {
    if let Some(name) = record.content.strip_prefix("tunnel:") {
        let id = live.tunnel_id(name).ok_or_else(|| anyhow!("record {} points at unknown tunnel {name}", record.name))?;
        record.content = format!("{id}.cfargotunnel.com");
    } else if record.content == "worker" {
        record.content = "100::".to_string();
    }
    Ok(())
}

fn dns_actions(zone_name: &str, lines: &[String], zone: &ZoneLive, live: &Live, actions: &mut Vec<Action>) -> Result<()> {
    let mut declared = Vec::new();
    for line in lines {
        let mut record = Record::parse(zone_name, line)?;
        resolve_content(&mut record, live)?;
        declared.push(record);
    }
    let mut remaining: Vec<(String, Record)> = zone.records.iter().filter_map(|row| live_record(row)).collect();
    let mut unmatched = Vec::new();
    for record in declared {
        match remaining.iter().position(|(_, existing)| existing.identity() == record.identity()) {
            Some(index) => {
                let (id, existing) = remaining.remove(index);
                if existing.proxied != record.proxied || existing.ttl != record.ttl {
                    actions.push(action("update", "dns", zone_name, &format!("{} {}", record.kind, record.name), format!("proxied {} -> {}, ttl {} -> {}", existing.proxied, record.proxied, existing.ttl, record.ttl), Some(id)));
                }
            }
            None => unmatched.push(record),
        }
    }
    for record in unmatched {
        if record.single_valued() {
            if let Some(index) = remaining.iter().position(|(_, existing)| existing.kind == record.kind && existing.name == record.name) {
                let (id, existing) = remaining.remove(index);
                actions.push(action("update", "dns", zone_name, &format!("{} {}", record.kind, record.name), format!("content {} -> {}", existing.content, record.content), Some(id)));
                continue;
            }
        }
        actions.push(action("create", "dns", zone_name, &format!("{} {}", record.kind, record.name), record.to_line(zone_name), None));
    }
    for (id, record) in remaining {
        actions.push(action("delete", "dns", zone_name, &format!("{} {}", record.kind, record.name), format!("{} (proxied {})", record.content, record.proxied), Some(id)));
    }
    Ok(())
}

fn route_actions(zone_name: &str, lines: &[String], zone: &ZoneLive, actions: &mut Vec<Action>) -> Result<()> {
    let mut live_routes: BTreeMap<String, (String, String)> = zone.routes.iter().filter_map(|row| Some((row["pattern"].as_str()?.to_string(), (row["id"].as_str()?.to_string(), row["script"].as_str().unwrap_or("").to_string())))).collect();
    for line in lines {
        let (pattern, script) = parse_route(line)?;
        match live_routes.remove(&pattern) {
            None => actions.push(action("create", "route", zone_name, &pattern, script, None)),
            Some((id, current)) if current != script => actions.push(action("update", "route", zone_name, &pattern, format!("{current} -> {script}"), Some(id))),
            Some(_) => {}
        }
    }
    for (pattern, (id, script)) in live_routes {
        actions.push(action("delete", "route", zone_name, &pattern, script, Some(id)));
    }
    Ok(())
}

fn domain_actions(zone_name: &str, lines: &[String], zone: &ZoneLive, live: &Live, actions: &mut Vec<Action>) -> Result<()> {
    let mut live_domains: BTreeMap<String, (String, String)> = live
        .worker_domains
        .iter()
        .filter(|row| row["zone_id"].as_str() == Some(zone.id.as_str()))
        .filter_map(|row| Some((row["hostname"].as_str()?.to_string(), (row["id"].as_str()?.to_string(), row["service"].as_str().unwrap_or("").to_string()))))
        .collect();
    for line in lines {
        let (hostname, script) = parse_route(line)?;
        match live_domains.remove(&hostname) {
            None => actions.push(action("create", "domain", zone_name, &hostname, script, None)),
            Some((_, current)) if current != script => actions.push(action("update", "domain", zone_name, &hostname, format!("{current} -> {script}"), None)),
            Some(_) => {}
        }
    }
    for (hostname, (id, script)) in live_domains {
        actions.push(action("delete", "domain", zone_name, &hostname, script, Some(id)));
    }
    Ok(())
}

/// Every key of the declared value must be present with the same value in the live one.
fn contains(live: &Value, declared: &Value) -> bool {
    match (live, declared) {
        (Value::Object(live_map), Value::Object(declared_map)) => declared_map.iter().all(|(key, value)| live_map.get(key).is_some_and(|candidate| contains(candidate, value))),
        _ => live == declared,
    }
}

fn setting_actions(zone_name: &str, declared: &BTreeMap<String, toml::Value>, zone: &ZoneLive, actions: &mut Vec<Action>) {
    for (key, value) in declared {
        let Ok(wanted) = serde_json::to_value(value) else { continue };
        let current = zone.settings.get(key).cloned().unwrap_or(Value::Null);
        if !contains(&current, &wanted) {
            actions.push(action("update", "setting", zone_name, key, format!("{current} -> {wanted}"), Some(zone.id.clone())));
        }
    }
}

/// The plan only applies when it is the committed one: tracked, unmodified, and equal to a fresh computation.
pub fn require_committed(plan_path: &Path, committed: &Plan, fresh: &Plan) -> Result<()> {
    let directory = plan_path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    let file_name = plan_path.file_name().ok_or_else(|| anyhow!("plan path has no file name"))?;
    let tracked = Command::new("git").arg("-C").arg(directory).args(["ls-files", "--error-unmatch"]).arg(file_name).output().context("run git ls-files")?;
    if !tracked.status.success() {
        bail!("{} is not tracked by git: commit the plan before applying it", plan_path.display());
    }
    let unchanged = Command::new("git").arg("-C").arg(directory).args(["diff", "--quiet", "HEAD", "--"]).arg(file_name).status().context("run git diff")?;
    if !unchanged.success() {
        bail!("{} differs from HEAD: commit the plan before applying it", plan_path.display());
    }
    if committed != fresh {
        bail!("the committed plan no longer matches the estate file and the live state: run `fw estate plan`, commit it, then apply");
    }
    Ok(())
}

pub fn apply_action(cloudflare: &Cloudflare, estate: &Estate, live: &Live, item: &Action) -> Result<()> {
    let account = &cloudflare.account;
    let zone = live.zones.iter().find(|zone| zone.name == item.scope);
    let zone_id = zone.map(|zone| zone.id.as_str()).unwrap_or_default();
    match (item.kind.as_str(), item.op.as_str()) {
        ("dns", "delete") => {
            cloudflare.call("DELETE", &format!("/zones/{zone_id}/dns_records/{}", item.id.as_deref().unwrap_or_default()), None)?;
        }
        ("dns", "create") | ("dns", "update") => {
            let declared = estate.zone.get(&item.scope).ok_or_else(|| anyhow!("zone {} is not declared", item.scope))?;
            let (kind, name) = item.target.split_once(' ').ok_or_else(|| anyhow!("bad dns target {}", item.target))?;
            let mut candidates = Vec::new();
            for line in &declared.record {
                let mut record = Record::parse(&item.scope, line)?;
                resolve_content(&mut record, live)?;
                if record.kind == kind && record.name == name {
                    candidates.push(record);
                }
            }
            let record = candidates.into_iter().find(|record| item.op == "update" || record.to_line(&item.scope) == item.detail).ok_or_else(|| anyhow!("record {} not found in the estate file", item.target))?;
            let body = record_body(&record)?;
            match item.id.as_deref() {
                Some(id) => cloudflare.call("PATCH", &format!("/zones/{zone_id}/dns_records/{id}"), Some(&body))?,
                None => cloudflare.call("POST", &format!("/zones/{zone_id}/dns_records"), Some(&body))?,
            };
        }
        ("route", "delete") => {
            cloudflare.call("DELETE", &format!("/zones/{zone_id}/workers/routes/{}", item.id.as_deref().unwrap_or_default()), None)?;
        }
        ("route", "create") => {
            cloudflare.call("POST", &format!("/zones/{zone_id}/workers/routes"), Some(&json!({ "pattern": item.target, "script": item.detail })))?;
        }
        ("route", "update") => {
            let script = item.detail.rsplit("-> ").next().unwrap_or_default();
            cloudflare.call("PUT", &format!("/zones/{zone_id}/workers/routes/{}", item.id.as_deref().unwrap_or_default()), Some(&json!({ "pattern": item.target, "script": script })))?;
        }
        ("domain", "delete") => {
            cloudflare.call("DELETE", &format!("/accounts/{account}/workers/domains/{}", item.id.as_deref().unwrap_or_default()), None)?;
        }
        ("domain", "create") | ("domain", "update") => {
            let script = item.detail.rsplit("-> ").next().unwrap_or_default();
            cloudflare.call("PUT", &format!("/accounts/{account}/workers/domains"), Some(&json!({ "hostname": item.target, "service": script, "zone_id": zone_id, "environment": "production" })))?;
        }
        ("setting", "update") => {
            let wanted = estate.zone_settings(&item.scope).get(&item.target).and_then(|value| serde_json::to_value(value).ok()).unwrap_or(Value::Null);
            cloudflare.call("PATCH", &format!("/zones/{zone_id}/settings/{}", item.target), Some(&json!({ "value": wanted })))?;
        }
        ("dnssec", "update") => {
            cloudflare.call("PATCH", &format!("/zones/{zone_id}/dnssec"), Some(&json!({ "status": "active" })))?;
        }
        ("worker", "delete") => {
            cloudflare.call("DELETE", &format!("/accounts/{account}/workers/scripts/{}", item.target), None)?;
        }
        ("bucket", "create") => {
            cloudflare.call("POST", &format!("/accounts/{account}/r2/buckets"), Some(&json!({ "name": item.target })))?;
        }
        ("bucket-lifecycle", "update") => {
            let days = estate.bucket.get(&item.target).and_then(|bucket| bucket.abort_multipart_days).unwrap_or(1);
            let rule = json!({ "id": "abort-incomplete-multipart", "enabled": true, "conditions": { "prefix": "" }, "abortMultipartUploadsTransition": { "condition": { "type": "Age", "maxAge": u64::from(days) * 86_400 } } });
            let existing = live.bucket_lifecycle.get(&item.target).and_then(|lifecycle| lifecycle["rules"].as_array()).cloned().unwrap_or_default();
            let mut rules = existing;
            rules.push(rule);
            cloudflare.call("PUT", &format!("/accounts/{account}/r2/buckets/{}/lifecycle", item.target), Some(&json!({ "rules": rules })))?;
        }
        (kind, op) => bail!("no executor for {op} {kind}"),
    }
    Ok(())
}

fn record_body(record: &Record) -> Result<Value> {
    let mut body = json!({ "type": record.kind, "name": record.name, "proxied": record.proxied, "ttl": record.ttl });
    match record.kind.as_str() {
        "SRV" => {
            let parts: Vec<&str> = record.content.split_whitespace().collect();
            let [weight, port, target] = parts[..] else { bail!("SRV content `{}` is not `weight port target`", record.content) };
            body["data"] = json!({ "priority": record.priority.unwrap_or(0), "weight": weight.parse::<u16>()?, "port": port.parse::<u16>()?, "target": target });
        }
        "CAA" => {
            let mut parts = record.content.splitn(3, ' ');
            let (Some(flags), Some(tag), Some(value)) = (parts.next(), parts.next(), parts.next()) else { bail!("CAA content `{}` is not `flags tag \"value\"`", record.content) };
            body["data"] = json!({ "flags": flags.parse::<u8>()?, "tag": tag, "value": value.trim_matches('"') });
        }
        "MX" => {
            body["content"] = json!(record.content);
            body["priority"] = json!(record.priority.unwrap_or(10));
        }
        "TXT" => body["content"] = json!(normalize_content("TXT", &record.content)),
        _ => body["content"] = json!(record.content),
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zone_with(records: Vec<Value>) -> ZoneLive {
        ZoneLive { id: "z".into(), name: "a.com".into(), records, ..ZoneLive::default() }
    }

    fn row(id: &str, kind: &str, name: &str, content: &str, proxied: bool) -> Value {
        json!({ "id": id, "type": kind, "name": name, "content": content, "proxied": proxied, "ttl": 1 })
    }

    #[test]
    fn dns_diff_creates_updates_and_deletes() {
        let zone = zone_with(vec![row("1", "A", "x.a.com", "1.1.1.1", false), row("2", "AAAA", "gone.a.com", "100::", true), row("3", "A", "same.a.com", "2.2.2.2", true)]);
        let lines = vec!["A x | 3.3.3.3".to_string(), "A same proxied | 2.2.2.2".to_string(), "TXT @ | hello".to_string()];
        let mut actions = Vec::new();
        dns_actions("a.com", &lines, &zone, &Live::default(), &mut actions).expect("diff");
        let summary: Vec<(String, String)> = actions.iter().map(|item| (item.op.clone(), item.target.clone())).collect();
        assert!(summary.contains(&("update".into(), "A x.a.com".into())));
        assert!(summary.contains(&("delete".into(), "AAAA gone.a.com".into())));
        assert!(summary.contains(&("create".into(), "TXT a.com".into())));
        assert_eq!(actions.len(), 3);
    }

    #[test]
    fn matching_state_gives_an_empty_plan() {
        let zone = zone_with(vec![row("1", "TXT", "a.com", "\"v=spf1 -all\"", false)]);
        let mut actions = Vec::new();
        dns_actions("a.com", &["TXT @ | v=spf1 -all".to_string()], &zone, &Live::default(), &mut actions).expect("diff");
        assert!(actions.is_empty());
    }

    #[test]
    fn setting_comparison_is_a_subset_match() {
        let live = json!({ "strict_transport_security": { "enabled": true, "max_age": 63072000, "preload": true } });
        assert!(contains(&live, &json!({ "strict_transport_security": { "enabled": true } })));
        assert!(!contains(&live, &json!({ "strict_transport_security": { "max_age": 1 } })));
    }

    #[test]
    fn record_bodies_for_structured_types() {
        let caa = Record::parse("a.com", "CAA @ | 0 issue \"letsencrypt.org\"").expect("parse");
        assert_eq!(record_body(&caa).expect("body")["data"]["value"], "letsencrypt.org");
        let srv = Record::parse("a.com", "SRV _x._tcp prio=5 | 1 443 host.example.com").expect("parse");
        assert_eq!(record_body(&srv).expect("body")["data"]["port"], 443);
    }
}
