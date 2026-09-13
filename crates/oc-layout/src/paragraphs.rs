//! Lines back into paragraphs (PIPELINE §7, R2 §B.6).
//!
//! "The single hardest core problem in the taxonomy, and the one where a plausible-looking
//! output can be silently wrong" (R1 §A.2.2). A PDF has no paragraphs in it. It has lines, and
//! the paragraph boundaries have to be inferred from three cues, in this order of reliability:
//!
//! 1. **The short last line.** In a justified book every line but the last of a paragraph is
//!    pinned to the right margin, so a line that falls short of the block's dominant right
//!    edge ends its paragraph. This is the most reliable single cue there is, and it is
//!    parameterised as Calibre's unwrap factor (`paragraph.line_unwrap_factor`).
//! 2. **The first-line indent**, when the book uses one. Which it is — indent or blank line —
//!    is decided once for the whole book by [`infer_convention`], because a book is
//!    internally consistent and a page is not: a chapter opening indents nothing and a page of
//!    dialogue indents everything.
//! 3. **Extra leading**, which separates paragraphs in books that do not indent, and also
//!    appears in books that do.
//!
//! And the fourth thing, which is not a cue but a consequence: paragraphs are **merged across
//! column and page boundaries** when none of the three says the paragraph ended. A paragraph
//! split by a page break and a paragraph split by a column break are different failure modes
//! and both have to be handled (R1 §A.11 #24).

use std::collections::{BTreeMap, BTreeSet};

use oc_core::thresholds::Thresholds;
use oc_model::geom::Rect;
use oc_model::ids::BlockId;
use oc_model::lang::LangTag;
use oc_model::layout::{Block, BlockKindHint, Para, ParagraphConvention};
use oc_model::ledger::{LedgerDelta, LedgerEntry, Reason};
use oc_model::text::Line;

use oc_text::dehyphen::{dehyphenate, Decision, DocLexicon, HyphenAction, LINE_BREAK_HYPHENS};

use crate::blocks::LayoutPage;
use crate::continuity::runs_on;

/// Decide, once for the whole book, how it marks a paragraph start (PIPELINE §7 step 1).
pub fn infer_convention(
    pages: &[LayoutPage],
    blocks: &[Vec<Block>],
    t: &Thresholds,
) -> ParagraphConvention {
    let minimum = t.paragraph.indent_min_em as f32;
    let share = t.paragraph.indent_convention_min_share as f32;

    let mut lines = 0u32;
    let mut indented = 0u32;
    for (page, page_blocks) in pages.iter().zip(blocks) {
        let em = page_em(page);
        for block in page_blocks {
            for line in &block.lines {
                lines += 1;
                if line.indent_pt >= minimum * em {
                    indented += 1;
                }
            }
        }
    }

    if lines > 0 && indented as f32 / lines as f32 >= share {
        ParagraphConvention::FirstLineIndent
    } else {
        ParagraphConvention::BlankLine
    }
}

/// Rebuild the document's paragraphs.
///
/// Blocks are taken in reading order, page by page, and cut into paragraphs; a paragraph that
/// the last cue says has not ended carries on into the next block, whichever column or page
/// that block is on.
pub fn reconstruct_paragraphs(
    pages: &[LayoutPage],
    blocks: &[Vec<Block>],
    convention: ParagraphConvention,
    t: &Thresholds,
) -> Reconstruction {
    let mut paragraphs: Vec<Open> = Vec::new();
    let mut open: Option<Open> = None;

    for (index, page) in pages.iter().enumerate() {
        let em = page_em(page);
        let Some(page_blocks) = blocks.get(index) else {
            continue;
        };
        for block in page_blocks {
            // A block that is not flowing text is not part of a paragraph, and it interrupts
            // whatever was open: a figure between two halves of a sentence is still a figure.
            if block.kind_hint != BlockKindHint::Text {
                if let Some(finished) = open.take() {
                    paragraphs.push(finished);
                }
                continue;
            }

            for (position, line) in block.lines.iter().enumerate() {
                let text = text_of(pages, index, line);
                let starts = starts_paragraph(block, position, line, convention, em, t);
                let continues = match (&open, starts) {
                    (Some(previous), false) => previous.accepts(index, &text),
                    _ => false,
                };
                if !continues {
                    if let Some(finished) = open.take() {
                        paragraphs.push(finished);
                    }
                    open = Some(Open::new(block, line, &text, index, em, t));
                    continue;
                }
                if let Some(current) = open.as_mut() {
                    current.push(block, line, &text, index, t);
                }
            }
        }
    }
    if let Some(finished) = open.take() {
        paragraphs.push(finished);
    }

    let mut minted: BTreeSet<String> = BTreeSet::new();
    let mut built = Reconstruction::default();
    for open in paragraphs {
        let (para, texts, pages) = open.finish(&mut minted);
        built.paragraphs.push(para);
        built.line_texts.push(texts);
        built.line_pages.push(pages);
    }
    built
}

/// The paragraphs, with the lines they were built from kept alongside.
///
/// The line texts are carried rather than re-derived from `Para::text`, because dehyphenation
/// has to know where the line boundaries *were*: a hyphen followed by a space in a joined
/// paragraph is not necessarily a line break, and guessing at them after the fact would put
/// the one decision the conservation law cannot check back on a guess.
#[derive(Clone, Debug, Default)]
pub struct Reconstruction {
    pub paragraphs: Vec<Para>,
    /// Per paragraph, the text of each line it was built from, in order.
    pub line_texts: Vec<Vec<String>>,
    /// Per paragraph, the page each of those lines came from.
    pub line_pages: Vec<Vec<u32>>,
}

/// Resolve every hyphenated line break in the document's paragraphs (PIPELINE §7 step 6).
///
/// This is the only part of `paragraphs` that changes the text, and the only reason the stage
/// is Budgeted rather than Conserving. Each decision is a `Dehyphenate` ledger entry holding
/// exactly the one character that left, which is the ledger's half of invariant I-5; the other
/// half — that the word left behind is the two pieces concatenated and nothing else — is a
/// property of the join itself, and is where `oc_text::dehyphen`'s own property test lives.
///
/// A kept hyphen produces no entry at all, because nothing was removed. That asymmetry is the
/// fail-closed rule showing through: the safe outcome is also the one with nothing to record.
pub fn dehyphenate_paragraphs(
    built: &mut Reconstruction,
    lexicon: &DocLexicon,
    lang: &LangTag,
    stage: &'static str,
) -> LedgerDelta {
    let mut delta = LedgerDelta::default();

    for (index, para) in built.paragraphs.iter_mut().enumerate() {
        let Some(lines) = built.line_texts.get(index) else {
            continue;
        };
        let pages = built.line_pages.get(index);
        let mut text = String::new();
        let mut offset: u32 = 0;

        for (position, line) in lines.iter().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let next = lines
                .get(position + 1)
                .map(|next| next.trim())
                .unwrap_or_default();

            let decision = if next.is_empty() {
                None
            } else {
                Some(dehyphenate(line, next, lexicon, lang))
            };

            match decision.as_ref().map(Decision::resolved) {
                Some(HyphenAction::Join) => {
                    // The hyphen goes, and so does the space that would have joined the two
                    // lines: the word is one word.
                    let kept = line
                        .strip_suffix(LINE_BREAK_HYPHENS)
                        .unwrap_or(line)
                        .to_owned();
                    let removed = line
                        .chars()
                        .next_back()
                        .map(String::from)
                        .unwrap_or_default();
                    let width = u32::try_from(kept.chars().count()).unwrap_or(u32::MAX);
                    delta.push(LedgerEntry::removed(
                        stage,
                        Reason::Dehyphenate,
                        pages
                            .and_then(|pages| pages.get(position).copied())
                            .unwrap_or(para.pages.0),
                        (
                            offset.saturating_add(width),
                            offset.saturating_add(width).saturating_add(1),
                        ),
                        removed,
                    ));
                    text.push_str(&kept);
                    offset = offset.saturating_add(width);
                }
                _ => {
                    text.push_str(line);
                    offset = offset
                        .saturating_add(u32::try_from(line.chars().count()).unwrap_or(u32::MAX));
                    if !next.is_empty() {
                        text.push(' ');
                    }
                }
            }
        }
        para.text = text;
    }

    delta
}

/// One line's text, found on the page it came from.
///
/// Keyed on the run ids: `blocks` re-measures a line's indent against its block, so the copy
/// in the block is deliberately not equal to the copy on the page.
fn text_of(pages: &[LayoutPage], page: usize, line: &Line) -> String {
    pages
        .get(page)
        .and_then(|page| {
            page.lines
                .iter()
                .find(|candidate| candidate.line.runs == line.runs)
        })
        .map(|candidate| candidate.text.clone())
        .unwrap_or_default()
}

/// A paragraph under construction.
struct Open {
    blocks: Vec<BlockId>,
    lines: Vec<Line>,
    texts: Vec<String>,
    /// The page each line came from, so a ledger entry can say where a hyphen was.
    line_pages: Vec<u32>,
    first_line_indent: bool,
    pages: (u32, u32),
    bbox: Rect,
    /// The last line's share of its block's measure, which is what says whether the paragraph
    /// is still open.
    last_fill: f32,
    /// Whether the last line ends on a hyphen, for the dehyphenation that follows.
    last_ends_with_hyphen: bool,
    unwrap_factor: f32,
}

impl Open {
    fn new(block: &Block, line: &Line, text: &str, page: usize, _em: f32, t: &Thresholds) -> Self {
        let page = u32::try_from(page).unwrap_or(u32::MAX);
        Self {
            blocks: vec![block.id],
            lines: vec![line.clone()],
            texts: vec![text.to_owned()],
            line_pages: vec![page],
            first_line_indent: line.indent_pt > 0.0,
            pages: (page, page),
            bbox: line.bbox,
            last_fill: fill_of(block, line),
            last_ends_with_hyphen: line.ends_with_hyphen,
            unwrap_factor: t.paragraph.line_unwrap_factor as f32,
        }
    }

    fn push(&mut self, block: &Block, line: &Line, text: &str, page: usize, _t: &Thresholds) {
        if self.blocks.last() != Some(&block.id) {
            self.blocks.push(block.id);
        }
        self.lines.push(line.clone());
        self.texts.push(text.to_owned());
        self.line_pages
            .push(u32::try_from(page).unwrap_or(u32::MAX));
        self.pages.1 = u32::try_from(page).unwrap_or(u32::MAX);
        self.bbox = union(self.bbox, line.bbox);
        self.last_fill = fill_of(block, line);
        self.last_ends_with_hyphen = line.ends_with_hyphen;
    }

    /// Whether this paragraph takes the next line.
    ///
    /// The short last line is the cue, and the one extra condition is for the case it cannot
    /// see: across a page boundary a paragraph may also be ended by the text itself, which the
    /// continuity proxy reads.
    fn accepts(&self, page: usize, next: &str) -> bool {
        if self.last_fill < self.unwrap_factor {
            return false;
        }
        let crosses_page = u32::try_from(page).unwrap_or(u32::MAX) != self.pages.1;
        if !crosses_page {
            return true;
        }
        self.texts
            .last()
            .is_some_and(|last| runs_on(last, next) || self.last_ends_with_hyphen)
    }

    fn finish(self, minted: &mut BTreeSet<String>) -> (Para, Vec<String>, Vec<u32>) {
        let text = join(&self.texts);
        let base = BlockId::derive(self.pages.0, self.bbox, &text);
        let mut id = base;
        for suffix in 0..32u8 {
            id = base.with_collision_suffix(suffix);
            if minted.insert(id.as_str().to_owned()) {
                break;
            }
        }
        (
            Para {
                id,
                blocks: self.blocks,
                lines: self.lines,
                text,
                first_line_indent: self.first_line_indent,
                pages: self.pages,
            },
            self.texts,
            self.line_pages,
        )
    }
}

/// Join a paragraph's lines with single spaces.
///
/// Hyphenated line breaks are left exactly as they are: whether `pipe-` + `line` is one word
/// is `dehyphen`'s question, it is answered under invariant I-5, and a joiner that guessed
/// here would be making that decision silently and without a ledger entry.
fn join(texts: &[String]) -> String {
    texts
        .iter()
        .map(|text| text.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether a line starts a new paragraph on its own evidence.
fn starts_paragraph(
    block: &Block,
    position: usize,
    line: &Line,
    convention: ParagraphConvention,
    em: f32,
    t: &Thresholds,
) -> bool {
    if convention == ParagraphConvention::FirstLineIndent
        && line.indent_pt >= t.paragraph.indent_min_em as f32 * em
    {
        return true;
    }
    // Extra leading above the line, against the block's own modal leading. Only meaningful
    // inside a block: between blocks the gap is what separated them in the first place.
    if position > 0 {
        let leading = modal_leading(block);
        if leading > 0.0 {
            let previous = &block.lines[position - 1];
            let gap = line.baseline_y - previous.baseline_y;
            if gap > leading * t.paragraph.extra_leading_ratio as f32 {
                return true;
            }
        }
    }
    false
}

/// How much of its block's measure a line fills.
fn fill_of(block: &Block, line: &Line) -> f32 {
    let measure = (block.bbox.x1 - block.bbox.x0).max(1.0);
    ((line.bbox.x1 - block.bbox.x0) / measure).clamp(0.0, 1.0)
}

/// The block's modal baseline-to-baseline distance, to the nearest point.
fn modal_leading(block: &Block) -> f32 {
    let mut counts: BTreeMap<i64, (usize, f32)> = BTreeMap::new();
    for pair in block.lines.windows(2) {
        let gap = pair[1].baseline_y - pair[0].baseline_y;
        if gap <= 0.0 {
            continue;
        }
        let entry = counts.entry(gap.round() as i64).or_insert((0, 0.0));
        entry.0 += 1;
        entry.1 += gap;
    }
    counts
        .iter()
        .max_by_key(|(bucket, (count, _))| (*count, std::cmp::Reverse(**bucket)))
        .map(|(_, (count, sum))| sum / *count as f32)
        .unwrap_or(0.0)
}

/// The page's em: the median height of its line boxes.
fn page_em(page: &LayoutPage) -> f32 {
    crate::columns::median_height(
        &page
            .lines
            .iter()
            .map(|line| line.line.bbox)
            .collect::<Vec<_>>(),
    )
}

fn union(a: Rect, b: Rect) -> Rect {
    Rect {
        x0: a.x0.min(b.x0),
        y0: a.y0.min(b.y0),
        x1: a.x1.max(b.x1),
        y1: a.y1.max(b.y1),
    }
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use oc_core::thresholds::T;
    use oc_model::extract::PageRef;
    use oc_model::text::RunId;

    use crate::blocks::{LayoutLine, Segment};

    /// A page of lines at a 12 pt leading, each with the text it carries and the share of the
    /// measure it fills. `fill` is what a justified line does to the right margin: 1.0 reaches
    /// it, 0.3 is a paragraph's last line.
    struct Builder {
        lines: Vec<LayoutLine>,
        next_run: u32,
    }

    const MEASURE: f32 = 200.0;
    const LEFT: f32 = 50.0;
    const EM: f32 = 10.0;

    impl Builder {
        fn new() -> Self {
            Self {
                lines: Vec::new(),
                next_run: 0,
            }
        }

        fn line(mut self, row: usize, indent: f32, fill: f32, text: &str) -> Self {
            let y0 = 50.0 + row as f32 * 12.0;
            let bbox = Rect {
                x0: LEFT + indent,
                y0,
                x1: LEFT + MEASURE * fill,
                y1: y0 + EM,
            };
            let run = RunId(self.next_run);
            self.next_run += 1;
            self.lines.push(LayoutLine {
                line: Line {
                    runs: vec![run],
                    bbox,
                    baseline_y: y0 + 8.0,
                    ends_with_hyphen: text.trim_end().ends_with('-'),
                    indent_pt: indent,
                    right_gap_pt: MEASURE * (1.0 - fill),
                },
                text: text.to_owned(),
                segments: vec![Segment {
                    run,
                    bbox,
                    text: text.to_owned(),
                }],
            });
            self
        }

        fn page(self, index: u32) -> LayoutPage {
            LayoutPage {
                page: PageRef::new(index),
                width_pt: 400.0,
                height_pt: 600.0,
                lines: self.lines,
            }
        }
    }

    /// Segment the built pages exactly as the stage does, so the paragraphs are built on real
    /// blocks with real block-relative indents.
    fn paragraphs_of(pages: Vec<LayoutPage>) -> (ParagraphConvention, Vec<Para>) {
        let blocks: Vec<Vec<Block>> = pages
            .iter()
            .map(|page| crate::blocks::segment_blocks(page, &T).0)
            .collect();
        let convention = infer_convention(&pages, &blocks, &T);
        (
            convention,
            reconstruct_paragraphs(&pages, &blocks, convention, &T).paragraphs,
        )
    }

    /// A book that indents, with no extra space between paragraphs: the indent is the only
    /// cue there is, and without it the whole page is one paragraph.
    #[test]
    fn an_indent_starts_a_paragraph_when_nothing_else_marks_one() {
        let page = Builder::new()
            .line(0, 12.0, 1.0, "the first paragraph opens")
            .line(1, 0.0, 1.0, "and runs to the margin")
            .line(2, 0.0, 0.3, "and stops.")
            .line(3, 12.0, 1.0, "the second paragraph opens")
            .line(4, 0.0, 0.4, "and stops too.")
            .page(0);
        let (convention, paragraphs) = paragraphs_of(vec![page]);

        assert_eq!(convention, ParagraphConvention::FirstLineIndent);
        assert_eq!(paragraphs.len(), 2, "{paragraphs:#?}");
        assert!(paragraphs[0].text.starts_with("the first paragraph"));
        assert!(paragraphs[1].text.starts_with("the second paragraph"));
        assert!(paragraphs[0].first_line_indent);
    }

    /// The short last line, which is the cue that works in a book that does not indent.
    #[test]
    fn a_short_line_ends_its_paragraph() {
        let page = Builder::new()
            .line(0, 0.0, 1.0, "a paragraph that fills the measure")
            .line(1, 0.0, 0.2, "and then does not.")
            .line(2, 0.0, 1.0, "a second one, also filling it")
            .line(3, 0.0, 0.9, "and very nearly filling it again.")
            .page(0);
        let (convention, paragraphs) = paragraphs_of(vec![page]);

        assert_eq!(convention, ParagraphConvention::BlankLine);
        assert_eq!(paragraphs.len(), 2, "{paragraphs:#?}");
        assert!(paragraphs[1].text.ends_with("nearly filling it again."));
    }

    /// A line that nearly fills the measure is not a paragraph end. This is the cue
    /// over-firing in dialogue-heavy prose, and the factor is what holds it back.
    #[test]
    fn a_nearly_full_line_does_not_end_a_paragraph() {
        let page = Builder::new()
            .line(0, 0.0, 1.0, "one")
            .line(1, 0.0, 0.5, "two")
            .line(2, 0.0, 0.3, "three.")
            .page(0);
        let (_, paragraphs) = paragraphs_of(vec![page]);
        assert_eq!(paragraphs.len(), 1, "{paragraphs:#?}");
        assert_eq!(paragraphs[0].text, "one two three.");
    }

    /// The merge that matters most: a paragraph interrupted by a page break is one paragraph.
    #[test]
    fn a_paragraph_merges_across_a_page_break() {
        let first = Builder::new()
            .line(0, 0.0, 1.0, "the page ends in the middle of a")
            .page(0);
        let second = Builder::new()
            .line(0, 0.0, 0.3, "sentence that finishes here.")
            .page(1);
        let (_, paragraphs) = paragraphs_of(vec![first, second]);

        assert_eq!(paragraphs.len(), 1, "{paragraphs:#?}");
        assert!(paragraphs[0].is_merged());
        assert_eq!(paragraphs[0].pages, (0, 1));
        assert_eq!(
            paragraphs[0].text,
            "the page ends in the middle of a sentence that finishes here."
        );
    }

    /// And the refusal: a page that ends its sentence, followed by a page that starts a new
    /// one, is two paragraphs however full the last line was.
    #[test]
    fn a_finished_sentence_does_not_merge_across_a_page_break() {
        let first = Builder::new()
            .line(0, 0.0, 1.0, "the page ends with a full stop.")
            .page(0);
        let second = Builder::new()
            .line(0, 0.0, 0.3, "A new sentence begins.")
            .page(1);
        let (_, paragraphs) = paragraphs_of(vec![first, second]);

        assert_eq!(paragraphs.len(), 2, "{paragraphs:#?}");
    }

    /// A hyphen at a page break is evidence in its own right: the word is unfinished, so the
    /// paragraph is, whatever the punctuation says.
    #[test]
    fn a_hyphen_at_a_page_break_merges_the_paragraph() {
        let first = Builder::new()
            .line(0, 0.0, 1.0, "the page ends on a broken pipe-")
            .page(0);
        let second = Builder::new()
            .line(0, 0.0, 0.3, "Line that finishes it.")
            .page(1);
        let (_, paragraphs) = paragraphs_of(vec![first, second]);

        assert_eq!(paragraphs.len(), 1, "{paragraphs:#?}");
        assert!(paragraphs[0].is_merged());
    }

    /// The join leaves the hyphen alone: whether `pipe-` + `line` is one word is a decision
    /// with a ledger entry attached, and this is not the stage that makes it.
    #[test]
    fn joining_lines_does_not_resolve_a_hyphen() {
        let page = Builder::new()
            .line(0, 0.0, 1.0, "a broken pipe-")
            .line(1, 0.0, 0.3, "line here.")
            .page(0);
        let (_, paragraphs) = paragraphs_of(vec![page]);
        assert_eq!(paragraphs[0].text, "a broken pipe- line here.");
    }

    /// Two paragraphs of identical text on one page still get different ids.
    ///
    /// They have to be genuinely separate paragraphs to be two: two short lines under each
    /// other are one, because "short" is measured against the block's own measure and a block
    /// of two short lines has a short measure. That is the rule working, not failing — a
    /// paragraph's last line is short *relative to the lines above it*.
    #[test]
    fn every_paragraph_gets_its_own_id() {
        let page = Builder::new()
            .line(0, 0.0, 1.0, "the same words")
            .line(1, 0.0, 0.3, "twice over.")
            .line(4, 0.0, 1.0, "the same words")
            .line(5, 0.0, 0.3, "twice over.")
            .page(0);
        let (_, paragraphs) = paragraphs_of(vec![page]);
        assert_eq!(paragraphs.len(), 2, "{paragraphs:#?}");
        assert_eq!(paragraphs[0].text, paragraphs[1].text);
        assert_ne!(paragraphs[0].id, paragraphs[1].id);
    }
}
