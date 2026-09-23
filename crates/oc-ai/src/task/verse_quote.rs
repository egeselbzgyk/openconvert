//! Task 4, `verse_quote`: batches of exactly ten, a book-wide cap of thirty, and deterministic
//! counter-evidence that a model's label does not override (PHASE 10 detail 5, ARCHITECTURE §9.6,
//! R10 §6.13, ratified note N-4).
//!
//! **Batches of `llm.verse_quote_blocks_per_call`.** The cap `llm.max_blocks_per_book` therefore
//! costs at most three of the book's eight calls. A block past the cap is never sent: its
//! escalation predicate abstained ("the book's block budget is spent", `oc-structure::escalate`),
//! and it takes the deterministic default.
//!
//! **Counter-evidence wins.** `preformatted` needs the monospace flag — a block the geometry says
//! is proportional type is not code, and a wrongly declared `<pre>` breaks reflow on a phone.
//! `verse` needs at least `verse.min_lines` lines and a short-line ratio above
//! `verse.llm_short_line_ratio_min` — two lines are not a stanza. A label that fails its
//! counter-evidence is **downgraded** to the block's deterministic default, not rejected: the other
//! blocks of the batch were answered on their own evidence.
//!
//! Each batch is its own call and its own verdict. A batch gate S refuses leaves its ten blocks
//! at the default and says nothing about the next batch.

use std::collections::{BTreeMap, BTreeSet};

use oc_model::ids::BlockId;

use crate::prompt::v1::verse_quote::{BlockKind, BlockSummary, VerseQuoteAnswer, VerseQuoteInput};
use crate::session::{Asker, TaskResult};

/// The `Decision.fallback` of a block whose label its counter-evidence overrode.
pub const COUNTER_EVIDENCE: &str = "counter_evidence";

/// The numbers the task reads, from `thresholds.toml`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VerseQuoteLimits {
    /// `llm.verse_quote_blocks_per_call`.
    pub blocks_per_call: usize,
    /// `llm.max_blocks_per_book`.
    pub max_blocks: usize,
    /// `verse.min_lines`.
    pub verse_min_lines: u32,
    /// `verse.llm_short_line_ratio_min`: `verse` needs a short-line ratio above it.
    pub verse_min_short_line_ratio: f32,
}

/// What the deterministic path knows about a block that the model is not shown as a number.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockEvidence {
    pub lines: u32,
    pub short_line_ratio: f32,
    pub monospace: bool,
    /// The deterministic default the block has now: what a downgrade returns it to.
    pub default: BlockKind,
}

/// One ambiguous block: what the model is shown, and what it is checked against.
#[derive(Clone, Debug, PartialEq)]
pub struct AmbiguousBlock {
    pub summary: BlockSummary,
    pub evidence: BlockEvidence,
}

/// A label, after the deterministic counter-evidence has had its say.
pub fn counter_evidence(
    kind: BlockKind,
    evidence: &BlockEvidence,
    limits: &VerseQuoteLimits,
) -> BlockKind {
    match kind {
        BlockKind::Preformatted if !evidence.monospace => evidence.default,
        BlockKind::Verse
            if evidence.lines < limits.verse_min_lines
                || evidence.short_line_ratio <= limits.verse_min_short_line_ratio =>
        {
            evidence.default
        }
        admitted => admitted,
    }
}

/// One batch's labels, after counter-evidence, and which of them were downgraded.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KindEdit {
    pub kinds: BTreeMap<BlockId, BlockKind>,
    /// Blocks whose label the counter-evidence overrode: recorded, because a model that keeps
    /// calling two-line blocks verse is a finding.
    pub downgraded: BTreeSet<BlockId>,
}

/// One batch: its blocks, in the order sent, and how its question ended.
#[derive(Clone, Debug, PartialEq)]
pub struct Batch {
    pub blocks: Vec<BlockId>,
    pub result: TaskResult<KindEdit>,
}

/// The blocks the book may send, batched: at most `max_blocks` of them, in reading order, in
/// batches of at most `blocks_per_call`.
pub fn batches<'a>(
    blocks: &'a [AmbiguousBlock],
    limits: &VerseQuoteLimits,
) -> Vec<&'a [AmbiguousBlock]> {
    let sendable = &blocks[..blocks.len().min(limits.max_blocks)];
    sendable.chunks(limits.blocks_per_call.max(1)).collect()
}

/// Ask about the first `granted` batches; the rest are not asked and keep their defaults.
pub fn run(
    asker: &mut dyn Asker,
    blocks: &[AmbiguousBlock],
    granted: usize,
    limits: &VerseQuoteLimits,
    max_tokens: u32,
) -> Vec<Batch> {
    batches(blocks, limits)
        .into_iter()
        .enumerate()
        .map(|(index, batch)| {
            let ids: Vec<BlockId> = batch.iter().map(|block| block.summary.id).collect();
            if index >= granted {
                return Batch {
                    blocks: ids,
                    result: TaskResult::Unasked(crate::session::Unasked::Budget(
                        oc_model::doc::Warning::new(
                            crate::budget::W_LLM_BUDGET_EXHAUSTED,
                            oc_model::doc::Severity::Warn,
                        )
                        .with_arg("task", crate::provider::Purpose::VerseQuote.as_str())
                        .with_arg("calls", granted.to_string()),
                    )),
                };
            }
            Batch {
                blocks: ids,
                result: ask_batch(asker, batch, limits, max_tokens),
            }
        })
        .collect()
}

fn ask_batch(
    asker: &mut dyn Asker,
    batch: &[AmbiguousBlock],
    limits: &VerseQuoteLimits,
    max_tokens: u32,
) -> TaskResult<KindEdit> {
    let input = VerseQuoteInput {
        blocks: batch.iter().map(|block| block.summary.clone()).collect(),
    };
    let request = match crate::prompt::v1::verse_quote::request(&input, max_tokens) {
        Ok(request) => request,
        Err(error) => {
            return TaskResult::Unasked(crate::session::Unasked::Unavailable(
                crate::provider::LlmError::Protocol(error.to_string()),
            ))
        }
    };
    let asked = match asker.ask(&request) {
        Ok(asked) => asked,
        Err(unasked) => return TaskResult::Unasked(unasked),
    };
    match crate::gates::schema::gate_response::<VerseQuoteAnswer>(&asked.response, &input) {
        Ok(answer) => {
            let mut edit = KindEdit::default();
            for block in batch {
                let Some(kind) = answer.kinds.get(&block.summary.id) else {
                    continue;
                };
                let settled = counter_evidence(*kind, &block.evidence, limits);
                if settled != *kind {
                    edit.downgraded.insert(block.summary.id);
                }
                edit.kinds.insert(block.summary.id, settled);
            }
            TaskResult::Admitted {
                trace: asked.trace,
                answer: edit,
            }
        }
        Err(failure) => TaskResult::Rejected {
            trace: asked.trace,
            failure,
        },
    }
}
