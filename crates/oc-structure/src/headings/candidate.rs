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
pub fn legible(text: &str, t: &Thresholds) -> bool {
    let visible: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
    if visible.is_empty() {
        return false;
    }
    let alnum = visible.iter().filter(|c| c.is_alphanumeric()).count();
    let min = usize::try_from(t.headings.legible_min_chars.max(1)).unwrap_or(usize::MAX);
    let all_digits = visible.iter().all(|c| c.is_ascii_digit() || *c == '.');
    (alnum >= min || (all_digits && alnum > 0))
        && alnum as f64 >= t.headings.legible_min_share * visible.len() as f64
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
