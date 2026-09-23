//! Ollama, through its own chat endpoint (D10, PHASE 11 detail 2).
//!
//! **Why not `/v1/chat/completions`.** D10 asks two things of Ollama that its OpenAI-compatible layer
//! cannot carry: the JSON Schema in Ollama's `format` field, and an explicit `num_ctx`. That layer's
//! request type has neither field, nor `keep_alive` nor `think`, and drops what it does not know
//! without a word (`ollama/openai/openai.go`, read 2026-09-23). A `num_ctx` sent there is a
//! `num_ctx` ignored — and Ollama's default context is 2 048 tokens, past which the prompt is cut
//! from the front, silently. So this adapter speaks `POST /api/chat`: the same messages, the same
//! greedy decoding and the same answer as every other adapter, in the one request shape where the
//! context size is actually honoured (DECISIONS_LOG 2026-09-23).
//!
//! **The context is never too small.** Every request states `options.num_ctx`: the configured
//! context, or more when the prompt needs it — the prompt's byte length (a token is never shorter
//! than one byte of UTF-8, so bytes bound tokens from above), the chat template's overhead, and
//! the answer's token cap. And it sets `truncate: false` and `shift: false`, so an Ollama new
//! enough to know them refuses a prompt that would not fit instead of cutting it.

use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Map, Value};

use super::{LlmError, LlmProvider, LlmRequest, LlmResponse, ProviderCaps, ThinkingControl};
use crate::transport::Transport;

/// Ollama's chat endpoint.
pub const OLLAMA_CHAT: &str = "/api/chat";

/// Everything the adapter needs to know about the Ollama behind its transport.
#[derive(Clone, Debug, PartialEq)]
pub struct OllamaConfig {
    /// The model as Ollama names it, e.g. `qwen3:1.7b`. Also the provider's id.
    pub model: String,
    /// `llm.temperature`.
    pub temperature: f32,
    /// How long one call may take.
    pub timeout: Duration,
    /// `llm.ollama_num_ctx`: the least context any request asks for.
    pub num_ctx: u32,
    /// `llm.ollama_keep_alive_secs`: how long Ollama keeps the model loaded after a request.
    pub keep_alive_secs: u32,
    /// `llm.ollama_template_overhead_tokens`: what the chat template adds around the messages.
    pub template_overhead_tokens: u32,
}

/// Ollama as an [`LlmProvider`].
pub struct Ollama<T: Transport> {
    transport: T,
    config: OllamaConfig,
}

impl<T: Transport> Ollama<T> {
    pub fn new(transport: T, config: OllamaConfig) -> Self {
        Self { transport, config }
    }

    /// The transport the adapter sends through.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// The context this request asks for: the configured one, or what the prompt and its answer
    /// can occupy, whichever is larger. A book whose prompts all fit asks for the same context on
    /// every call, so Ollama does not reload the model between them.
    pub fn num_ctx_for(&self, request: &LlmRequest) -> u32 {
        let needed = prompt_token_bound(request, self.config.template_overhead_tokens)
            .saturating_add(request.max_tokens);
        self.config.num_ctx.max(needed)
    }

    /// The request body, exactly as it goes on the wire, in a fixed key order.
    pub fn body(&self, request: &LlmRequest) -> Result<String, LlmError> {
        let schema: Value = serde_json::from_str(request.schema).map_err(|error| {
            LlmError::Protocol(format!("the task's schema is not JSON: {error}"))
        })?;

        let mut options = Map::new();
        options.insert("temperature".to_owned(), json!(self.config.temperature));
        options.insert("num_ctx".to_owned(), json!(self.num_ctx_for(request)));
        options.insert("num_predict".to_owned(), json!(request.max_tokens));

        let mut body = Map::new();
        body.insert("model".to_owned(), json!(self.config.model));
        body.insert(
            "messages".to_owned(),
            json!([
                { "role": "system", "content": request.system_prefix },
                { "role": "user", "content": request.user },
            ]),
        );
        body.insert("stream".to_owned(), json!(false));
        body.insert("format".to_owned(), schema);
        body.insert("options".to_owned(), Value::Object(options));
        body.insert("keep_alive".to_owned(), json!(self.config.keep_alive_secs));
        body.insert("think".to_owned(), json!(false));
        body.insert("truncate".to_owned(), json!(false));
        body.insert("shift".to_owned(), json!(false));
        Ok(Value::Object(body).to_string())
    }
}

/// An upper bound on the tokens the rendered prompt occupies: every byte of the system prefix and
/// the question, plus the chat template's overhead. Byte-level tokenizers never produce more tokens
/// than bytes, so this is never short — which is the only direction that matters here.
pub fn prompt_token_bound(request: &LlmRequest, template_overhead_tokens: u32) -> u32 {
    let bytes = request
        .system_prefix
        .len()
        .saturating_add(request.user.len());
    u32::try_from(bytes)
        .unwrap_or(u32::MAX)
        .saturating_add(template_overhead_tokens)
}

impl<T: Transport> LlmProvider for Ollama<T> {
    fn id(&self) -> &str {
        &self.config.model
    }

    fn capabilities(&self) -> ProviderCaps {
        ProviderCaps::json_schema()
    }

    fn thinking_control(&self) -> ThinkingControl {
        ThinkingControl::OllamaThink
    }

    fn complete(&self, request: &LlmRequest) -> Result<LlmResponse, LlmError> {
        let body = self.body(request)?;
        let reply = self
            .transport
            .post_json(OLLAMA_CHAT, &body, self.config.timeout)?;
        read_reply(&reply)
    }
}

/// The parts of an `/api/chat` reply this adapter reads.
#[derive(Deserialize)]
struct Reply {
    message: Message,
    done_reason: Option<String>,
    prompt_eval_count: Option<u32>,
    prompt_eval_cached_count: Option<u32>,
    eval_count: Option<u32>,
}

#[derive(Deserialize)]
struct Message {
    content: Option<String>,
    thinking: Option<String>,
}

/// A chat reply as a response. Thinking is kept as `reasoning`, where gate S refuses it; an absent
/// `content` is an empty answer, which gate S refuses as unparseable.
fn read_reply(reply: &str) -> Result<LlmResponse, LlmError> {
    let reply: Reply =
        serde_json::from_str(reply).map_err(|error| LlmError::Protocol(error.to_string()))?;
    Ok(LlmResponse {
        text: reply.message.content.unwrap_or_default(),
        reasoning: reply
            .message
            .thinking
            .filter(|thinking| !thinking.trim().is_empty()),
        tokens_in: reply.prompt_eval_count.unwrap_or_default(),
        tokens_out: reply.eval_count.unwrap_or_default(),
        cached: false,
        cached_tokens: reply.prompt_eval_cached_count,
        finish_reason: reply.done_reason,
    })
}
