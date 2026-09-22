//! Versioned prompts (ARCHITECTURE §9.2, IMPLEMENTATION_PLAN Appendix A).
//!
//! Every word a model is shown lives in a file under `crates/oc-ai/prompts/<task>/v<N>/` —
//! `system.md`, `user.tmpl`, `grammar.gbnf`, `schema.json` — and the modules under [`v1`] are
//! `include_str!` wrappers over them plus the typed payload each template is filled with. Nothing
//! in the prompt path is a Rust string literal, so a prompt edit is a text diff a reviewer can
//! read, and every artifact is hashed into something: the grammar into the cache key directly, the
//! rest through [`PROMPT_VERSION`] (ratified R-7).
//!
//! `system.md` is the prefix every task shares byte for byte (ARCHITECTURE §9.3). It is committed
//! once per task directory, as §9.2's layout has it, and test 8.1 holds the four copies identical.
//!
//! A released version is frozen: `prompts/v1.sha256` pins every artifact, because the cache key
//! carries the version and not the system prefix's hash, and an edited prefix under an unchanged
//! version would be answered from cache entries recorded against the old one.

pub mod render;

/// The prompt version every artifact below belongs to, hashed into the cache key (D13.8).
///
/// Bumping it invalidates every cached decision and every cassette recorded against the old
/// prompts, whose files are deleted in the same commit (Appendix B.4, rule 3).
pub const PROMPT_VERSION: u32 = 1;

/// The artifacts of prompt version 1, one module per task.
pub mod v1 {
    pub mod book_structure;
    pub mod heading_roles;
    pub mod metadata;
    pub mod verse_quote;
}

use crate::provider::{LlmRequest, Purpose};

/// The four files one task's prompt consists of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Artifacts {
    pub purpose: Purpose,
    /// `system.md`: the shared prefix.
    pub system: &'static str,
    /// `user.tmpl`: the per-task message, with `{{…}}` slots.
    pub user_template: &'static str,
    /// `grammar.gbnf`: what the answer is decoded under.
    pub grammar: &'static str,
    /// `schema.json`: the same shape for servers that prefer JSON Schema.
    pub schema: &'static str,
}

impl Artifacts {
    /// A request for this task, around a user message already rendered from `user_template`.
    pub(crate) fn request(&self, user: String, max_tokens: u32) -> LlmRequest {
        LlmRequest {
            purpose: self.purpose,
            prompt_version: PROMPT_VERSION,
            system_prefix: self.system,
            user,
            grammar: self.grammar,
            schema: self.schema,
            max_tokens,
        }
    }
}

/// The current version's artifacts for a task.
pub fn artifacts(purpose: Purpose) -> Artifacts {
    match purpose {
        Purpose::Metadata => v1::metadata::ARTIFACTS,
        Purpose::HeadingRoles => v1::heading_roles::ARTIFACTS,
        Purpose::BookStructure => v1::book_structure::ARTIFACTS,
        Purpose::VerseQuote => v1::verse_quote::ARTIFACTS,
    }
}
