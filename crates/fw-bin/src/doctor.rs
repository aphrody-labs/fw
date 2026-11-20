#![forbid(unsafe_code)]

//! Zero-spawn native system & workspace diagnostics (`fw doctor`).
//! Probes PATH binaries via `std::env::split_paths` + `fs::metadata` (zero subprocess spawns),
//! checks `FX_HOME` writability, git repo state (data-only), API keys, and local Ollama TCP.

use fw_fx::config::FxConfig;
use fw_fx::indexer::WorkspaceIndexer;
use serde_json::{json, Value};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    let candidates: Vec<String> = if cfg!(windows) {
        vec![format!("{name}.exe"), format!("{name}.cmd"), name.to_owned()]
    } else {
        vec![name.to_owned()]
    };
    for dir in std::env::split_paths(&path_var) {
        for cand in &candidates {
            let p = dir.join(cand);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

fn probe_exe_native(name: &str) -> Value {
    match find_in_path(name) {
        Some(path) => {
            let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            json!({ "tool": name, "ok": true, "path": path.display().to_string(), "bytes": bytes })
        }
        None => json!({ "tool": name, "ok": false, "error": "not found in PATH" }),
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

    // `fw` is a native binary: only `git` (workspace state) is required to run. The build toolchains are
    // reported as `optional`, so a host without cargo, rustc, bun or uv still gets `ok: true`.
    let tools: Vec<Value> = [("git", false), ("cargo", true), ("rustc", true), ("bun", true), ("uv", true)]
        .into_iter()
        .map(|(name, optional)| {
            let mut probe = probe_exe_native(name);
            probe["optional"] = json!(optional);
            probe
        })
        .collect();
    let core_ok = tools
        .iter()
        .filter(|t| !t["optional"].as_bool().unwrap_or(false))
        .all(|t| t["ok"].as_bool().unwrap_or(false));

    json!({
        "ok": core_ok && fx_home_writable,
        "zeroSubprocess": true,
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
