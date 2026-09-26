//! Which blocks could be headings, and on what evidence (PIPELINE §8.2).
//!
//! Membership of a candidate style cluster is necessary and not sufficient. A caption is
//! often set smaller and italic; an epigraph is often set larger, centred and italic and is
//! **not a heading at all**; a running head is a heading in every geometric respect and is
//! furniture. So each candidate carries the three pieces of evidence PIPELINE §8.2 names, and
//! two of them are required:
//!
//! - **a short line**, under `headings.short_line_max_width_ratio` of its column;
//! - **not ending in a sentence-continuing character**, because a heading is not half of a
//!   sentence;
//! - **whitespace above greater than the body leading** — recorded, and satisfied trivially
//!   by a block at the top of its column, which is where chapter headings live. Requiring a
//!   measured gap there would reject exactly the headings that matter most.

use oc_core::thresholds::Thresholds;
use oc_model::extract::FontInfo;
use oc_model::ids::{BlockId, ClusterId};
use serde::Serialize;

use crate::headings::cluster::StyleInventory;
use crate::view::BlockView;

/// A block that could be a heading, with the evidence for and against.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HeadingCandidate {
    pub block: BlockId,
    /// Position in the document's reading order.
    pub order: u32,
    pub page: u32,
    pub text: String,
    pub cluster: ClusterId,
    /// The share of the column the block's widest line covers.
    pub width_ratio: f32,
    /// Under `headings.short_line_max_width_ratio` of the column.
    pub short_line: bool,
    /// Ends in `,`, `;`, `:` or a dash: half a sentence, not a heading.
    pub sentence_continuing: bool,
    /// Air above it, or the top of its column.
    pub space_above: bool,
    /// The text reads as words or a number rather than as marks: a scan's text layer turns a
    /// rule, a smudge or a line drawing into `/ l \` set large, and that is not a heading.
    pub legible: bool,
}

impl HeadingCandidate {
    /// Whether the evidence admits this block as a heading at all.
    ///
    /// Two of the three conditions, and the third is recorded rather than required — see the
    /// module docs.
    pub fn is_admissible(&self) -> bool {
        self.short_line && !self.sentence_continuing && self.legible
    }
}

/// Find the heading candidates among a document's blocks.
pub fn heading_candidates(
    blocks: &[BlockView],
    inventory: &StyleInventory,
    fonts: &[FontInfo],
    t: &Thresholds,
) -> Vec<HeadingCandidate> {
    let candidate_clusters = inventory.candidates(t);
    if candidate_clusters.is_empty() {
        return Vec::new();
    }
    let body_size = inventory.body_size_pt();
    let leading = body_size * t.headings.space_above_min_em as f32;

    blocks
        .iter()
        .filter_map(|block| {
            let cluster = dominant_cluster(block, inventory, fonts, t)?;
            if !candidate_clusters.contains(&cluster) {
                return None;
            }
            let text = block.text.trim().to_owned();
            if text.is_empty() {
                return None;
            }
            let width_ratio = block.width_ratio();
            Some(HeadingCandidate {
                block: block.id,
                order: block.order,
                page: block.page,
                sentence_continuing: ends_mid_sentence(&text),
                short_line: f64::from(width_ratio) < t.headings.short_line_max_width_ratio,
                // A block with nothing above it in its column has all the whitespace there
                // is above it.
                space_above: block.space_above_pt <= 0.0 || block.space_above_pt > leading,
                legible: legible(&text, t),
                width_ratio,
                text,
                cluster,
            })
        })
        .collect()
}

/// The cluster holding most of a block's characters, if it holds enough of them.
///
/// `headings.cluster_char_share_min` is what keeps a heading with one italic word in it in
/// the heading's cluster, and what refuses a body paragraph whose first character is a drop
/// cap set at four times the body size.
pub fn dominant_cluster(
    block: &BlockView,
    inventory: &StyleInventory,
    fonts: &[FontInfo],
    t: &Thresholds,
) -> Option<ClusterId> {
    let mut totals: std::collections::BTreeMap<ClusterId, u64> = std::collections::BTreeMap::new();
    let mut total = 0u64;
    for run in block.runs() {
        let chars =
            u64::try_from(run.text.chars().filter(|c| !c.is_whitespace()).count()).unwrap_or(0);
        if chars == 0 {
            continue;
        }
        total = total.saturating_add(chars);
        if let Some(cluster) = inventory.cluster_of(run, fonts, t) {
            let entry = totals.entry(cluster).or_default();
            *entry = entry.saturating_add(chars);
        }
    }
    if total == 0 {
        return None;
    }
    let (cluster, count) = totals
        .into_iter()
        .max_by_key(|(id, count)| (*count, id.0))?;
    let share = count as f64 / total as f64;
    (share >= t.headings.cluster_char_share_min).then_some(cluster)
}

/// Whether a heading's text reads as text: enough letters or digits, and mostly letters or
/// digits rather than marks. Script-free — `is_alphanumeric` is every script's letters.
///
/// A number is text however short it is and however many of its characters are full stops:
/// digits of any script with the stops a heading hangs on them or sets between them (`7`, `1.`,
/// `2.1.`). Counted by share, `1.` is half marks, and a novel whose chapters open `1.` … `9.`
/// lost exactly those nine while `10.` onwards passed.
pub fn legible(text: &str, t: &Thresholds) -> bool {
    let visible: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
    if visible.is_empty() {
        return false;
    }
    let alnum = visible.iter().filter(|c| c.is_alphanumeric()).count();
    let min = usize::try_from(t.headings.legible_min_chars.max(1)).unwrap_or(usize::MAX);
    let number = alnum > 0 && visible.iter().all(|c| c.is_numeric() || *c == '.');
    number || (alnum >= min && alnum as f64 >= t.headings.legible_min_share * visible.len() as f64)
}

/// Whether the text ends in a character that continues a sentence.
///
/// Not "does not end in a full stop": most headings end in no punctuation at all, and a
/// rule that required one would reject every chapter title in every novel.
fn ends_mid_sentence(text: &str) -> bool {
    matches!(
        text.chars().next_back(),
        Some(',' | ';' | ':' | '-' | '\u{2013}' | '\u{2014}')
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use oc_core::thresholds::T;
    use oc_model::extract::{FontId, PageRef};
    use oc_model::geom::Rect;
    use oc_model::layout::BlockKindHint;
    use oc_model::text::{Line, Run, RunId, TextProvenance};

    use crate::headings::cluster::cluster_styles;
    use crate::view::LineView;

    fn fonts() -> Vec<FontInfo> {
        vec![FontInfo {
            id: FontId(0),
            name: "Test-Regular".to_owned(),
            family_key: "test".to_owned(),
            serif: true,
            fixed_pitch: false,
            symbolic: false,
            type3: false,
            embedded: true,
        }]
    }

    /// A one-line, one-run block at `size_pt`, `width` points wide in a 450 pt column.
    fn block(page: u32, order: u32, y0: f32, text: &str, size_pt: f32, width: f32) -> BlockView {
        let bbox = Rect {
            x0: 77.0,
            y0,
            x1: 77.0 + width,
            y1: y0 + size_pt,
        };
        let run = Run {
            id: RunId(order),
            page: PageRef::new(page),
            text: text.to_owned(),
            bbox,
            baseline_y: bbox.y1,
            font: FontId(0),
            size_pt,
            weight: 400,
            italic: false,
            superscript: false,
            subscript: false,
            provenance: TextProvenance::Pdf,
            glyph_range: (0, 0),
        };
        let line = LineView {
            line: Line {
                runs: vec![run.id],
                bbox,
                baseline_y: bbox.y1,
                ends_with_hyphen: false,
                indent_pt: 0.0,
                right_gap_pt: 0.0,
            },
            text: text.to_owned(),
            runs: vec![run],
            glue: false,
        };
        BlockView {
            id: BlockId::derive(page, bbox, text),
            page,
            order,
            bbox,
            column: 0,
            kind_hint: BlockKindHint::Text,
            text: text.to_owned(),
            lines: vec![line],
            column_width_pt: 450.0,
            space_above_pt: 0.0,
            page_height_pt: 792.0,
            para_starts: Vec::new(),
            continues: None,
        }
    }

    /// A novel whose chapters open with nothing but a number and its full stop — `1.`, `2.` …
    /// `12.` — set larger than the body, over pages of body text. Every label is a heading
    /// candidate: `1.` is half punctuation by count and wholly a chapter number, exactly as
    /// `10.` is.
    #[test]
    fn a_chapter_number_with_its_full_stop_is_a_heading_candidate() {
        let body = "Body text set full out across the measure, running on at length.";
        let mut blocks = Vec::new();
        let mut order = 0;
        for chapter in 1..=12u32 {
            let first = chapter * 4;
            blocks.push(block(
                first,
                order,
                122.0,
                &format!("{chapter}."),
                21.25,
                15.0,
            ));
            order += 1;
            for (page, top) in [(first, 204.0), (first + 1, 77.0), (first + 2, 77.0)] {
                for line in 0..20u16 {
                    let y0 = top + f32::from(line) * 21.0;
                    blocks.push(block(page, order, y0, body, 15.0, 440.0));
                    order += 1;
                }
            }
        }
        let runs: Vec<Run> = blocks.iter().flat_map(|b| b.runs().cloned()).collect();
        let inventory = cluster_styles(&runs, &fonts(), &T);
        let admitted: Vec<String> = heading_candidates(&blocks, &inventory, &fonts(), &T)
            .into_iter()
            .filter(HeadingCandidate::is_admissible)
            .map(|candidate| candidate.text)
            .collect();
        let expected: Vec<String> = (1..=12).map(|n| format!("{n}.")).collect();
        assert_eq!(admitted, expected);
    }

    /// A number with the full stops a heading hangs on it or sets inside it reads as text, in
    /// any script's digits; marks alone, or a digit among marks, still do not.
    #[test]
    fn a_number_and_its_full_stops_are_legible_and_marks_are_not() {
        for label in ["1.", "9.", "10.", "2.1.", "7", "\u{0661}.", "\u{0967}."] {
            assert!(legible(label, &T), "{label:?} is a number");
        }
        for marks in [".", "..", "|", "/ l |", "1 |", "- -"] {
            assert!(!legible(marks, &T), "{marks:?} is marks");
        }
    }
}
