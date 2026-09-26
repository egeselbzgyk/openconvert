//! One OpenAI-compatible chat client, over whichever `Transport` it is given (D10, ARCHITECTURE
//! §9.1).
//!
//! The sidecar we own, LM Studio, vLLM, any custom endpoint speak `POST /v1/chat/completions` with
//! `messages`, greedy decoding and a constrained answer. Sharing one wire format is what makes every
//! call mockable in `cargo test` (D8), and it is why this endpoint, not llama.cpp's `/completion`,
//! is the interface even for the sidecar we own. (Ollama's own compatibility layer drops the two
//! fields D10 requires of it — `num_ctx` and `format` — so Ollama is [`super::ollama`].)
//!
//! **What the endpoint can constrain** is its [`ProviderCaps`]: the GBNF grammar (`grammar`), the
//! JSON Schema (`response_format`), or neither. With neither, the task's schema goes in the prompt
//! after the question — the system prefix already tells the model to answer with JSON matching the
//! grammar it was given — and gate S, which never relaxes, carries the whole burden (PHASE 11
//! detail 1). The user message is otherwise sent verbatim.
//!
//! The body is built in a fixed key order (`serde_json`'s `preserve_order` is on across the
//! workspace), so the same request is the same bytes on every run and every machine.

use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::provider::{
    Constraint, LlmError, LlmProvider, LlmRequest, LlmResponse, ProviderCaps, ThinkingControl,
};
use crate::transport::Transport;

/// The one path every provider answers on.
const CHAT_COMPLETIONS: &str = "/v1/chat/completions";

/// Qwen's soft switch for "do not think", appended to the shared prefix for a generic endpoint
/// (D10). A control token for the model's chat template, not prose — and appended to all four
/// tasks' prefix alike, so it is still one prefix.
const NO_THINK_SWITCH: &str = "/no_think";

/// Everything a client needs to know about the endpoint behind its transport.
#[derive(Clone, Debug, PartialEq)]
pub struct ClientConfig {
    pub model_id: String,
    pub constraint: Constraint,
    pub thinking: ThinkingControl,
    /// `llm.temperature`: zero, so that the same question gets the same answer (D13.8).
    pub temperature: f32,
    /// How long one call may take before the transport gives up on it.
    pub timeout: Duration,
}

/// A custom OpenAI-compatible endpoint (PHASE 11 detail 3): whatever the capability probe found it
/// can constrain, and D10's thinking lever for a generic endpoint — `/no_think` for a Qwen-family
/// model, nothing for any other, whose thinking gate S refuses if it comes back.
pub fn custom_endpoint<T: Transport>(
    transport: T,
    model_id: String,
    caps: ProviderCaps,
    temperature: f32,
    timeout: Duration,
) -> OpenAiCompatible<T> {
    let thinking = generic_thinking(&model_id);
    OpenAiCompatible::new(
        transport,
        ClientConfig {
            model_id,
            constraint: caps.constraint,
            thinking,
            temperature,
            timeout,
        },
    )
}

/// How a generic endpoint is told not to think (D10): the Qwen family's `/no_think`, which is only
/// a control word to a Qwen chat template, and to anything else only noise.
pub fn generic_thinking(model_id: &str) -> ThinkingControl {
    if model_id.to_ascii_lowercase().contains(QWEN_FAMILY) {
        ThinkingControl::NoThinkSuffix
    } else {
        ThinkingControl::None
    }
}

/// How a model id names the Qwen family, whatever else it says (`qwen3:1.7b`, `Qwen/Qwen3-4B`).
const QWEN_FAMILY: &str = "qwen";

/// An OpenAI-compatible endpoint as an [`LlmProvider`].
pub struct OpenAiCompatible<T: Transport> {
    transport: T,
    config: ClientConfig,
}

impl<T: Transport> OpenAiCompatible<T> {
    pub fn new(transport: T, config: ClientConfig) -> Self {
        Self { transport, config }
    }

    /// The transport the client sends through.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// The request body, exactly as it goes on the wire.
    pub fn body(&self, request: &LlmRequest) -> Result<String, LlmError> {
        let system = match self.config.thinking {
            ThinkingControl::NoThinkSuffix => format!("{}{NO_THINK_SWITCH}", request.system_prefix),
            _ => request.system_prefix.to_owned(),
        };

        let constraint = if request.is_free_text() {
            None
        } else {
            Some(self.config.constraint)
        };
        let user = match constraint {
            Some(Constraint::None) => schema_in_prompt(request),
            _ => request.user.clone(),
        };

        let mut body = Map::new();
        body.insert("model".to_owned(), json!(self.config.model_id));
        body.insert(
            "messages".to_owned(),
            json!([
                { "role": "system", "content": system },
                { "role": "user", "content": user },
            ]),
        );
        body.insert("temperature".to_owned(), json!(self.config.temperature));
        body.insert("max_tokens".to_owned(), json!(request.max_tokens));

        match constraint.unwrap_or(Constraint::None) {
            Constraint::Gbnf => {
                body.insert("grammar".to_owned(), json!(request.grammar));
            }
            Constraint::JsonSchema => {
                let schema: Value = serde_json::from_str(request.schema).map_err(|error| {
                    LlmError::Protocol(format!("the task's schema is not JSON: {error}"))
                })?;
                body.insert(
                    "response_format".to_owned(),
                    json!({
                        "type": "json_schema",
                        "json_schema": {
                            "name": request.purpose.as_str(),
                            "strict": true,
                            "schema": schema,
                        },
                    }),
                );
            }
            Constraint::None => {}
        }

        match self.config.thinking {
            ThinkingControl::ChatTemplateKwargs => {
                body.insert(
                    "chat_template_kwargs".to_owned(),
                    json!({ "enable_thinking": false }),
                );
            }
            ThinkingControl::OllamaThink => {
                body.insert("think".to_owned(), json!(false));
            }
            ThinkingControl::NoThinkSuffix | ThinkingControl::None => {}
        }

        Ok(Value::Object(body).to_string())
    }
}

impl<T: Transport> LlmProvider for OpenAiCompatible<T> {
    fn id(&self) -> &str {
        &self.config.model_id
    }

    fn capabilities(&self) -> ProviderCaps {
        ProviderCaps {
            constraint: self.config.constraint,
        }
    }

    fn thinking_control(&self) -> ThinkingControl {
        self.config.thinking
    }

    fn complete(&self, request: &LlmRequest) -> Result<LlmResponse, LlmError> {
        let body = self.body(request)?;
        let reply = self
            .transport
            .post_json(CHAT_COMPLETIONS, &body, self.config.timeout)?;
        read_completion(&reply)
    }
}

/// The user message for an endpoint that constrains nothing: the question, then the task's JSON
/// Schema — an artifact of the prompt version like the rest, so no prompt text is written here.
pub fn schema_in_prompt(request: &LlmRequest) -> String {
    format!(
        "{}{SCHEMA_SEPARATOR}{}",
        request.user,
        request.schema.trim_end()
    )
}

/// Between the question and the schema: a blank line, as between any two parts of a prompt.
const SCHEMA_SEPARATOR: &str = "\n\n";

/// The parts of a chat completion this client reads. Everything else a server sends is ignored.
#[derive(Deserialize)]
struct Completion {
    choices: Vec<Choice>,
    usage: Option<Usage>,
    /// llama-server's own per-request timings, where `cache_n` is the prompt tokens it reused.
    timings: Option<Timings>,
}

#[derive(Deserialize)]
struct Timings {
    cache_n: Option<u32>,
}

#[derive(Deserialize)]
struct PromptTokensDetails {
    cached_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct Message {
    content: Option<String>,
    reasoning_content: Option<String>,
}

#[derive(Deserialize)]
struct Usage {
    prompt_tokens: u32,
    completion_tokens: u32,
    prompt_tokens_details: Option<PromptTokensDetails>,
}

/// A chat completion's first choice, as a response. An absent `content` is an empty answer —
/// which gate S refuses as unparseable — not a protocol failure: the server replied, and the model
/// said nothing.
fn read_completion(reply: &str) -> Result<LlmResponse, LlmError> {
    let completion: Completion =
        serde_json::from_str(reply).map_err(|error| LlmError::Protocol(error.to_string()))?;
    let Some(choice) = completion.choices.into_iter().next() else {
        return Err(LlmError::Protocol(
            "the completion has no choices".to_owned(),
        ));
    };
    let cached_tokens = completion
        .timings
        .and_then(|timings| timings.cache_n)
        .or_else(|| {
            completion
                .usage
                .as_ref()
                .and_then(|usage| usage.prompt_tokens_details.as_ref())
                .and_then(|details| details.cached_tokens)
        });
    let (tokens_in, tokens_out) = completion.usage.map_or((0, 0), |usage| {
        (usage.prompt_tokens, usage.completion_tokens)
    });
    Ok(LlmResponse {
        text: choice.message.content.unwrap_or_default(),
        reasoning: choice
            .message
            .reasoning_content
            .filter(|reasoning| !reasoning.trim().is_empty()),
        tokens_in,
        tokens_out,
        cached: false,
        cached_tokens,
        finish_reason: choice.finish_reason,
    })
}
