//! Real CLI surface, persistent config/MCP/session management, direct tool runner,
//! real multi-turn `fx ask`, and interactive stdin REPL.

use crate::acp::AcpServer;
use crate::agent::{run_agent_turn, undo_last_edit};
use crate::compactor::compact_turns;
use crate::config::{FX_VERSION, FxConfig, discover_agents_md, discover_skills};
use crate::gateway::builtin_providers;
use crate::indexer::WorkspaceIndexer;
use crate::mcp::McpServerConfig;
use crate::permissions::{PermissionEngine, PermissionMode, YOLO_WARNING_TEXT};
use crate::session::{SessionState, list_sessions};
use crate::tools::{builtin_tool_specs, execute_builtin_tool, safe_git_readonly};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

pub const TOP_LEVEL_COMMANDS: [&str; 26] = [
    "help", "ask", "acp", "pr", "issue", "login", "logout", "setup", "status",
    "permissions", "mcp", "slack", "models", "provider", "doctor", "teams",
    "session", "sessions", "resume", "credits", "usage", "upgrade", "replay",
    "workspace", "config", "completion",
];

pub const SLASH_COMMANDS: [&str; 41] = [
    "/quit", "/clear", "/new", "/launch", "/close", "/agents", "/reset",
    "/resume", "/rename", "/fork", "/help", "/login", "/logout", "/provider",
    "/status", "/image", "/images", "/model", "/effort", "/permissions",
    "/allowlist", "/stats", "/usage", "/undo", "/mcp", "/skills", "/copy",
    "/feedback", "/trace", "/compact", "/settings", "/credits", "/paste",
    "/fast", "/ultrafast", "/statusline", "/notifications", "/workspace",
    "/shell", "/version", "/exit",
];

pub fn program_status_osc(state: &str) -> String {
    match state {
        "busy" => "\x1b]9;4;3;0\x07".to_owned(),
        "waiting" => "\x1b]9;4;4;100\x07".to_owned(),
        _ => "\x1b]9;4;0;0\x07".to_owned(),
    }
}

pub fn build_status_json(cfg: &FxConfig, workspace: &Path) -> Value {
    let idx = WorkspaceIndexer::open(workspace);
    let git = idx.read_git_info();
    let skills = discover_skills(workspace, &cfg.fx_home);
    let agents_md = discover_agents_md(workspace, &cfg.fx_home);
    let mode = PermissionMode::parse(&cfg.permission_mode).unwrap_or(PermissionMode::Auto);

    json!({
        "ok": true,
        "version": FX_VERSION,
        "runtime": "fw-fx-rust",
        "workspace": workspace.display().to_string(),
        "fx_home": cfg.fx_home.display().to_string(),
        "model": cfg.model,
        "provider": cfg.provider,
        "gateway_chat_url": cfg.gateway_chat_url,
        "permission_mode": mode.label(),
        "auto_compact_percent": cfg.auto_compact_percent,
        "max_agent_steps": cfg.max_agent_steps,
        "api_key_present": cfg.api_key_present,
        "git": git,
        "skills_discovered": skills.len(),
        "agents_md_files": agents_md.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        "builtin_tools": builtin_tool_specs().len(),
    })
}

pub fn build_doctor_json(cfg: &FxConfig, workspace: &Path) -> Value {
    let idx = WorkspaceIndexer::open(workspace);
    let git = idx.read_git_info();
    let fx_home_ok = std::fs::create_dir_all(&cfg.fx_home).is_ok();
    let checks = vec![
        json!({ "name": "workspace", "status": if workspace.is_dir() { "ok" } else { "fail" }, "detail": workspace.display().to_string() }),
        json!({ "name": "fx_home_writable", "status": if fx_home_ok { "ok" } else { "fail" }, "detail": cfg.fx_home.display().to_string() }),
        json!({ "name": "git_indexer", "status": "ok", "detail": if git.is_git_repo { format!("branch={}", git.head_ref.as_deref().unwrap_or("detached")) } else { "non-git directory".to_owned() } }),
        json!({ "name": "credentials", "status": if cfg.api_key_present { "ok" } else { "warn" }, "detail": if cfg.api_key_present { "API key found in env" } else { "set AI_GATEWAY_API_KEY / OPENAI_API_KEY or point FX_GATEWAY_CHAT_URL to local Ollama" } }),
    ];
    json!({ "ok": fx_home_ok && workspace.is_dir(), "version": FX_VERSION, "checks": checks })
}

fn load_mcp_map(path: &Path) -> BTreeMap<String, McpServerConfig> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_mcp_map(path: &Path, map: &BTreeMap<String, McpServerConfig>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(map).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

fn flag_value(args: &[String], name: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone())
}

pub fn run_cli(args: &[String]) -> Result<i32, String> {
    let mut cfg = FxConfig::from_env();
    let workspace = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let json_flag = args.iter().any(|a| a == "--json");

    if args.is_empty() || args[0] == "-c" {
        let resume_latest = args.first().is_some_and(|a| a == "-c");
        return run_interactive_repl(&mut cfg, &workspace, resume_latest);
    }

    let cmd = args[0].as_str();
    match cmd {
        "help" | "-h" | "--help" => {
            println!("fx {FX_VERSION} (pure-Rust native coding agent)");
            println!("Usage:");
            println!("  fx                               Interactive coding agent REPL (-c resumes latest session)");
            println!("  fx ask [flags] <prompt>          Run real multi-turn tool-calling agent (--json --resume <id> --model <m> --url <u> --full-access --no-save)");
            println!("  fx tool <name> '<json>'          Execute a built-in workspace tool directly (read_file, write_file, edit_file, glob_files, grep_files, shell)");
            println!("  fx config <list|get|set> ...     Read/write ~/.fx/config.json");
            println!("  fx mcp <list|add|remove> ...     Manage MCP servers in ~/.fx/mcp.json");
            println!("  fx sessions | session <show|del> Inspect or delete saved sessions in ~/.fx/sessions/");
            println!("  fx acp                           Run Agent Client Protocol JSON-RPC 2.0 server over stdio");
            println!("  fx status|doctor|models|workspace|permissions|pr|completion");
            Ok(0)
        }
        "-V" | "--version" | "version" => {
            println!("fx {FX_VERSION}");
            Ok(0)
        }
        "status" => {
            let v = build_status_json(&cfg, &workspace);
            if json_flag {
                println!("{}", serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?);
            } else {
                println!("fx {} | provider={} | model={} | url={} | permissions={}",
                    FX_VERSION, cfg.provider, cfg.model, cfg.gateway_chat_url, cfg.permission_mode);
            }
            Ok(0)
        }
        "doctor" => {
            let v = build_doctor_json(&cfg, &workspace);
            if json_flag {
                println!("{}", serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?);
            } else {
                for c in v["checks"].as_array().into_iter().flatten() {
                    println!("[{}] {}: {}", c["status"].as_str().unwrap_or("?"), c["name"].as_str().unwrap_or("?"), c["detail"].as_str().unwrap_or(""));
                }
            }
            Ok(if v["ok"].as_bool().unwrap_or(false) { 0 } else { 1 })
        }
        "tool" => {
            let tool_name = args.get(1).ok_or_else(|| "usage: fx tool <name> '<json-args>'".to_owned())?;
            let raw_json = args.get(2).map(String::as_str).unwrap_or("{}");
            let parsed: Value = serde_json::from_str(raw_json).map_err(|e| format!("invalid tool JSON: {e}"))?;
            let res = execute_builtin_tool(&workspace, tool_name, &parsed)?;
            println!("{}", serde_json::to_string_pretty(&res).map_err(|e| e.to_string())?);
            Ok(0)
        }
        "ask" => {
            if let Some(m) = flag_value(args, "--model") {
                cfg.model = m;
            }
            if let Some(u) = flag_value(args, "--url") {
                cfg.gateway_chat_url = u;
            }
            let full_access = args.iter().any(|a| a == "--full-access");
            let no_save = args.iter().any(|a| a == "--no-save");
            let resume_id = flag_value(args, "--resume");

            let mut prompt_parts = Vec::new();
            let mut skip_next = false;
            for a in &args[1..] {
                if skip_next {
                    skip_next = false;
                    continue;
                }
                if matches!(a.as_str(), "--model" | "--url" | "--resume") {
                    skip_next = true;
                    continue;
                }
                if matches!(a.as_str(), "--json" | "--full-access" | "--no-save") {
                    continue;
                }
                prompt_parts.push(a.as_str());
            }
            let prompt = prompt_parts.join(" ");
            if prompt.trim().is_empty() {
                return Err("usage: fx ask [--json] [--model <m>] [--url <u>] [--resume <id>] [--full-access] <prompt>".to_owned());
            }

            let mut session = match resume_id {
                Some(id) => SessionState::load_from_dir(&cfg.sessions_dir(), &id)?,
                None => SessionState::new(&workspace, &cfg.model, &cfg.provider),
            };
            let mode = if full_access {
                eprintln!("{YOLO_WARNING_TEXT}");
                PermissionMode::Yolo
            } else {
                PermissionMode::parse(&cfg.permission_mode).unwrap_or(PermissionMode::Auto)
            };
            let perms = PermissionEngine::new(mode);
            let outcome = run_agent_turn(&cfg, &workspace, &mut session, &perms, &prompt, !no_save)?;
            if json_flag {
                println!("{}", serde_json::to_string_pretty(&outcome).map_err(|e| e.to_string())?);
            } else {
                println!("{}", outcome.final_output);
            }
            Ok(0)
        }
        "config" => {
            let sub = args.get(1).map(String::as_str).unwrap_or("list");
            match sub {
                "list" => {
                    let v = json!({
                        "config_file": cfg.config_file().display().to_string(),
                        "model": cfg.model,
                        "provider": cfg.provider,
                        "permission_mode": cfg.permission_mode,
                        "effort": cfg.effort,
                        "gateway_chat_url": cfg.gateway_chat_url,
                        "auto_compact_percent": cfg.auto_compact_percent,
                        "max_agent_steps": cfg.max_agent_steps,
                    });
                    println!("{}", serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?);
                    Ok(0)
                }
                "get" => {
                    let key = args.get(2).ok_or_else(|| "usage: fx config get <key>".to_owned())?;
                    let val = match key.as_str() {
                        "model" => cfg.model,
                        "provider" => cfg.provider,
                        "permission_mode" | "permissions" => cfg.permission_mode,
                        "effort" => cfg.effort,
                        "gateway_chat_url" | "url" => cfg.gateway_chat_url,
                        "auto_compact_percent" => cfg.auto_compact_percent.to_string(),
                        "max_agent_steps" => cfg.max_agent_steps.to_string(),
                        _ => return Err(format!("unknown key `{key}`")),
                    };
                    println!("{val}");
                    Ok(0)
                }
                "set" => {
                    let key = args.get(2).ok_or_else(|| "usage: fx config set <key> <value>".to_owned())?;
                    let val = args.get(3).ok_or_else(|| "usage: fx config set <key> <value>".to_owned())?;
                    let saved = cfg.set_persisted_key(key, val)?;
                    println!("saved {key}={val} in {}", saved.display());
                    Ok(0)
                }
                _ => Err("usage: fx config <list|get <key>|set <key> <value>>".to_owned()),
            }
        }
        "mcp" => {
            let path = cfg.mcp_config_file();
            let sub = args.get(1).map(String::as_str).unwrap_or("list");
            let mut map = load_mcp_map(&path);
            match sub {
                "list" => {
                    println!("{}", serde_json::to_string_pretty(&map).map_err(|e| e.to_string())?);
                    Ok(0)
                }
                "add" => {
                    let name = args.get(2).ok_or_else(|| "usage: fx mcp add <name> <command_or_url>".to_owned())?;
                    let target = args.get(3).ok_or_else(|| "usage: fx mcp add <name> <command_or_url>".to_owned())?;
                    let transport = if target.starts_with("http://") || target.starts_with("https://") {
                        "http"
                    } else {
                        "stdio"
                    };
                    map.insert(
                        name.clone(),
                        McpServerConfig {
                            name: name.clone(),
                            transport: transport.to_owned(),
                            command_or_url: target.clone(),
                            enabled: true,
                        },
                    );
                    save_mcp_map(&path, &map)?;
                    println!("added MCP server `{name}` ({transport}) -> {}", path.display());
                    Ok(0)
                }
                "remove" | "rm" => {
                    let name = args.get(2).ok_or_else(|| "usage: fx mcp remove <name>".to_owned())?;
                    if map.remove(name).is_none() {
                        return Err(format!("MCP server `{name}` not found"));
                    }
                    save_mcp_map(&path, &map)?;
                    println!("removed MCP server `{name}`");
                    Ok(0)
                }
                _ => Err("usage: fx mcp <list|add <name> <cmd_or_url>|remove <name>>".to_owned()),
            }
        }
        "acp" => {
            let mut server = AcpServer::new(cfg, workspace);
            server.run_stdio()?;
            Ok(0)
        }
        "models" | "provider" => {
            let providers = builtin_providers();
            if json_flag {
                println!("{}", serde_json::to_string_pretty(&providers).map_err(|e| e.to_string())?);
            } else {
                for p in providers {
                    println!("{:<20} {} ({})", p.id, p.display_name, p.base_url);
                }
            }
            Ok(0)
        }
        "permissions" => {
            if let Some(next_mode) = args.get(1)
                && !next_mode.starts_with("--")
            {
                let parsed = PermissionMode::parse(next_mode)
                    .ok_or_else(|| "valid permission modes: ask | auto | full-access".to_owned())?;
                cfg.set_persisted_key("permission_mode", parsed.label())?;
                println!("permission mode set to {}", parsed.display_label());
                return Ok(0);
            }
            let mode = PermissionMode::parse(&cfg.permission_mode).unwrap_or(PermissionMode::Auto);
            if json_flag {
                println!("{}", serde_json::to_string_pretty(&json!({ "mode": mode.label(), "display": mode.display_label() })).map_err(|e| e.to_string())?);
            } else {
                println!("permission mode: {} ({})", mode.display_label(), mode.label());
            }
            Ok(0)
        }
        "sessions" => {
            let items = list_sessions(&cfg.sessions_dir());
            if json_flag {
                println!("{}", serde_json::to_string_pretty(&items).map_err(|e| e.to_string())?);
            } else if items.is_empty() {
                println!("no saved sessions in {}", cfg.sessions_dir().display());
            } else {
                for s in items {
                    println!("{}  turns={:<3}  model={}  {}", s.id, s.turns, s.model, s.title);
                }
            }
            Ok(0)
        }
        "session" => {
            let sub = args.get(1).map(String::as_str).unwrap_or("list");
            match sub {
                "show" => {
                    let id = args.get(2).ok_or_else(|| "usage: fx session show <id>".to_owned())?;
                    let s = SessionState::load_from_dir(&cfg.sessions_dir(), id)?;
                    println!("{}", serde_json::to_string_pretty(&s).map_err(|e| e.to_string())?);
                    Ok(0)
                }
                "delete" | "rm" => {
                    let id = args.get(2).ok_or_else(|| "usage: fx session delete <id>".to_owned())?;
                    let p = cfg.sessions_dir().join(format!("{id}.json"));
                    std::fs::remove_file(&p).map_err(|e| format!("{}: {e}", p.display()))?;
                    println!("deleted session {id}");
                    Ok(0)
                }
                _ => run_cli(&["sessions".to_owned()]),
            }
        }
        "workspace" => {
            let idx = WorkspaceIndexer::open(&workspace);
            let git = idx.read_git_info();
            let files = idx.list_files(200);
            if json_flag {
                println!("{}", serde_json::to_string_pretty(&json!({ "workspace": workspace.display().to_string(), "git": git, "files": files })).map_err(|e| e.to_string())?);
            } else {
                println!("workspace={} branch={:?} indexed={}", workspace.display(), git.head_ref, files.len());
            }
            Ok(0)
        }
        "pr" | "issue" => {
            let status = safe_git_readonly(&workspace, &["status", "--short"])?;
            let diff_stat = safe_git_readonly(&workspace, &["diff", "--stat"]).unwrap_or_default();
            if json_flag {
                println!("{}", serde_json::to_string_pretty(&json!({ "ok": true, "command": cmd, "status": status, "diff_stat": diff_stat })).map_err(|e| e.to_string())?);
            } else {
                println!("{status}{diff_stat}");
            }
            Ok(0)
        }
        "completion" => {
            let shell = args.get(1).map(String::as_str).unwrap_or("bash");
            println!("# fx {FX_VERSION} {shell} completion\ncomplete -W \"{}\" fx", TOP_LEVEL_COMMANDS.join(" "));
            Ok(0)
        }
        other => Err(format!("unknown fx command `{other}`; run `fx help`")),
    }
}

fn run_interactive_repl(cfg: &mut FxConfig, workspace: &Path, resume_latest: bool) -> Result<i32, String> {
    let mut session = if resume_latest {
        let items = list_sessions(&cfg.sessions_dir());
        match items.last() {
            Some(last) => SessionState::load_from_dir(&cfg.sessions_dir(), &last.id)?,
            None => SessionState::new(workspace, &cfg.model, &cfg.provider),
        }
    } else {
        SessionState::new(workspace, &cfg.model, &cfg.provider)
    };
    let mut perms = PermissionEngine::new(
        PermissionMode::parse(&cfg.permission_mode).unwrap_or(PermissionMode::Auto),
    );

    println!("fx {FX_VERSION} | session={} | model={} | mode={}", session.id, cfg.model, perms.mode.display_label());
    println!("Type a prompt, or /help, /model <m>, /permissions <ask|auto|yolo>, /undo, /compact, /status, /quit.");

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match trimmed {
            "/quit" | "/exit" => break,
            "/help" => println!("Slash commands: /model <m>, /permissions <ask|auto|yolo>, /undo, /compact, /status, /skills, /sessions, /quit"),
            "/status" => println!("{}", serde_json::to_string_pretty(&build_status_json(cfg, workspace)).unwrap_or_default()),
            "/undo" => match undo_last_edit(&mut session) {
                Ok(msg) => println!("undo: {msg}"),
                Err(e) => eprintln!("undo: {e}"),
            },
            "/compact" => {
                let (res, proj) = compact_turns(&session.turns, 4);
                session.turns = proj;
                println!("compacted {} turns (saved ~{} tokens)", res.compacted_turn_count, res.saved_tokens);
            }
            "/skills" => {
                for s in discover_skills(workspace, &cfg.fx_home) {
                    println!("- {}: {}", s.name, s.description);
                }
            }
            "/sessions" => {
                for s in list_sessions(&cfg.sessions_dir()) {
                    println!("{} ({} turns) {}", s.id, s.turns, s.title);
                }
            }
            s if s.starts_with("/model ") => {
                let m = s.trim_start_matches("/model ").trim();
                cfg.model = m.to_owned();
                println!("model={}", cfg.model);
            }
            s if s.starts_with("/permissions ") => {
                let m = s.trim_start_matches("/permissions ").trim();
                match PermissionMode::parse(m) {
                    Some(parsed) => {
                        perms.mode = parsed;
                        cfg.permission_mode = parsed.label().to_owned();
                        println!("permissions={}", parsed.display_label());
                    }
                    None => eprintln!("invalid mode (use ask, auto, or full-access)"),
                }
            }
            prompt => {
                let _ = write!(stdout, "{}", program_status_osc("busy"));
                let _ = stdout.flush();
                match run_agent_turn(cfg, workspace, &mut session, &perms, prompt, true) {
                    Ok(out) => println!("{}", out.final_output),
                    Err(e) => eprintln!("error: {e}"),
                }
                let _ = write!(stdout, "{}", program_status_osc("idle"));
                let _ = stdout.flush();
            }
        }
    }
    Ok(0)
}
