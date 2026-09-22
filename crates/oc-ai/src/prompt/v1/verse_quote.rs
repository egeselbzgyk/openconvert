//! Task 4, `verse_quote`, version 1 (ARCHITECTURE §9.6, R10 §6.13).
//!
//! The input is one batch of ambiguous indented blocks — `llm.verse_quote_blocks_per_call` of
//! them at most — each with its verbatim text and its geometry as categories: an indent that is
//! shallow or deep, a line count, an average line length in words. The answer names a kind per
//! block, and because the kind is a CSS class a wrong answer degrades presentation and cannot
//! corrupt text.

use std::collections::BTreeMap;

use oc_model::ids::BlockId;
use serde::{Deserialize, Serialize};

use crate::gates::schema::{bijection, closed, Answer};
use crate::gates::GateFailure;
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

/// What an ambiguous indented block is: a wrapper and a CSS class, never its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BlockKind {
    Verse,
    Blockquote,
    Preformatted,
    Paragraph,
}

impl BlockKind {
    /// Every kind, in the order the grammar lists them.
    pub const ALL: [BlockKind; 4] = [
        BlockKind::Verse,
        BlockKind::Blockquote,
        BlockKind::Preformatted,
        BlockKind::Paragraph,
    ];

    /// The kind as the grammar spells it.
    pub fn as_str(self) -> &'static str {
        match self {
            BlockKind::Verse => "verse",
            BlockKind::Blockquote => "blockquote",
            BlockKind::Preformatted => "preformatted",
            BlockKind::Paragraph => "paragraph",
        }
    }

    /// The kind a name spells, exactly.
    pub fn from_name(name: &str) -> Option<BlockKind> {
        BlockKind::ALL
            .into_iter()
            .find(|kind| kind.as_str() == name)
    }
}

/// The answer, as gate S admits it: one kind for every block of the batch.
///
/// The model's own checks-and-balances — `preformatted` needs the monospace flag, `verse` needs
/// three lines — are the task's validation, applied after this gate (Phase 10, test 10.12).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerseQuoteAnswer {
    pub kinds: BTreeMap<BlockId, BlockKind>,
}

/// The answer as JSON spells it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerseQuoteWire {
    b: Vec<KindEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KindEntry {
    id: String,
    k: String,
}

impl Answer for VerseQuoteAnswer {
    type Context = VerseQuoteInput;
    type Wire = VerseQuoteWire;

    fn check(wire: VerseQuoteWire, batch: &VerseQuoteInput) -> Result<Self, GateFailure> {
        let mut named = Vec::with_capacity(wire.b.len());
        for entry in &wire.b {
            named.push((
                entry.id.as_str(),
                closed("k", &entry.k, BlockKind::from_name)?,
            ));
        }
        bijection(
            "block",
            batch.blocks.iter().map(|block| block.id.to_string()),
            wire.b.iter().map(|entry| entry.id.clone()),
        )?;

        let mut kinds = BTreeMap::new();
        for block in &batch.blocks {
            if let Some((_, kind)) = named.iter().find(|(id, _)| *id == block.id.as_str()) {
                kinds.insert(block.id, *kind);
            }
        }
        Ok(VerseQuoteAnswer { kinds })
    }
}
