//! Real multi-turn tool-calling agent loop with HTTP (`std::net::TcpStream`) + HTTPS transport,
//! `PermissionEngine` enforcement, file-edit `UndoEntry` tracking, `recovery::step` retries,
//! `compactor::compact_turns`, and `~/.fx/sessions/` persistence.

use crate::compactor::{compact_turns, should_compact};
use crate::config::{FxConfig, discover_agents_md, discover_skills};
use crate::permissions::{PermissionEngine, PermissionMode, PermissionTargetKind, RuleDecision};
use crate::recovery::{self, Event, Failure, FailureKind, Start, State as RecoveryState};
use crate::session::{SessionState, UndoEntry};
use crate::tools::{builtin_tool_specs, execute_builtin_tool};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpResponse {
    pub status: u16,
    pub retry_after_ms: Option<u32>,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTurnOutcome {
    pub final_output: String,
    pub steps: u32,
    pub tool_calls_executed: usize,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub session_id: String,
    pub saved_path: Option<String>,
}

/// Pure-Rust native HTTP/HTTPS (TLS 1.3 via `ureq` + `rustls`, zero subprocesses).
#[cfg(not(target_arch = "wasm32"))]
pub fn post_json(url: &str, api_key: Option<&str>, payload: &Value) -> Result<HttpResponse, String> {
    let body_bytes = serde_json::to_vec(payload).map_err(|e| e.to_string())?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(120)))
        .build()
        .into();

    let mut req = agent
        .post(url)
        .header("Content-Type", "application/json")
        .header("User-Agent", concat!("fw/", env!("CARGO_PKG_VERSION")));
    if let Some(key) = api_key
        && !key.is_empty()
    {
        req = req.header("Authorization", &format!("Bearer {key}"));
    }

    let mut resp = req.send(body_bytes.as_slice()).map_err(|e| format!("http {url}: {e}"))?;
    let status = resp.status().as_u16();
    let retry_after_ms = resp
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u32>().ok())
        .map(|secs| secs.saturating_mul(1_000));
    let body = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("read body: {e}"))?;

    Ok(HttpResponse {
        status,
        retry_after_ms,
        body,
    })
}

#[cfg(target_arch = "wasm32")]
pub fn post_json(_url: &str, _api_key: Option<&str>, _payload: &Value) -> Result<HttpResponse, String> {
    Err("wasm32 transport uses host fetch bridge".to_owned())
}

pub fn resolve_api_key(cfg: &FxConfig) -> Option<String> {
    for var in [
        "AI_GATEWAY_API_KEY",
        "OPENAI_API_KEY",
        "OPENROUTER_API_KEY",
        "XAI_API_KEY",
        "ANTHROPIC_API_KEY",
    ] {
        if let Ok(v) = std::env::var(var)
            && !v.trim().is_empty()
        {
            return Some(v);
        }
    }
    cfg.api_key_override.clone()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn build_system_prompt(workspace: &Path, cfg: &FxConfig) -> String {
    let mut sys = format!(
        "You are fx (Rust), a coding agent operating in workspace `{}`. Use the provided tools directly to inspect and edit files.\n",
        workspace.display()
    );
    for path in discover_agents_md(workspace, &cfg.fx_home) {
        if let Ok(content) = std::fs::read_to_string(&path) {
            let clipped: String = content.chars().take(4000).collect();
            sys.push_str(&format!("\n--- {} ---\n{}\n", path.display(), clipped));
        }
    }
    let skills = discover_skills(workspace, &cfg.fx_home);
    if !skills.is_empty() {
        sys.push_str("\nAvailable skills (read SKILL.md on demand):\n");
        for s in skills.iter().take(24) {
            sys.push_str(&format!("- {}: {}\n", s.name, s.description));
        }
    }
    sys
}

fn openai_tools_json() -> Vec<Value> {
    builtin_tool_specs()
        .into_iter()
        .map(|spec| {
            json!({
                "type": "function",
                "function": {
                    "name": spec.name,
                    "description": spec.description,
                    "parameters": spec.parameters
                }
            })
        })
        .collect()
}

fn failure_kind_for_http(status: u16, body: &str) -> FailureKind {
    match status {
        0 => FailureKind::Offline,
        401 | 403 => FailureKind::SignIn,
        402 => FailureKind::Quota,
        429 => FailureKind::RateLimited,
        400 | 422 if body.to_ascii_lowercase().contains("context") || body.to_ascii_lowercase().contains("too long") => {
            FailureKind::TooLong
        }
        400 | 404 | 422 => FailureKind::Rejected,
        500..=599 => FailureKind::Server,
        _ => FailureKind::Dropped,
    }
}

/// Execute a real multi-turn agent loop against the configured OpenAI-compatible / Vercel AI Gateway endpoint.
pub fn run_agent_turn(
    cfg: &FxConfig,
    workspace: &Path,
    session: &mut SessionState,
    perms: &PermissionEngine,
    user_prompt: &str,
    save_session: bool,
) -> Result<AgentTurnOutcome, String> {
    let api_key = resolve_api_key(cfg);
    let is_local = cfg.gateway_chat_url.starts_with("http://127.0.0.1")
        || cfg.gateway_chat_url.starts_with("http://localhost");
    if api_key.is_none() && !is_local {
        return Err(
            "No API key configured. Set AI_GATEWAY_API_KEY (or OPENAI_API_KEY) or point FX_GATEWAY_CHAT_URL to a local OpenAI-compatible server (e.g. http://127.0.0.1:11434/v1/chat/completions)."
                .to_owned(),
        );
    }

    session.push_turn("user", user_prompt);

    // Compact older turns if we crossed FX_AUTO_COMPACT_PERCENT
    let used_tokens: usize = session.turns.iter().map(|t| t.estimated_tokens).sum();
    let projected_turns = if should_compact(used_tokens, 128_000, cfg.auto_compact_percent) {
        let (_, proj) = compact_turns(&session.turns, 6);
        proj
    } else {
        session.turns.clone()
    };

    let mut messages: Vec<Value> = Vec::with_capacity(projected_turns.len() + 1);
    messages.push(json!({ "role": "system", "content": build_system_prompt(workspace, cfg) }));
    for t in &projected_turns {
        messages.push(json!({ "role": t.role, "content": t.content }));
    }

    let tools = openai_tools_json();
    let mut steps = 0u32;
    let mut tool_calls_executed = 0usize;
    let mut total_prompt_tokens = 0u64;
    let mut total_completion_tokens = 0u64;
    let mut final_output = String::new();

    while steps < cfg.max_agent_steps {
        steps += 1;
        let resp_json = send_with_recovery(cfg, api_key.as_deref(), &messages, &tools)?;

        if let Some(usage) = resp_json.get("usage") {
            let pt = usage.get("prompt_tokens").and_then(Value::as_u64).unwrap_or(0);
            let ct = usage.get("completion_tokens").and_then(Value::as_u64).unwrap_or(0);
            total_prompt_tokens += pt;
            total_completion_tokens += ct;
            session.totals.record(pt, ct, 0, 0, 0.0);
        }

        let msg = resp_json
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|arr| arr.first())
            .and_then(|c| c.get("message"))
            .ok_or_else(|| format!("gateway response missing choices[0].message: {resp_json}"))?;

        let content_str = msg.get("content").and_then(Value::as_str).unwrap_or("").to_owned();
        let tool_calls = msg.get("tool_calls").and_then(Value::as_array);

        if let Some(calls) = tool_calls
            && !calls.is_empty()
        {
            messages.push(msg.clone());
            for call in calls {
                let call_id = call.get("id").and_then(Value::as_str).unwrap_or("call_0");
                let func = call.get("function").unwrap_or(&Value::Null);
                let tool_name = func.get("name").and_then(Value::as_str).unwrap_or("");
                let raw_args = func.get("arguments").and_then(Value::as_str).unwrap_or("{}");
                let parsed_args: Value = serde_json::from_str(raw_args).unwrap_or_else(|_| json!({}));

                let target = parsed_args
                    .get("path")
                    .or_else(|| parsed_args.get("command"))
                    .or_else(|| parsed_args.get("url"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let target_kind = if tool_name == "shell" {
                    PermissionTargetKind::CommandCwd
                } else if parsed_args.get("path").is_some() {
                    PermissionTargetKind::PathExisting
                } else {
                    PermissionTargetKind::None
                };

                let decision = perms.evaluate(workspace, tool_name, target, target_kind);
                let tool_output = match decision {
                    RuleDecision::Deny => {
                        json!({ "ok": false, "error": format!("permission denied for `{tool_name}` on `{target}`") })
                    }
                    RuleDecision::Ask | RuleDecision::Review if perms.mode != PermissionMode::Yolo => {
                        json!({
                            "ok": false,
                            "error": format!("tool `{tool_name}` on `{target}` requires approval (mode={}); re-run with --full-access or allowlist", perms.mode.label())
                        })
                    }
                    _ => {
                        // Capture undo snapshot before mutating files on disk.
                        if matches!(tool_name, "write_file" | "edit_file")
                            && !target.is_empty()
                        {
                            let p = if Path::new(target).is_absolute() {
                                Path::new(target).to_path_buf()
                            } else {
                                workspace.join(target)
                            };
                            let prev = std::fs::read_to_string(&p).ok();
                            session.undo_stack.push(UndoEntry {
                                path: p.display().to_string(),
                                previous_content: prev,
                            });
                        }
                        tool_calls_executed += 1;
                        match execute_builtin_tool(workspace, tool_name, &parsed_args) {
                            Ok(val) => val,
                            Err(err) => json!({ "ok": false, "error": err }),
                        }
                    }
                };

                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": call_id,
                    "name": tool_name,
                    "content": tool_output.to_string()
                }));
            }
            continue;
        }

        final_output = content_str;
        break;
    }

    session.push_turn("assistant", &final_output);
    let saved_path = if save_session {
        Some(
            session
                .save_to_dir(&cfg.sessions_dir())?
                .display()
                .to_string(),
        )
    } else {
        None
    };

    Ok(AgentTurnOutcome {
        final_output,
        steps,
        tool_calls_executed,
        prompt_tokens: total_prompt_tokens,
        completion_tokens: total_completion_tokens,
        session_id: session.id.clone(),
        saved_path,
    })
}

fn send_with_recovery(
    cfg: &FxConfig,
    api_key: Option<&str>,
    messages: &[Value],
    tools: &[Value],
) -> Result<Value, String> {
    let mut rec_state = RecoveryState::new();
    let _ = recovery::step(
        &mut rec_state,
        Event::Start(Start {
            fast: cfg.fast,
            ..Start::default()
        }),
        now_ms(),
    );

    let payload = json!({
        "model": cfg.model,
        "messages": messages,
        "tools": tools,
        "stream": false
    });

    for _attempt in 0..4 {
        match post_json(&cfg.gateway_chat_url, api_key, &payload) {
            Ok(resp) if (200..300).contains(&resp.status) => {
                let _ = recovery::step(&mut rec_state, Event::Succeeded, now_ms());
                return serde_json::from_str(&resp.body)
                    .map_err(|e| format!("invalid JSON from gateway: {e} (body={})", &resp.body[..resp.body.len().min(200)]));
            }
            Ok(resp) => {
                let kind = failure_kind_for_http(resp.status, &resp.body);
                let plan = recovery::step(
                    &mut rec_state,
                    Event::Failed(Failure {
                        kind,
                        retry_after_ms: resp.retry_after_ms,
                    }),
                    now_ms(),
                );
                if matches!(
                    kind,
                    FailureKind::SignIn | FailureKind::Quota | FailureKind::Rejected | FailureKind::ContentFilter
                ) {
                    return Err(format!("HTTP {}: {}", resp.status, resp.body.trim()));
                }
                if plan.send.is_none() {
                    let wait_ms = resp.retry_after_ms.unwrap_or(200).min(2_000) as u64;
                    std::thread::sleep(Duration::from_millis(wait_ms));
                    let _ = recovery::step(&mut rec_state, Event::Tick, now_ms() + wait_ms + 30_000);
                }
            }
            Err(err) => {
                let _ = recovery::step(
                    &mut rec_state,
                    Event::Failed(Failure {
                        kind: FailureKind::Offline,
                        retry_after_ms: None,
                    }),
                    now_ms(),
                );
                return Err(err);
            }
        }
    }
    Err("gateway request failed after recovery retries".to_owned())
}

/// Undo the most recent file edit made by the agent on disk.
pub fn undo_last_edit(session: &mut SessionState) -> Result<String, String> {
    let entry = session
        .undo_stack
        .pop()
        .ok_or_else(|| "undo stack is empty".to_owned())?;
    let p = Path::new(&entry.path);
    match entry.previous_content {
        Some(prev) => {
            std::fs::write(p, prev).map_err(|e| e.to_string())?;
            Ok(format!("restored {}", entry.path))
        }
        None => {
            if p.exists() {
                std::fs::remove_file(p).map_err(|e| e.to_string())?;
            }
            Ok(format!("removed newly created {}", entry.path))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn real_multi_turn_tool_calling_and_undo_against_local_http_server() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local test server");
        let addr = listener.local_addr().expect("local addr");
        let turn_counter = Arc::new(AtomicUsize::new(0));
        let counter_bg = turn_counter.clone();

        let server = std::thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().expect("accept");
                let mut raw = Vec::new();
                let mut chunk = [0u8; 4096];
                let mut expected_total: Option<usize> = None;
                loop {
                    let n = stream.read(&mut chunk).expect("read");
                    if n == 0 {
                        break;
                    }
                    raw.extend_from_slice(&chunk[..n]);
                    if expected_total.is_none()
                        && let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n")
                    {
                        let hdrs = String::from_utf8_lossy(&raw[..pos]);
                        let cl = hdrs
                            .lines()
                            .find_map(|l| {
                                let (k, v) = l.split_once(':')?;
                                k.eq_ignore_ascii_case("content-length")
                                    .then(|| v.trim().parse::<usize>().ok())
                                    .flatten()
                            })
                            .unwrap_or(0);
                        expected_total = Some(pos + 4 + cl);
                    }
                    if expected_total.is_some_and(|tot| raw.len() >= tot) {
                        break;
                    }
                }
                let req = String::from_utf8_lossy(&raw);
                let step = counter_bg.fetch_add(1, Ordering::SeqCst);
                let body = if step == 0 {
                    json!({
                        "choices": [{
                            "message": {
                                "role": "assistant",
                                "content": "",
                                "tool_calls": [{
                                    "id": "call_write_1",
                                    "type": "function",
                                    "function": {
                                        "name": "write_file",
                                        "arguments": "{\"path\":\"hello_fx_real.txt\",\"content\":\"written by rust fx agent\"}"
                                    }
                                }]
                            }
                        }],
                        "usage": { "prompt_tokens": 42, "completion_tokens": 12 }
                    })
                    .to_string()
                } else {
                    assert!(req.contains("call_write_1"), "second turn must include tool result");
                    json!({
                        "choices": [{
                            "message": {
                                "role": "assistant",
                                "content": "Created hello_fx_real.txt cleanly."
                            }
                        }],
                        "usage": { "prompt_tokens": 60, "completion_tokens": 10 }
                    })
                    .to_string()
                };
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(resp.as_bytes()).expect("write");
            }
        });

        let tmp = std::env::temp_dir().join(format!("fw-fx-agent-test-{}", now_ms()));
        std::fs::create_dir_all(&tmp).expect("mkdir");
        let mut cfg = FxConfig::from_env();
        cfg.fx_home = tmp.join(".fx");
        cfg.gateway_chat_url = format!("http://{addr}/v1/chat/completions");
        let mut session = SessionState::new(&tmp, &cfg.model, "openai-compatible");
        let perms = PermissionEngine::new(PermissionMode::Auto);

        let outcome = run_agent_turn(&cfg, &tmp, &mut session, &perms, "create hello_fx_real.txt", true)
            .expect("agent turn succeeds");
        server.join().expect("server thread");

        assert_eq!(outcome.steps, 2);
        assert_eq!(outcome.tool_calls_executed, 1);
        assert_eq!(outcome.final_output, "Created hello_fx_real.txt cleanly.");
        assert_eq!(outcome.prompt_tokens, 102);
        assert_eq!(
            std::fs::read_to_string(tmp.join("hello_fx_real.txt")).expect("file written"),
            "written by rust fx agent"
        );
        assert!(outcome.saved_path.is_some());

        // Real undo removes the newly created file from disk!
        let undone = undo_last_edit(&mut session).expect("undo");
        assert!(undone.contains("hello_fx_real.txt"));
        assert!(!tmp.join("hello_fx_real.txt").exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
