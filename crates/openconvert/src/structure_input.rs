//! Turning what `layout` and `paragraphs` produced into the view `structure` reads.
//!
//! The boundary is `oc_structure::view::BlockView`, and everything that has to be *measured*
//! rather than merely carried is measured here: the width of the column a block sits in, the
//! air above it, and the runs behind each of its lines. Nine rules in PIPELINE §8 ask those
//! same three questions, and answering them once at the boundary is both cheaper and far
//! easier to check than answering them nine times.

use oc_structure::view::{BlockView, LineView};

use crate::pipeline::{text_of, LayoutStage, TextStage};

/// Build the block views for a laid-out document, in document reading order.
///
/// Document order, not page order: `structure` walks the book. `layout` numbers reading
/// order within a page, so the document order is the pages in order and each page's blocks in
/// their own reading order — which is what the flattening below produces by construction.
pub fn block_views(text: &TextStage, layout: &LayoutStage) -> Vec<BlockView> {
    let mut views = Vec::new();
    let mut order = 0u32;

    for (index, blocks) in layout.blocks.iter().enumerate() {
        let Some(page) = layout.pages.get(index) else {
            continue;
        };
        let columns = layout.columns.get(index);
        // Where the previous block in each column ended, so the gap above is a gap within a
        // column rather than a gap down the page: in two columns, the block at the top of the
        // right column has nothing above it, and the last block of the left column is not it.
        let mut column_bottom: std::collections::BTreeMap<u8, f32> =
            std::collections::BTreeMap::new();

        for block in blocks {
            let column_width_pt = columns
                .and_then(|layout| layout.columns.get(usize::from(block.column)))
                .map(|(x0, x1)| x1 - x0)
                .unwrap_or(page.width_pt);
            let space_above_pt = column_bottom
                .get(&block.column)
                .map(|bottom| (block.bbox.y0 - bottom).max(0.0))
                .unwrap_or(0.0);
            column_bottom.insert(block.column, block.bbox.y1);

            let lines: Vec<LineView> = block
                .lines
                .iter()
                .map(|line| LineView {
                    text: text_of(page, line).to_owned(),
                    bbox: line.bbox,
                    indent_pt: line.indent_pt,
                    right_gap_pt: line.right_gap_pt,
                    runs: text
                        .pages
                        .get(index)
                        .map(|source| {
                            line.runs
                                .iter()
                                .filter_map(|id| {
                                    source.runs.get(usize::try_from(id.0).unwrap_or(usize::MAX))
                                })
                                .cloned()
                                .collect()
                        })
                        .unwrap_or_default(),
                })
                .collect();

            views.push(BlockView {
                id: block.id,
                page: block.page.index,
                order,
                bbox: block.bbox,
                column: block.column,
                kind_hint: block.kind_hint,
                text: lines
                    .iter()
                    .map(|line| line.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
                    .trim()
                    .to_owned(),
                lines,
                column_width_pt,
                space_above_pt,
                page_height_pt: page.height_pt,
            });
            order = order.saturating_add(1);
        }
    }
    views
}
