//! Run-in headings: the case where the font-size z-score is literally zero (R10 §6.7).
//!
//! A run-in heading is a bold or italic phrase at the start of a paragraph, set at body size
//! and inline with the text that follows it. Geometry says nothing about it — same size, same
//! line, same block — so the style-cluster method cannot see it at all, and a cheap detector
//! of its own is the only thing that can.
//!
//! **In v1 a run-in candidate is not a heading.** It is a proposal, recorded with its
//! evidence, which Phase 10 may confirm. That asymmetry is deliberate: a false positive here
//! splits a paragraph and moves its first clause into the table of contents, which is a
//! visible corruption of the book; a false negative loses a level of navigation, which is
//! not. Detail 4 of the plan says so, and this module's output shape says so too — it returns
//! candidates, and nothing in v1 promotes them.

use oc_core::thresholds::Thresholds;
use oc_model::ids::BlockId;
use serde::Serialize;

use crate::view::BlockView;

/// The characters a run-in heading is terminated by (detail 4).
const TERMINATORS: [char; 4] = ['.', '\u{2014}', ':', '\u{2013}'];

/// A proposal that a paragraph opens with a run-in heading.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunInCandidate {
    pub block: BlockId,
    pub order: u32,
    /// The phrase itself, without its terminator.
    pub text: String,
    pub words: u32,
    pub bold: bool,
    pub italic: bool,
}

/// Find the run-in heading candidates in a document.
///
/// Only the *first* run of a block is considered, and only when the block has a second run
/// that is not in the same style: a paragraph set entirely in bold is a bold paragraph, and
/// a one-run block is whatever the style clustering already said it was.
pub fn run_in_candidates(blocks: &[BlockView], t: &Thresholds) -> Vec<RunInCandidate> {
    let max_words = u32::try_from(t.headings.runin_max_words).unwrap_or(u32::MAX);

    blocks
        .iter()
        .filter_map(|block| {
            let mut runs = block.runs();
            let first = runs.next()?;
            let bold = i64::from(first.weight) >= t.headings.bold_weight_min;
            let italic = first.italic;
            if !bold && !italic {
                return None;
            }
            // Something must follow it in a different style, or it is not *run-in*.
            let rest = runs.next()?;
            if (i64::from(rest.weight) >= t.headings.bold_weight_min) == bold
                && rest.italic == italic
            {
                return None;
            }

            let text = first.text.trim();
            let stripped = text.trim_end_matches(|c: char| TERMINATORS.contains(&c) || c == ' ');
            if stripped.len() == text.len() {
                // No terminator: a bold phrase mid-thought, not a heading.
                return None;
            }
            let words = u32::try_from(stripped.split_whitespace().count()).unwrap_or(u32::MAX);
            if words == 0 || words > max_words {
                return None;
            }

            Some(RunInCandidate {
                block: block.id,
                order: block.order,
                text: stripped.to_owned(),
                words,
                bold,
                italic,
            })
        })
        .collect()
}
