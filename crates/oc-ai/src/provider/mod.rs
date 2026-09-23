//! What the pipeline asks a model: which task, in which words, bounded by which grammar.
//!
//! [`LlmRequest`] carries the whole question, and everything in it but the user message is a
//! `&'static str` taken from the prompt artifacts. That is the byte-identity guarantee in type
//! form: the system prefix of a request *is* a `system.md`, not a string something assembled, so
//! nothing on the way to the wire can make two tasks' prefixes differ (D8, ARCHITECTURE §9.3).
//!
//! **The adapters** (D10, PHASE 11) are the submodules: [`local_sidecar`] — the `llama-server` the
//! engine or the app owns, or any endpoint the probe finds to be one; [`openai_compatible`] — the
//! one OpenAI-compatible client, and a custom endpoint as a configuration of it; [`ollama`] —
//! Ollama's own chat endpoint, the one place its context size and schema can be set. Each is a
//! [`LlmProvider`] over a [`crate::transport::Transport`], so none of them opens a socket.

pub mod local_sidecar;
pub mod ollama;
pub mod openai_compatible;

use oc_model::decision::LlmTrace;
use serde::Serialize;

use crate::digest::{hex, sha256};
use crate::transport::TransportError;

/// Something that answers requests: a model behind an endpoint, or a cassette (D10).
///
/// Three v1 adapters answer from a model — the sidecar the engine or the app owns, a BYO
/// OpenAI-compatible endpoint, and Ollama ([`ProviderKind`]). The first two are
/// [`openai_compatible::OpenAiCompatible`] over a different `Transport` with a different
/// [`ProviderCaps`] and [`ThinkingControl`]; Ollama is [`ollama::Ollama`]. The test tiers answer
/// from cassettes and never from a model (A8.3).
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

/// Which adapter a provider is (D10, PHASE 11). The report and the command line name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// `llama-server`: the one the engine starts, the one the desktop app owns, or one a user runs
    /// that the capability probe recognises. GBNF and `chat_template_kwargs`.
    LocalSidecar,
    /// Any other OpenAI-compatible endpoint — LM Studio, vLLM, a server off this machine. What it
    /// can constrain is what the capability probe found.
    OpenAiCompatible,
    /// Ollama, through its own `/api/chat`: `format`, `options.num_ctx`, `keep_alive`, `think`.
    Ollama,
}

impl ProviderKind {
    /// The name the report prints and `--llm-provider` does not: the command line says `builtin`
    /// for the first, because that is what a user calls it (UI_UX §2.4).
    pub fn as_str(self) -> &'static str {
        match self {
            ProviderKind::LocalSidecar => "local_sidecar",
            ProviderKind::OpenAiCompatible => "openai_compatible",
            ProviderKind::Ollama => "ollama",
        }
    }
}

/// What a provider can do to bound an answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProviderCaps {
    pub constraint: Constraint,
}

impl ProviderCaps {
    /// GBNF in the request: `llama-server`.
    pub const fn grammar() -> Self {
        Self {
            constraint: Constraint::Gbnf,
        }
    }

    /// A JSON Schema in the request: Ollama's `format`, or `response_format`.
    pub const fn json_schema() -> Self {
        Self {
            constraint: Constraint::JsonSchema,
        }
    }

    /// Neither: the schema goes in the prompt, gate S carries the whole burden, and the book's
    /// report says so ([`W_LLM_UNCONSTRAINED`]).
    pub const fn neither() -> Self {
        Self {
            constraint: Constraint::None,
        }
    }
}

/// Raised once per book when its answers came from a provider that constrains neither by grammar
/// nor by schema (PHASE 11 detail 1): unconstrained decoding fails gate S more often, and each
/// failure is a decision made without the model. The report says why.
pub const W_LLM_UNCONSTRAINED: &str = "W_LLM_UNCONSTRAINED";

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
    /// How many prompt tokens the server found already in its KV cache — `llama-server` reports
    /// it as `timings.cache_n`, and OpenAI-compatible servers as
    /// `usage.prompt_tokens_details.cached_tokens`. `None` when the reply says neither.
    pub cached_tokens: Option<u32>,
    /// Why the model stopped: `stop` at the end of an answer, `length` when `max_tokens` cut it
    /// off — in which case gate S will find it unparseable.
    pub finish_reason: Option<String>,
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
    /// Replay has no recording of this question (Appendix B.3 rule 2). Never a fallback to a real
    /// call, and never the closest recording's answer.
    #[error("no cassette is recorded under key {key}; nearest recording: {nearest:?}")]
    CassetteMiss {
        key: String,
        nearest: Option<crate::cassette::Nearest>,
    },
    /// A recording exists under the key and disagrees with the current prompt artifacts — a
    /// hand-edited cassette, or a prompt edited without a version bump (Appendix B.3 rule 3).
    #[error("the cassette under key {key} is stale: {reason}")]
    StaleCassette { key: String, reason: String },
    /// A cassette file could not be read or written.
    #[error("cassette I/O: {0}")]
    Cassette(String),
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
