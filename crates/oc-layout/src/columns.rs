//! Column detection: where the gutters are, and which column each block sits in
//! (PIPELINE §6 step 2, R2 §B.2).
//!
//! A gutter is a valley in the page's x-projection. Three things have to be true of it, and
//! the third is the one that makes the method work on real books:
//!
//! 1. it is at least `layout.columns.gutter_min_width_em` wide;
//! 2. it is at least `layout.columns.gutter_emptiness_min` free of ink;
//! 3. it is that free over a contiguous vertical span covering at least
//!    `layout.columns.gutter_height_share_min` of the text height.
//!
//! The third is not the whole page height on purpose. A full-width floating title crosses
//! the gutter near the top of the page, and a criterion measured over the whole height would
//! lose the gutter to it — which is exactly the configuration `reading_order`'s pre-masking
//! exists for, and losing the gutter first would make the pre-mask unreachable.
//!
//! The gutter score — width × emptiness — is the confidence signal this stage contributes,
//! and a page whose best valley is shallow or interrupted falls back to one column with a
//! warning rather than guessing (PIPELINE §6, failure modes).

use std::collections::BTreeMap;

use oc_core::thresholds::Thresholds;
use oc_model::geom::Rect;
use oc_model::layout::{Block, BlockKindHint};

use crate::blocks::{LayoutLine, Segment};

/// The resolution of the projection, in points. One point is finer than any gutter this is
/// asked about and coarse enough that a 600 pt page is 600 bins.
const BIN_PT: f32 = 1.0;

/// One valley between two columns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gutter {
    pub x0: f32,
    pub x1: f32,
    /// The fraction of the gutter free of ink over the span that qualified it.
    pub emptiness: f32,
    /// The fraction of the text height that span covers.
    pub height_share: f32,
    /// Width × emptiness — the confidence signal (PIPELINE §6).
    pub score: f32,
}

impl Gutter {
    pub fn width(&self) -> f32 {
        (self.x1 - self.x0).max(0.0)
    }
}

/// How a page divides into columns.
#[derive(Clone, Debug, PartialEq)]
pub struct ColumnLayout {
    /// The x-range of each column, left to right. Always at least one.
    pub columns: Vec<(f32, f32)>,
    pub gutters: Vec<Gutter>,
}

impl ColumnLayout {
    /// The single-column answer, which is also the fallback.
    pub fn single(x0: f32, x1: f32) -> Self {
        Self {
            columns: vec![(x0, x1)],
            gutters: Vec::new(),
        }
    }

    pub fn count(&self) -> usize {
        self.columns.len()
    }

    /// Which column a block belongs to: the one holding the most of it.
    ///
    /// "The most of it" rather than "the one holding its centre", because a floating title
    /// spans several and its centre lands in whichever column the page's arithmetic puts it
    /// in — an answer with no meaning. The span is what matters, and [`spans_columns`] is
    /// what asks about it.
    pub fn column_of(&self, bbox: Rect) -> u8 {
        let mut best = (0usize, f32::NEG_INFINITY);
        for (index, (x0, x1)) in self.columns.iter().enumerate() {
            let overlap = (bbox.x1.min(*x1) - bbox.x0.max(*x0)).max(0.0);
            if overlap > best.1 {
                best = (index, overlap);
            }
        }
        u8::try_from(best.0).unwrap_or(u8::MAX)
    }

    /// Whether a block reaches meaningfully into more than one column — a floating title, a
    /// full-width table, a rule. These are what `reading_order` pre-masks.
    pub fn spans_columns(&self, bbox: Rect) -> bool {
        if self.columns.len() < 2 {
            return false;
        }
        self.columns
            .iter()
            .filter(|(x0, x1)| {
                let overlap = (bbox.x1.min(*x1) - bbox.x0.max(*x0)).max(0.0);
                let column_width = (x1 - x0).max(1.0);
                overlap / column_width >= MIN_COLUMN_SHARE_TO_COUNT
            })
            .count()
            > 1
    }
}

/// How much of a column a block must cover before it counts as being *in* that column for
/// the purpose of "does this block span columns".
///
/// A justified line in the left column can overhang the gutter by a hair; a title crossing
/// the gutter covers most of both. A quarter separates those two without needing to know
/// anything else about the page.
const MIN_COLUMN_SHARE_TO_COUNT: f32 = 0.25;

/// Find the page's columns from the x-projection of its ink.
///
/// `boxes` are run boxes, not line boxes: a line may span a gutter — that is the whole
/// problem — while a run may not, because `words` breaks one at any gap wider than
/// `text.line_split_gap_em`. `em` is the page's em, from the median line height. `limit` caps
/// how many columns may be returned, which is how the cross-page continuity check re-runs the
/// page with one column fewer (PIPELINE §6 step 4); pass `usize::MAX` for no cap.
pub fn detect_columns(boxes: &[Rect], em: f32, limit: usize, t: &Thresholds) -> ColumnLayout {
    let Some(area) = text_area(boxes) else {
        return ColumnLayout::single(0.0, 0.0);
    };
    if limit <= 1 {
        return ColumnLayout::single(area.x0, area.x1);
    }
    let min_width = em * t.layout.columns.gutter_min_width_em as f32;
    let min_emptiness = t.layout.columns.gutter_emptiness_min as f32;
    let min_height_share = t.layout.columns.gutter_height_share_min as f32;
    let text_height = (area.y1 - area.y0).max(1.0);

    // Every one-point strip of the text area, scored by the tallest contiguous span over
    // which it is free of ink.
    let bins = ((area.x1 - area.x0) / BIN_PT).ceil().max(0.0) as usize;
    let mut qualifies = vec![false; bins];
    let mut share = vec![0.0f32; bins];
    for (index, flag) in qualifies.iter_mut().enumerate() {
        let strip = Rect {
            x0: area.x0 + index as f32 * BIN_PT,
            y0: area.y0,
            x1: area.x0 + (index + 1) as f32 * BIN_PT,
            y1: area.y1,
        };
        let (from, span) = free_window(strip, boxes, min_emptiness);
        share[index] = span / text_height;
        // Tall and empty is not enough, and this is the condition that separates a gutter
        // from the rest of the page's whitespace. The blank lower half of a short column is
        // tall and empty; so is the outer margin. What makes a valley a *gutter* is text on
        // both sides of it at the same heights, which neither of those has.
        *flag = share[index] >= min_height_share && flanked(strip, from, from + span, boxes);
    }

    let mut gutters = Vec::new();
    let mut start: Option<usize> = None;
    for index in 0..=bins {
        let inside = index < bins && qualifies[index];
        match (inside, start) {
            (true, None) => start = Some(index),
            (false, Some(from)) => {
                let x0 = area.x0 + from as f32 * BIN_PT;
                let x1 = area.x0 + index as f32 * BIN_PT;
                let width = x1 - x0;
                if width >= min_width {
                    let height_share = share[from..index].iter().copied().fold(0.0, f32::max);
                    let emptiness = emptiness_of(
                        Rect {
                            x0,
                            y0: area.y0,
                            x1,
                            y1: area.y1,
                        },
                        boxes,
                    );
                    gutters.push(Gutter {
                        x0,
                        x1,
                        emptiness,
                        height_share,
                        score: width * emptiness,
                    });
                }
                start = None;
            }
            _ => {}
        }
    }

    if gutters.is_empty() {
        return ColumnLayout::single(area.x0, area.x1);
    }

    // Under a cap, the strongest valleys survive: a gutter's score is its width times its
    // emptiness, so the one that goes is the shallowest or the narrowest.
    if gutters.len() + 1 > limit {
        gutters.sort_by(|a, b| b.score.total_cmp(&a.score));
        gutters.truncate(limit.saturating_sub(1));
        gutters.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    }

    let mut columns = Vec::with_capacity(gutters.len() + 1);
    let mut left = area.x0;
    for gutter in &gutters {
        columns.push((left, gutter.x0));
        left = gutter.x1;
    }
    columns.push((left, area.x1));
    ColumnLayout { columns, gutters }
}

/// Write the column layout onto the blocks: the column index, and the hint that says a block
/// crosses columns and must be pre-masked before the page is cut.
pub fn assign_columns(blocks: &mut [Block], columns: &ColumnLayout) {
    for block in blocks.iter_mut() {
        block.column = columns.column_of(block.bbox);
        if columns.spans_columns(block.bbox) && block.kind_hint == BlockKindHint::Text {
            block.kind_hint = BlockKindHint::FloatingTitle;
        }
    }
}

/// Whether there is text on both sides of a strip, within the vertical window that made it a
/// candidate.
fn flanked(strip: Rect, from: f32, to: f32, boxes: &[Rect]) -> bool {
    let overlaps = |b: &Rect| b.y1 > from && b.y0 < to;
    let left = boxes
        .iter()
        .any(|b| b.x1 <= strip.x0 && b.x0 < strip.x0 && overlaps(b));
    let right = boxes
        .iter()
        .any(|b| b.x0 >= strip.x1 && b.x1 > strip.x1 && overlaps(b));
    left && right
}

/// Split any line that spans a gutter into one line per column.
///
/// This is the repair PROGRESS.md carried forward from Phase 2: `text` clusters a line by
/// baseline alone, so two columns printed at the same height arrive as one line with one
/// bounding box, one indent and one right gap across both of them. Every paragraph rule that
/// reads those fields would be reading a number about two columns at once.
///
/// It happens *here*, after the columns are known, and not in `text`, because the split is
/// exactly as good as the column hypothesis is. When the continuity check disbelieves that
/// hypothesis and re-runs the page with one column fewer, these lines are not split — and
/// that is what makes the re-run mean something rather than being the same answer computed
/// twice.
pub fn split_lines_at_gutters(lines: &[LayoutLine], columns: &ColumnLayout) -> Vec<LayoutLine> {
    if columns.count() < 2 {
        return lines.to_vec();
    }
    let mut split = Vec::with_capacity(lines.len());
    for line in lines {
        if line.segments.len() < 2 {
            split.push(line.clone());
            continue;
        }
        let mut by_column: BTreeMap<u8, Vec<Segment>> = BTreeMap::new();
        for segment in &line.segments {
            by_column
                .entry(columns.column_of(segment.bbox))
                .or_default()
                .push(segment.clone());
        }
        if by_column.len() < 2 {
            split.push(line.clone());
            continue;
        }
        for (_, segments) in by_column {
            if let Some(part) = LayoutLine::from_segments(line, segments) {
                split.push(part);
            }
        }
    }
    split
}

/// The tallest contiguous vertical window over which a strip is at least `min_emptiness`
/// free of the obstacles that cross it, as `(start, height)`.
///
/// Tolerant by exactly that fraction, which is what lets one crossing line — a floating
/// title, a rule — leave the gutter beneath it intact instead of erasing it.
fn free_window(strip: Rect, boxes: &[Rect], min_emptiness: f32) -> (f32, f32) {
    let height = strip.y1 - strip.y0;
    if height <= 0.0 {
        return (strip.y0, 0.0);
    }
    let bins = (height / BIN_PT).ceil().max(0.0) as usize;
    if bins == 0 {
        return (strip.y0, 0.0);
    }
    let mut covered = vec![false; bins];
    for b in boxes {
        if (b.x1.min(strip.x1) - b.x0.max(strip.x0)) <= 0.0 {
            continue;
        }
        let from = (((b.y0 - strip.y0) / BIN_PT).floor().max(0.0) as usize).min(bins);
        let to = (((b.y1 - strip.y0) / BIN_PT).ceil().max(0.0) as usize).min(bins);
        for bin in covered.iter_mut().take(to).skip(from) {
            *bin = true;
        }
    }

    // The longest window whose covered fraction stays within the tolerance, by two pointers
    // over the prefix sums. Linear, and it answers the question the threshold asks rather
    // than a proxy for it.
    let tolerance = (1.0 - min_emptiness).max(0.0);
    let mut prefix = vec![0u32; bins + 1];
    for (index, hit) in covered.iter().enumerate() {
        prefix[index + 1] = prefix[index] + u32::from(*hit);
    }
    let mut best = (0usize, 0usize);
    let mut from = 0usize;
    for to in 1..=bins {
        while from < to {
            let width = to - from;
            let inked = f32::from(u16::try_from(prefix[to] - prefix[from]).unwrap_or(u16::MAX));
            if inked <= tolerance * width as f32 {
                break;
            }
            from += 1;
        }
        if to - from > best.1 - best.0 {
            best = (from, to);
        }
    }
    (
        strip.y0 + best.0 as f32 * BIN_PT,
        (best.1 - best.0) as f32 * BIN_PT,
    )
}

/// The fraction of a rectangle free of ink.
fn emptiness_of(rect: Rect, boxes: &[Rect]) -> f32 {
    let area = (rect.x1 - rect.x0).max(0.0) * (rect.y1 - rect.y0).max(0.0);
    if area <= 0.0 {
        return 1.0;
    }
    let mut inked = 0.0;
    for b in boxes {
        let overlap_x = (b.x1.min(rect.x1) - b.x0.max(rect.x0)).max(0.0);
        let overlap_y = (b.y1.min(rect.y1) - b.y0.max(rect.y0)).max(0.0);
        inked += overlap_x * overlap_y;
    }
    (1.0 - inked / area).clamp(0.0, 1.0)
}

/// The bounding box of everything on the page, or `None` when there is nothing.
pub fn text_area(boxes: &[Rect]) -> Option<Rect> {
    let mut area = Rect {
        x0: f32::INFINITY,
        y0: f32::INFINITY,
        x1: f32::NEG_INFINITY,
        y1: f32::NEG_INFINITY,
    };
    for b in boxes {
        area.x0 = area.x0.min(b.x0);
        area.y0 = area.y0.min(b.y0);
        area.x1 = area.x1.max(b.x1);
        area.y1 = area.y1.max(b.y1);
    }
    area.x0.is_finite().then_some(area)
}

/// The median height of a set of boxes, used as the page's em.
pub fn median_height(boxes: &[Rect]) -> f32 {
    let mut heights: Vec<i64> = boxes
        .iter()
        .map(|b| ((b.y1 - b.y0).max(0.0) * 100.0).round() as i64)
        .collect();
    if heights.is_empty() {
        return 0.0;
    }
    heights.sort_unstable();
    heights[heights.len() / 2] as f32 / 100.0
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use oc_core::thresholds::T;
    use oc_model::extract::PageRef;
    use oc_model::ids::BlockId;

    /// The run boxes of a body of text: one 10 pt line every 12 pt, as a page sets them.
    fn ink(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<Rect> {
        let mut boxes = Vec::new();
        let mut top = y0;
        while top + 10.0 <= y1 {
            boxes.push(Rect {
                x0,
                y0: top,
                x1,
                y1: top + 10.0,
            });
            top += 12.0;
        }
        boxes
    }

    /// The page's em, as `detect_columns` is always called with it.
    const EM: f32 = 10.0;

    fn block_at(x0: f32, y0: f32, x1: f32, y1: f32) -> Block {
        let bbox = Rect { x0, y0, x1, y1 };
        Block {
            id: BlockId::derive(0, bbox, ""),
            page: PageRef::new(0),
            bbox,
            lines: Vec::new(),
            column: 0,
            kind_hint: BlockKindHint::Text,
            furniture: None,
            reading_index: 0,
        }
    }

    fn two_columns() -> Vec<Rect> {
        let mut boxes = ink(50.0, 50.0, 240.0, 400.0);
        boxes.extend(ink(300.0, 50.0, 490.0, 400.0));
        boxes
    }

    #[test]
    fn a_wide_valley_between_two_bodies_of_text_is_a_gutter() {
        let layout = detect_columns(&two_columns(), EM, usize::MAX, &T);
        assert_eq!(layout.count(), 2, "{:?}", layout.gutters);
        assert!(layout.gutters[0].x0 >= 240.0 && layout.gutters[0].x1 <= 300.0);
        assert!(layout.gutters[0].score > 0.0);
    }

    /// The failure this criterion exists for: the empty lower half of a short column is tall
    /// and empty and is not a gutter, because there is nothing on the far side of it.
    #[test]
    fn the_blank_half_of_a_short_column_is_not_a_gutter() {
        let mut boxes = ink(50.0, 50.0, 240.0, 400.0);
        boxes.extend(ink(300.0, 50.0, 490.0, 120.0));
        let layout = detect_columns(&boxes, EM, usize::MAX, &T);
        assert_eq!(layout.count(), 2, "the real gutter is still found");
        for gutter in &layout.gutters {
            assert!(
                gutter.x1 <= 300.5,
                "a gutter was found inside the short column: {gutter:?}"
            );
        }
    }

    /// One column of prose has no gutter, and the fallback is one column rather than a guess.
    #[test]
    fn a_single_column_page_has_no_gutter() {
        let layout = detect_columns(&ink(50.0, 50.0, 490.0, 400.0), EM, usize::MAX, &T);
        assert_eq!(layout.count(), 1);
        assert!(layout.gutters.is_empty());
    }

    /// A title crossing the gutter near the top does not destroy it — the whole reason the
    /// qualifying span is a share of the text height rather than all of it.
    #[test]
    fn a_crossing_title_does_not_destroy_the_gutter() {
        let mut boxes = two_columns();
        boxes.push(Rect {
            x0: 150.0,
            y0: 20.0,
            x1: 400.0,
            y1: 40.0,
        });
        let layout = detect_columns(&boxes, EM, usize::MAX, &T);
        assert_eq!(layout.count(), 2, "{:?}", layout.gutters);
    }

    /// Under a cap of one column there are no gutters, whatever the page looks like: this is
    /// how the continuity check re-runs a page it does not believe.
    #[test]
    fn a_capped_page_reports_one_column() {
        let layout = detect_columns(&two_columns(), EM, 1, &T);
        assert_eq!(layout.count(), 1);
        assert!(layout.gutters.is_empty());
    }

    /// And the block that crosses it is marked, because that is what `reading_order` masks.
    #[test]
    fn a_block_that_crosses_the_gutter_is_marked_a_floating_title() {
        let mut boxes = two_columns();
        boxes.push(Rect {
            x0: 150.0,
            y0: 20.0,
            x1: 400.0,
            y1: 40.0,
        });
        let layout = detect_columns(&boxes, EM, usize::MAX, &T);
        let mut blocks = vec![
            block_at(50.0, 50.0, 240.0, 400.0),
            block_at(300.0, 50.0, 490.0, 400.0),
            block_at(150.0, 20.0, 400.0, 40.0),
        ];
        assign_columns(&mut blocks, &layout);

        assert_eq!(blocks[2].kind_hint, BlockKindHint::FloatingTitle);
        assert_eq!(blocks[0].kind_hint, BlockKindHint::Text);
        assert_eq!(blocks[0].column, 0);
        assert_eq!(blocks[1].column, 1);
    }
}
