//! Gate V: the region an edit touches does not read worse after it (D13.5, ARCHITECTURE §6.2).
//!
//! RT B13 found the first definition of this gate unusable: "Gopher-style statistics do not
//! worsen" compared a twenty-dimensional vector with no combination rule, and most of those
//! statistics are document-level — datatrove's quality filter wants fifty words and fails any
//! block — so the gate would have either never fired or always fired. The fix is here as
//! ARCHITECTURE §6.2 states it: a **fixed ordered tuple** of statistics that are defined on a
//! region, each with an explicit epsilon, and a rule that is strict dominance —
//!
//! ```text
//! V(region) = ( dup_line_ratio,            ε = llm.gate_v_ratio_eps
//!               top_2gram_ratio,           ε = llm.gate_v_ratio_eps
//!               top_3gram_ratio,           ε = llm.gate_v_ratio_eps
//!               non_alpha_word_ratio,      ε = llm.gate_v_ratio_eps
//!               heading_tree_violations )  ε = llm.gate_v_violations_eps
//!
//! for every component i: V_after[i] ≤ V_before[i] + ε_i, or the edit reverts
//! ```
//!
//! — and a statistic undefined on the region (too few lines or words to have a value) is
//! **skipped, not defaulted**. A default would be an invented number standing in for "no
//! evidence", which is exactly what B13 warned against.
//!
//! **One definition in two places.** The four text statistics are `oc_text::stats`'s Gopher
//! family, restated because `oc-ai` may not depend on `oc-text` (DECISIONS.md Appendix A;
//! `docs/DECISIONS_LOG.md`, 2026-09-22). `gate_v_statistics_are_oc_texts` holds the two equal on
//! the same lines, so the restatement cannot drift. They differ in one way only, and on purpose:
//! where `oc-text` reports `0.0` for a region with nothing to measure, this reports `None`.

use std::collections::{BTreeMap, BTreeSet};

use oc_model::doc::{Content, Section};
use oc_model::document::Document;
use oc_model::ids::BlockId;

use super::GateFailure;

/// The part of a book an edit touches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Region {
    /// The whole flow: what a `heading_roles` or `book_structure` edit touches, and where
    /// `heading_tree_violations` is the load-bearing component.
    Whole,
    /// These blocks and nothing else: a `verse_quote` batch. A content item is in the region when
    /// its own block id is — for a paragraph, any block it was assembled from.
    Blocks(BTreeSet<BlockId>),
}

/// What gate V compares: the fixed tuple, plus the sizes that say why a component is undefined.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RegionStats {
    /// Lines that repeat an earlier line, over all lines. `None` with no lines.
    pub dup_line_ratio: Option<f32>,
    /// The share of characters taken by the most repeated word 2-gram. `None` below two words.
    pub top_2gram_ratio: Option<f32>,
    /// The same for 3-grams. `None` below three words.
    pub top_3gram_ratio: Option<f32>,
    /// Words with no letter in them, over all words. `None` with no words.
    pub non_alpha_word_ratio: Option<f32>,
    /// Level skips (including a first heading below level 1) plus page-order inversions.
    pub heading_tree_violations: u32,
    /// How many lines the region holds. Not compared.
    pub lines: usize,
    /// How many words. Not compared.
    pub words: usize,
}

/// The epsilon of each component, from `llm.gate_v_ratio_eps` and `llm.gate_v_violations_eps`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tolerance {
    pub ratio: f32,
    pub violations: u32,
}

impl RegionStats {
    /// The tuple's components, in the order gate V compares them. Closed: nothing else is compared,
    /// and in particular nothing that is only defined on a whole document.
    pub const STATISTICS: [&'static str; 5] = [
        "dup_line_ratio",
        "top_2gram_ratio",
        "top_3gram_ratio",
        "non_alpha_word_ratio",
        "heading_tree_violations",
    ];

    /// Measure a region of a book.
    ///
    /// A region's lines are what a reader meets: one per paragraph and heading, one per line of
    /// verse or preformatted text. Notes, tables and captions are outside every region — no task
    /// edits them.
    pub fn measure(document: &Document, region: &Region) -> RegionStats {
        let mut lines: Vec<String> = Vec::new();
        let mut headings: Vec<(u8, u32)> = Vec::new();
        for section in &document.sections {
            collect_section(section, region, &mut lines, &mut headings);
        }

        let words: Vec<&str> = lines
            .iter()
            .flat_map(|line| line.split_whitespace())
            .collect();
        RegionStats {
            dup_line_ratio: (!lines.is_empty())
                .then(|| duplicate_fraction(lines.iter().map(String::as_str))),
            top_2gram_ratio: top_ngram_share(&words, 2),
            top_3gram_ratio: top_ngram_share(&words, 3),
            non_alpha_word_ratio: (!words.is_empty()).then(|| {
                ratio(
                    words
                        .iter()
                        .filter(|word| !word.chars().any(char::is_alphabetic))
                        .count(),
                    words.len(),
                )
            }),
            heading_tree_violations: heading_tree_violations(&headings),
            lines: lines.len(),
            words: words.len(),
        }
    }

    /// The ratio components with their names, in tuple order.
    fn ratios(&self) -> [(&'static str, Option<f32>); 4] {
        [
            (Self::STATISTICS[0], self.dup_line_ratio),
            (Self::STATISTICS[1], self.top_2gram_ratio),
            (Self::STATISTICS[2], self.top_3gram_ratio),
            (Self::STATISTICS[3], self.non_alpha_word_ratio),
        ]
    }
}

/// Gate V: every component after is at most its value before plus its epsilon.
///
/// The first component that worsened, in tuple order, is the one named — a fixed order is what
/// makes the failure the same on every run.
pub fn gate_validity(
    before: &RegionStats,
    after: &RegionStats,
    tolerance: &Tolerance,
) -> Result<(), GateFailure> {
    for ((statistic, then), (_, now)) in before.ratios().into_iter().zip(after.ratios()) {
        if let (Some(then), Some(now)) = (then, now) {
            if now > then + tolerance.ratio {
                return Err(GateFailure::Worsened {
                    statistic,
                    before: then,
                    after: now,
                });
            }
        }
    }
    if after.heading_tree_violations
        > before
            .heading_tree_violations
            .saturating_add(tolerance.violations)
    {
        return Err(GateFailure::Worsened {
            statistic: RegionStats::STATISTICS[4],
            before: before.heading_tree_violations as f32,
            after: after.heading_tree_violations as f32,
        });
    }
    Ok(())
}

fn collect_section(
    section: &Section,
    region: &Region,
    lines: &mut Vec<String>,
    headings: &mut Vec<(u8, u32)>,
) {
    let page = section.source_pages.0;
    if let Some(heading) = &section.heading {
        if in_region(region, &[heading.id]) {
            push_lines(&heading.text(), lines);
            headings.push((heading.level, page));
        }
    }
    collect_content(&section.content, region, page, lines, headings);
    for child in &section.children {
        collect_section(child, region, lines, headings);
    }
}

fn collect_content(
    content: &[Content],
    region: &Region,
    page: u32,
    lines: &mut Vec<String>,
    headings: &mut Vec<(u8, u32)>,
) {
    for item in content {
        match item {
            Content::Paragraph(para) => {
                let mut ids = vec![para.id];
                ids.extend(para.blocks.iter().copied());
                if in_region(region, &ids) {
                    push_lines(&para.text, lines);
                }
            }
            Content::Heading(heading) => {
                if in_region(region, &[heading.id]) {
                    push_lines(&heading.text(), lines);
                    headings.push((heading.level, page));
                }
            }
            Content::Verse(verse) => {
                if in_region(region, &[verse.id]) {
                    for line in verse.stanzas.iter().flatten() {
                        push_lines(&oc_model::doc::spans_text(line), lines);
                    }
                }
            }
            Content::Preformatted(pre) => {
                if in_region(region, &[pre.id]) {
                    for line in &pre.lines {
                        push_lines(line, lines);
                    }
                }
            }
            Content::List(list) => collect_list(list, region, page, lines, headings),
            Content::BlockQuote(inner) | Content::Epigraph(inner) => {
                collect_content(inner, region, page, lines, headings);
            }
            Content::Figure(_)
            | Content::Table(_)
            | Content::NoteRefAnchor(_)
            | Content::PageBreak(_)
            | Content::Rule => {}
        }
    }
}

fn collect_list(
    list: &oc_model::doc::List,
    region: &Region,
    page: u32,
    lines: &mut Vec<String>,
    headings: &mut Vec<(u8, u32)>,
) {
    for item in &list.items {
        collect_content(&item.content, region, page, lines, headings);
        if let Some(nested) = &item.nested {
            collect_list(nested, region, page, lines, headings);
        }
    }
}

fn in_region(region: &Region, ids: &[BlockId]) -> bool {
    match region {
        Region::Whole => true,
        Region::Blocks(blocks) => ids.iter().any(|id| blocks.contains(id)),
    }
}

/// A piece of text as lines: split where it breaks, trimmed, empty lines dropped — `oc-text`'s
/// definition of a line.
fn push_lines(text: &str, lines: &mut Vec<String>) {
    lines.extend(
        text.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned),
    );
}

/// Level skips, a first heading below level 1, and headings whose page precedes the previous one.
fn heading_tree_violations(headings: &[(u8, u32)]) -> u32 {
    let root_skip = headings.first().is_some_and(|(level, _)| *level > 1);
    let pairwise = headings.windows(2).filter(|pair| {
        let ((from_level, from_page), (to_level, to_page)) = (pair[0], pair[1]);
        to_level > from_level.saturating_add(1) || to_page < from_page
    });
    u32::try_from(pairwise.count() + usize::from(root_skip)).unwrap_or(u32::MAX)
}

/// The share of items that repeat an earlier one: an item seen `k` times contributes `k − 1`.
fn duplicate_fraction<'a>(items: impl Iterator<Item = &'a str>) -> f32 {
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    let mut total = 0usize;
    for item in items {
        *seen.entry(item).or_default() += 1;
        total += 1;
    }
    let duplicates: usize = seen.values().map(|count| count.saturating_sub(1)).sum();
    ratio(duplicates, total)
}

/// The share of the words' characters taken by the most frequent repeated word `n`-gram, or
/// `None` when there are fewer than `n` words and so no `n`-gram at all.
fn top_ngram_share(words: &[&str], n: usize) -> Option<f32> {
    if words.len() < n {
        return None;
    }
    let total_chars: usize = words.iter().map(|word| word.chars().count()).sum();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for window in words.windows(n) {
        *counts.entry(window.join(" ")).or_default() += 1;
    }
    Some(
        counts
            .into_iter()
            .filter(|(_, count)| *count > 1)
            .map(|(gram, count)| {
                let chars = gram.chars().filter(|ch| !ch.is_whitespace()).count();
                ratio(chars * count, total_chars)
            })
            .fold(0.0f32, f32::max),
    )
}

fn ratio(part: usize, whole: usize) -> f32 {
    if whole == 0 {
        return 0.0;
    }
    part as f32 / whole as f32
}
