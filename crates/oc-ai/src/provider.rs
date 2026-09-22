//! What the pipeline asks a model: which task, in which words, bounded by which grammar.
//!
//! [`LlmRequest`] carries the whole question, and everything in it but the user message is a
//! `&'static str` taken from the prompt artifacts. That is the byte-identity guarantee in type
//! form: the system prefix of a request *is* a `system.md`, not a string something assembled, so
//! nothing on the way to the wire can make two tasks' prefixes differ (D8, ARCHITECTURE §9.3).

use oc_model::decision::LlmTrace;
use serde::Serialize;

use crate::digest::{hex, sha256};

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
