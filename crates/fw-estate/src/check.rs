use crate::cloudflare::Live;
use crate::model::{Estate, Record};
use crate::plan::{live_record, Plan};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub id: &'static str,
    pub error: bool,
    pub subject: String,
    pub message: String,
}

fn finding(id: &'static str, error: bool, subject: impl Into<String>, message: impl Into<String>) -> Finding {
    Finding { id, error, subject: subject.into(), message: message.into() }
}

fn host_of(pattern: &str) -> &str {
    pattern.split('/').next().unwrap_or(pattern)
}

/// `*.example.com` covers any deeper name; any other pattern covers only itself.
fn host_matches(pattern: &str, host: &str) -> bool {
    match pattern.strip_prefix("*.") {
        Some(suffix) => host.ends_with(&format!(".{suffix}")),
        None => pattern == host,
    }
}

fn is_private_target(content: &str) -> bool {
    let host = content.split_whitespace().last().unwrap_or(content).trim_end_matches('.');
    let private_name = [".mesh", ".internal", ".local", ".lan"].iter().any(|suffix| host.ends_with(suffix));
    let octets: Vec<u8> = host.split('.').filter_map(|part| part.parse().ok()).collect();
    let private_ip = match octets[..] {
        [10, ..] | [192, 168, ..] | [127, ..] => true,
        [172, second, ..] => (16..32).contains(&second),
        [100, second, ..] => (64..128).contains(&second),
        _ => false,
    };
    private_name || private_ip
}

pub fn run(estate: &Estate, live: &Live, plan: &Plan, cluster_hosts: &BTreeSet<String>) -> Vec<Finding> {
    let mut findings = Vec::new();
    let routes: Vec<(String, String)> = live.zones.iter().flat_map(|zone| zone.routes.iter()).filter_map(|row| Some((row["pattern"].as_str()?.to_string(), row["script"].as_str().unwrap_or("").to_string()))).collect();
    let domains: BTreeMap<String, String> = live.worker_domains.iter().filter_map(|row| Some((row["hostname"].as_str()?.to_string(), row["service"].as_str().unwrap_or("").to_string()))).collect();
    let ingress: Vec<(String, String, String)> = live
        .tunnel_ingress
        .iter()
        .flat_map(|(id, rules)| {
            let tunnel = live.tunnel_name(id).unwrap_or_else(|| id.clone());
            rules.iter().filter_map(move |rule| Some((tunnel.clone(), rule["hostname"].as_str()?.to_string(), rule["service"].as_str().unwrap_or("").to_string())))
        })
        .collect();

    for zone in &live.zones {
        let records: Vec<(String, Record)> = zone.records.iter().filter_map(live_record).collect();
        for (_, record) in &records {
            if record.name.starts_with("*.") {
                findings.push(finding("dns.wildcard", false, &record.name, format!("{} wildcard record: every unrouted subdomain resolves", record.kind)));
            }
            if is_private_target(&record.content) {
                findings.push(finding("dns.private-target", true, format!("{} {}", record.kind, record.name), format!("public zone points at a private name or address ({})", record.content)));
            }
            if !record.proxied || !matches!(record.kind.as_str(), "A" | "AAAA" | "CNAME") {
                continue;
            }
            let routed = routes.iter().any(|(pattern, _)| host_matches(host_of(pattern), &record.name));
            let domain = domains.contains_key(&record.name);
            let tunnel_target = record.content.strip_suffix(".cfargotunnel.com");
            let ingress_hit = ingress.iter().find(|(_, host, _)| host_matches(host, &record.name));
            if let Some(id) = tunnel_target
                && live.tunnel_name(id).is_none()
            {
                findings.push(finding("dns.dead-origin", true, &record.name, format!("CNAME to tunnel {id}, which does not exist")));
                continue;
            }
            let served = match (record.content.as_str(), tunnel_target) {
                ("100::", _) => routed || domain,
                (_, Some(_)) => routed || domain || ingress_hit.is_some(),
                _ => true,
            };
            if !served {
                let why = if tunnel_target.is_some() { "tunnel ingress has no rule for it (catch-all 404), no Worker route or domain" } else { "no Worker route or custom domain: Cloudflare answers 522" };
                findings.push(finding("dns.dead-origin", true, &record.name, why));
            }
        }
        if !records.iter().any(|(_, record)| record.kind == "CAA" && record.name == zone.name) {
            findings.push(finding("dns.caa", false, &zone.name, "no CAA record at the apex"));
        }
        let has_mail = records.iter().any(|(_, record)| record.kind == "MX");
        if has_mail {
            for (needle, id, label) in [("v=spf1", "dns.spf", "SPF"), ("v=DMARC1", "dns.dmarc", "DMARC")] {
                if !records.iter().any(|(_, record)| record.kind == "TXT" && record.content.contains(needle)) {
                    findings.push(finding(id, true, &zone.name, format!("zone receives mail but has no {label} record")));
                }
            }
        }
        match zone.dnssec.as_str() {
            "active" => {}
            "pending" => findings.push(finding("zone.dnssec", true, &zone.name, "DNSSEC is pending: the DS record is not yet published at the registrar")),
            state => findings.push(finding("zone.dnssec", true, &zone.name, format!("DNSSEC is {state}"))),
        }
        if zone.status != "active" {
            findings.push(finding("zone.status", true, &zone.name, format!("zone status is {}: nameservers are not delegated to Cloudflare", zone.status)));
        }
        let hsts = &zone.settings["security_header"]["strict_transport_security"];
        if hsts["enabled"] != Value::Bool(true) || hsts["include_subdomains"] != Value::Bool(true) || hsts["preload"] != Value::Bool(true) {
            findings.push(finding("zone.hsts", false, &zone.name, "HSTS is not enabled with includeSubDomains and preload"));
        }
        if !estate.zone.contains_key(&zone.name) {
            findings.push(finding("estate.undeclared-zone", true, &zone.name, "zone exists in the account but not in the estate file"));
        }
    }

    for (host, script) in &domains {
        for (pattern, route_script) in &routes {
            if host_of(pattern) == host && route_script != script {
                findings.push(finding("route.double-owner", true, host, format!("custom domain -> {script} and route {pattern} -> {route_script}")));
            }
        }
    }
    let mut owners: BTreeMap<&str, &str> = BTreeMap::new();
    for (name, service) in &estate.service {
        for hostname in &service.hostnames {
            if let Some(previous) = owners.insert(hostname, name) {
                findings.push(finding("estate.double-owner", true, hostname, format!("claimed by services {previous} and {name}")));
            }
        }
        for host in &service.selfhosted_hosts {
            if !cluster_hosts.contains(host) {
                findings.push(finding("estate.unknown-host", true, name, format!("self-hosted host {host} is not in the cluster file")));
            }
        }
    }

    let bound_services: BTreeSet<String> = live.worker_settings.values().flat_map(|settings| settings["bindings"].as_array().into_iter().flatten()).filter(|binding| binding["type"] == "service").filter_map(|binding| binding["service"].as_str().map(str::to_string)).collect();
    let d1: BTreeSet<&str> = live.d1.iter().filter_map(|row| row["uuid"].as_str()).collect();
    let kv: BTreeSet<&str> = live.kv.iter().filter_map(|row| row["id"].as_str()).collect();
    let queues: BTreeSet<&str> = live.queues.iter().filter_map(|row| row["queue_name"].as_str()).collect();
    let buckets: BTreeSet<&str> = live.buckets.iter().filter_map(|row| row["name"].as_str()).collect();
    let worker_names: BTreeSet<&str> = live.workers.iter().filter_map(|row| row["id"].as_str()).collect();
    for (name, settings) in &live.worker_settings {
        if !estate.worker.contains_key(name) && !estate.retired.worker.contains_key(name) {
            findings.push(finding("estate.undeclared-worker", false, name, "Worker exists but is not declared in the estate file"));
        }
        if settings["observability"]["enabled"] != Value::Bool(true) {
            findings.push(finding("worker.observability", false, name, "observability is off"));
        }
        match settings["compatibility_date"].as_str() {
            Some(date) if age_days(date) > u64::from(estate.estate.compat_max_age_days) => findings.push(finding("worker.compat-date", false, name, format!("compatibility_date {date} is older than {} days", estate.estate.compat_max_age_days))),
            None => findings.push(finding("worker.compat-date", false, name, "no compatibility_date")),
            _ => {}
        }
        for binding in settings["bindings"].as_array().into_iter().flatten() {
            let label = binding["name"].as_str().unwrap_or("?");
            let missing = match binding["type"].as_str().unwrap_or("") {
                "r2_bucket" => binding["bucket_name"].as_str().filter(|bucket| !buckets.contains(bucket)).map(str::to_string),
                "d1" => binding["id"].as_str().filter(|id| !d1.contains(id)).map(|id| format!("D1 {id}")),
                "kv_namespace" => binding["namespace_id"].as_str().filter(|id| !kv.contains(id)).map(|id| format!("KV {id}")),
                "queue" => binding["queue_name"].as_str().filter(|queue| !queues.contains(queue)).map(str::to_string),
                "service" => binding["service"].as_str().filter(|service| !worker_names.contains(service)).map(str::to_string),
                _ => None,
            };
            if let Some(target) = missing {
                findings.push(finding("worker.dangling-binding", true, name, format!("binding {label} points at {target}, which does not exist")));
            }
        }
        let routed = routes.iter().any(|(_, script)| script == name) || domains.values().any(|script| script == name);
        let has_cron = settings["schedules"].as_array().is_some_and(|schedules| !schedules.is_empty());
        let requests = live.requests_14d.get(name).copied().unwrap_or((0, 0));
        if !routed && !bound_services.contains(name) && !has_cron && requests.0 == 0 {
            findings.push(finding("worker.unreferenced", false, name, "no route, custom domain or service binding, and no request in 14 days"));
        }
        if requests.0 >= 1000 && requests.1 * 20 > requests.0 {
            findings.push(finding("worker.errors", true, name, format!("{} errors in {} requests over 14 days", requests.1, requests.0)));
        }
    }
    for name in estate.worker.keys() {
        if !live.worker_settings.contains_key(name) {
            findings.push(finding("estate.missing-worker", true, name, "declared Worker is not deployed"));
        }
    }

    for bucket in &live.buckets {
        let name = bucket["name"].as_str().unwrap_or_default();
        if !estate.bucket.contains_key(name) {
            findings.push(finding("estate.undeclared-bucket", false, name, "R2 bucket exists but is not declared in the estate file"));
        }
        let has_abort = live.bucket_lifecycle.get(name).and_then(|lifecycle| lifecycle["rules"].as_array()).is_some_and(|rules| rules.iter().any(|rule| rule.get("abortMultipartUploadsTransition").is_some()));
        if !has_abort {
            findings.push(finding("r2.abort-multipart", false, name, "no lifecycle rule aborting incomplete multipart uploads"));
        }
    }
    for token in &live.tokens {
        if token["expires_on"].is_null() {
            findings.push(finding("token.no-expiry", false, token["name"].as_str().unwrap_or("?"), "API token never expires"));
        }
    }
    for app in &live.access_apps {
        let domain = app["domain"].as_str().map(host_of).unwrap_or_default();
        let known = live.zones.iter().any(|zone| zone.records.iter().any(|row| row["name"].as_str() == Some(domain)));
        if !domain.is_empty() && !domain.ends_with("cloudflareaccess.com") && !known {
            findings.push(finding("access.no-dns", false, domain, format!("Access application {} has no DNS record", app["name"].as_str().unwrap_or("?"))));
        }
    }
    for action in &plan.actions {
        findings.push(finding("drift", false, format!("{} {} {}", action.op, action.kind, action.target), action.detail.clone()));
    }
    findings
}

/// Whole days between a `YYYY-MM-DD` date and today; 0 when the date does not parse.
fn age_days(date: &str) -> u64 {
    let parts: Vec<i64> = date.split('-').filter_map(|part| part.parse().ok()).collect();
    let [year, month, day] = parts[..] else { return 0 };
    let shifted_year = if month <= 2 { year - 1 } else { year };
    let era = shifted_year.div_euclid(400);
    let year_of_era = shifted_year - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    let today = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|elapsed| (elapsed.as_secs() / 86_400) as i64).unwrap_or(0);
    u64::try_from(today - days).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_patterns_cover_deeper_names_only() {
        assert!(host_matches("*.a.com", "x.a.com"));
        assert!(!host_matches("*.a.com", "a.com"));
        assert!(host_matches("a.com", "a.com"));
    }

    #[test]
    fn private_targets_are_detected() {
        assert!(is_private_target("10 50061 fr1.mesh"));
        assert!(is_private_target("10.200.77.1"));
        assert!(is_private_target("172.20.0.1"));
        assert!(!is_private_target("172.32.0.1"));
        assert!(!is_private_target("mx1.mail.ovh.net"));
    }

    #[test]
    fn age_of_a_known_date() {
        assert!(age_days("1970-01-02") > 20_000);
        assert_eq!(age_days("garbage"), 0);
    }
}
