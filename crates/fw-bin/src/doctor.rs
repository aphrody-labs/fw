#![forbid(unsafe_code)]

//! Actionable system & workspace diagnostics (`fw doctor`) — real probes only, zero showcase fluff.

use fw_fx::config::FxConfig;
use fw_fx::indexer::WorkspaceIndexer;
use serde_json::{json, Value};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

fn probe_exe(name: &str, version_arg: &str) -> Value {
    match Command::new(name).arg(version_arg).output() {
        Ok(out) if out.status.success() => {
            let first = String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_owned();
            json!({ "tool": name, "ok": true, "version": first })
        }
        Ok(out) => json!({ "tool": name, "ok": false, "exit": out.status.code() }),
        Err(_) => json!({ "tool": name, "ok": false, "error": "not found in PATH" }),
    }
}

fn probe_tcp(addr: &str) -> bool {
    addr.parse::<SocketAddr>()
        .ok()
        .is_some_and(|sa| TcpStream::connect_timeout(&sa, Duration::from_millis(150)).is_ok())
}

pub fn run_doctor(cwd: &Path) -> Value {
    let cfg = FxConfig::from_env();
    let idx = WorkspaceIndexer::open(cwd);
    let git = idx.read_git_info();
    let fx_home_writable = std::fs::create_dir_all(&cfg.fx_home).is_ok();
    let ollama_reachable = probe_tcp("127.0.0.1:11434");

    let tools = vec![
        probe_exe("git", "--version"),
        probe_exe("cargo", "--version"),
        probe_exe("bun", "--version"),
        probe_exe(if cfg!(windows) { "curl.exe" } else { "curl" }, "--version"),
    ];
    let all_tools_ok = tools.iter().all(|t| t["ok"].as_bool().unwrap_or(false));

    json!({
        "ok": all_tools_ok && fx_home_writable,
        "workspace": cwd.display().to_string(),
        "git": git,
        "fx_home": { "path": cfg.fx_home.display().to_string(), "writable": fx_home_writable },
        "llm_ready": {
            "api_key_in_env": cfg.api_key_present,
            "local_ollama_11434": ollama_reachable,
            "gateway_chat_url": cfg.gateway_chat_url
        },
        "toolchain": tools
    })
}
