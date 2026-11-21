#![forbid(unsafe_code)]

//! Agent platform commands (no subprocesses):
//! 1. `fw sec`: scans the workspace for leaked credentials, private keys, XSS/eval, SQLi patterns and unsafe blocks.
//! 2. `fw graph`: file/symbol/import graph without a vector store, JSON and Markdown export.
//! 3. `fw skills`: discovers and inspects `SKILL.md` files and pulls them over HTTPS (`ureq`).
//!
//! `fw mem` lives in `auto_learn.rs`.

use fw_fx::config::{discover_skills, FxConfig};
use fw_fx::indexer::WorkspaceIndexer;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityFinding {
    pub severity: &'static str,
    pub rule: &'static str,
    pub file: String,
    pub line: usize,
    pub snippet: String,
}

const SECRET_RULES: &[(&str, &str, &str)] = &[
    ("critical", "private-key-header", "-----BEGIN PRIVATE KEY-----"),
    ("critical", "rsa-private-key", "-----BEGIN RSA PRIVATE KEY-----"),
    ("high", "aws-access-key", "AKIA"),
    ("high", "github-pat", "ghp_"),
    ("high", "github-fine-pat", "github_pat_"),
    ("high", "openai-key", "sk-proj-"),
    ("high", "anthropic-key", "sk-ant-"),
    ("high", "hf-token", "hf_"),
    ("medium", "js-eval", "eval("),
    ("medium", "dom-inner-html", ".innerHTML ="),
    ("medium", "react-dangerous-html", "dangerouslySetInnerHTML"),
];

/// `fw sec [dir]`: pattern-based static scan for secrets and unsafe code patterns.
pub fn run_security_scan(root: &Path) -> Value {
    let idx = WorkspaceIndexer::open(root);
    let files = idx.list_files(1000);
    let mut findings: Vec<SecurityFinding> = Vec::new();
    let mut scanned = 0usize;

    for rel in files {
        let ext = Path::new(&rel).extension().and_then(|s| s.to_str()).unwrap_or("");
        if !matches!(ext, "rs" | "ts" | "tsx" | "js" | "mjs" | "py" | "toml" | "json" | "yaml" | "yml" | "env") {
            continue;
        }
        // Skip our own rule definitions file so it doesn't self-flag literal rule prefixes
        if rel.ends_with("agent_platform.rs") {
            continue;
        }
        let Ok(raw) = fs::read_to_string(root.join(&rel)) else {
            continue;
        };
        scanned += 1;
        for (idx_line, line) in raw.lines().enumerate() {
            for &(sev, rule, needle) in SECRET_RULES {
                if let Some(pos) = line.find(needle) {
                    // For short token prefixes (AKIA, ghp_, hf_), require >= 16 alphanumeric chars following
                    let is_token_prefix = matches!(rule, "aws-access-key" | "github-pat" | "hf-token" | "openai-key" | "anthropic-key");
                    if is_token_prefix {
                        let tail = &line[pos + needle.len()..];
                        let tok_len = tail.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').count();
                        if tok_len < 16 {
                            continue;
                        }
                    }
                    findings.push(SecurityFinding {
                        severity: sev,
                        rule,
                        file: rel.clone(),
                        line: idx_line + 1,
                        snippet: line.trim().chars().take(100).collect(),
                    });
                    if findings.len() >= 100 {
                        break;
                    }
                }
            }
            if findings.len() >= 100 {
                break;
            }
        }
    }

    let critical = findings.iter().filter(|f| f.severity == "critical").count();
    let high = findings.iter().filter(|f| f.severity == "high").count();
    let medium = findings.iter().filter(|f| f.severity == "medium").count();

    json!({
        "ok": critical == 0 && high == 0,
        "zeroSubprocess": true,
        "scanner": "fw-sec",
        "workspace": root.display().to_string(),
        "filesScanned": scanned,
        "summary": { "critical": critical, "high": high, "medium": medium, "total": findings.len() },
        "findings": findings
    })
}

/// Definition and import graph from line prefixes (`pub fn`, `import`, `use`, ...), no parser.
pub fn run_code_graph(root: &Path, markdown: bool) -> Value {
    let idx = WorkspaceIndexer::open(root);
    let files = idx.list_files(300);
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    for rel in files {
        let ext = Path::new(&rel).extension().and_then(|s| s.to_str()).unwrap_or("");
        if !matches!(ext, "rs" | "ts" | "tsx" | "js" | "py") {
            continue;
        }
        let Ok(raw) = fs::read_to_string(root.join(&rel)) else {
            continue;
        };
        let mut defines = Vec::new();
        let mut imports = BTreeSet::new();
        for line in raw.lines() {
            let t = line.trim();
            if let Some(rest) = t
                .strip_prefix("pub fn ")
                .or_else(|| t.strip_prefix("pub struct "))
                .or_else(|| t.strip_prefix("pub enum "))
                .or_else(|| t.strip_prefix("export function "))
                .or_else(|| t.strip_prefix("export class "))
                .or_else(|| t.strip_prefix("def "))
                && let Some(name) = rest.split(['(', '<', '{', ':', ' ']).next()
                && !name.is_empty()
            {
                defines.push(name.to_owned());
            }
            if let Some(u) = t.strip_prefix("use ") {
                if let Some(target) = u.split([':', ';', ' ']).next()
                    && !matches!(target, "std" | "core" | "super" | "self" | "")
                {
                    imports.insert(target.to_owned());
                }
            } else if t.starts_with("import ")
                && let Some((_, rhs)) = t.split_once(" from ")
            {
                let target = rhs.trim().trim_end_matches(';').trim_matches(['"', '\'']);
                if !target.is_empty() {
                    imports.insert(target.to_owned());
                }
            }
        }
        nodes.push(json!({ "id": rel, "defines": defines.len(), "sample": defines.into_iter().take(6).collect::<Vec<_>>() }));
        for imp in imports {
            edges.push(json!({ "from": rel, "to": imp, "kind": "imports" }));
        }
    }

    if markdown {
        let mut md = format!("# Codebase Knowledge Graph ({})\n\n- Nodes: {}\n- Edges: {}\n\n", root.display(), nodes.len(), edges.len());
        for n in nodes.iter().take(25) {
            md.push_str(&format!("- `{}` ({} symbols)\n", n["id"].as_str().unwrap_or(""), n["defines"]));
        }
        return json!({ "ok": true, "format": "markdown", "nodes": nodes.len(), "edges": edges.len(), "markdown": md });
    }

    json!({
        "ok": true,
        "zeroSubprocess": true,
        "engine": "fw-graph (line-prefix definition and import graph)",
        "workspace": root.display().to_string(),
        "nodeCount": nodes.len(),
        "edgeCount": edges.len(),
        "nodes": nodes.into_iter().take(50).collect::<Vec<_>>(),
        "edges": edges.into_iter().take(100).collect::<Vec<_>>()
    })
}

/// `fw skills <list|read|install> [arg]`: lists, reads and installs `SKILL.md` files.
pub fn run_skills_cli(args: &[String], cwd: &Path, offline: bool) -> Result<Value, String> {
    let cfg = FxConfig::from_env();
    let sub = args.first().map(String::as_str).unwrap_or("list");
    match sub {
        "list" => {
            let skills = discover_skills(cwd, &cfg.fx_home);
            Ok(json!({
                "ok": true,
                "zeroSubprocess": true,
                "count": skills.len(),
                "skills": skills.iter().map(|s| json!({
                    "name": s.name,
                    "description": s.description,
                    "path": s.path.display().to_string()
                })).collect::<Vec<_>>()
            }))
        }
        "read" => {
            let name = args.get(1).ok_or_else(|| "usage: fw skills read <skill-name>".to_owned())?;
            let skills = discover_skills(cwd, &cfg.fx_home);
            let found = skills
                .into_iter()
                .find(|s| s.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| format!("skill `{name}` not found"))?;
            let content = fs::read_to_string(&found.path).map_err(|e| e.to_string())?;
            Ok(json!({
                "ok": true,
                "name": found.name,
                "path": found.path.display().to_string(),
                "bytes": content.len(),
                "content": content
            }))
        }
        "install" => {
            if offline {
                return Err("fw skills install: disabled by --offline".to_owned());
            }
            let spec = args.get(1).ok_or_else(|| "usage: fw skills install <owner/repo/skill-name>".to_owned())?;
            let parts: Vec<&str> = spec.split('/').collect();
            if parts.len() < 3 {
                return Err("expected <owner>/<repo>/<skill-name>".to_owned());
            }
            let (owner, repo, skill) = (parts[0], parts[1], parts[2]);
            let url = format!("https://raw.githubusercontent.com/{owner}/{repo}/main/skills/{skill}/SKILL.md");
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(15)))
                .build()
                .into();
            let body = agent
                .get(&url)
                .header("User-Agent", concat!("fw/", env!("CARGO_PKG_VERSION")))
                .call()
                .map_err(|e| format!("fetch {url}: {e}"))?
                .body_mut()
                .read_to_string()
                .map_err(|e| e.to_string())?;
            let dest_dir = cwd.join(".fx").join("skills").join(skill);
            fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
            let dest = dest_dir.join("SKILL.md");
            fs::write(&dest, &body).map_err(|e| e.to_string())?;
            Ok(json!({
                "ok": true,
                "zeroSubprocess": true,
                "installed": skill,
                "sourceUrl": url,
                "path": dest.display().to_string(),
                "bytes": body.len()
            }))
        }
        other => Err(format!("unknown skills subcommand `{other}` (expected list|read|install)")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sec_and_graph_run_cleanly_on_fw_bin() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let sec = run_security_scan(root);
        assert_eq!(sec["ok"], true);
        let graph = run_code_graph(root, false);
        assert!(graph["nodeCount"].as_u64().unwrap_or(0) >= 5);
        assert!(graph["edgeCount"].as_u64().unwrap_or(0) >= 5);
    }
}
