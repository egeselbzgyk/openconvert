//! The printed table of contents, parsed off the page (PIPELINE §8.1).
//!
//! This is the best-measured approach anywhere in Part B: a TOC-based baseline scored
//! **P_ED ≥ 0.9 (median), the best of all approaches evaluated** in HiPS, beating every
//! font-clustering and every ML method (R2 §B.5). It is worth having even when an outline
//! exists, because the two agreeing is a confidence signal and the two disagreeing is a
//! warning — and when the outline is absent, which is most PDFs, it is the only ground truth
//! the book contains.
//!
//! A leader is required, and that is a known limit rather than an oversight. A contents page
//! whose folios are set flush right with nothing between them and the title reaches this
//! parser as `"Preface i"` — one space, indistinguishable from a two-word title — because the
//! gap is geometry and a line's text is not. Recovering it means measuring the gap between
//! the last two runs of the line, which is a second detector and belongs with the corpus that
//! would say how often it is needed (Phase 7).

use oc_core::thresholds::Thresholds;
use serde::Serialize;

use crate::headings::numbering::is_roman;
use crate::view::BlockView;

/// The characters a leader is drawn with, besides the spaces between them.
const LEADER_FILL: [char; 6] = ['.', '\u{00B7}', '\u{2026}', '_', '-', '\u{2027}'];

/// One line of a printed contents page.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TocEntry {
    pub title: String,
    /// The folio as printed: `"1"`, `"iv"`. A *label*, not an index — front matter restarts
    /// the numbering and the two are not the same thing (`PageRef::label`).
    pub folio: String,
    /// 1-based, from the entry's indent relative to the shallowest entry on the page.
    pub level: u8,
}

/// A page that is a table of contents, and what it says.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TocPage {
    pub page: u32,
    pub entries: Vec<TocEntry>,
    /// The share of the page's lines that parsed as entries. The evidence for calling it a
    /// contents page at all.
    pub line_share: f32,
}

/// Find the printed table of contents, if the book has one in its front matter.
///
/// Searches only the first `toc.max_front_pages` pages: a list of tables in the middle of a
/// reference work parses identically and is not the contents.
pub fn parse_toc_page(blocks: &[BlockView], t: &Thresholds) -> Option<TocPage> {
    let max_page = u32::try_from(t.toc.max_front_pages).unwrap_or(u32::MAX);
    let mut pages: Vec<u32> = blocks
        .iter()
        .map(|block| block.page)
        .filter(|page| *page < max_page)
        .collect();
    pages.dedup();
    pages.sort_unstable();
    pages.dedup();

    pages
        .into_iter()
        .filter_map(|page| parse_one_page(blocks, page, t))
        // The first such page, not the best: a book with a contents and a list of figures has
        // both, and the contents comes first.
        .next()
}

fn parse_one_page(blocks: &[BlockView], page: u32, t: &Thresholds) -> Option<TocPage> {
    let lines: Vec<(&str, f32)> = blocks
        .iter()
        .filter(|block| block.page == page)
        .flat_map(|block| block.lines.iter())
        .map(|line| (line.text.trim(), line.bbox.x0))
        .filter(|(text, _)| !text.is_empty())
        .collect();
    if lines.is_empty() {
        return None;
    }

    let parsed: Vec<(String, String, f32)> = lines
        .iter()
        .filter_map(|(text, x0)| {
            parse_entry_line(text, t).map(|(title, folio)| (title, folio, *x0))
        })
        .collect();

    let line_share = (parsed.len() as f64 / lines.len() as f64) as f32;
    if i64::try_from(parsed.len()).unwrap_or(i64::MAX) < t.toc.min_entries
        || f64::from(line_share) < t.toc.min_line_share
    {
        return None;
    }

    // Levels from the indent ladder. The shallowest entry is level 1; each distinct indent
    // deeper than it is one level further in. Quantised by grouping equal-to-within-a-point
    // indents, because a justified contents page does not set its indents to the micron.
    let mut steps: Vec<f32> = parsed.iter().map(|(_, _, x0)| *x0).collect();
    steps.sort_by(f32::total_cmp);
    steps.dedup_by(|a, b| (*a - *b).abs() < 1.0);

    let entries = parsed
        .into_iter()
        .map(|(title, folio, x0)| TocEntry {
            title,
            folio,
            level: u8::try_from(
                steps
                    .iter()
                    .position(|step| (x0 - step).abs() < 1.0 || x0 < *step)
                    .unwrap_or(0)
                    + 1,
            )
            .unwrap_or(1)
            .clamp(1, 6),
        })
        .collect();

    Some(TocPage {
        page,
        entries,
        line_share,
    })
}

/// `Title . . . . 12` — a title, a leader, and a folio.
///
/// The leader must carry at least `toc.min_leader_chars` *drawn* fill characters, spaces not
/// counted. That is the whole discriminator: `"It happened in 1984"` and `"Preface i"` are
/// not contents lines and differ from one only by the absence of dots.
fn parse_entry_line(text: &str, t: &Thresholds) -> Option<(String, String)> {
    let text = text.trim();
    // The folio is what follows the last space or fill character. Found from the end rather
    // than by splitting on whitespace, because a producer may draw the leader with no spaces
    // in it at all (`Method....12`).
    let (boundary, separator) = text
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace() || LEADER_FILL.contains(c))?;
    let folio = text.get(boundary + separator.len_utf8()..)?.trim();
    if folio.is_empty() || !(folio.chars().all(|c| c.is_ascii_digit()) || is_roman(folio)) {
        return None;
    }

    let head = text.get(..boundary + separator.len_utf8())?;
    let title = head
        .trim_end_matches(|c: char| c.is_whitespace() || LEADER_FILL.contains(&c))
        .trim();
    if title.is_empty() {
        return None;
    }
    let fill = head
        .get(title.len()..)
        .map(|leader| leader.chars().filter(|c| LEADER_FILL.contains(c)).count())
        .unwrap_or_default();
    if i64::try_from(fill).unwrap_or(i64::MAX) < t.toc.min_leader_chars {
        return None;
    }
    Some((title.to_owned(), folio.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> &'static Thresholds {
        &oc_core::thresholds::T
    }

    #[test]
    fn a_dotted_leader_line_parses_into_title_and_folio() {
        assert_eq!(
            parse_entry_line("Chapter One . . . . . . . . 1", t()),
            Some(("Chapter One".to_owned(), "1".to_owned()))
        );
        assert_eq!(
            parse_entry_line("Preface . . . . iv", t()),
            Some(("Preface".to_owned(), "iv".to_owned()))
        );
        assert_eq!(
            parse_entry_line("Method....12", t()),
            Some(("Method".to_owned(), "12".to_owned()))
        );
    }

    /// The three things that are not contents lines, and are the reason the leader is
    /// required: a sentence that ends in a number, a two-word title, and a price.
    #[test]
    fn prose_that_ends_in_a_number_is_not_a_contents_line() {
        assert_eq!(parse_entry_line("It happened in 1984", t()), None);
        assert_eq!(parse_entry_line("Preface i", t()), None);
        assert_eq!(parse_entry_line("Chapter One . 1", t()), None);
        assert_eq!(parse_entry_line(". . . . 12", t()), None);
        assert_eq!(parse_entry_line("Chapter One . . . . end", t()), None);
    }
}
