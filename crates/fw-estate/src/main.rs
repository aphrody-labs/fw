use anyhow::{anyhow, bail, Context, Result};
use fw_estate::check::{self, Finding};
use fw_estate::cloudflare::{Cloudflare, Live};
use fw_estate::model::{Estate, Record};
use fw_estate::plan::{self, Plan};
use fw_estate::providers::{self, Ovh};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const USAGE: &str = "fw estate <plan|apply|check|import|registrar-ds <zone>> [--estate cluster/estate.toml] [--providers]
  plan          diff the live providers against the estate file and write the plan file
  apply         execute the committed plan file (tracked, unmodified, equal to a fresh plan)
  check         best-practice findings; exit 1 when one is an error (--providers adds OVH and Hostinger)
  import        print zone sections (records, routes, domains) generated from live Cloudflare state
  registrar-ds  publish the Cloudflare DS record of a zone at the OVH registrar";

fn main() {
    if let Err(error) = run() {
        eprintln!("fw estate: {error:#}");
        std::process::exit(2);
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).ok_or_else(|| anyhow!(USAGE))?;
    let estate_path = args.iter().position(|arg| arg == "--estate").and_then(|index| args.get(index + 1)).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("cluster/estate.toml"));
    let with_providers = args.iter().any(|arg| arg == "--providers");
    let (estate, text) = Estate::load(&estate_path)?;
    let cloudflare = Cloudflare::from_env(&estate.estate.token_env, &estate.estate.account_env)?;
    let plan_path = estate_path.parent().unwrap_or_else(|| Path::new(".")).join(&estate.estate.plan_file);
    match command {
        "plan" => {
            let live = Live::load(&cloudflare)?;
            let fresh = plan::compute(&estate, &text, &live)?;
            print_plan(&fresh);
            std::fs::write(&plan_path, serde_json::to_string_pretty(&fresh)? + "\n").with_context(|| format!("write {}", plan_path.display()))?;
            println!("wrote {} ({} actions)", plan_path.display(), fresh.actions.len());
        }
        "apply" => {
            let committed: Plan = serde_json::from_str(&std::fs::read_to_string(&plan_path).with_context(|| format!("read {}", plan_path.display()))?)?;
            let live = Live::load(&cloudflare)?;
            let fresh = plan::compute(&estate, &text, &live)?;
            plan::require_committed(&plan_path, &committed, &fresh)?;
            for item in &fresh.actions {
                plan::apply_action(&cloudflare, &estate, &live, item).with_context(|| format!("{} {} {}", item.op, item.kind, item.target))?;
                println!("applied {} {} {} {}", item.op, item.kind, item.scope, item.target);
            }
            let after = plan::compute(&estate, &text, &Live::load(&cloudflare)?)?;
            println!("remaining actions after apply: {}", after.actions.len());
            print_plan(&after);
            if !after.actions.is_empty() {
                std::process::exit(1);
            }
        }
        "check" => {
            let live = Live::load(&cloudflare)?;
            let fresh = plan::compute(&estate, &text, &live)?;
            let cluster_hosts = cluster_hosts(&estate_path, &estate.estate.cluster_file)?;
            let mut findings = check::run(&estate, &live, &fresh, &cluster_hosts);
            if with_providers {
                let ovh = estate.estate.ovh_config_env.as_deref().and_then(|name| Ovh::from_config(name).ok());
                let token = estate.estate.hostinger_token_env.as_deref().and_then(|name| std::env::var(name).ok());
                findings.extend(providers::findings(&live, ovh.as_ref(), token.as_deref()));
            }
            print_findings(&estate, &findings);
            if findings.iter().any(|finding| finding.error) {
                std::process::exit(1);
            }
        }
        "import" => import(&Live::load(&cloudflare)?),
        "registrar-ds" => {
            let zone = args.get(1).filter(|arg| !arg.starts_with("--")).ok_or_else(|| anyhow!("registrar-ds needs a zone name"))?;
            let id = Live::load_zone_id(&cloudflare, zone)?;
            let dnssec = cloudflare.get(&format!("/zones/{id}/dnssec"))?;
            let ovh = Ovh::from_config(estate.estate.ovh_config_env.as_deref().unwrap_or("OVH_CONFIG"))?;
            let number = |key: &str| dnssec[key].as_u64().ok_or_else(|| anyhow!("Cloudflare DNSSEC payload lacks {key}"));
            let algorithm: u64 = dnssec["algorithm"].as_str().and_then(|value| value.parse().ok()).ok_or_else(|| anyhow!("Cloudflare DNSSEC payload lacks algorithm"))?;
            let public_key = dnssec["public_key"].as_str().ok_or_else(|| anyhow!("Cloudflare DNSSEC payload lacks public_key"))?;
            ovh.publish_ds(zone, number("key_tag")?, algorithm, number("flags")?, public_key)?;
            println!("published the DS record of {zone} at the OVH registrar");
        }
        _ => bail!("{USAGE}"),
    }
    Ok(())
}

fn print_plan(plan: &Plan) {
    for item in &plan.actions {
        println!("{:<7} {:<16} {:<18} {:<46} {}", item.op, item.kind, item.scope, item.target, item.detail);
    }
}

fn print_findings(estate: &Estate, findings: &[Finding]) {
    for item in findings {
        let level = if item.error { "ERROR" } else { "warn " };
        let practice = estate.practice.get(item.id).map(|practice| format!("  [{}]", practice.source)).unwrap_or_default();
        println!("{level} {:<24} {:<40} {}{practice}", item.id, item.subject, item.message);
    }
    println!("{} findings, {} errors", findings.len(), findings.iter().filter(|item| item.error).count());
}

fn cluster_hosts(estate_path: &Path, cluster_file: &str) -> Result<BTreeSet<String>> {
    let path = estate_path.parent().unwrap_or_else(|| Path::new(".")).join(cluster_file);
    let text = std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let value: toml::Value = toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    Ok(value.get("hosts").and_then(toml::Value::as_table).map(|table| table.keys().cloned().collect()).unwrap_or_default())
}

fn import(live: &Live) {
    for zone in &live.zones {
        println!("[zone.\"{}\"]\ndnssec = true\nrecord = [", zone.name);
        let mut lines: Vec<String> = zone
            .records
            .iter()
            .filter_map(plan::live_record)
            .map(|(_, mut record): (String, Record)| {
                if let Some(id) = record.content.strip_suffix(".cfargotunnel.com") {
                    record.content = format!("tunnel:{}", live.tunnel_name(id).unwrap_or_else(|| id.to_string()));
                } else if record.content == "100::" {
                    record.content = "worker".to_string();
                }
                record.to_line(&zone.name)
            })
            .collect();
        lines.sort_by_key(|line| (line.split(' ').nth(1).map(|name| (name != "@", name.to_string())), line.clone()));
        for line in lines {
            println!("  {},", toml::Value::String(line));
        }
        println!("]");
        let mut routes: Vec<String> = zone.routes.iter().filter_map(|row| Some(format!("{} => {}", row["pattern"].as_str()?, row["script"].as_str().unwrap_or("")))).collect();
        routes.sort();
        println!("route = [");
        for route in routes {
            println!("  {},", toml::Value::String(route));
        }
        println!("]");
        let mut domains: Vec<String> = live.worker_domains.iter().filter(|row| row["zone_id"].as_str() == Some(zone.id.as_str())).filter_map(|row| Some(format!("{} => {}", row["hostname"].as_str()?, row["service"].as_str().unwrap_or("")))).collect();
        domains.sort();
        println!("domain = [");
        for domain in domains {
            println!("  {},", toml::Value::String(domain));
        }
        println!("]\n");
    }
}
