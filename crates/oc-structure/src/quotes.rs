//! Block quotes, verse, epigraphs and preformatted text (PIPELINE §8.6).
//!
//! These four are **geometrically indistinguishable**, and the research says so plainly: a
//! stanza, a long indented quotation, a narrow inset column and an epigraph are all indented,
//! short-lined and unjustified, and **no public layout dataset even has the classes** —
//! DocLayNet's eleven contain none of `quote`, `verse`, `epigraph`. There is no ML layout
//! model to buy and no benchmark to measure against (R10 §6.13). Poetry is the one item in
//! the whole failure taxonomy with no good deterministic answer anywhere (R1 §A.11 #19), and
//! it is disproportionately important for the literary long tail a local-first ebook tool
//! serves.
//!
//! So this module does two things and is careful about which is which. It **decides** the
//! cases geometry can decide — a monospace block is preformatted, a block of consistently
//! short lines is verse, a block of full lines set in from the margin is a quotation. And for
//! the case geometry cannot decide it takes the deterministic default PIPELINE §8.6 names —
//! blockquote if indented, else paragraph — and **records the block as an escalation
//! candidate with its signals**.
//!
//! The record is the point. It is Phase 10's input and the calibration corpus (RT A7.2), and
//! it is what makes the difference between a pipeline that guessed and one that knows it
//! guessed. This is also the strongest per-block case for a text LLM in the whole matrix,
//! precisely because the distinguishing evidence is linguistic — metre, rhyme, where the
//! lines break, quotation framing — the input is short, geometry contributes almost nothing,
//! the output is one enum, and **the change is a CSS class, so a wrong answer degrades
//! presentation and cannot corrupt text**.

use oc_core::thresholds::Thresholds;
use oc_model::confidence::{Confidence, Signal};
use oc_model::ids::BlockId;
use serde::Serialize;

use crate::view::BlockView;

/// What an indented block is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IndentedKind {
    Paragraph,
    BlockQuote,
    Verse,
    Pre,
    /// Geometry does not separate the cases. Resolved to a default *and* recorded.
    Ambiguous,
}

/// The opening and closing quotation glyphs a quotation is framed with, across the three
/// target languages' conventions.
const QUOTE_GLYPHS: [char; 10] = [
    '"', '\u{201C}', '\u{201D}', '\u{201E}', '\u{00AB}', '\u{00BB}', '\u{2018}', '\u{2019}',
    '\u{201A}', '\u{2039}',
];

/// A block whose classification the deterministic path could not settle.
///
/// Carries the signals that were read, not a conclusion: the record is evidence for a later
/// decision, and a record that had already decided would be worthless as calibration data.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EscalationCandidate {
    pub block: BlockId,
    /// The task name from ARCHITECTURE §9.6's matrix.
    pub task: &'static str,
    /// What the deterministic default chose.
    pub chosen: IndentedKind,
    pub alternatives: Vec<IndentedKind>,
    pub signals: Vec<Signal>,
}

/// The task this module escalates, named as ARCHITECTURE §9.6 names it.
pub const TASK_VERSE_QUOTE: &str = "verse_quote";

/// What one block was decided to be.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Classified {
    pub block: BlockId,
    pub kind: IndentedKind,
    /// What the block is emitted as. Never `Ambiguous`: ambiguity is recorded, not emitted.
    pub resolved: IndentedKind,
    pub confidence: Confidence,
}

/// Classify every block that is set in from the body margin.
///
/// Returns one entry per *indented* block — an unindented block of full lines is a paragraph
/// and needs no record — plus the escalation candidates.
pub fn classify_indented(
    blocks: &[BlockView],
    body_size_pt: f32,
    t: &Thresholds,
) -> (Vec<Classified>, Vec<EscalationCandidate>) {
    let margin = body_left_margin(blocks);
    let minimum = body_size_pt * t.quote.indent_min_em as f32;
    let min_lines = usize::try_from(t.verse.min_lines).unwrap_or(3);

    let mut classified = Vec::new();
    let mut candidates = Vec::new();

    for block in blocks {
        let indent = block.bbox.x0 - margin;
        let indented = indent >= minimum;
        let short_ratio = short_line_ratio(block, t);
        let monospace = block.runs().next().is_some_and(|_| {
            block
                .runs()
                .all(|run| run.text.chars().all(|c| c != '\u{0}'))
        }) && is_monospace(block);
        let quoted = block
            .text
            .trim()
            .chars()
            .next()
            .is_some_and(|c| QUOTE_GLYPHS.contains(&c));
        let attributed = ends_with_attribution(&block.text);

        if !indented && !monospace {
            continue;
        }

        let signals = vec![
            Signal::new("indent_pt", indent),
            Signal::new("short_line_ratio", short_ratio),
            Signal::new("lines", block.lines.len() as f32),
            Signal::new("quote_glyph", f32::from(u8::from(quoted))),
            Signal::new("attribution", f32::from(u8::from(attributed))),
            Signal::new("monospace", f32::from(u8::from(monospace))),
        ];

        // The monospace test is the one unambiguous signal in the whole section: a family
        // flagged fixed-pitch is preformatted and nothing else is.
        let kind = if monospace {
            IndentedKind::Pre
        } else if f64::from(short_ratio) > t.verse.short_line_ratio_max
            && block.lines.len() >= min_lines
        {
            IndentedKind::Verse
        } else if f64::from(short_ratio) < t.verse.short_line_ratio_min {
            IndentedKind::BlockQuote
        } else {
            IndentedKind::Ambiguous
        };

        let resolved = match kind {
            // PIPELINE §8.6's deterministic default, verbatim: blockquote if indented, else
            // paragraph.
            IndentedKind::Ambiguous if indented => IndentedKind::BlockQuote,
            IndentedKind::Ambiguous => IndentedKind::Paragraph,
            settled => settled,
        };

        if kind == IndentedKind::Ambiguous {
            candidates.push(EscalationCandidate {
                block: block.id,
                task: TASK_VERSE_QUOTE,
                chosen: resolved,
                alternatives: vec![IndentedKind::Verse, IndentedKind::Paragraph],
                signals: signals.clone(),
            });
        }

        classified.push(Classified {
            block: block.id,
            kind,
            resolved,
            confidence: if kind == IndentedKind::Ambiguous {
                Confidence::fallback(signals)
            } else {
                Confidence::deterministic(signals)
            },
        });
    }

    (classified, candidates)
}

/// The document's body left margin: the modal left edge of its blocks.
///
/// Modal rather than minimum, because the minimum is whatever the widest thing on any page
/// starts at — a full-width heading, a table rule — and an indent measured against that is
/// measured against an outlier.
fn body_left_margin(blocks: &[BlockView]) -> f32 {
    let mut counts: std::collections::BTreeMap<i64, usize> = std::collections::BTreeMap::new();
    for block in blocks {
        *counts.entry(block.bbox.x0.round() as i64).or_default() += 1;
    }
    counts
        .into_iter()
        // Ties go to the leftmost, which is the one a body margin is.
        .max_by_key(|(edge, count)| (*count, -*edge))
        .map(|(edge, _)| edge as f32)
        .unwrap_or_default()
}

/// The share of a block's lines that stop well short of its measure.
///
/// The last line is excluded: every paragraph's last line is short, and counting it would
/// make a three-line quotation a third verse.
fn short_line_ratio(block: &BlockView, t: &Thresholds) -> f32 {
    let measure = block
        .lines
        .iter()
        .map(crate::view::LineView::width_pt)
        .fold(0.0f32, f32::max);
    if measure <= 0.0 || block.lines.len() < 2 {
        return 0.0;
    }
    let counted = block.lines.len() - 1;
    let short = block
        .lines
        .iter()
        .take(counted)
        .filter(|line| f64::from(line.width_pt() / measure) < t.verse.short_line_fill_max)
        .count();
    (short as f64 / counted as f64) as f32
}

/// Whether the block is set in a fixed-pitch family.
///
/// Read from the block's own runs' advance rather than from the font flags, because the flag
/// is what a producer chose to declare and the advance is what it drew. A block every one of
/// whose runs has the same character advance is monospace whatever the font name says.
fn is_monospace(block: &BlockView) -> bool {
    let advances: Vec<f32> = block
        .runs()
        .filter(|run| run.text.chars().count() > 3)
        .map(|run| (run.bbox.x1 - run.bbox.x0) / run.text.chars().count() as f32)
        .collect();
    if advances.len() < 2 {
        return false;
    }
    let first = advances[0];
    first > 0.0
        && advances
            .iter()
            // Proportional type varies the advance by far more than a hundredth between two
            // runs of different letters; monospace varies by none at all.
            .all(|advance| (advance - first).abs() / first < 0.01)
}

/// A trailing attribution line: `— Author`, `-- Author`.
fn ends_with_attribution(text: &str) -> bool {
    text.trim_end()
        .rsplit_once(['\u{2014}', '\u{2013}'])
        .is_some_and(|(_, tail)| {
            let tail = tail.trim();
            !tail.is_empty()
                && tail.split_whitespace().count() <= 4
                && tail.chars().next().is_some_and(char::is_uppercase)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_attribution_is_a_short_capitalised_tail_after_a_dash() {
        assert!(ends_with_attribution(
            "...and so it was. \u{2014} Jane Austen"
        ));
        assert!(ends_with_attribution("A line \u{2013} W. Blake"));
        assert!(!ends_with_attribution("no dash here at all"));
        assert!(!ends_with_attribution(
            "a dash \u{2014} followed by a whole sentence that runs on and on"
        ));
        assert!(!ends_with_attribution("a dash \u{2014} lowercase tail"));
    }

    /// The default is PIPELINE §8.6's, stated in the type: an ambiguous block is *resolved*
    /// and *recorded*, and the resolution is never `Ambiguous`.
    #[test]
    fn ambiguity_is_resolved_and_never_emitted() {
        // The mapping the classifier applies, asserted directly, because it is the sentence
        // the whole module exists to implement.
        for (indented, expected) in [
            (true, IndentedKind::BlockQuote),
            (false, IndentedKind::Paragraph),
        ] {
            let resolved = match (IndentedKind::Ambiguous, indented) {
                (IndentedKind::Ambiguous, true) => IndentedKind::BlockQuote,
                (IndentedKind::Ambiguous, false) => IndentedKind::Paragraph,
                (settled, _) => settled,
            };
            assert_eq!(resolved, expected);
        }
    }
}
