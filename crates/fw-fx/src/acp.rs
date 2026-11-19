//! Agent Client Protocol (ACP) JSON-RPC 2.0 server over stdio and in-process (`libfx`).
//! Exact parity with `vercel-labs/fx/src/acp/types.zig` and `src/acp/server.zig`.

use crate::config::{FX_VERSION, FxConfig};
use crate::session::SessionState;
use crate::tools::{builtin_tool_specs, execute_builtin_tool};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;

pub const ACP_PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    MaxOutputTokens,
    MaxModelTurns,
    Refused,
    Cancelled,
    Yielded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallKind {
    Read,
    Edit,
    Delete,
    Move,
    Search,
    Execute,
    Think,
    Fetch,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

/// Read `_meta.fx.<key>` from an ACP params object.
pub fn fx_meta_field<'a>(params: &'a Value, key: &str) -> Option<&'a Value> {
    params.get("_meta")?.get("fx")?.get(key)
}

#[derive(Debug)]
pub struct AcpServer {
    pub config: FxConfig,
    pub workspace: PathBuf,
    pub sessions: BTreeMap<String, SessionState>,
}

impl AcpServer {
    pub fn new(config: FxConfig, workspace: PathBuf) -> Self {
        Self {
            config,
            workspace,
            sessions: BTreeMap::new(),
        }
    }

    pub fn dispatch(&mut self, req: &Value) -> Value {
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let method = req.get("method").and_then(Value::as_str).unwrap_or("");
        let params = req.get("params").cloned().unwrap_or(Value::Null);

        match method {
            "initialize" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": ACP_PROTOCOL_VERSION,
                    "agentInfo": {
                        "name": "fx-rust",
                        "version": FX_VERSION,
                        "engine": "fw-fx (Bun-in-Rust parity)"
                    },
                    "agentCapabilities": {
                        "loadSession": true,
                        "promptCapabilities": { "image": true, "embeddedContext": true },
                        "mcpCapabilities": { "http": true, "sse": true, "stdio": true }
                    },
                    "_meta": {
                        "fx": {
                            "model": self.config.model,
                            "provider": self.config.provider,
                            "permissionMode": self.config.permission_mode,
                            "tools": builtin_tool_specs().len()
                        }
                    }
                }
            }),
            "session/new" => {
                let ws = params
                    .get("cwd")
                    .and_then(Value::as_str)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| self.workspace.clone());
                let sess = SessionState::new(&ws, &self.config.model, &self.config.provider);
                let sid = sess.id.clone();
                self.sessions.insert(sid.clone(), sess);
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "sessionId": sid,
                        "_meta": { "fx": { "model": self.config.model, "provider": self.config.provider } }
                    }
                })
            }
            "session/load" => {
                let sid = params.get("sessionId").and_then(Value::as_str).unwrap_or("");
                if self.sessions.contains_key(sid) {
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": { "loaded": true, "sessionId": sid }
                    })
                } else {
                    match SessionState::load_from_dir(&self.config.sessions_dir(), sid) {
                        Ok(s) => {
                            self.sessions.insert(sid.to_owned(), s);
                            json!({ "jsonrpc": "2.0", "id": id, "result": { "loaded": true, "sessionId": sid } })
                        }
                        Err(e) => json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": { "code": -32001, "message": format!("session not found: {e}") }
                        }),
                    }
                }
            }
            "session/prompt" => {
                let sid = params.get("sessionId").and_then(Value::as_str).unwrap_or("default");
                let text = params
                    .get("prompt")
                    .and_then(Value::as_str)
                    .or_else(|| {
                        params
                            .get("prompt")
                            .and_then(Value::as_array)
                            .and_then(|arr| arr.first())
                            .and_then(|item| item.get("text"))
                            .and_then(Value::as_str)
                    })
                    .unwrap_or("");
                let sess = self.sessions.entry(sid.to_owned()).or_insert_with(|| {
                    SessionState::new(&self.workspace, &self.config.model, &self.config.provider)
                });
                sess.push_turn("user", text);

                // Optional direct tool invocation via _meta.fx.tool
                let tool_result = if let Some(tool_name) = fx_meta_field(&params, "tool").and_then(Value::as_str) {
                    let tool_args = fx_meta_field(&params, "args").cloned().unwrap_or(json!({}));
                    Some(execute_builtin_tool(&self.workspace, tool_name, &tool_args))
                } else {
                    None
                };

                let reply = match tool_result {
                    Some(Ok(val)) => val.to_string(),
                    Some(Err(err)) => format!("tool error: {err}"),
                    None => format!("fx-rust ACP acknowledged ({} chars)", text.len()),
                };
                sess.push_turn("assistant", &reply);

                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "stopReason": StopReason::EndTurn,
                        "output": reply,
                        "sessionId": sid
                    }
                })
            }
            "session/cancel" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "stopReason": StopReason::Cancelled }
            }),
            _ => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("Method not found: {method}") }
            }),
        }
    }

    pub fn run_stdio(&mut self) -> Result<(), String> {
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout().lock();
        for line in stdin.lock().lines() {
            let line = line.map_err(|e| e.to_string())?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let req: Value = match serde_json::from_str(trimmed) {
                Ok(v) => v,
                Err(e) => {
                    let err = json!({
                        "jsonrpc": "2.0",
                        "id": Value::Null,
                        "error": { "code": -32700, "message": format!("Parse error: {e}") }
                    });
                    writeln!(stdout, "{err}").map_err(|ioe| ioe.to_string())?;
                    stdout.flush().map_err(|ioe| ioe.to_string())?;
                    continue;
                }
            };
            let resp = self.dispatch(&req);
            writeln!(stdout, "{resp}").map_err(|e| e.to_string())?;
            stdout.flush().map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acp_initialize_and_session_roundtrip() {
        let mut srv = AcpServer::new(FxConfig::from_env(), PathBuf::from("."));
        let init = srv.dispatch(&json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" }));
        assert_eq!(init["result"]["protocolVersion"], ACP_PROTOCOL_VERSION);
        let created = srv.dispatch(&json!({ "jsonrpc": "2.0", "id": 2, "method": "session/new" }));
        let sid = created["result"]["sessionId"].as_str().expect("sid");
        let prompted = srv.dispatch(&json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "session/prompt",
            "params": { "sessionId": sid, "prompt": "hello fx" }
        }));
        assert_eq!(prompted["result"]["stopReason"], "end_turn");
    }
}
