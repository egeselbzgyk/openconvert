//! One OpenAI-compatible chat client, over whichever `Transport` it is given (D10, ARCHITECTURE
//! §9.1).
//!
//! Every provider v1 has — the sidecar we own, Ollama, LM Studio, any custom endpoint — speaks
//! `POST /v1/chat/completions` with `messages`, greedy decoding and a constrained answer. Sharing
//! one wire format is what makes every call mockable in `cargo test` (D8), and it is why this
//! endpoint, not llama.cpp's `/completion`, is the interface even for the sidecar we own.
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

        let mut body = Map::new();
        body.insert("model".to_owned(), json!(self.config.model_id));
        body.insert(
            "messages".to_owned(),
            json!([
                { "role": "system", "content": system },
                { "role": "user", "content": request.user },
            ]),
        );
        body.insert("temperature".to_owned(), json!(self.config.temperature));
        body.insert("max_tokens".to_owned(), json!(request.max_tokens));

        match self.config.constraint {
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

/// The parts of a chat completion this client reads. Everything else a server sends is ignored.
#[derive(Deserialize)]
struct Completion {
    choices: Vec<Choice>,
    usage: Option<Usage>,
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
        finish_reason: choice.finish_reason,
    })
}
