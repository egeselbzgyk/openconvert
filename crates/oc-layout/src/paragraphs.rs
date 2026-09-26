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
use oc_model::text::{Line, RunId};

use oc_text::dehyphen::{dehyphenate, DocLexicon, HyphenAction, LINE_BREAK_HYPHENS};

use crate::blocks::{LayoutLine, LayoutPage};
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

/// Where a laid-out document's paragraphs begin and end — PIPELINE §7, recorded as decisions
/// on the blocks `layout` built.
///
/// `structure` reads blocks, and it has to: a heading, a list, a note and a stanza are decided
/// on the block `layout` put together. But a block of running text is not a paragraph. A page
/// of a novel is one block holding a dozen paragraphs, and the paragraph that runs off its foot
/// carries on at the top of the next page. So this stage decides where every paragraph of
/// running text starts and which block carries on which, and `structure` cuts the blocks it
/// emits as running text at exactly these places and joins exactly these pairs — one decision,
/// made here and read there, rather than a second predicate in `structure` standing in for it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParagraphPlan {
    /// Per block, the positions of the lines, after its first, that open a new paragraph.
    pub starts: BTreeMap<BlockId, Vec<usize>>,
    /// Per block, the block whose last paragraph its first line carries on — across a page or
    /// a column boundary, never within one column of one page.
    pub continues: BTreeMap<BlockId, BlockId>,
    /// Per block, the positions of the lines whose line-break hyphen was resolved by a join:
    /// the line runs on into the next without a space. The hyphen itself is already gone from
    /// the line's text, under a `Dehyphenate` entry.
    pub glued: BTreeMap<BlockId, BTreeSet<usize>>,
    /// Line-break hyphens joined, and kept.
    pub hyphens_joined: u32,
    pub hyphens_kept: u32,
}

impl ParagraphPlan {
    /// Whether a block's line opens a paragraph of its own.
    pub fn starts_at(&self, block: BlockId, position: usize) -> bool {
        self.starts
            .get(&block)
            .is_some_and(|starts| starts.binary_search(&position).is_ok())
    }

    /// Whether a block's line runs on into the next line without a space.
    pub fn glued_at(&self, block: BlockId, position: usize) -> bool {
        self.glued
            .get(&block)
            .is_some_and(|glued| glued.contains(&position))
    }
}

/// One line of a text block, as the plan reasons about it.
struct PlanLine {
    /// Where the line lives in its page's `lines`, which is where its text is.
    at: Option<usize>,
    text: String,
    size_pt: f32,
    /// How wide the line's first word is set, estimated from its first run's advance: what a
    /// line before it would have had to leave free for the word to have fitted there.
    first_word_pt: f32,
}

/// One text block, in document order.
struct PlanBlock<'a> {
    page: usize,
    block: &'a Block,
    lines: Vec<PlanLine>,
    em: f32,
}

impl PlanBlock<'_> {
    /// Whether the block's last paragraph is still open when the block ends: its last line
    /// reaches the measure, or ends on a hyphen.
    fn ends_open(&self, t: &Thresholds) -> bool {
        let (Some(line), Some(last)) = (self.block.lines.last(), self.lines.last()) else {
            return false;
        };
        fill_of(self.block, line) >= t.paragraph.line_unwrap_factor as f32
            || ends_with_break_hyphen(&last.text)
    }
}

/// Decide the document's paragraphs, and resolve its line-break hyphens (PIPELINE §7).
///
/// The texts in `pages` are changed in exactly one way: a line-break hyphen the tiers decided
/// to join is removed from the end of its line, and each removal is one `Dehyphenate` entry in
/// the delta — I-5, entry by entry. Everything else is recorded in the plan and changes no text.
pub fn plan_paragraphs(
    pages: &mut [LayoutPage],
    blocks: &[Vec<Block>],
    convention: ParagraphConvention,
    lexicon: &DocLexicon,
    lang: &LangTag,
    stage: &'static str,
    t: &Thresholds,
) -> (ParagraphPlan, LedgerDelta) {
    let mut plan = decide(pages, blocks, convention, t);
    let delta = resolve_hyphens(pages, blocks, &mut plan, lexicon, lang, stage, t);
    (plan, delta)
}

/// The paragraph boundaries and the carry-overs, without touching a hyphen.
fn decide(
    pages: &[LayoutPage],
    blocks: &[Vec<Block>],
    convention: ParagraphConvention,
    t: &Thresholds,
) -> ParagraphPlan {
    let unwrap = t.paragraph.line_unwrap_factor as f32;
    let barrier = t.layout.block.size_barrier_ratio as f32;
    let indent_min = t.paragraph.indent_min_em as f32;
    let order = text_blocks(pages, blocks);
    let mut plan = ParagraphPlan::default();

    for entry in &order {
        let block = entry.block;
        let mut starts = Vec::new();
        for position in 1..block.lines.len() {
            let line = &block.lines[position];
            let previous = &block.lines[position - 1];
            let opens = starts_paragraph(block, position, line, convention, entry.em, t)
                || fill_of(block, previous) < unwrap
                || differs(
                    entry.lines[position - 1].size_pt,
                    entry.lines[position].size_pt,
                    barrier,
                );
            if opens {
                starts.push(position);
            }
        }
        if !starts.is_empty() {
            plan.starts.insert(block.id, starts);
        }
    }

    for (index, first) in order.iter().enumerate() {
        if !first.ends_open(t) {
            continue;
        }
        let Some(next) = carry_over_candidate(&order, index, barrier) else {
            continue;
        };
        let crosses = next.page != first.page || next.block.column != first.block.column;
        if !crosses {
            continue;
        }
        let (Some(last), Some(head), Some(head_line)) = (
            first.lines.last(),
            next.lines.first(),
            next.block.lines.first(),
        ) else {
            continue;
        };
        let indented = next.block.lines.len() > 1 && head_line.indent_pt >= indent_min * next.em;
        // The first word of the next page would have fitted at the end of this page's last line:
        // the typesetter broke the line there because the paragraph ended, not because the line
        // was full (Tesseract's paragraph finder, `FirstWordWouldHaveFit`). A justified line
        // that continues leaves at most a word space free.
        let last_line = first.block.lines.last();
        let space = last.size_pt.max(head.size_pt) * t.paragraph.word_space_em as f32;
        let would_have_fit = last_line.is_some_and(|line| {
            head.first_word_pt > 0.0 && line.right_gap_pt > head.first_word_pt + space
        });
        // A book that indents every paragraph says so on the next page too: a first line set
        // full out, reaching the measure, is the paragraph above still going — even when the
        // page happened to end on a full stop, which about one page in eight does, and the next
        // sentence opens with a capital. Only for such a book, and only for a line that is
        // running text by its shape: a chapter number or a scene break is short, and a heading
        // is a block of another size that is never a candidate.
        let full_out = convention == ParagraphConvention::FirstLineIndent
            && next.block.lines.len() > 1
            && fill_of(next.block, head_line) >= unwrap
            && head
                .text
                .trim_start()
                .chars()
                .next()
                .is_some_and(|ch| !ch.is_numeric());
        let hyphen = ends_with_break_hyphen(&last.text);
        let carries = !indented
            && (hyphen || !would_have_fit)
            && (hyphen || runs_on(&last.text, &head.text) || full_out);
        if carries {
            plan.continues.insert(next.block.id, first.block.id);
        }
    }
    plan
}

/// The block a paragraph that is still open at the end of `order[index]` could carry on into.
///
/// The next text block in reading order — except that a block on the same page set at a
/// materially different size is passed over: a footnote sits between the foot of the text and
/// the top of the next page in reading order, and a paragraph does not end because one did.
/// On the next page there is no passing over: the first text block there is the candidate or
/// nothing is, because a chapter heading is exactly a block of a different size and a
/// paragraph must never be carried past one.
fn carry_over_candidate<'o, 'a>(
    order: &'o [PlanBlock<'a>],
    index: usize,
    barrier: f32,
) -> Option<&'o PlanBlock<'a>> {
    let first = order.get(index)?;
    let size = first.lines.last()?.size_pt;
    for next in order.get(index + 1..)? {
        if next.page > first.page + 1 {
            return None;
        }
        let head = next.lines.first()?.size_pt;
        if !differs(size, head, barrier) {
            return Some(next);
        }
        if next.page != first.page {
            return None;
        }
    }
    None
}

/// Resolve the line-break hyphens inside every paragraph the plan describes, and across every
/// carry-over.
fn resolve_hyphens(
    pages: &mut [LayoutPage],
    blocks: &[Vec<Block>],
    plan: &mut ParagraphPlan,
    lexicon: &DocLexicon,
    lang: &LangTag,
    stage: &'static str,
    t: &Thresholds,
) -> LedgerDelta {
    // Decided on the text as it stands, then applied: a decision must not read a line an
    // earlier decision already changed.
    let mut joins: Vec<(usize, usize, BlockId, usize)> = Vec::new();
    {
        let order = text_blocks(pages, blocks);
        let by_id: BTreeMap<BlockId, usize> = order
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.block.id, index))
            .collect();
        // The block each block's last paragraph carries on into, which is the reverse of
        // `continues`.
        let carried_into: BTreeMap<BlockId, BlockId> = plan
            .continues
            .iter()
            .map(|(next, first)| (*first, *next))
            .collect();

        for entry in &order {
            let id = entry.block.id;
            for (position, line) in entry.lines.iter().enumerate() {
                if !ends_with_break_hyphen(&line.text) {
                    continue;
                }
                let next_text = if position + 1 < entry.lines.len() {
                    if plan.starts_at(id, position + 1) {
                        None
                    } else {
                        Some(entry.lines[position + 1].text.as_str())
                    }
                } else {
                    carried_into
                        .get(&id)
                        .and_then(|next| by_id.get(next))
                        .and_then(|&next| order[next].lines.first())
                        .map(|head| head.text.as_str())
                };
                let Some(next_text) = next_text else {
                    continue;
                };
                let decision = dehyphenate(line.text.trim(), next_text.trim(), lexicon, lang, t);
                match decision.resolved() {
                    HyphenAction::Join => {
                        if let Some(at) = line.at {
                            joins.push((entry.page, at, id, position));
                        }
                    }
                    _ => plan.hyphens_kept += 1,
                }
            }
        }
    }

    let mut delta = LedgerDelta::default();
    for (page, at, block, position) in joins {
        let Some(line) = pages.get_mut(page).and_then(|page| page.lines.get_mut(at)) else {
            continue;
        };
        let trimmed = line.text.trim_end();
        let Some(hyphen) = trimmed.chars().next_back() else {
            continue;
        };
        if !LINE_BREAK_HYPHENS.contains(&hyphen) {
            continue;
        }
        let kept = trimmed[..trimmed.len() - hyphen.len_utf8()].to_owned();
        let offset = u32::try_from(kept.chars().count()).unwrap_or(u32::MAX);
        delta.push(LedgerEntry::removed(
            stage,
            Reason::Dehyphenate,
            u32::try_from(page).unwrap_or(u32::MAX),
            (offset, offset.saturating_add(1)),
            hyphen.to_string(),
        ));
        line.text = kept;
        plan.glued.entry(block).or_default().insert(position);
        plan.hyphens_joined += 1;
    }
    delta
}

/// The document's text blocks in reading order, with each line's text and size looked up
/// once.
fn text_blocks<'a>(pages: &[LayoutPage], blocks: &'a [Vec<Block>]) -> Vec<PlanBlock<'a>> {
    let mut order = Vec::new();
    for (index, page) in pages.iter().enumerate() {
        let Some(page_blocks) = blocks.get(index) else {
            continue;
        };
        // Where each line lives, keyed on its runs: a block's copy of a line is re-measured
        // against the block and is deliberately not equal to the page's copy.
        let at: BTreeMap<&[RunId], usize> = page
            .lines
            .iter()
            .enumerate()
            .map(|(position, line)| (line.line.runs.as_slice(), position))
            .collect();
        let em = page_em(page);
        for block in page_blocks {
            if block.kind_hint != BlockKindHint::Text || block.lines.is_empty() {
                continue;
            }
            let lines = block
                .lines
                .iter()
                .map(|line| {
                    let found = at.get(line.runs.as_slice()).copied();
                    let source = found.and_then(|position| page.lines.get(position));
                    PlanLine {
                        at: found,
                        text: source.map(|line| line.text.clone()).unwrap_or_default(),
                        size_pt: source.map_or(0.0, LayoutLine::size_pt),
                        first_word_pt: source.map_or(0.0, first_word_width),
                    }
                })
                .collect();
            order.push(PlanBlock {
                page: index,
                block,
                lines,
                em,
            });
        }
    }
    order
}

/// The width of a line's first word, from the advance of the run it starts: the run's width over
/// its characters, times the word's. Zero when the line has no run to measure.
fn first_word_width(line: &LayoutLine) -> f32 {
    let Some(segment) = line
        .segments
        .iter()
        .find(|segment| !segment.text.trim().is_empty())
    else {
        return 0.0;
    };
    let chars = segment.text.chars().count().max(1) as f32;
    let advance = (segment.bbox.x1 - segment.bbox.x0).max(0.0) / chars;
    let word = segment
        .text
        .split_whitespace()
        .next()
        .map_or(0, |word| word.chars().count()) as f32;
    advance * word
}

/// Whether two sizes are far enough apart to be two different things, by the same barrier
/// `layout` splits blocks with. A size of zero is no evidence either way.
fn differs(a: f32, b: f32, barrier: f32) -> bool {
    let largest = a.max(b);
    a > 0.0 && b > 0.0 && (a - b).abs() / largest > barrier
}

fn ends_with_break_hyphen(text: &str) -> bool {
    text.trim_end()
        .chars()
        .next_back()
        .is_some_and(|ch| LINE_BREAK_HYPHENS.contains(&ch))
}

/// Rebuild the document's paragraphs from a plan: each text block cut where the plan says a
/// paragraph starts, and each carry-over joined to the paragraph it continues.
///
/// This is the plan read back as paragraphs — what `structure` does to the blocks it emits as
/// running text, done here over every text block so that the stage's own tests, and the
/// report, see the decisions as a reader would.
pub fn paragraphs_of_plan(
    pages: &[LayoutPage],
    blocks: &[Vec<Block>],
    plan: &ParagraphPlan,
) -> Vec<Para> {
    let order = text_blocks(pages, blocks);
    let mut minted: BTreeSet<String> = BTreeSet::new();
    let mut built: Vec<(Para, bool)> = Vec::new();
    // The paragraph each block's last piece ended up in, for the carry-overs to find.
    let mut last_piece: BTreeMap<BlockId, usize> = BTreeMap::new();

    for entry in &order {
        let block = entry.block;
        let page = u32::try_from(entry.page).unwrap_or(u32::MAX);
        let mut pieces: Vec<(Vec<usize>, bool)> = Vec::new();
        for position in 0..block.lines.len() {
            if position == 0 || plan.starts_at(block.id, position) {
                pieces.push((Vec::new(), position == 0));
            }
            if let Some(piece) = pieces.last_mut() {
                piece.0.push(position);
            }
        }
        for (positions, opens_block) in pieces {
            let mut text = String::new();
            for &position in &positions {
                let piece = entry.lines[position].text.trim();
                text.push_str(piece);
                if !plan.glued_at(block.id, position) {
                    text.push(' ');
                }
            }
            let text = text.trim_end().to_owned();
            let lines: Vec<Line> = positions
                .iter()
                .map(|&position| block.lines[position].clone())
                .collect();
            let glued_end = positions
                .last()
                .is_some_and(|&position| plan.glued_at(block.id, position));

            let carried = opens_block
                .then(|| plan.continues.get(&block.id))
                .flatten()
                .and_then(|first| last_piece.get(first).copied());
            if let Some(into) = carried {
                let (para, glued) = &mut built[into];
                if !*glued {
                    para.text.push(' ');
                }
                para.text.push_str(&text);
                para.blocks.push(block.id);
                para.lines.extend(lines);
                para.pages.1 = page;
                *glued = glued_end;
                last_piece.insert(block.id, into);
                continue;
            }

            let bbox = lines
                .iter()
                .map(|line| line.bbox)
                .reduce(union)
                .unwrap_or(block.bbox);
            let base = BlockId::derive(page, bbox, &text);
            let mut id = base;
            for suffix in 0..32u8 {
                id = base.with_collision_suffix(suffix);
                if minted.insert(id.as_str().to_owned()) {
                    break;
                }
            }
            built.push((
                Para {
                    id,
                    blocks: vec![block.id],
                    first_line_indent: lines.first().is_some_and(|line| line.indent_pt > 0.0),
                    lines,
                    text,
                    pages: (page, page),
                    // `structure`'s half of the type, empty until it has run (IR_SKETCH).
                    spans: Vec::new(),
                    drop_cap: false,
                    align: oc_model::doc::Align::Left,
                    lang: None,
                    confidence: None,
                },
                glued_end,
            ));
            last_piece.insert(block.id, built.len() - 1);
        }
    }
    built.into_iter().map(|(para, _)| para).collect()
}

/// Rebuild the document's paragraphs, without resolving a hyphen.
///
/// The plan, read back as paragraphs: blocks taken in reading order, cut at every paragraph
/// start, and a paragraph that the last cue says has not ended carried into the block that
/// continues it, whichever column or page that block is on.
pub fn reconstruct_paragraphs(
    pages: &[LayoutPage],
    blocks: &[Vec<Block>],
    convention: ParagraphConvention,
    t: &Thresholds,
) -> Reconstruction {
    let plan = decide(pages, blocks, convention, t);
    Reconstruction {
        paragraphs: paragraphs_of_plan(pages, blocks, &plan),
        plan,
    }
}

/// The paragraphs, with the plan they were read from.
#[derive(Clone, Debug, Default)]
pub struct Reconstruction {
    pub paragraphs: Vec<Para>,
    pub plan: ParagraphPlan,
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
    use crate::blocks::TEST_SIZE_PT;
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
                    size_pt: TEST_SIZE_PT,
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
            .map(|page| {
                crate::blocks::segment_blocks(
                    page,
                    &crate::columns::ColumnLayout::single(0.0, 400.0),
                    &T,
                )
                .0
            })
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
