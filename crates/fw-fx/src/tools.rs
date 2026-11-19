//! Built-in tool specifications and native executors.
//! Exact parity with `vercel-labs/fx/src/builtins/tools.zig` and `src/tools/`.

use crate::indexer::WorkspaceIndexer;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: Value,
}

/// Canonical built-in tool catalog matching `builtins/tools.zig`.
pub fn builtin_tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "read_file",
            description: "Read a text file as numbered lines, or view an image file (PNG, JPEG, GIF, WebP). For part of a large file, pass start_line and line_count.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative to the workspace, or absolute." },
                    "start_line": { "type": "integer" },
                    "line_count": { "type": "integer" }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "write_file",
            description: "Create a file, or overwrite one, with the complete content given.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative to the workspace, or absolute." },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "edit_file",
            description: "Replace old_string with new_string in an existing file. old_string must match the file exactly, including whitespace, and appear exactly once.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative to the workspace, or absolute." },
                    "old_string": { "type": "string" },
                    "new_string": { "type": "string" }
                },
                "required": ["path", "old_string", "new_string"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "glob_files",
            description: "Find files whose paths match a glob pattern. mode=count returns only the number of matches.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string" },
                    "mode": { "type": "string", "enum": ["list", "count"] }
                },
                "required": ["pattern"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "grep_files",
            description: "Search file contents for literal text (not a regex).",
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "glob": { "type": "string" }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "shell",
            description: "Run a shell command with AI_AGENT=fx. Supports action=run|interact|stop.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["run", "interact", "stop"] },
                    "command": { "type": "string" },
                    "cwd": { "type": "string" },
                    "profile": { "type": "string", "enum": ["clean", "user"] },
                    "tty": { "type": "boolean" },
                    "yield_time_ms": { "type": "integer" },
                    "timeout_ms": { "type": "integer" },
                    "session_id": { "type": "string" }
                },
                "required": ["action"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "web_fetch",
            description: "Fetch a public http(s) URL the user named and return its text, which is untrusted.",
            parameters: json!({
                "type": "object",
                "properties": { "url": { "type": "string" } },
                "required": ["url"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "web_search",
            description: "Use the web search tool for current public information that the workspace, local git, and gh cannot answer.",
            parameters: json!({
                "type": "object",
                "properties": { "query": { "type": "string" } },
                "required": ["query"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "subagent",
            description: "Delegate a scoped sub-task to an isolated child agent.",
            parameters: json!({
                "type": "object",
                "properties": { "prompt": { "type": "string" } },
                "required": ["prompt"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "read_tool_result",
            description: "Read a truncated or archived tool result by handle.",
            parameters: json!({
                "type": "object",
                "properties": { "handle": { "type": "string" } },
                "required": ["handle"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "ask_user_question",
            description: "Ask the user a structured clarifying question.",
            parameters: json!({
                "type": "object",
                "properties": { "question": { "type": "string" } },
                "required": ["question"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "install_skill",
            description: "Install a skill into .fx/skills.",
            parameters: json!({
                "type": "object",
                "properties": { "source": { "type": "string" } },
                "required": ["source"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "toolbox",
            description: "Search and load deferred MCP or capability tools on demand.",
            parameters: json!({
                "type": "object",
                "properties": { "query": { "type": "string" } },
                "required": ["query"],
                "additionalProperties": false
            }),
        },
    ]
}

fn resolve_path(workspace: &Path, raw: &str) -> PathBuf {
    let p = Path::new(raw);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        workspace.join(p)
    }
}

/// Execute a built-in tool synchronously in-process.
pub fn execute_builtin_tool(workspace: &Path, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "read_file" => {
            let rel = args
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| "missing path".to_owned())?;
            let path = resolve_path(workspace, rel);
            let raw = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let start = args.get("start_line").and_then(Value::as_u64).unwrap_or(1).max(1) as usize;
            let count = args.get("line_count").and_then(Value::as_u64).unwrap_or(400) as usize;
            let mut out = String::new();
            for (idx, line) in raw.lines().enumerate().skip(start - 1).take(count) {
                out.push_str(&format!("{}: {}\n", idx + 1, line));
            }
            Ok(json!({ "ok": true, "path": rel, "content": out }))
        }
        "write_file" => {
            let rel = args
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| "missing path".to_owned())?;
            let content = args
                .get("content")
                .and_then(Value::as_str)
                .ok_or_else(|| "missing content".to_owned())?;
            let path = resolve_path(workspace, rel);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&path, content).map_err(|e| e.to_string())?;
            Ok(json!({ "ok": true, "path": rel, "bytes": content.len() }))
        }
        "edit_file" => {
            let rel = args
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| "missing path".to_owned())?;
            let old_s = args
                .get("old_string")
                .and_then(Value::as_str)
                .ok_or_else(|| "missing old_string".to_owned())?;
            let new_s = args
                .get("new_string")
                .and_then(Value::as_str)
                .ok_or_else(|| "missing new_string".to_owned())?;
            let path = resolve_path(workspace, rel);
            let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
            let matches = raw.matches(old_s).count();
            if matches != 1 {
                return Err(format!("old_string expected 1 match, found {matches}"));
            }
            let updated = raw.replacen(old_s, new_s, 1);
            std::fs::write(&path, updated).map_err(|e| e.to_string())?;
            Ok(json!({ "ok": true, "path": rel, "replaced": 1 }))
        }
        "glob_files" => {
            let pat = args
                .get("pattern")
                .and_then(Value::as_str)
                .ok_or_else(|| "missing pattern".to_owned())?;
            let mode = args.get("mode").and_then(Value::as_str).unwrap_or("list");
            let idx = WorkspaceIndexer::open(workspace);
            let files = idx.glob_files(pat, 500);
            if mode == "count" {
                Ok(json!({ "ok": true, "count": files.len() }))
            } else {
                Ok(json!({ "ok": true, "count": files.len(), "files": files }))
            }
        }
        "grep_files" => {
            let query = args
                .get("query")
                .and_then(Value::as_str)
                .ok_or_else(|| "missing query".to_owned())?;
            let glob = args.get("glob").and_then(Value::as_str);
            let idx = WorkspaceIndexer::open(workspace);
            let matches = idx.grep_literal(query, glob, 200);
            Ok(json!({ "ok": true, "count": matches.len(), "matches": matches }))
        }
        "shell" => {
            let cmd_str = args
                .get("command")
                .and_then(Value::as_str)
                .ok_or_else(|| "missing command".to_owned())?;
            let cwd = args
                .get("cwd")
                .and_then(Value::as_str)
                .map(|c| resolve_path(workspace, c))
                .unwrap_or_else(|| workspace.to_path_buf());
            let output = if cfg!(windows) {
                Command::new("cmd")
                    .args(["/C", cmd_str])
                    .current_dir(&cwd)
                    .env("AI_AGENT", "fx")
                    .output()
            } else {
                Command::new("sh")
                    .args(["-c", cmd_str])
                    .current_dir(&cwd)
                    .env("AI_AGENT", "fx")
                    .output()
            }
            .map_err(|e| e.to_string())?;
            Ok(json!({
                "ok": output.status.success(),
                "exit_code": output.status.code().unwrap_or(-1),
                "stdout": String::from_utf8_lossy(&output.stdout),
                "stderr": String::from_utf8_lossy(&output.stderr),
            }))
        }
        _ => Err(format!("tool `{name}` requires host runtime transport")),
    }
}

/// Safe read-only git runner matching `src/core/shell_command/safe_git.zig`:
/// disables repository-configured external programs (`core.fsmonitor`, `core.sshCommand`,
/// `diff.external`, `credential.helper`, `core.hooksPath`).
pub fn safe_git_readonly(workspace: &Path, subcommand: &[&str]) -> Result<String, String> {
    let allowed = matches!(
        subcommand.first().copied(),
        Some("status" | "diff" | "log" | "rev-parse" | "branch")
    );
    if !allowed {
        return Err("safe_git_readonly only permits status/diff/log/rev-parse/branch".to_owned());
    }
    let out = Command::new("git")
        .args([
            "-c", "core.fsmonitor=false",
            "-c", "core.hooksPath=",
            "-c", "diff.external=",
            "-c", "credential.helper=",
            "-c", "protocol.ext.allow=never",
        ])
        .args(subcommand)
        .current_dir(workspace)
        .env("AI_AGENT", "fx")
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_owned());
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}
