//! Reading order: recursive XY-cut with pre-masking (PIPELINE §6 step 3, R2 §B.2, R10 §4.2).
//!
//! The evidence here is unusually decisive and it points away from learning. XY-Cut++ reaches
//! 0.988 BLEU-4 against LayoutReader's 0.788 at 23× the speed, and LayoutReader *collapses*
//! to 0.595 on three-column pages — worse than naive XY-cut's 0.702. On Manhattan layouts,
//! which is what a novel, a textbook or a manual is, plain recursive XY-cut scores 100 %
//! while the learned method scores 96.0 %. So there is no model here, and there is not
//! going to be one; the post-v1 hook is an ONNX model supplying the *pre-mask classes* while
//! geometry keeps the order.
//!
//! Three things make this XY-Cut++ rather than the 1984 algorithm:
//!
//! 1. **Pre-masking.** Floating titles, figures, tables and full-width rules are lifted out
//!    before the cut, because a single element spanning two columns makes the first vertical
//!    cut impossible and the whole page degenerates into one top-to-bottom sort — the classic
//!    two-column interleave.
//! 2. **The split direction comes from the region's own content**, not a fixed order: the
//!    wider valley wins, each measured against what it would have to be to mean something —
//!    a gutter's width for x, a line's height for y.
//! 3. **Masked elements are put back by distance**, next to the block they are nearest,
//!    before it when they sit above it.

use oc_core::thresholds::Thresholds;
use oc_model::geom::Rect;
use oc_model::layout::{Block, BlockKindHint};

use crate::columns::{median_height, ColumnLayout};

/// How much of a block a mask rectangle must cover before the block is treated as that
/// element rather than as text (PIPELINE §6: "no two blocks overlap by more than 50 %").
const MASK_OVERLAP_SHARE: f32 = 0.5;

/// Put the page's blocks into reading order, writing `reading_index` on each.
///
/// `masks` are regions `ingest` already knows are not body text — images and vector rules.
/// A block mostly inside one of them is pre-masked along with the blocks that span columns.
pub fn reading_order(blocks: &mut [Block], columns: &ColumnLayout, masks: &[Rect], t: &Thresholds) {
    if blocks.is_empty() {
        return;
    }

    for block in blocks.iter_mut() {
        if block.kind_hint == BlockKindHint::Text && covered_by(block.bbox, masks) {
            block.kind_hint = BlockKindHint::Image;
        }
    }

    let masked: Vec<usize> = (0..blocks.len())
        .filter(|index| blocks[*index].kind_hint != BlockKindHint::Text)
        .collect();
    let flowing: Vec<usize> = (0..blocks.len())
        .filter(|index| blocks[*index].kind_hint == BlockKindHint::Text)
        .collect();

    let boxes: Vec<Rect> = blocks.iter().map(|block| block.bbox).collect();
    let em = median_height(
        &blocks
            .iter()
            .flat_map(|block| block.lines.iter().map(|line| line.bbox))
            .collect::<Vec<_>>(),
    );
    let min_x_gap = em * t.layout.columns.gutter_min_width_em as f32;
    let min_y_gap = em * t.layout.block.separator_height_ratio as f32;

    let mut order: Vec<usize> = Vec::with_capacity(blocks.len());
    cut(&flowing, &boxes, min_x_gap, min_y_gap, &mut order);

    // Masked elements go back next to the block they are nearest, above it if they sit
    // above it — which is what puts a floating title at the top of its page rather than
    // wherever a naive sort would have left it.
    for index in masked {
        let position = insertion_point(index, &order, &boxes, columns);
        order.insert(position, index);
    }

    // Columns are written here rather than in `assign_columns` for the masked blocks, since
    // a block that spans columns belongs to the first one it reaches.
    for (position, index) in order.iter().enumerate() {
        blocks[*index].reading_index = u32::try_from(position).unwrap_or(u32::MAX);
        blocks[*index].column = columns.column_of(boxes[*index]);
    }
}

/// Where a masked block belongs in an order that was computed without it.
///
/// **Before the first block it sits above and shares a column with.** That one sentence
/// handles both shapes at once: a full-width title above two columns shares a column with the
/// first block of the left column and sits above it, so it lands at the top of the page,
/// which is where a reader meets it; a figure halfway down a column is above only the rest of
/// that column, so it lands there and not at the top.
///
/// A *column*, not a raw x-overlap. A centred title and a short heading at the left margin
/// need not overlap at all — on `f02` they do not — and the title would then slot in after
/// the heading it plainly precedes.
///
/// Nearest-by-distance is the fallback, for a masked block with nothing below it that it
/// shares a column with — a figure at the foot of a page. That is where R2 §B.2's
/// IoU-weighted distance comes in: an element inside a column's x-range is nearer to that
/// column's text than its centres alone say.
fn insertion_point(index: usize, order: &[usize], boxes: &[Rect], columns: &ColumnLayout) -> usize {
    let me = boxes[index];
    let mine = columns_touched(me, columns);
    let shares_column = |other: Rect| {
        columns_touched(other, columns)
            .iter()
            .any(|c| mine.contains(c))
    };

    if let Some(position) = order
        .iter()
        .position(|other| me.y1 <= boxes[*other].y0 && shares_column(boxes[*other]))
    {
        return position;
    }

    let mut best: Option<(i64, usize)> = None;
    for (position, other) in order.iter().enumerate() {
        let key = (weighted_distance(me, boxes[*other]) * 100.0).round() as i64;
        if best.is_none_or(|(best_key, _)| key < best_key) {
            best = Some((key, position));
        }
    }
    match best {
        None => order.len(),
        Some((_, position)) => {
            if me.y0 <= boxes[order[position]].y0 {
                position
            } else {
                position + 1
            }
        }
    }
}

/// Centre-to-centre distance, discounted by how much the two boxes overlap.
///
/// The IoU-weighted distance of R2 §B.2: an element sitting inside a column's x-range is
/// nearer to that column's text than its centres alone say, and a caption directly under a
/// figure is nearer still.
fn weighted_distance(a: Rect, b: Rect) -> f32 {
    let dx = ((a.x0 + a.x1) - (b.x0 + b.x1)).abs() / 2.0;
    let dy = ((a.y0 + a.y1) - (b.y0 + b.y1)).abs() / 2.0;
    let overlap_x = (a.x1.min(b.x1) - a.x0.max(b.x0)).max(0.0);
    let overlap_y = (a.y1.min(b.y1) - a.y0.max(b.y0)).max(0.0);
    let union = area(a) + area(b) - overlap_x * overlap_y;
    let iou = if union > 0.0 {
        overlap_x * overlap_y / union
    } else {
        0.0
    };
    (dx * dx + dy * dy).sqrt() * (1.0 - iou)
}

/// Which columns a box reaches into at all.
fn columns_touched(bbox: Rect, columns: &ColumnLayout) -> Vec<usize> {
    let touched: Vec<usize> = columns
        .columns
        .iter()
        .enumerate()
        .filter(|(_, (x0, x1))| (bbox.x1.min(*x1) - bbox.x0.max(*x0)).max(0.0) > 0.0)
        .map(|(index, _)| index)
        .collect();
    if touched.is_empty() {
        vec![usize::from(columns.column_of(bbox))]
    } else {
        touched
    }
}

fn area(r: Rect) -> f32 {
    (r.x1 - r.x0).max(0.0) * (r.y1 - r.y0).max(0.0)
}

/// Whether a box is mostly inside one of the mask regions.
fn covered_by(bbox: Rect, masks: &[Rect]) -> bool {
    let own = area(bbox);
    if own <= 0.0 {
        return false;
    }
    masks.iter().any(|mask| {
        let overlap_x = (bbox.x1.min(mask.x1) - bbox.x0.max(mask.x0)).max(0.0);
        let overlap_y = (bbox.y1.min(mask.y1) - bbox.y0.max(mask.y0)).max(0.0);
        overlap_x * overlap_y / own > MASK_OVERLAP_SHARE
    })
}

/// The recursive cut itself.
///
/// Both valleys are measured, and the wider one wins — the region's own content deciding the
/// direction rather than a fixed alternation. A region with a gutter through it is cut on x;
/// a column is cut on y; a region with neither is emitted top to bottom, left to right,
/// which for a single block is the whole answer.
fn cut(region: &[usize], boxes: &[Rect], min_x_gap: f32, min_y_gap: f32, order: &mut Vec<usize>) {
    if region.len() <= 1 {
        order.extend_from_slice(region);
        return;
    }

    let x_gap = widest_gap(region, boxes, Axis::X);
    let y_gap = widest_gap(region, boxes, Axis::Y);
    let x_ok = x_gap.map(|(width, _)| width >= min_x_gap).unwrap_or(false);
    let y_ok = y_gap.map(|(width, _)| width >= min_y_gap).unwrap_or(false);

    let axis = match (x_ok, y_ok) {
        (false, false) => {
            let mut rest = region.to_vec();
            rest.sort_by_key(|index| {
                let b = boxes[*index];
                ((b.y0 * 100.0).round() as i64, (b.x0 * 100.0).round() as i64)
            });
            order.extend(rest);
            return;
        }
        (true, false) => Axis::X,
        (false, true) => Axis::Y,
        // Both are real valleys: the wider one is the more significant cut, in units of what
        // that direction needs to mean something.
        (true, true) => {
            let x_score = x_gap.map(|(width, _)| width / min_x_gap).unwrap_or(0.0);
            let y_score = y_gap.map(|(width, _)| width / min_y_gap).unwrap_or(0.0);
            if x_score >= y_score {
                Axis::X
            } else {
                Axis::Y
            }
        }
    };

    let at = match axis {
        Axis::X => x_gap.map(|(_, at)| at).unwrap_or_default(),
        Axis::Y => y_gap.map(|(_, at)| at).unwrap_or_default(),
    };
    let (before, after): (Vec<usize>, Vec<usize>) = region.iter().partition(|index| {
        let b = boxes[**index];
        match axis {
            Axis::X => b.x0 < at,
            Axis::Y => b.y0 < at,
        }
    });
    if before.is_empty() || after.is_empty() {
        let mut rest = region.to_vec();
        rest.sort_by_key(|index| {
            let b = boxes[*index];
            ((b.y0 * 100.0).round() as i64, (b.x0 * 100.0).round() as i64)
        });
        order.extend(rest);
        return;
    }
    cut(&before, boxes, min_x_gap, min_y_gap, order);
    cut(&after, boxes, min_x_gap, min_y_gap, order);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Axis {
    X,
    Y,
}

/// The widest empty band across a region on one axis, and where it ends.
///
/// A sweep over the projected intervals: sort by start, carry the furthest end seen, and the
/// gaps between them are the bands. Returns the width and the coordinate the split is made
/// at, which is the far side of the band.
fn widest_gap(region: &[usize], boxes: &[Rect], axis: Axis) -> Option<(f32, f32)> {
    let mut intervals: Vec<(f32, f32)> = region
        .iter()
        .map(|index| {
            let b = boxes[*index];
            match axis {
                Axis::X => (b.x0, b.x1),
                Axis::Y => (b.y0, b.y1),
            }
        })
        .collect();
    intervals.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut best: Option<(f32, f32)> = None;
    let mut reach = intervals.first()?.1;
    for (start, end) in intervals.iter().skip(1) {
        if *start > reach {
            let width = start - reach;
            if best.is_none_or(|(best_width, _)| width > best_width) {
                best = Some((width, *start));
            }
        }
        reach = reach.max(*end);
    }
    best
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Row 3.4 of the Phase 3 table.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use oc_core::thresholds::T;
    use oc_model::extract::PageRef;
    use oc_model::ids::BlockId;
    use oc_model::text::Line;
    use proptest::prelude::*;

    fn block_at(x0: f32, y0: f32, x1: f32, y1: f32) -> Block {
        let bbox = Rect { x0, y0, x1, y1 };
        let line = Line {
            runs: Vec::new(),
            bbox,
            baseline_y: y1,
            ends_with_hyphen: false,
            indent_pt: 0.0,
            right_gap_pt: 0.0,
        };
        Block {
            id: BlockId::derive(0, bbox, ""),
            page: PageRef::new(0),
            bbox,
            lines: vec![line],
            column: 0,
            kind_hint: BlockKindHint::Text,
            furniture: None,
            reading_index: 0,
        }
    }

    proptest! {
        /// Row 3.4. On a single-column page reading order *is* the y order, and nothing about
        /// widths, ragged edges or the order the blocks arrive in may disturb that.
        #[test]
        fn prop_single_column_order_is_monotone_in_y(
            widths in proptest::collection::vec(40.0f32..200.0, 2..12),
            shuffle in 0usize..97,
        ) {
            let mut blocks: Vec<Block> = widths
                .iter()
                .enumerate()
                .map(|(index, width)| {
                    let y0 = 50.0 + index as f32 * 24.0;
                    block_at(50.0, y0, 50.0 + width, y0 + 10.0)
                })
                .collect();
            // The input order is not the answer, and must not be mistaken for it.
            let count = blocks.len();
            blocks.rotate_left(shuffle % count);

            let columns = ColumnLayout::single(50.0, 250.0);
            reading_order(&mut blocks, &columns, &[], &T);

            let mut ordered: Vec<&Block> = blocks.iter().collect();
            ordered.sort_by_key(|block| block.reading_index);
            for pair in ordered.windows(2) {
                prop_assert!(
                    pair[0].bbox.y0 <= pair[1].bbox.y0,
                    "reading order is not monotone in y: {:?} then {:?}",
                    pair[0].bbox,
                    pair[1].bbox
                );
            }
            let indices: std::collections::BTreeSet<u32> =
                blocks.iter().map(|block| block.reading_index).collect();
            prop_assert_eq!(indices.len(), blocks.len(), "reading_index is a permutation");
        }
    }

    /// Two columns, and the whole of the left one comes first — the property the fixture
    /// test asserts on a real page, here without a PDF in the way.
    #[test]
    fn a_two_column_page_is_read_down_then_across() {
        let mut blocks = Vec::new();
        for row in 0..4 {
            let y0 = 50.0 + row as f32 * 24.0;
            blocks.push(block_at(50.0, y0, 240.0, y0 + 10.0));
            blocks.push(block_at(300.0, y0, 490.0, y0 + 10.0));
        }
        let columns = ColumnLayout {
            columns: vec![(50.0, 240.0), (300.0, 490.0)],
            gutters: vec![crate::columns::Gutter {
                x0: 240.0,
                x1: 300.0,
                emptiness: 1.0,
                height_share: 1.0,
                score: 60.0,
            }],
        };
        reading_order(&mut blocks, &columns, &[], &T);

        let mut ordered: Vec<&Block> = blocks.iter().collect();
        ordered.sort_by_key(|block| block.reading_index);
        let lefts: Vec<usize> = ordered
            .iter()
            .enumerate()
            .filter(|(_, block)| block.bbox.x0 < 250.0)
            .map(|(position, _)| position)
            .collect();
        assert_eq!(
            lefts,
            vec![0, 1, 2, 3],
            "the left column is read first: {ordered:#?}"
        );
    }

    /// A masked element with nothing below it in its column goes last, not first: the
    /// fallback path, which is the one a figure at the foot of a page takes.
    #[test]
    fn a_masked_block_with_nothing_below_it_goes_last() {
        let mut blocks = vec![
            block_at(50.0, 50.0, 240.0, 60.0),
            block_at(50.0, 80.0, 240.0, 90.0),
        ];
        let mut figure = block_at(50.0, 110.0, 240.0, 200.0);
        figure.kind_hint = BlockKindHint::Image;
        blocks.push(figure);

        let columns = ColumnLayout::single(50.0, 240.0);
        reading_order(&mut blocks, &columns, &[], &T);

        assert_eq!(blocks[2].reading_index, 2);
    }
}
