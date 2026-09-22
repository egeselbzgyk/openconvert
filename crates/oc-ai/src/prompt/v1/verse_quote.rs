//! Task 4, `verse_quote`, version 1 (ARCHITECTURE §9.6, R10 §6.13).
//!
//! The input is one batch of ambiguous indented blocks — `llm.verse_quote_blocks_per_call` of
//! them at most — each with its verbatim text and its geometry as categories: an indent that is
//! shallow or deep, a line count, an average line length in words. The answer names a kind per
//! block, and because the kind is a CSS class a wrong answer degrades presentation and cannot
//! corrupt text.

use oc_model::ids::BlockId;
use serde::Serialize;

use crate::prompt::render::{self, RenderError};
use crate::prompt::Artifacts;
use crate::provider::{LlmRequest, Purpose};

pub const ARTIFACTS: Artifacts = Artifacts {
    purpose: Purpose::VerseQuote,
    system: include_str!("../../../prompts/verse_quote/v1/system.md"),
    user_template: include_str!("../../../prompts/verse_quote/v1/user.tmpl"),
    grammar: include_str!("../../../prompts/verse_quote/v1/grammar.gbnf"),
    schema: include_str!("../../../prompts/verse_quote/v1/schema.json"),
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Indent {
    Shallow,
    Deep,
}

/// One ambiguous block, as the model sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BlockSummary {
    /// The block's id: what the answer's `id` must name.
    pub id: BlockId,
    /// Verbatim, line breaks kept: where a line breaks is half the evidence for verse.
    pub text: String,
    pub indent: Indent,
    pub lines: u32,
    pub avg_line_words: u32,
    pub centered: bool,
    pub monospace: bool,
}

/// One batch of blocks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct VerseQuoteInput {
    pub blocks: Vec<BlockSummary>,
}

/// The verse-or-quote request for this batch.
pub fn request(input: &VerseQuoteInput, max_tokens: u32) -> Result<LlmRequest, RenderError> {
    let payload = render::json(input)?;
    let user = render::fill(ARTIFACTS.user_template, &[("payload", &payload)])?;
    Ok(ARTIFACTS.request(user, max_tokens))
}
