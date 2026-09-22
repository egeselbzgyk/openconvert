//! What the pipeline asks a model: which task, in which words, bounded by which grammar.
//!
//! [`LlmRequest`] carries the whole question, and everything in it but the user message is a
//! `&'static str` taken from the prompt artifacts. That is the byte-identity guarantee in type
//! form: the system prefix of a request *is* a `system.md`, not a string something assembled, so
//! nothing on the way to the wire can make two tasks' prefixes differ (D8, ARCHITECTURE §9.3).

use oc_model::decision::LlmTrace;
use serde::Serialize;

use crate::digest::{hex, sha256};
use crate::transport::TransportError;

/// Something that answers requests: a model behind an endpoint, or a cassette (D10).
///
/// Two v1 implementations answer from a model — the sidecar the engine or the app owns, and a
/// BYO OpenAI-compatible endpoint — and both are [`crate::openai::OpenAiCompatible`] over a
/// different `Transport` and a different [`ThinkingControl`]. The test tiers answer from
/// cassettes and never from a model (A8.3).
pub trait LlmProvider: Send + Sync {
    /// The model's id, as the cache key and every trace carry it.
    fn id(&self) -> &str;

    /// How the provider constrains an answer's shape.
    fn capabilities(&self) -> ProviderCaps;

    /// How the provider is told not to think (D10). The one place providers genuinely differ;
    /// whatever it does, gate S asserts the result.
    fn thinking_control(&self) -> ThinkingControl;

    /// Ask the question.
    fn complete(&self, request: &LlmRequest) -> Result<LlmResponse, LlmError>;
}

/// What a provider can do to bound an answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProviderCaps {
    pub constraint: Constraint,
}

/// How an answer's shape is constrained at decoding time (ARCHITECTURE §9.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Constraint {
    /// The GBNF grammar, in the request body — llama-server.
    Gbnf,
    /// The JSON Schema, as `response_format` — a server that prefers it.
    JsonSchema,
    /// Neither: the answer is unconstrained, and gate S carries the whole burden.
    None,
}

/// How a provider is told not to think (D10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThinkingControl {
    /// `chat_template_kwargs: {"enable_thinking": false}` — the sidecar we own.
    ChatTemplateKwargs,
    /// `think: false` — Ollama.
    OllamaThink,
    /// `/no_think` appended to the shared prefix — a generic endpoint serving a Qwen-family model.
    NoThinkSuffix,
    /// Nothing is sent. Gate S still refuses any thinking that comes back.
    None,
}

/// A provider's answer, before any gate has seen it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LlmResponse {
    /// What the model said, verbatim.
    pub text: String,
    /// Reasoning the server separated out of the answer — llama-server's `reasoning_content`. Its
    /// presence is a thinking block, wherever the server put it.
    pub reasoning: Option<String>,
    pub tokens_in: u32,
    pub tokens_out: u32,
    /// Whether the answer came from the cache or a cassette rather than from a model.
    pub cached: bool,
}

/// Why a question got no answer. Distinct from a gate failure, where an answer came back and was
/// refused: here there is nothing to refuse, and the caller converts deterministically (RT D20).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LlmError {
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// The endpoint replied with something that is not a chat completion.
    #[error("the endpoint's reply is not a chat completion: {0}")]
    Protocol(String),
}

/// The four v1 tasks (D13.6), named as ARCHITECTURE §9.6 names them.
///
/// Four and closed. A task is a question whose wrong answer renames something the book already
/// contains and cannot add or remove a character — `docs/DECISIONS_LOG.md`, 2026-09-20, "the line
/// between the LLM and the deterministic pipeline" — and a fifth task would have to answer that
/// question before it could be a variant here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    /// Task 1: title, authors and the rest, from pages 1–3.
    Metadata,
    /// Task 2: a role for every heading-style cluster.
    HeadingRoles,
    /// Task 3: where front matter ends, parts begin and back matter begins.
    BookStructure,
    /// Task 4: verse, block quotation, preformatted or paragraph, for ambiguous indented blocks.
    VerseQuote,
}

impl Purpose {
    /// Every task, in the order D13.6 numbers them.
    pub const ALL: [Purpose; 4] = [
        Purpose::Metadata,
        Purpose::HeadingRoles,
        Purpose::BookStructure,
        Purpose::VerseQuote,
    ];

    /// The task's name: its prompt directory, its cassette directory, and the `kind` of the
    /// `Decision` it informs.
    pub fn as_str(self) -> &'static str {
        match self {
            Purpose::Metadata => "metadata",
            Purpose::HeadingRoles => "heading_roles",
            Purpose::BookStructure => "book_structure",
            Purpose::VerseQuote => "verse_quote",
        }
    }
}

/// One question for a model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LlmRequest {
    pub purpose: Purpose,
    /// The prompt version the artifacts below belong to, hashed into the cache key.
    pub prompt_version: u32,
    /// The shared prefix: the same bytes for every task (test 8.1).
    pub system_prefix: &'static str,
    /// The only part of a request that varies: the task's template with the book's payload in it.
    pub user: String,
    /// The GBNF grammar the answer is decoded under.
    pub grammar: &'static str,
    /// The same shape as a JSON Schema, for a server that prefers one (ARCHITECTURE §9.2).
    pub schema: &'static str,
    /// `llm.max_output_tokens_per_call`, passed in by the caller.
    pub max_tokens: u32,
}

impl LlmRequest {
    /// The SHA-256 of the grammar text: the `grammar_hash` of the cache key (ARCHITECTURE §9.2), so
    /// a grammar edit cannot be answered from a cache entry decoded under the old one.
    pub fn grammar_sha256(&self) -> [u8; 32] {
        sha256(self.grammar.as_bytes())
    }
}

/// The trace of one call as the IR records it (D13.8): which model, which prompt version, and what
/// it was shown and what it said — by hash, never by text, so a report or a diagnostic bundle
/// carries no part of the book (D13.9).
///
/// The input hash is of the user message: the one part of a request that varies, the rest being
/// named exactly by the prompt version.
pub fn trace(
    model_id: &str,
    request: &LlmRequest,
    output: &str,
    cached: bool,
    ms: u32,
) -> LlmTrace {
    LlmTrace {
        model_id: model_id.to_owned(),
        prompt_version: request.prompt_version.to_string(),
        input_sha256: hex(&sha256(request.user.as_bytes())),
        output_sha256: hex(&sha256(output.as_bytes())),
        cached,
        ms,
    }
}
