//! Reusable Intelligent Terminal -> Rio GPU Terminal bridge:
//! - OSC 7501 sidebar/agent program status (`\x1b]7501;<state>;<label>\x07`)
//! - Native Windows ConPTY + NamedPipe (`\\.\pipe\rio-ai-master-<id>`) + SDDL ACLs
//! - Cloudflare Workers AI / AI Gateway + Streamable HTTP MCP/ACP/A2A edge routing

use serde::{Deserialize, Serialize};

/// Supported ACP / delegation providers from Microsoft Intelligent Terminal (`AgentRegistry.h`).
pub const BUILTIN_ACP_AGENTS: &[&str] = &[
    "copilot",
    "claude",
    "codex",
    "gemini",
    "opencode",
    "antigravity",
];

/// Session MCP tools exposed by Intelligent Terminal's master endpoint.
pub const SESSION_MCP_TOOLS: &[&str] = &[
    "run_command_in_current_shell",
    "create_workspace",
    "delegate_task_in_new_workspace",
    "request_user_input",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Osc7501State {
    Idle,
    Working,
    NeedsInput,
    Error,
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Osc7501Status {
    pub state: Osc7501State,
    pub label: String,
}

impl Osc7501Status {
    /// Encodes an OSC 7501 program status escape sequence (`\x1b]7501;<state>;<label>\x07`).
    pub fn encode(&self) -> String {
        let s = match self.state {
            Osc7501State::Idle => "idle",
            Osc7501State::Working => "working",
            Osc7501State::NeedsInput => "needs-input",
            Osc7501State::Error => "error",
            Osc7501State::Done => "done",
        };
        format!("\x1b]7501;{s};{}\x07", self.label)
    }

    /// Parses an OSC 7501 payload (`7501;<state>;<label>` or full escape sequence).
    pub fn parse(raw: &str) -> Option<Self> {
        let stripped = raw
            .trim()
            .strip_prefix("\x1b]")
            .unwrap_or(raw.trim())
            .trim_end_matches('\x07')
            .trim_end_matches("\x1b\\");
        let rest = stripped.strip_prefix("7501;")?;
        let mut parts = rest.splitn(2, ';');
        let state = match parts.next()?.trim().to_ascii_lowercase().as_str() {
            "idle" => Osc7501State::Idle,
            "working" | "running" => Osc7501State::Working,
            "needs-input" | "waiting" => Osc7501State::NeedsInput,
            "error" | "failed" => Osc7501State::Error,
            "done" | "complete" => Osc7501State::Done,
            _ => return None,
        };
        let label = parts.next().unwrap_or("").trim().to_string();
        Some(Self { state, label })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowsNativeTerminalConfig {
    pub conpty_in_process: bool,
    pub master_pipe_name: String,
    pub sddl_current_user_only: String,
    pub renderer_backend: String,
    pub clipboard_dib_to_png: bool,
}

impl WindowsNativeTerminalConfig {
    pub fn default_for_session(session_id: &str) -> Self {
        Self {
            conpty_in_process: true,
            master_pipe_name: format!(r"\\.\pipe\rio-wta-master-{session_id}"),
            sddl_current_user_only: "D:(A;;GA;;;OW)".into(),
            renderer_backend: "wgpu-dx12-directwrite".into(),
            clipboard_dib_to_png: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudflareAiEdgeConfig {
    pub gateway_base_url: String,
    pub mcp_endpoint: String,
    pub acp_endpoint: String,
    pub a2a_endpoint: String,
    pub zero_trust_access: bool,
    pub workers_ai_model: String,
}

impl Default for CloudflareAiEdgeConfig {
    fn default() -> Self {
        Self {
            gateway_base_url: "https://gateway.ai.cloudflare.com/v1".into(),
            mcp_endpoint: "/mcp".into(),
            acp_endpoint: "/acp".into(),
            a2a_endpoint: "/a2a".into(),
            zero_trust_access: true,
            workers_ai_model: "@cf/meta/llama-3.3-70b-instruct-fp8-fast".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osc7501_windows_and_cloudflare_bridge_roundtrip() {
        let status = Osc7501Status {
            state: Osc7501State::Working,
            label: "building rio-ai".into(),
        };
        let encoded = status.encode();
        let parsed = Osc7501Status::parse(&encoded).expect("parse osc7501");
        assert_eq!(parsed, status);

        let win = WindowsNativeTerminalConfig::default_for_session("tab-01");
        assert!(win.master_pipe_name.starts_with(r"\\.\pipe\rio-wta-master-"));
        assert!(win.conpty_in_process);

        let cf = CloudflareAiEdgeConfig::default();
        assert_eq!(cf.mcp_endpoint, "/mcp");
        assert!(cf.zero_trust_access);
        assert!(BUILTIN_ACP_AGENTS.contains(&"antigravity"));
        assert_eq!(SESSION_MCP_TOOLS.len(), 4);
    }
}
