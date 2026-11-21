#![forbid(unsafe_code)]

//! Universal Autonomous Agent Onboarding, Config Bridge & Reproducible Workflows (`fw onboard`, `fw workflow`).
//! Eliminates hardcoded Gemini/Claude lock-in by discovering and exporting canonical config
//! (`AGENTS.md`, skills, MCP, ACP, persistent memory) for ANY autonomous agent runtime.

use crate::abliterate::run_abliterate_cli;
use crate::agent_platform::{run_code_graph, run_security_scan};
use crate::auto_learn::{run_claude_mem_cli, XfsDatasetStore};
use crate::cachy_tune::run_tune_cli;
use crate::doctor::run_doctor;
use crate::local_studio::{build_abliterate_pipeline_plan, build_repo_map, run_vram_cli};
use fw_apm::unified_pm::{scan_unified_workspace, unified_sbom_cyclonedx};
use fw_fx::config::{discover_agents_md, discover_skills, FxConfig};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

pub const UNIVERSAL_AGENT_TARGETS: &[(&str, &str, &str)] = &[
    ("universal", "AGENTS.md", ".agents/skills"),
    ("claude", "CLAUDE.md", ".claude/skills"),
    ("gemini", "GEMINI.md", ".gemini/skills"),
    ("codex", "AGENTS.md", ".codex/skills"),
    ("cursor", ".cursor/rules/fw.mdc", ".cursor/skills"),
    ("windsurf", ".windsurfrules", ".windsurf/skills"),
    ("opencode", "opencode.json", ".opencode/skills"),
    ("zed", ".zed/settings.json", ".zed/skills"),
    ("acp", "fw acp (stdio JSON-RPC 2.0)", ".fx/skills"),
    ("mcp", "fw serve /mcp (Streamable HTTP)", ".fx/skills"),
];

/// `fw onboard [dir]` — Single < 500-token structured discovery bundle for any autonomous agent.
pub fn run_onboard(args: &[String], cwd: &Path) -> Result<Value, String> {
    if args.first().map(String::as_str) == Some("export") {
        let target = args.get(1).map(String::as_str).unwrap_or("all");
        let out_dir = args
            .windows(2)
            .find(|w| w[0] == "--dir")
            .map(|w| Path::new(&w[1]).to_path_buf())
            .unwrap_or_else(|| cwd.to_path_buf());
        return export_universal_agent_configs(&out_dir, target);
    }

    let target = args
        .first()
        .filter(|a| !a.starts_with('-'))
        .map(|s| cwd.join(s))
        .unwrap_or_else(|| cwd.to_path_buf());

    let cfg = FxConfig::from_env();
    let doc = run_doctor(&target);
    let skills = discover_skills(&target, &cfg.fx_home);
    let instr = discover_agents_md(&target, &cfg.fx_home);
    let mem = run_claude_mem_cli(&["list".to_owned()], &XfsDatasetStore::resolve(None)).unwrap_or_else(|_| json!({ "count": 0 }));
    let tune_hw = run_tune_cli(&["hardware".to_owned()]).unwrap_or_else(|_| json!({}));

    Ok(json!({
        "ok": true,
        "zeroSubprocess": true,
        "product": "fw",
        "version": env!("CARGO_PKG_VERSION"),
        "workspace": target.display().to_string(),
        "isaTier": tune_hw["isaTier"],
        "allocator": tune_hw["allocator"],
        "git": doc["git"],
        "llmReady": doc["llm_ready"],
        "instructionsFiles": instr.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        "skillsCount": skills.len(),
        "memoryCount": mem["count"],
        "universalTargets": UNIVERSAL_AGENT_TARGETS.iter().map(|(id, cfg_f, sk)| json!({
            "agent": id,
            "config": cfg_f,
            "skillsDir": sk
        })).collect::<Vec<_>>(),
        "recommendedJourney": [
            "1. fw onboard                     — Orient workspace, git, hardware, skills & memory",
            "2. fw repomap && fw graph --md    — Map symbols and deterministic import graph",
            "3. fw sec                         — Verify zero leaked secrets or unsafe patterns",
            "4. fw sh -c \"...\" | fw tool ...   — Run zero-fork GNU/Linux pipes or workspace tools",
            "5. fw abliterate | fw hf | fw vram — Run local GGUF/HF/Heretic/DPO workflows",
            "6. fw workflow run discover       — Execute reproducible end-to-end verification"
        ]
    }))
}

/// Generates vendor-neutral MCP/ACP/instructions bridge files from ONE canonical `fw` config.
pub fn export_universal_agent_configs(out_dir: &Path, target: &str) -> Result<Value, String> {
    fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let mcp_json = json!({
        "mcpServers": {
            "fw": {
                "command": "fw",
                "args": ["acp"],
                "httpUrl": "http://127.0.0.1:4380/mcp",
                "transport": "streamable-http-or-stdio"
            }
        }
    });
    let opencode_json = json!({
        "$schema": "https://opencode.ai/config.json",
        "instructions": ["AGENTS.md"],
        "mcp": {
            "fw": { "type": "remote", "url": "http://127.0.0.1:4380/mcp" }
        }
    });
    let zed_json = json!({
        "agent_servers": {
            "fw": { "command": "fw", "args": ["acp"] }
        }
    });

    let mut written = Vec::new();
    let targets: Vec<&str> = if target == "all" {
        vec!["mcp", "opencode", "zed"]
    } else {
        vec![target]
    };

    for t in targets {
        match t {
            "mcp" | "claude" | "cursor" | "gemini" => {
                let p = out_dir.join(".mcp.json");
                fs::write(&p, serde_json::to_string_pretty(&mcp_json).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
                written.push(p.display().to_string());
            }
            "opencode" => {
                let p = out_dir.join("opencode.json");
                fs::write(&p, serde_json::to_string_pretty(&opencode_json).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
                written.push(p.display().to_string());
            }
            "zed" => {
                let dir = out_dir.join(".zed");
                fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let p = dir.join("settings.json");
                fs::write(&p, serde_json::to_string_pretty(&zed_json).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
                written.push(p.display().to_string());
            }
            other => return Err(format!("unknown export target `{other}` (expected all|mcp|opencode|zed)")),
        }
    }

    Ok(json!({
        "ok": true,
        "zeroSubprocess": true,
        "exported": written
    }))
}

/// `fw workflow <list|run> [discover|abliterate-e2e|pkg-audit|perf-check]`
pub fn run_workflow_cli(args: &[String], cwd: &Path) -> Result<Value, String> {
    let sub = args.first().map(String::as_str).unwrap_or("list");
    match sub {
        "list" => Ok(json!({
            "ok": true,
            "workflows": [
                { "name": "discover", "steps": ["doctor", "repomap", "graph", "sec"] },
                { "name": "abliterate-e2e", "steps": ["vram", "abliterate plan", "abliterate biprojected", "abliterate ara", "abliterate sweep"] },
                { "name": "pkg-audit", "steps": ["tree (scan)", "sec", "export (CycloneDX SBOM)"] },
                { "name": "perf-check", "steps": ["tune hardware", "tune bench"] }
            ]
        })),
        "run" => {
            let name = args.get(1).map(String::as_str).unwrap_or("discover");
            match name {
                "discover" => {
                    let doc = run_doctor(cwd);
                    let map = build_repo_map(cwd, 60);
                    let graph = run_code_graph(cwd, false);
                    let sec = run_security_scan(cwd);
                    Ok(json!({
                        "ok": doc["ok"].as_bool().unwrap_or(false) && sec["ok"].as_bool().unwrap_or(false),
                        "workflow": "discover",
                        "zeroSubprocess": true,
                        "doctorOk": doc["ok"],
                        "mappedFiles": map["mappedFiles"],
                        "totalSymbols": map["totalSymbols"],
                        "graphNodes": graph["nodeCount"],
                        "graphEdges": graph["edgeCount"],
                        "securitySummary": sec["summary"]
                    }))
                }
                "abliterate-e2e" => {
                    let vram = run_vram_cli(&[]);
                    let plan = build_abliterate_pipeline_plan("Qwen/Qwen3-8B", 8.0, 12.0);
                    let bip = run_abliterate_cli(&["biprojected".to_owned()], cwd)?;
                    let ara = run_abliterate_cli(&["ara".to_owned()], cwd)?;
                    let sweep = run_abliterate_cli(&["sweep".to_owned()], cwd)?;
                    Ok(json!({
                        "ok": true,
                        "workflow": "abliterate-e2e",
                        "zeroSubprocess": true,
                        "vramRecommendedNgl": vram["plan"]["recommended_ngl"],
                        "pipelineSteps": plan["steps"].as_array().map_or(0, Vec::len),
                        "biprojectedKl": bip["report"]["kl_divergence"],
                        "biprojectedRefusalAfter": bip["report"]["refusal_projection_after"],
                        "araConverged": ara["manifoldConverged"],
                        "paretoFrontierCount": sweep["paretoFrontierCount"]
                    }))
                }
                "pkg-audit" => {
                    let lock = scan_unified_workspace(cwd);
                    let sbom = unified_sbom_cyclonedx(&lock);
                    let sec = run_security_scan(cwd);
                    Ok(json!({
                        "ok": sec["ok"],
                        "workflow": "pkg-audit",
                        "zeroSubprocess": true,
                        "packageCounts": lock.counts,
                        "sbomComponents": sbom["components"].as_array().map_or(0, Vec::len),
                        "securitySummary": sec["summary"]
                    }))
                }
                "perf-check" => {
                    let hw = run_tune_cli(&["hardware".to_owned()])?;
                    let bench = run_tune_cli(&["bench".to_owned()])?;
                    Ok(json!({
                        "ok": true,
                        "workflow": "perf-check",
                        "zeroSubprocess": true,
                        "isaTier": hw["isaTier"],
                        "allocator": hw["allocator"],
                        "bench": bench
                    }))
                }
                other => Err(format!("unknown workflow `{other}` (expected discover|abliterate-e2e|pkg-audit|perf-check)")),
            }
        }
        other => Err(format!("unknown workflow subcommand `{other}` (expected list|run)")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn onboard_and_workflows_execute_zero_subprocess() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let ob = run_onboard(&[], root).expect("onboard");
        assert_eq!(ob["ok"], true);
        assert!(ob["universalTargets"].as_array().unwrap().len() >= 8);

        let wf = run_workflow_cli(&["run".to_owned(), "abliterate-e2e".to_owned()], root).expect("wf");
        assert_eq!(wf["ok"], true);
        assert_eq!(wf["araConverged"], true);
    }
}
