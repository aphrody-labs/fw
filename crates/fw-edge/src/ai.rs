//! Workers AI gateway: `/v1/models`, `/v1/chat/completions` (JSON and SSE re-framed as OpenAI chunks),
//! `/v1/embeddings`, default model selection with the 10 minute catalog cache.
//! Port of `listEdgeModels`, `defaultEdgeModel`, `defaultEmbeddingModel`, `toOpenAiChunks`, `handleChatCompletions`
//! and the embeddings route in `cloudflare/worker.ts`.
//!
//! The pure parts (ranking, default selection, text extraction, SSE re-framing) are plain functions; the runtime
//! binding sits behind the [`Inference`] trait so they run in the native unit tests with a mock.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::rc::Rc;

use futures_util::{Stream, StreamExt, stream};
use serde_json::{Map, Value, json};
use worker::{ByteStream, Env};

use crate::os::{body_object, js_string, make_id, now_ms, num_value, truthy};

const MODEL_CACHE_MS: f64 = 10.0 * 60.0 * 1000.0;
const NO_AI: &str = "This Worker has no Workers AI binding: add `\"ai\": { \"binding\": \"AI\" }` to wrangler.jsonc and redeploy";

#[derive(Debug)]
pub enum AiError {
    /// `InferenceUnavailableError`: no binding, or an empty catalog.
    Unavailable(String),
    Upstream(String),
}

impl AiError {
    pub fn message(&self) -> &str {
        match self {
            AiError::Unavailable(m) | AiError::Upstream(m) => m,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawModel {
    pub name: String,
    pub description: Option<String>,
    pub properties: Vec<(String, Value)>,
}

impl RawModel {
    fn prop(&self, id: &str) -> Option<&Value> {
        self.properties.iter().find(|(k, _)| k == id).map(|(_, v)| v)
    }
}

/// Parses the array returned by `env.AI.models()`.
pub fn raw_models_from_json(v: &Value) -> Vec<RawModel> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|m| {
                    Some(RawModel {
                        name: m.get("name")?.as_str()?.to_string(),
                        description: m.get("description").and_then(Value::as_str).map(String::from),
                        properties: m
                            .get("properties")
                            .and_then(Value::as_array)
                            .map(|ps| {
                                ps.iter()
                                    .filter_map(|p| Some((p.get("property_id")?.as_str()?.to_string(), p.get("value").cloned().unwrap_or(Value::Null))))
                                    .collect()
                            })
                            .unwrap_or_default(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq)]
pub struct EdgeModel {
    pub id: String,
    pub description: Option<String>,
    pub context_window: Option<f64>,
    pub function_calling: bool,
    pub reasoning: bool,
    pub vision: bool,
    pub paid: bool,
    pub price_per_m_input: Option<f64>,
}

impl EdgeModel {
    pub fn to_value(&self) -> Value {
        let mut m = Map::new();
        m.insert("id".into(), self.id.clone().into());
        m.insert("object".into(), "model".into());
        m.insert("owned_by".into(), "cloudflare-workers-ai".into());
        if let Some(d) = &self.description {
            m.insert("description".into(), d.clone().into());
        }
        if let Some(c) = self.context_window {
            m.insert("context_window".into(), num_value(c));
        }
        m.insert("function_calling".into(), self.function_calling.into());
        m.insert("reasoning".into(), self.reasoning.into());
        m.insert("vision".into(), self.vision.into());
        m.insert("paid".into(), self.paid.into());
        if let Some(p) = self.price_per_m_input {
            m.insert("price_per_m_input".into(), num_value(p));
        }
        Value::Object(m)
    }
}

/// JavaScript `Number(value)`; `None` is NaN.
fn js_number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() { Some(0.0) } else { t.parse::<f64>().ok() }
        }
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        Value::Null => Some(0.0),
        _ => None,
    }
}

fn input_price(price: Option<&Value>) -> Option<f64> {
    let list = price?.as_array()?;
    let entry = list
        .iter()
        .find(|p| p.get("unit").and_then(Value::as_str).is_some_and(|u| u.contains("input")))
        .or_else(|| list.first())?;
    entry.get("price").and_then(Value::as_f64)
}

/// ICU-root-like ordering for ASCII (`String.prototype.localeCompare`): punctuation, digits, then letters
/// case-insensitively with lower case first on ties.
fn locale_cmp(a: &str, b: &str) -> Ordering {
    const PUNCT: &str = "_-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$";
    let primary = |c: char| -> (u8, u32) {
        if let Some(i) = PUNCT.find(c) {
            (1, i as u32)
        } else if c.is_ascii_digit() {
            (2, c as u32)
        } else {
            (3, c.to_lowercase().next().unwrap_or(c) as u32)
        }
    };
    let pa: Vec<_> = a.chars().map(primary).collect();
    let pb: Vec<_> = b.chars().map(primary).collect();
    pa.cmp(&pb).then_with(|| {
        // tertiary: lower case before upper case
        a.chars().zip(b.chars()).map(|(x, y)| y.is_lowercase().cmp(&x.is_lowercase())).find(|o| o.is_ne()).unwrap_or(Ordering::Equal)
    })
}

/// Catalog filter and ranking of `listEdgeModels`: `@cf/` only, no deprecated, LoRA or beta models,
/// tool-calling first, then largest context, then id.
pub fn rank_models(raw: &[RawModel]) -> Vec<EdgeModel> {
    let mut models: Vec<EdgeModel> = raw
        .iter()
        .filter(|m| m.name.starts_with("@cf/"))
        .filter(|m| !["planned_deprecation_date", "lora", "beta"].iter().any(|p| m.prop(p).is_some()))
        .map(|m| EdgeModel {
            id: m.name.clone(),
            description: m.description.clone(),
            context_window: m.prop("context_window").and_then(js_number).filter(|n| *n != 0.0),
            function_calling: m.prop("function_calling").is_some(),
            reasoning: m.prop("reasoning").is_some(),
            vision: m.prop("vision").is_some(),
            paid: m.prop("require_workers_paid").is_some(),
            price_per_m_input: input_price(m.prop("price")),
        })
        .collect();
    models.sort_by(|x, y| {
        y.function_calling
            .cmp(&x.function_calling)
            .then_with(|| y.context_window.unwrap_or(0.0).total_cmp(&x.context_window.unwrap_or(0.0)))
            .then_with(|| locale_cmp(&x.id, &y.id))
    });
    models
}

/// `defaultEdgeModel`: the largest-context tool-calling model that runs on any plan.
pub fn pick_chat_model(models: &[EdgeModel]) -> Option<&EdgeModel> {
    models.iter().find(|m| m.function_calling && !m.paid).or_else(|| models.first())
}

/// `defaultEmbeddingModel`: the cheapest per input token on any plan, then the largest context.
pub fn pick_embedding_model(models: &[EdgeModel]) -> Option<&EdgeModel> {
    let mut free: Vec<&EdgeModel> = models.iter().filter(|m| !m.paid).collect();
    free.sort_by(|x, y| {
        x.price_per_m_input
            .unwrap_or(f64::INFINITY)
            .total_cmp(&y.price_per_m_input.unwrap_or(f64::INFINITY))
            .then_with(|| y.context_window.unwrap_or(0.0).total_cmp(&x.context_window.unwrap_or(0.0)))
    });
    free.first().copied()
}

/// `extractAiText`.
pub fn extract_ai_text(v: &Value) -> String {
    if let Value::String(s) = v {
        return s.clone();
    }
    if let Value::Object(rec) = v {
        if let Some(Value::String(s)) = rec.get("response") {
            return s.clone();
        }
        if let Some(Value::String(s)) = rec.get("output_text") {
            return s.clone();
        }
        if let Some(first) = rec.get("choices").and_then(Value::as_array).and_then(|c| c.first()) {
            if let Some(Value::String(s)) = first.pointer("/message/content") {
                return s.clone();
            }
            if let Some(Value::String(s)) = first.get("text") {
                return s.clone();
            }
        }
    }
    v.to_string()
}

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// Re-frames a Workers AI SSE stream (legacy `response` or OpenAI `choices[].delta`) as OpenAI chat chunks.
pub struct Reframer {
    id: String,
    created: i64,
    model: String,
    buffer: Vec<u8>,
}

impl Reframer {
    pub fn new(model: &str) -> Self {
        let now = now_ms();
        Reframer { id: format!("chatcmpl-cf-{}", now as i64), created: (now / 1000.0).floor() as i64, model: model.into(), buffer: Vec::new() }
    }

    fn chunk(&self, delta: Value, finish: Option<&str>) -> String {
        let body = json!({
            "id": self.id,
            "object": "chat.completion.chunk",
            "created": self.created,
            "model": self.model,
            "choices": [{ "index": 0, "delta": delta, "finish_reason": finish }],
        });
        format!("data: {body}\n\n")
    }

    /// The opening `role` chunk.
    pub fn start(&self) -> String {
        self.chunk(json!({ "role": "assistant" }), None)
    }

    /// Feeds raw bytes; returns the chunks to emit. A trailing partial line stays buffered.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.buffer.extend_from_slice(bytes);
        let mut out = Vec::new();
        while let Some(nl) = self.buffer.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.buffer.drain(..=nl).collect();
            let line = String::from_utf8_lossy(&line[..line.len() - 1]).into_owned();
            let data = line.strip_prefix("data:").map_or("", str::trim);
            if data.is_empty() || data == "[DONE]" {
                continue;
            }
            let Ok(ev) = serde_json::from_str::<Value>(data) else { continue };
            let delta = ev.pointer("/choices/0/delta");
            let text = match ev.get("response") {
                Some(Value::String(s)) => Some(s.as_str()),
                _ => delta.and_then(|d| d.get("content")).and_then(Value::as_str),
            };
            if let Some(r) = delta.and_then(|d| d.get("reasoning_content")).and_then(Value::as_str).filter(|r| !r.is_empty()) {
                out.push(self.chunk(json!({ "reasoning_content": r }), None));
            }
            if let Some(t) = text.filter(|t| !t.is_empty()) {
                out.push(self.chunk(json!({ "content": t }), None));
            }
        }
        out
    }

    /// The closing `stop` chunk and `[DONE]` (a partial last line is dropped, as in the TS TransformStream).
    pub fn finish(&self) -> String {
        format!("{}data: [DONE]\n\n", self.chunk(json!({}), Some("stop")))
    }
}

/// The Workers AI binding, plus the optional Vectorize index, as the gateway sees them.
#[allow(async_fn_in_trait)]
pub trait Inference {
    /// `Ok(None)` when the binding has no `models()` method.
    async fn models(&self, task: &str) -> Result<Option<Vec<RawModel>>, String>;
    async fn run(&self, model: &str, input: &Value) -> Result<Value, String>;
    /// `Ok(None)` when the model did not return a `ReadableStream`.
    async fn run_stream(&self, model: &str, input: &Value) -> Result<Option<ByteStream>, String>;
    fn has_vectorize(&self) -> bool;
    async fn vectorize_upsert(&self, vectors: &Value) -> Result<(), String>;
}

type CacheEntry = (f64, Rc<Vec<EdgeModel>>);

thread_local! {
    /// Catalog cache per task (one AI binding per isolate in production).
    static MODEL_CACHE: RefCell<HashMap<String, CacheEntry>> = RefCell::new(HashMap::new());
}

/// `listEdgeModels`: live Workers AI catalog for one task, cached for 10 minutes.
pub async fn list_edge_models<I: Inference>(ai: &I, task: &str) -> Result<Rc<Vec<EdgeModel>>, String> {
    let now = now_ms();
    if let Some(hit) = MODEL_CACHE.with(|c| c.borrow().get(task).filter(|(at, _)| now - at < MODEL_CACHE_MS).map(|(_, m)| m.clone())) {
        return Ok(hit);
    }
    let Some(raw) = ai.models(task).await? else { return Ok(Rc::new(Vec::new())) };
    let models = Rc::new(rank_models(&raw));
    MODEL_CACHE.with(|c| c.borrow_mut().insert(task.to_string(), (now_ms(), models.clone())));
    Ok(models)
}

pub async fn default_edge_model<I: Inference>(ai: &I) -> Result<String, AiError> {
    let models = list_edge_models(ai, "Text Generation").await.map_err(AiError::Upstream)?;
    pick_chat_model(&models)
        .map(|m| m.id.clone())
        .ok_or_else(|| AiError::Unavailable("Workers AI returned no text-generation models for this account".into()))
}

pub async fn default_embedding_model<I: Inference>(ai: &I) -> Result<String, AiError> {
    let models = list_edge_models(ai, "Text Embeddings").await.map_err(AiError::Upstream)?;
    pick_embedding_model(&models)
        .map(|m| m.id.clone())
        .ok_or_else(|| AiError::Unavailable("Workers AI returned no text-embedding models for this account".into()))
}

async fn resolve_model<I: Inference>(ai: &I, requested: Option<&Value>) -> Result<String, AiError> {
    match requested.and_then(Value::as_str) {
        Some(m) if m.starts_with("@cf/") => Ok(m.to_string()),
        _ => default_edge_model(ai).await,
    }
}

fn require_ai<I: Inference>(ai: Option<&I>) -> Result<&I, AiError> {
    ai.ok_or_else(|| AiError::Unavailable(NO_AI.into()))
}

/// `runInferenceText`: `(text, model)` for ACP and A2A prompts.
pub async fn run_inference_text<I: Inference>(ai: Option<&I>, requested: Option<&Value>, messages: Value) -> Result<(String, String), AiError> {
    let ai = require_ai(ai)?;
    let model = resolve_model(ai, requested).await?;
    let raw = ai.run(&model, &json!({ "messages": messages })).await.map_err(AiError::Upstream)?;
    Ok((extract_ai_text(&raw), model))
}

pub fn openai_error(status: u16, kind: &str, message: &str) -> (u16, Value) {
    (status, json!({ "error": { "message": message, "type": kind } }))
}

fn ai_error_reply(e: &AiError) -> (u16, Value) {
    match e {
        AiError::Unavailable(m) => openai_error(503, "fw_no_inference", m),
        AiError::Upstream(m) => openai_error(502, "fw_upstream_error", m),
    }
}

/// `GET /v1/models`.
pub async fn models_route<I: Inference>(ai: Option<&I>) -> (u16, Value) {
    let Some(ai) = ai else {
        return (200, json!({ "object": "list", "data": [], "error": "Workers AI binding `AI` is not configured" }));
    };
    let run = async {
        let models = list_edge_models(ai, "Text Generation").await?;
        let default = default_edge_model(ai).await.map_err(|e| e.message().to_string())?;
        let data: Vec<Value> = models.iter().map(EdgeModel::to_value).collect();
        Ok::<Value, String>(json!({ "object": "list", "data": data, "default": default }))
    };
    match run.await {
        Ok(v) => (200, v),
        Err(e) => openai_error(502, "fw_upstream_error", &e),
    }
}

pub enum ChatReply {
    Json(u16, Value),
    Stream { model: String, source: ByteStream },
}

/// `POST /v1/chat/completions`.
pub async fn chat_route<I: Inference>(ai: Option<&I>, body_text: &str) -> ChatReply {
    let body = match serde_json::from_str::<Value>(body_text) {
        Ok(Value::Object(m)) => Some(m),
        _ => None,
    };
    let messages = body.as_ref().and_then(|b| b.get("messages")).filter(|m| m.as_array().is_some_and(|a| !a.is_empty()));
    let (Some(body), Some(messages)) = (body.as_ref(), messages) else {
        let (s, v) = openai_error(400, "invalid_request_error", "`messages` must be a non-empty array");
        return ChatReply::Json(s, v);
    };
    let mut input = Map::new();
    input.insert("messages".into(), messages.clone());
    let mut options = Map::new();
    for k in ["temperature", "max_tokens"] {
        if let Some(v) = body.get(k).filter(|v| v.is_number()) {
            options.insert(k.into(), v.clone());
        }
    }
    let reply = async {
        let ai = require_ai(ai)?;
        let model = resolve_model(ai, body.get("model")).await?;
        if body.get("stream").is_some_and(truthy) {
            input.insert("stream".into(), true.into());
            input.extend(options);
            return match ai.run_stream(&model, &Value::Object(input)).await.map_err(AiError::Upstream)? {
                Some(source) => Ok(ChatReply::Stream { model, source }),
                None => {
                    let (s, v) = openai_error(502, "fw_upstream_error", &format!("{model} did not return a stream"));
                    Ok(ChatReply::Json(s, v))
                }
            };
        }
        input.extend(options);
        let raw = ai.run(&model, &Value::Object(input)).await.map_err(AiError::Upstream)?;
        let now = now_ms();
        Ok::<ChatReply, AiError>(ChatReply::Json(
            200,
            json!({
                "id": format!("chatcmpl-cf-{}", now as i64),
                "object": "chat.completion",
                "created": (now / 1000.0).floor() as i64,
                "model": model,
                "choices": [{ "index": 0, "message": { "role": "assistant", "content": extract_ai_text(&raw) }, "finish_reason": "stop" }],
                "raw": raw,
            }),
        ))
    };
    match reply.await {
        Ok(r) => r,
        Err(e) => {
            let (s, v) = ai_error_reply(&e);
            ChatReply::Json(s, v)
        }
    }
}

/// The SSE body: `role` chunk, content and reasoning chunks, `stop`, `[DONE]`.
pub fn reframe_stream(source: ByteStream, model: &str) -> impl Stream<Item = Result<Vec<u8>, worker::Error>> + 'static {
    let re = Rc::new(RefCell::new(Reframer::new(model)));
    let head = re.borrow().start().into_bytes();
    let (mid, tail) = (re.clone(), re);
    stream::once(async move { Ok(head) })
        .chain(source.flat_map(move |item| {
            let frames: Vec<Result<Vec<u8>, worker::Error>> = match item {
                Ok(bytes) => mid.borrow_mut().push(&bytes).into_iter().map(|f| Ok(f.into_bytes())).collect(),
                Err(e) => vec![Err(e)],
            };
            stream::iter(frames)
        }))
        .chain(stream::once(async move { Ok(tail.borrow().finish().into_bytes()) }))
}

/// `POST /v1/embeddings`; `tenant` keys Vectorize ids.
pub async fn embeddings_route<I: Inference>(ai: Option<&I>, body_text: &str, tenant: &str) -> (u16, Value) {
    let body = body_object(body_text);
    let inputs: Vec<String> = match body.get("input") {
        Some(Value::Array(a)) => a.iter().map(js_string).collect(),
        Some(Value::String(s)) => vec![s.clone()],
        _ => Vec::new(),
    };
    if inputs.is_empty() {
        return openai_error(400, "invalid_request_error", "`input` must be a string or a non-empty array of strings");
    }
    let produced = async {
        let ai = require_ai(ai)?;
        let model = match body.get("model").and_then(Value::as_str) {
            Some(m) if m.starts_with("@cf/") => m.to_string(),
            _ => default_embedding_model(ai).await?,
        };
        let res = ai.run(&model, &json!({ "text": inputs })).await.map_err(AiError::Upstream)?;
        let data = res.get("data").and_then(Value::as_array).cloned();
        match data {
            Some(d) if d.len() == inputs.len() => Ok((ai, model, d)),
            other => Err(AiError::Upstream(format!(
                "{model} returned {} vectors for {} inputs",
                other.map_or_else(|| "no".to_string(), |d| d.len().to_string()),
                inputs.len()
            ))),
        }
    };
    let (ai, model, vectors) = match produced.await {
        Ok(t) => t,
        Err(e) => return ai_error_reply(&e),
    };

    let mut upserted = 0;
    if body.get("upsertVectorize").is_some_and(truthy) && ai.has_vectorize() {
        let ids = body.get("ids").and_then(Value::as_array);
        let batch: Vec<Value> = vectors
            .iter()
            .enumerate()
            .map(|(idx, values)| {
                let id = ids
                    .and_then(|ids| ids.get(idx))
                    .filter(|v| !v.is_null())
                    .map_or_else(|| format!("{tenant}:{}", make_id("vec")), js_string);
                let text: String = {
                    let mut used = 0;
                    inputs[idx]
                        .chars()
                        .take_while(|c| {
                            used += c.len_utf16();
                            used <= 256
                        })
                        .collect()
                };
                json!({ "id": id, "values": values, "metadata": { "tenant": tenant, "text": text, "model": model } })
            })
            .collect();
        if let Err(e) = ai.vectorize_upsert(&Value::Array(batch.clone())).await {
            return openai_error(502, "fw_upstream_error", &e);
        }
        upserted = batch.len();
    }

    let estimate: usize = inputs.iter().map(|t| utf16_len(t).div_ceil(4).max(1)).sum();
    (
        200,
        json!({
            "object": "list",
            "model": model,
            "source": "workers-ai",
            "vectorizeUpserted": upserted,
            "data": vectors.iter().enumerate().map(|(i, e)| json!({ "object": "embedding", "index": i, "embedding": e })).collect::<Vec<_>>(),
            "usage": { "prompt_tokens": estimate, "total_tokens": estimate },
        }),
    )
}

// ---------------------------------------------------------------------------------------------------------------
// Runtime binding.
// ---------------------------------------------------------------------------------------------------------------

/// The `AI` (and optional `MEMORY_VEC`) bindings of the Worker, called through JS reflection so a binding without
/// `models()` or Vectorize behaves like the TypeScript worker's optional chaining.
pub struct WorkersAi {
    ai: wasm_bindgen::JsValue,
    vectorize: Option<wasm_bindgen::JsValue>,
}

impl WorkersAi {
    pub fn bind(env: &Env) -> Option<Self> {
        Some(WorkersAi { ai: crate::os::binding(env, "AI")?, vectorize: crate::os::binding(env, "MEMORY_VEC") })
    }

    fn call(target: &wasm_bindgen::JsValue, name: &str, args: &[wasm_bindgen::JsValue]) -> Result<Option<wasm_bindgen::JsValue>, String> {
        use wasm_bindgen::JsCast;
        let Some(f) = js_sys::Reflect::get(target, &name.into()).ok().and_then(|f| f.dyn_into::<js_sys::Function>().ok()) else {
            return Ok(None);
        };
        let arr: js_sys::Array = args.iter().collect();
        f.apply(target, &arr).map(Some).map_err(|e| crate::os::js_message(&e))
    }
}

impl Inference for WorkersAi {
    async fn models(&self, task: &str) -> Result<Option<Vec<RawModel>>, String> {
        let params = json!({ "task": task, "per_page": 200, "hide_experimental": true });
        let Some(promise) = Self::call(&self.ai, "models", &[crate::os::to_js(&params)])? else { return Ok(None) };
        let res = crate::os::settle(promise).await?;
        Ok(Some(raw_models_from_json(&crate::os::from_js(&res).unwrap_or(Value::Null))))
    }

    async fn run(&self, model: &str, input: &Value) -> Result<Value, String> {
        let promise = Self::call(&self.ai, "run", &[model.into(), crate::os::to_js(input)])?.ok_or("AI.run is not a function")?;
        let res = crate::os::settle(promise).await?;
        Ok(crate::os::from_js(&res).unwrap_or(Value::Null))
    }

    async fn run_stream(&self, model: &str, input: &Value) -> Result<Option<ByteStream>, String> {
        use wasm_bindgen::JsCast;
        let promise = Self::call(&self.ai, "run", &[model.into(), crate::os::to_js(input)])?.ok_or("AI.run is not a function")?;
        let res = crate::os::settle(promise).await?;
        Ok(res.dyn_into::<worker::worker_sys::web_sys::ReadableStream>().ok().map(ByteStream::from))
    }

    fn has_vectorize(&self) -> bool {
        self.vectorize.is_some()
    }

    async fn vectorize_upsert(&self, vectors: &Value) -> Result<(), String> {
        let index = self.vectorize.as_ref().ok_or("MEMORY_VEC is not bound")?;
        let promise = Self::call(index, "upsert", &[crate::os::to_js(vectors)])?.ok_or("MEMORY_VEC.upsert is not a function")?;
        crate::os::settle(promise).await.map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::future::Future;
    use std::task::{Context, Poll, Waker};

    use super::*;

    fn block_on<F: Future>(fut: F) -> F::Output {
        let mut fut = Box::pin(fut);
        let mut cx = Context::from_waker(Waker::noop());
        loop {
            if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
                return v;
            }
        }
    }

    fn prop(id: &str, v: &str) -> (String, Value) {
        (id.into(), Value::from(v))
    }

    fn model(name: &str, props: &[(&str, &str)]) -> RawModel {
        RawModel { name: name.into(), description: None, properties: props.iter().map(|(k, v)| prop(k, v)).collect() }
    }

    /// The two-model catalog of `mockAi()` in tests/fw_cloudflare.test.ts.
    struct MockAi {
        calls: RefCell<Vec<(String, Value)>>,
        embedding_catalog: Option<Vec<RawModel>>,
        vectors: RefCell<Vec<Value>>,
        vectorize: bool,
        short_embeddings: bool,
    }

    impl MockAi {
        fn new() -> Self {
            MockAi { calls: RefCell::new(vec![]), embedding_catalog: None, vectors: RefCell::new(vec![]), vectorize: false, short_embeddings: false }
        }
    }

    impl Inference for MockAi {
        async fn models(&self, task: &str) -> Result<Option<Vec<RawModel>>, String> {
            if task == "Text Embeddings" {
                return Ok(Some(self.embedding_catalog.clone().unwrap_or_else(|| vec![model("@cf/baai/bge-base-en-v1.5", &[("context_window", "512")])])));
            }
            Ok(Some(vec![
                model("@cf/test/small-tools", &[("context_window", "8192"), ("function_calling", "true")]),
                model("@cf/test/big-tools", &[("context_window", "131072"), ("function_calling", "true")]),
                model("@cf/test/paid-tools", &[("context_window", "1000000"), ("function_calling", "true"), ("require_workers_paid", "true")]),
                model("@cf/test/old", &[("planned_deprecation_date", "2026-12-01")]),
            ]))
        }
        async fn run(&self, model: &str, input: &Value) -> Result<Value, String> {
            self.calls.borrow_mut().push((model.into(), input.clone()));
            if self.short_embeddings {
                return Ok(json!({ "data": [] }));
            }
            if model.contains("bge") {
                return Ok(json!({ "data": [[0.1, 0.2, 0.3, 0.4]] }));
            }
            if model.contains("emb-") {
                let n = input["text"].as_array().map_or(0, Vec::len);
                return Ok(json!({ "data": vec![json!([1, 0]); n] }));
            }
            let last = input["messages"].as_array().and_then(|m| m.last()).and_then(|m| m["content"].as_str()).unwrap_or("").to_string();
            Ok(json!({ "response": format!("echo:{last}") }))
        }
        async fn run_stream(&self, _model: &str, _input: &Value) -> Result<Option<ByteStream>, String> {
            Ok(None)
        }
        fn has_vectorize(&self) -> bool {
            self.vectorize
        }
        async fn vectorize_upsert(&self, vectors: &Value) -> Result<(), String> {
            self.vectors.borrow_mut().extend(vectors.as_array().cloned().unwrap_or_default());
            Ok(())
        }
    }

    #[test]
    fn ranks_models_like_the_ts_worker() {
        let ai = MockAi::new();
        let models = block_on(list_edge_models(&ai, "Text Generation")).unwrap();
        let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["@cf/test/paid-tools", "@cf/test/big-tools", "@cf/test/small-tools"]);
        assert_eq!(block_on(default_edge_model(&ai)).unwrap(), "@cf/test/big-tools");
        let v = models[1].to_value();
        assert_eq!(
            serde_json::to_string(&v).unwrap(),
            r#"{"id":"@cf/test/big-tools","object":"model","owned_by":"cloudflare-workers-ai","context_window":131072,"function_calling":true,"reasoning":false,"vision":false,"paid":false}"#
        );
    }

    #[test]
    fn catalog_is_cached_for_ten_minutes() {
        let ai = MockAi::new();
        let a = block_on(list_edge_models(&ai, "Text Generation")).unwrap();
        let b = block_on(list_edge_models(&ai, "Text Generation")).unwrap();
        assert!(Rc::ptr_eq(&a, &b));
        MODEL_CACHE.with(|c| c.borrow_mut().get_mut("Text Generation").unwrap().0 -= MODEL_CACHE_MS + 1.0);
        let c = block_on(list_edge_models(&ai, "Text Generation")).unwrap();
        assert!(!Rc::ptr_eq(&a, &c));
    }

    #[test]
    fn filters_non_cf_lora_beta_and_orders_ties_by_id() {
        let raw = vec![
            model("openai/gpt", &[]),
            model("@cf/b/z-lora", &[("lora", "true")]),
            model("@cf/b/z-beta", &[("beta", "true")]),
            model("@cf/b/zeta", &[("context_window", "10")]),
            model("@cf/b/Alpha", &[("context_window", "10")]),
            model("@cf/b/beta-x", &[("context_window", "10")]),
        ];
        let ids: Vec<String> = rank_models(&raw).into_iter().map(|m| m.id).collect();
        assert_eq!(ids, ["@cf/b/Alpha", "@cf/b/beta-x", "@cf/b/zeta"]);
    }

    #[test]
    fn embedding_default_is_cheapest_then_largest_context() {
        let mut ai = MockAi::new();
        ai.embedding_catalog = Some(vec![model("@cf/test/emb-small", &[("context_window", "512")]), model("@cf/test/emb-large", &[("context_window", "8192")])]);
        assert_eq!(block_on(default_embedding_model(&ai)).unwrap(), "@cf/test/emb-large");
        let priced = RawModel {
            name: "@cf/p/cheap".into(),
            description: Some("d".into()),
            properties: vec![("price".into(), json!([{"unit": "per M output tokens", "price": 9.0}, {"unit": "per M input tokens", "price": 0.01}]))],
        };
        let free = [priced.clone(), model("@cf/p/pricey", &[("price", "x")])];
        let ranked = rank_models(&free);
        assert_eq!(ranked[0].price_per_m_input, Some(0.01));
        assert_eq!(pick_embedding_model(&ranked).unwrap().id, "@cf/p/cheap");
        let paid_only = rank_models(&[model("@cf/p/paid", &[("require_workers_paid", "true")])]);
        assert!(pick_embedding_model(&paid_only).is_none());
    }

    #[test]
    fn models_route_without_binding_and_with_mock() {
        let (s, v) = block_on(models_route::<MockAi>(None));
        assert_eq!(s, 200);
        assert_eq!(v["data"], json!([]));
        assert!(v["error"].as_str().unwrap().contains("Workers AI"));
        let (_, v) = block_on(models_route(Some(&MockAi::new())));
        assert_eq!(v["object"], "list");
        assert_eq!(v["default"], "@cf/test/big-tools");
        let ids: Vec<&str> = v["data"].as_array().unwrap().iter().map(|m| m["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["@cf/test/paid-tools", "@cf/test/big-tools", "@cf/test/small-tools"]);
    }

    fn chat(ai: Option<&MockAi>, body: Value) -> (u16, Value) {
        match block_on(chat_route(ai, &body.to_string())) {
            ChatReply::Json(s, v) => (s, v),
            ChatReply::Stream { .. } => panic!("unexpected stream"),
        }
    }

    #[test]
    fn chat_errors_and_json_completion() {
        let (s, v) = chat(None, json!({"messages": [{"role": "user", "content": "hello edge"}]}));
        assert_eq!(s, 503);
        assert!(v["error"]["message"].as_str().unwrap().contains("Workers AI binding"));
        assert_eq!(v["error"]["type"], "fw_no_inference");
        let ai = MockAi::new();
        assert_eq!(chat(Some(&ai), json!({})).0, 400);
        assert_eq!(chat(Some(&ai), json!({"messages": []})).0, 400);
        assert!(matches!(block_on(chat_route(Some(&ai), "not json")), ChatReply::Json(400, _)));

        let (s, v) = chat(
            Some(&ai),
            json!({"model": "@cf/meta/llama-3.3-70b-instruct-fp8-fast", "temperature": 0.2, "max_tokens": "x", "messages": [{"role": "user", "content": "ping ai"}]}),
        );
        assert_eq!(s, 200);
        assert_eq!(v["object"], "chat.completion");
        assert_eq!(v["choices"][0]["message"]["content"], "echo:ping ai");
        assert_eq!(v["choices"][0]["finish_reason"], "stop");
        assert_eq!(v["raw"]["response"], "echo:ping ai");
        assert!(v["id"].as_str().unwrap().starts_with("chatcmpl-cf-"));
        let calls = ai.calls.borrow();
        assert_eq!(calls[0].0, "@cf/meta/llama-3.3-70b-instruct-fp8-fast");
        assert_eq!(calls[0].1, json!({"messages": [{"role": "user", "content": "ping ai"}], "temperature": 0.2}));
        drop(calls);

        // default model comes from the live catalog
        chat(Some(&ai), json!({"messages": [{"role": "user", "content": "x"}]}));
        assert_eq!(ai.calls.borrow()[1].0, "@cf/test/big-tools");
        // a stream request on a model that returns no stream is an upstream error
        let (s, v) = chat(Some(&ai), json!({"stream": true, "messages": [{"role": "user", "content": "s"}]}));
        assert_eq!(s, 502);
        assert_eq!(v["error"]["message"], "@cf/test/big-tools did not return a stream");
    }

    #[test]
    fn reframes_sse_into_openai_chunks() {
        let mut re = Reframer::new("@cf/test/m");
        let mut frames = vec![re.start()];
        frames.extend(re.push(b"data: {\"response\":\"str\"}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"ea"));
        frames.extend(re.push(b"med\"}}]}\n\ndata: [DONE]\n\n"));
        frames.push(re.finish());
        let sse = frames.concat();
        let deltas: String = sse
            .split("\n\n")
            .filter(|f| f.starts_with("data: {"))
            .map(|f| serde_json::from_str::<Value>(&f[6..]).unwrap()["choices"][0]["delta"]["content"].as_str().unwrap_or("").to_string())
            .collect();
        assert_eq!(deltas, "streamed");
        assert!(sse.trim().ends_with("data: [DONE]"));
        let first: Value = serde_json::from_str(&sse.split("\n\n").next().unwrap()[6..]).unwrap();
        assert_eq!(first["choices"][0]["delta"], json!({"role": "assistant"}));
        assert_eq!(first["object"], "chat.completion.chunk");
        assert_eq!(first["model"], "@cf/test/m");
        assert_eq!(first["choices"][0]["finish_reason"], Value::Null);
        let parts: Vec<&str> = sse.split("\n\n").collect();
        let stop: Value = serde_json::from_str(&parts[parts.len() - 3][6..]).unwrap();
        assert_eq!(stop["choices"][0]["finish_reason"], "stop");
        assert_eq!(stop["choices"][0]["delta"], json!({}));
    }

    #[test]
    fn reframer_handles_reasoning_junk_and_split_utf8() {
        let mut re = Reframer::new("m");
        assert!(re.push(b": keep-alive\ndata: not json\ndata:\n").is_empty());
        let out = re.push(b"data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"think\",\"content\":\"ok\"}}]}\n");
        assert_eq!(out.len(), 2);
        assert!(out[0].contains("\"reasoning_content\":\"think\"") && out[1].contains("\"content\":\"ok\""));
        let line = "data: {\"response\":\"é\"}\n".as_bytes();
        let (a, b) = line.split_at(line.iter().position(|c| *c == 0xC3).unwrap() + 1);
        assert!(re.push(a).is_empty());
        assert!(re.push(b)[0].contains("\"content\":\"é\""));
        // a partial last line is dropped
        assert!(re.push(b"data: {\"response\":\"lost\"}").is_empty());
    }

    #[test]
    fn extracts_text_like_extract_ai_text() {
        assert_eq!(extract_ai_text(&json!("x")), "x");
        assert_eq!(extract_ai_text(&json!({"response": "a"})), "a");
        assert_eq!(extract_ai_text(&json!({"output_text": "b"})), "b");
        assert_eq!(extract_ai_text(&json!({"choices": [{"message": {"content": "c"}}]})), "c");
        assert_eq!(extract_ai_text(&json!({"choices": [{"text": "d"}]})), "d");
        assert_eq!(extract_ai_text(&json!({"other": 1})), r#"{"other":1}"#);
    }

    fn embed(ai: Option<&MockAi>, body: Value, tenant: &str) -> (u16, Value) {
        block_on(embeddings_route(ai, &body.to_string(), tenant))
    }

    #[test]
    fn embeddings_with_vectorize_upsert() {
        let mut ai = MockAi::new();
        ai.vectorize = true;
        let (s, v) = embed(Some(&ai), json!({"input": "fw edge os", "upsertVectorize": true, "ids": ["doc-1"]}), "vec-tenant");
        assert_eq!(s, 200);
        assert_eq!((v["object"].clone(), v["source"].clone(), v["vectorizeUpserted"].clone()), (json!("list"), json!("workers-ai"), json!(1)));
        assert_eq!(v["data"][0]["embedding"], json!([0.1, 0.2, 0.3, 0.4]));
        assert_eq!(v["model"], "@cf/baai/bge-base-en-v1.5");
        assert_eq!(v["usage"], json!({"prompt_tokens": 3, "total_tokens": 3}));
        let upserted = ai.vectors.borrow();
        assert_eq!(upserted[0]["id"], "doc-1");
        assert_eq!(upserted[0]["metadata"], json!({"tenant": "vec-tenant", "text": "fw edge os", "model": "@cf/baai/bge-base-en-v1.5"}));
        drop(upserted);
        // generated ids carry the tenant
        let (_, v) = embed(Some(&ai), json!({"input": ["a"], "upsertVectorize": true}), "vec-tenant");
        assert_eq!(v["vectorizeUpserted"], 1);
        assert!(ai.vectors.borrow()[1]["id"].as_str().unwrap().starts_with("vec-tenant:vec_"));
        // upsert only when asked
        assert_eq!(embed(Some(&ai), json!({"input": "x"}), "t").1["vectorizeUpserted"], 0);
    }

    #[test]
    fn embeddings_errors_and_default_model() {
        assert_eq!(embed(None, json!({"input": ["alpha", "beta"]}), "t").0, 503);
        assert_eq!(embed(Some(&MockAi::new()), json!({}), "t").0, 400);
        assert_eq!(embed(Some(&MockAi::new()), json!({"input": []}), "t").0, 400);
        let mut short = MockAi::new();
        short.short_embeddings = true;
        let (s, v) = embed(Some(&short), json!({"input": "x"}), "t");
        assert_eq!(s, 502);
        assert_eq!(v["error"]["message"], "@cf/baai/bge-base-en-v1.5 returned 0 vectors for 1 inputs");

        MODEL_CACHE.with(|c| c.borrow_mut().clear());
        let mut ai = MockAi::new();
        ai.embedding_catalog = Some(vec![model("@cf/test/emb-small", &[("context_window", "512")]), model("@cf/test/emb-large", &[("context_window", "8192")])]);
        let (_, v) = embed(Some(&ai), json!({"input": "x"}), "t");
        assert_eq!(v["model"], "@cf/test/emb-large");
        assert_eq!(ai.calls.borrow()[0].0, "@cf/test/emb-large");
        assert_eq!(ai.calls.borrow()[0].1, json!({"text": ["x"]}));
    }

    #[test]
    fn inference_text_for_acp_and_a2a() {
        let ai = MockAi::new();
        let (text, model) = block_on(run_inference_text(Some(&ai), None, json!([{"role": "user", "content": "summarize edge os"}]))).unwrap();
        assert_eq!((text.as_str(), model.as_str()), ("echo:summarize edge os", "@cf/test/big-tools"));
        let err = block_on(run_inference_text::<MockAi>(None, None, json!([]))).unwrap_err();
        assert!(err.message().contains("Workers AI binding"));
    }

    #[test]
    fn locale_order_matches_icu_for_model_ids() {
        assert_eq!(locale_cmp("a", "B"), Ordering::Less);
        assert_eq!(locale_cmp("a-b", "a_b"), Ordering::Greater);
        assert_eq!(locale_cmp("a1", "a-"), Ordering::Greater);
        assert_eq!(locale_cmp("abc", "ab"), Ordering::Greater);
        assert_eq!(locale_cmp("a", "A"), Ordering::Less);
    }
}
