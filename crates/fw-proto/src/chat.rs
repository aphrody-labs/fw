//! OpenAI-compatible chat for the control plane: requests go to the gateway that
//! `FxConfig` names (Vercel AI Gateway, OpenAI, OpenRouter, or a local llama.cpp /
//! Ollama server). Nothing is answered locally: a missing key or an unreachable
//! gateway is reported as an error that says what to set.

use fw_fx::agent::post_json;
use fw_fx::config::FxConfig;
use serde_json::{json, Value};
use std::time::Duration;

const JSON: &str = "application/json";

fn error_body(status: u16, kind: &str, message: impl Into<String>) -> (u16, &'static str, String) {
    (status, JSON, json!({ "error": { "message": message.into(), "type": kind } }).to_string())
}

fn is_local(url: &str) -> bool {
    ["http://127.0.0.1", "http://localhost", "http://[::1]"]
        .iter()
        .any(|p| url.starts_with(p))
}

/// `…/v1/chat/completions` -> `…/v1/models`.
fn models_url(chat_url: &str) -> String {
    let base = chat_url.trim_end_matches('/');
    let base = base.strip_suffix("/chat/completions").unwrap_or(base);
    format!("{base}/models")
}

/// Forwards one chat completion to the configured gateway. The control plane answers
/// whole responses, so `stream` is forced off; a `tool_choice` naming an in-process
/// tool (not an OpenAI value) is dropped rather than forwarded as an invalid field.
pub fn proxy_chat_completions(cfg: &FxConfig, api_key: Option<&str>, mut body: Value) -> (u16, &'static str, String) {
    let Some(obj) = body.as_object_mut() else {
        return error_body(400, "invalid_request_error", "request body must be a JSON object");
    };
    if !obj.get("messages").and_then(Value::as_array).is_some_and(|m| !m.is_empty()) {
        return error_body(400, "invalid_request_error", "`messages` must be a non-empty array");
    }
    if !obj.get("model").and_then(Value::as_str).is_some_and(|m| !m.is_empty()) {
        obj.insert("model".into(), json!(cfg.model));
    }
    obj.insert("stream".into(), json!(false));
    if let Some(choice) = obj.get("tool_choice").and_then(Value::as_str)
        && !matches!(choice, "auto" | "none" | "required")
    {
        obj.remove("tool_choice");
    }

    if api_key.is_none() && !is_local(&cfg.gateway_chat_url) {
        return error_body(
            503,
            "fw_no_credentials",
            format!(
                "No model credentials for {}. Set AI_GATEWAY_API_KEY (or OPENAI_API_KEY, OPENROUTER_API_KEY), or run `fw config set gateway_chat_url http://127.0.0.1:8080/v1/chat/completions` to use a local llama.cpp server.",
                cfg.gateway_chat_url
            ),
        );
    }

    match post_json(&cfg.gateway_chat_url, api_key, &body) {
        Ok(resp) if serde_json::from_str::<Value>(&resp.body).is_ok() => (resp.status, JSON, resp.body),
        Ok(resp) => error_body(
            502,
            "fw_gateway_error",
            format!(
                "{} answered HTTP {} with a non-JSON body: {}",
                cfg.gateway_chat_url,
                resp.status,
                resp.body.chars().take(300).collect::<String>()
            ),
        ),
        Err(e) => error_body(502, "fw_gateway_unreachable", format!("{e}. Check that the gateway is up or change gateway_chat_url.")),
    }
}

/// Lists the gateway's models, with the configured model first. When the gateway
/// cannot be listed, the configured model is still returned together with the reason.
pub fn list_models(cfg: &FxConfig, api_key: Option<&str>) -> Value {
    let url = models_url(&cfg.gateway_chat_url);
    let fetched: Result<Vec<Value>, String> = (|| {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(10)))
            .build()
            .into();
        let mut req = agent.get(&url).header("User-Agent", concat!("fw/", env!("CARGO_PKG_VERSION")));
        if let Some(key) = api_key.filter(|k| !k.is_empty()) {
            req = req.header("Authorization", &format!("Bearer {key}"));
        }
        let mut resp = req.call().map_err(|e| format!("GET {url}: {e}"))?;
        let status = resp.status().as_u16();
        let text = resp.body_mut().read_to_string().map_err(|e| format!("GET {url}: {e}"))?;
        if !(200..300).contains(&status) {
            return Err(format!("GET {url}: HTTP {status}"));
        }
        let v: Value = serde_json::from_str(&text).map_err(|e| format!("GET {url}: {e}"))?;
        v.get("data")
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| format!("GET {url}: no `data` array"))
    })();

    let configured = json!({ "id": cfg.model, "object": "model", "owned_by": "fw-config" });
    match fetched {
        Ok(mut data) => {
            match data.iter().position(|m| m.get("id").and_then(Value::as_str) == Some(cfg.model.as_str())) {
                Some(i) => data.swap(0, i),
                None => data.insert(0, configured),
            }
            json!({ "object": "list", "data": data, "default": cfg.model })
        }
        Err(e) => json!({ "object": "list", "data": [configured], "default": cfg.model, "error": e }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_with(url: &str) -> FxConfig {
        let mut cfg = FxConfig::from_env();
        cfg.gateway_chat_url = url.to_owned();
        cfg
    }

    #[test]
    fn models_url_derives_from_chat_url() {
        assert_eq!(models_url("https://ai-gateway.vercel.sh/v1/chat/completions"), "https://ai-gateway.vercel.sh/v1/models");
        assert_eq!(models_url("http://127.0.0.1:8080/v1/chat/completions/"), "http://127.0.0.1:8080/v1/models");
    }

    #[test]
    fn rejects_bodies_without_messages() {
        let cfg = cfg_with("http://127.0.0.1:9/v1/chat/completions");
        assert_eq!(proxy_chat_completions(&cfg, None, json!([])).0, 400);
        assert_eq!(proxy_chat_completions(&cfg, None, json!({ "messages": [] })).0, 400);
    }

    #[test]
    fn remote_gateway_without_key_is_503_with_remedy() {
        let cfg = cfg_with("https://gateway.invalid/v1/chat/completions");
        let (code, _, body) = proxy_chat_completions(&cfg, None, json!({ "messages": [{ "role": "user", "content": "hi" }] }));
        assert_eq!(code, 503);
        assert!(body.contains("AI_GATEWAY_API_KEY"), "{body}");
    }

    #[test]
    fn unreachable_local_gateway_is_502_not_a_fake_answer() {
        // Port 9 (discard) is closed on CI hosts and developer machines.
        let cfg = cfg_with("http://127.0.0.1:9/v1/chat/completions");
        let (code, _, body) = proxy_chat_completions(&cfg, None, json!({ "messages": [{ "role": "user", "content": "hi" }] }));
        assert_eq!(code, 502, "{body}");
        assert!(!body.contains("\"choices\""));
    }

    #[test]
    fn unreachable_gateway_still_lists_configured_model() {
        let cfg = cfg_with("http://127.0.0.1:9/v1/chat/completions");
        let v = list_models(&cfg, None);
        assert_eq!(v["data"][0]["id"], json!(cfg.model));
        assert!(v["error"].is_string());
    }
}
