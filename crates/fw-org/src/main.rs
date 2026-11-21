use anyhow::{Context, Result, anyhow, bail};
use fw_org::check::{self, Finding};
use fw_org::github::Github;
use fw_org::live::Live;
use fw_org::model::Org;
use fw_org::plan::{self, Identity, Plan};
use std::path::{Path, PathBuf};

const USAGE: &str = "fw org <plan|apply|check|import> [--org org.toml]
  plan    diff the live organization against org.toml and write the plan file
  apply   execute the committed plan file (tracked, unmodified, equal to a fresh plan)
  check   best-practice findings plus drift; exit 1 when one is an error
  import  print [repo.<name>] sections for the originals from live state";

fn main() {
    if let Err(error) = run() {
        eprintln!("fw org: {error:#}");
        std::process::exit(2);
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).ok_or_else(|| anyhow!(USAGE))?;
    let org_path = args.iter().position(|arg| arg == "--org").and_then(|index| args.get(index + 1)).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("org.toml"));
    let (org, text) = Org::load(&org_path)?;
    let directory = org_path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    let plan_path = directory.join(&org.org.plan_file);
    let product = std::fs::read_to_string(directory.join("product.toml")).context("read product.toml")?.parse::<toml::Table>().context("parse product.toml")?;
    if product.get("organization").and_then(toml::Value::as_str) != Some(org.org.name.as_str()) {
        bail!("org.toml names {} but product.toml organization is {:?}", org.org.name, product.get("organization"));
    }
    let identity = Identity::load(&directory.join(&org.org.identity_file), &org.org.name)?;
    let github = Github::from_env(&org.org.token_env)?;
    match command {
        "plan" => {
            let live = Live::load(&github, &org)?;
            let (fresh, _) = plan::compute(&org, &text, &live, &identity)?;
            print_plan(&fresh);
            std::fs::write(&plan_path, serde_json::to_string_pretty(&fresh)? + "\n").with_context(|| format!("write {}", plan_path.display()))?;
            println!("wrote {} ({} actions)", plan_path.display(), fresh.actions.len());
        }
        "apply" => {
            let committed: Plan = serde_json::from_str(&std::fs::read_to_string(&plan_path).with_context(|| format!("read {}", plan_path.display()))?)?;
            let live = Live::load(&github, &org)?;
            let (fresh, steps) = plan::compute(&org, &text, &live, &identity)?;
            plan::require_committed(&plan_path, &committed, &fresh)?;
            plan::execute(&github, &steps)?;
            let after_live = Live::load(&github, &org)?;
            let (after, _) = plan::compute(&org, &text, &after_live, &identity)?;
            println!("remaining actions after apply: {}", after.actions.len());
            print_plan(&after);
            if !after.actions.is_empty() {
                std::process::exit(1);
            }
        }
        "check" => {
            let live = Live::load(&github, &org)?;
            let (fresh, _) = plan::compute(&org, &text, &live, &identity)?;
            let findings = check::run(&org, &live, &fresh, &identity);
            print_findings(&org, &findings);
            if findings.iter().any(|item| item.error) {
                std::process::exit(1);
            }
        }
        "import" => {
            let live = Live::load(&github, &org)?;
            for repo in live.repos.iter().filter(|repo| repo.summary["fork"] != true && repo.summary["archived"] != true) {
                println!("[repo.\"{}\"]", repo.name);
                println!("description = {}", toml::Value::String(repo.summary["description"].as_str().unwrap_or_default().to_string()));
                println!("homepage = {}", toml::Value::String(repo.summary["homepage"].as_str().unwrap_or_default().to_string()));
                let topics: Vec<toml::Value> = repo.summary["topics"].as_array().into_iter().flatten().filter_map(|topic| topic.as_str().map(|text| toml::Value::String(text.to_string()))).collect();
                println!("topics = {}\n", toml::Value::Array(topics));
            }
        }
        _ => bail!("{USAGE}"),
    }
    Ok(())
}

fn print_plan(plan: &Plan) {
    for item in &plan.actions {
        println!("{:<8} {:<22} {:<10} {:<34} {}", item.op, item.kind, item.scope, item.target, item.detail);
    }
}

fn print_findings(org: &Org, findings: &[Finding]) {
    for item in findings {
        let level = if item.error { "ERROR" } else { "warn " };
        let practice = org.practice.get(item.id).map(|practice| format!("  [{}]", practice.source)).unwrap_or_default();
        println!("{level} {:<22} {:<34} {}{practice}", item.id, item.subject, item.message);
    }
    println!("{} findings, {} errors", findings.len(), findings.iter().filter(|item| item.error).count());
}
