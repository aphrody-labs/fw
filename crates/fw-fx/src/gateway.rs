//! Provider transport, SSE parser, and model catalog matching `vercel-labs/fx/src/gateway/`.
//! Supports Vercel AI Gateway, OpenAI Codex/ChatGPT, xAI Grok, Anthropic Messages,
//! and OpenAI-compatible Chat Completions (Ollama, OpenRouter, custom endpoints).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderProtocol {
    VercelGateway,
    OpenAiChatCompletions,
    OpenAiCodexResponses,
    AnthropicMessages,
    XaiGrok,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSpec {
    pub id: &'static str,
    pub display_name: &'static str,
    pub protocol: ProviderProtocol,
    pub base_url: &'static str,
    pub supports_prompt_caching: bool,
    pub supports_fast_route: bool,
}

pub fn builtin_providers() -> Vec<ProviderSpec> {
    vec![
        ProviderSpec {
            id: "gateway",
            display_name: "Vercel AI Gateway",
            protocol: ProviderProtocol::VercelGateway,
            base_url: "https://ai-gateway.vercel.sh",
            supports_prompt_caching: true,
            supports_fast_route: true,
        },
        ProviderSpec {
            id: "codex",
            display_name: "ChatGPT / OpenAI Codex",
            protocol: ProviderProtocol::OpenAiCodexResponses,
            base_url: "https://chatgpt.com/backend-api/codex",
            supports_prompt_caching: true,
            supports_fast_route: false,
        },
        ProviderSpec {
            id: "grok",
            display_name: "xAI Grok",
            protocol: ProviderProtocol::XaiGrok,
            base_url: "https://api.x.ai/v1",
            supports_prompt_caching: true,
            supports_fast_route: true,
        },
        ProviderSpec {
            id: "openai-compatible",
            display_name: "OpenAI-Compatible (Ollama / OpenRouter / Custom)",
            protocol: ProviderProtocol::OpenAiChatCompletions,
            base_url: "http://127.0.0.1:11434/v1",
            supports_prompt_caching: false,
            supports_fast_route: false,
        },
        ProviderSpec {
            id: "anthropic",
            display_name: "Anthropic Messages",
            protocol: ProviderProtocol::AnthropicMessages,
            base_url: "https://api.anthropic.com/v1",
            supports_prompt_caching: true,
            supports_fast_route: false,
        },
    ]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

/// Zero-copy-friendly Server-Sent Events parser matching `src/gateway/sse.zig`.
pub fn parse_sse_frames(raw: &str) -> Vec<SseEvent> {
    let mut events = Vec::new();
    let mut current_event: Option<String> = None;
    let mut data_lines: Vec<&str> = Vec::new();

    for line in raw.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            if !data_lines.is_empty() {
                events.push(SseEvent {
                    event: current_event.take(),
                    data: data_lines.join("\n"),
                });
                data_lines.clear();
            } else {
                current_event = None;
            }
            continue;
        }
        if line.starts_with(':') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("event:") {
            current_event = Some(rest.trim().to_owned());
        } else if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.strip_prefix(' ').unwrap_or(rest));
        }
    }
    if !data_lines.is_empty() {
        events.push(SseEvent {
            event: current_event,
            data: data_lines.join("\n"),
        });
    }
    events
}

/// Build an OpenAI Chat Completions request payload for Vercel AI Gateway / Ollama / OpenRouter.
pub fn build_chat_completions_request(model: &str, system_prompt: &str, user_prompt: &str, stream: bool) -> Value {
    let mut messages = Vec::new();
    if !system_prompt.is_empty() {
        messages.push(json!({ "role": "system", "content": system_prompt }));
    }
    messages.push(json!({ "role": "user", "content": user_prompt }));
    json!({
        "model": model,
        "stream": stream,
        "messages": messages
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sse_frames_cleanly() {
        let raw = "event: delta\ndata: {\"text\":\"hi\"}\n\ndata: [DONE]\n\n";
        let frames = parse_sse_frames(raw);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].event.as_deref(), Some("delta"));
        assert_eq!(frames[1].data, "[DONE]");
    }
}
