//! The attributed form of the conservation law's difference (PHASE 7.5 item 2).
//!
//! `ledger_check` answers *whether* a stage balanced. It cannot answer *what left*, because
//! `C(D)` is a multiset of characters and a multiset has no idea which paragraph it came
//! from. The first defect Phase 7.5 found cost forty minutes of hand-written binary search
//! over page prefixes to learn that one page of one book lost 644 characters — and that cost
//! is paid again for every class, by every person, forever.
//!
//! So the difference is taken over **labelled** text rather than over bare strings. The
//! label is whatever the stage calls the thing — a block id and its page on the way in, a
//! paragraph or a table cell on the way out — and it travels with the characters, so a
//! refusal can name the block that was dropped and the claimant that dropped it.
//!
//! This module is in `oc-core` and not in a stage on purpose. Every stage that moves text
//! between containers can lose it the same way, so the vocabulary for saying so belongs
//! where all of them can reach it (ARCHITECTURE §5).

use std::collections::BTreeMap;

use oc_model::extract::CharHistogram;
use oc_model::ledger::c_of;

/// A labelled piece of text on one side of a stage.
///
/// `claimed_by` is the part that makes the diff diagnostic rather than merely descriptive:
/// a block that vanished is a question, and a block that vanished *because a list said it
/// would emit it* is an answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unit {
    /// What names this text to a reader: `block 12`, `table t0 cell`, `note n3 body`.
    pub label: String,
    /// The zero-based page it came from, or `None` when the unit does not sit on one page.
    pub page: Option<u32>,
    /// The structure that took this unit out of the flow, when one did.
    pub claimed_by: Option<String>,
    pub text: String,
}

impl Unit {
    /// A unit nothing has claimed.
    pub fn new(label: impl Into<String>, page: Option<u32>, text: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            page,
            claimed_by: None,
            text: text.into(),
        }
    }

    /// The same unit, recorded as claimed by `claimant`.
    #[must_use]
    pub fn claimed_by(mut self, claimant: impl Into<String>) -> Self {
        self.claimed_by = Some(claimant.into());
        self
    }
}

/// What a stage was given, what it produced, and which labelled units did not survive.
#[derive(Clone, Debug, Default)]
pub struct Diff {
    /// `C(D_in) \ C(D_out)`: the characters the stage lost.
    pub lost: CharHistogram,
    /// `C(D_out) \ C(D_in)`: the characters the stage invented.
    pub appeared: CharHistogram,
    /// Input units whose text the output does not contain, longest first.
    pub unaccounted: Vec<Unit>,
    /// Output units whose text the input does not contain, longest first.
    pub unexpected: Vec<Unit>,
}

impl Diff {
    /// Whether the stage balanced. A balanced stage may still have unaccounted units — text
    /// moved between two blocks balances and is still wrong — so this is not the whole gate.
    pub fn balances(&self) -> bool {
        self.lost.is_empty() && self.appeared.is_empty()
    }
}

/// Whitespace-normalised text, which is what the comparison is over.
///
/// A block's text is its lines joined with a space; a paragraph's text is the same lines
/// after the stage has rewrapped them. Comparing them literally would report every paragraph
/// in the book as unaccounted, which is a diagnostic that names everything and therefore
/// nothing. Whitespace is outside `C` in spirit — I-1 counts it, but no stage is *supposed*
/// to preserve where the line breaks fell — so it is folded here and the character-level
/// difference above is what remains exact.
fn normalise(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The attributed difference between a stage's input and its output.
///
/// Two answers, and they are different questions. The histograms are exact and cheap: they
/// are what I-1 is stated over. The unit lists are an *attribution*, and attribution over a
/// multiset is not unique — if two blocks carry the same sentence and one of them is dropped,
/// nothing in the text says which. The rule here is containment: an input unit is accounted
/// for when its text appears somewhere in the output, and the units reported are the ones for
/// which no such place exists. That is sound in the direction that matters — a unit reported
/// as unaccounted really is absent — and it is deliberately silent about a unit that was
/// merely moved.
pub fn diff(input: &[Unit], output: &[Unit]) -> Diff {
    let mut before = CharHistogram::new();
    for unit in input {
        before = before.union(&c_of(&unit.text));
    }
    let mut after = CharHistogram::new();
    for unit in output {
        after = after.union(&c_of(&unit.text));
    }

    Diff {
        lost: before.difference(&after),
        appeared: after.difference(&before),
        unaccounted: missing_from(input, output),
        unexpected: missing_from(output, input),
    }
}

/// The units of `needles` whose text does not appear anywhere in `haystack`.
///
/// Exact match first, through a map, because nearly every unit is carried across unchanged
/// and a map lookup is the difference between a diagnostic that runs in a second and one that
/// runs in a minute on a three-hundred-page book. Only the units that miss pay for the
/// substring scan, and on a healthy document there are none.
fn missing_from(needles: &[Unit], haystack: &[Unit]) -> Vec<Unit> {
    let mut exact: BTreeMap<String, usize> = BTreeMap::new();
    for unit in haystack {
        let text = normalise(&unit.text);
        if !text.is_empty() {
            *exact.entry(text).or_default() += 1;
        }
    }
    // One string to scan, so a unit the output merged into a longer paragraph — a block
    // joined to its continuation, a drop cap joined to the paragraph it opens — is still
    // found. The separator is a newline so that two adjacent units cannot accidentally spell
    // a third across the join.
    let joined: String = haystack
        .iter()
        .map(|unit| normalise(&unit.text))
        .collect::<Vec<_>>()
        .join("\n");

    let mut missing: Vec<Unit> = Vec::new();
    for unit in needles {
        let text = normalise(&unit.text);
        if text.is_empty() {
            continue;
        }
        match exact.get_mut(&text) {
            // The output carries this text, and one copy of it is now spoken for. A second
            // input unit with the same words needs a *second* copy, which is why the count
            // is decremented rather than merely tested: two identical blocks and one
            // surviving paragraph is one block lost, and a membership test would call it
            // none.
            Some(count) if *count > 0 => {
                *count -= 1;
            }
            // The output carried this text and has run out of copies. The containment
            // fallback must not run here — the whole string is present, so it would always
            // match, and the missing copy would go unreported.
            Some(_) => missing.push(unit.clone()),
            // Never present as a unit of its own. It may still have been merged into a
            // longer one: a drop cap joined to the paragraph it opens, a block joined to its
            // continuation across a page break.
            None => {
                if !joined.contains(&text) {
                    missing.push(unit.clone());
                }
            }
        }
    }
    // Longest first: a reader fixing a conservation defect wants the paragraph that vanished,
    // not the stray page number, and a diagnostic that buries the first under a hundred of
    // the second has answered the wrong question.
    missing.sort_by(|left, right| {
        right
            .text
            .chars()
            .count()
            .cmp(&left.text.chars().count())
            .then_with(|| left.label.cmp(&right.label))
    });
    missing
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(label: &str, text: &str) -> Unit {
        Unit::new(label, Some(0), text)
    }

    #[test]
    fn a_stage_that_carried_everything_across_has_an_empty_diff() {
        let input = vec![
            unit("block 0", "the first paragraph"),
            unit("block 1", "and"),
        ];
        let output = vec![
            unit("para p0", "the first paragraph"),
            unit("para p1", "and"),
        ];

        let result = diff(&input, &output);

        assert!(result.balances());
        assert!(result.unaccounted.is_empty());
        assert!(result.unexpected.is_empty());
    }

    #[test]
    fn a_dropped_block_is_named_with_the_claimant_that_took_it() {
        let input = vec![
            unit("block 0", "kept"),
            unit("block 1", "taken and never emitted").claimed_by("list l0"),
        ];
        let output = vec![unit("para p0", "kept")];

        let result = diff(&input, &output);

        assert!(!result.balances());
        assert_eq!(
            result.lost,
            c_of("taken and never emitted"),
            "exactly the characters of the dropped block, and no others"
        );
        assert_eq!(result.unaccounted.len(), 1);
        assert_eq!(result.unaccounted[0].label, "block 1");
        assert_eq!(result.unaccounted[0].claimed_by.as_deref(), Some("list l0"));
    }

    #[test]
    fn a_block_merged_into_a_longer_paragraph_is_accounted_for() {
        // The drop-cap join and the paragraph continuation both have this shape, and a
        // diagnostic that called them losses would be noise on every healthy book.
        let input = vec![
            unit("block 0", "W"),
            unit("block 1", "hen the survey ended"),
        ];
        let output = vec![unit("para p0", "When the survey ended")];

        let result = diff(&input, &output);

        assert!(result.unaccounted.is_empty());
    }

    #[test]
    fn text_the_stage_invented_is_reported_in_the_other_direction() {
        let input = vec![unit("block 0", "once")];
        let output = vec![unit("para p0", "once"), unit("para p1", "once")];

        let result = diff(&input, &output);

        assert!(result.lost.is_empty());
        assert_eq!(result.appeared.total(), 4);
        // Both output units match the one input unit by containment, so duplication is a
        // histogram fact rather than a unit fact. The two questions are answered separately
        // and this is the one that answers it.
        assert!(!result.balances());
    }

    #[test]
    fn two_identical_blocks_and_one_survivor_reports_exactly_one_loss() {
        let input = vec![
            unit("block 0", "same words here"),
            unit("block 1", "same words here"),
        ];
        let output = vec![unit("para p0", "same words here")];

        let result = diff(&input, &output);

        assert_eq!(result.unaccounted.len(), 1);
    }

    #[test]
    fn whitespace_is_folded_so_rewrapping_is_not_a_loss() {
        let input = vec![unit("block 0", "a line\nand   another")];
        let output = vec![unit("para p0", "a line and another")];

        let result = diff(&input, &output);

        assert!(result.unaccounted.is_empty());
    }
}
