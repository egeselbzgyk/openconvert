//! Turning what `layout` and `paragraphs` produced into the view `structure` reads.
//!
//! The boundary is `oc_structure::view::BlockView`, and everything that has to be *measured*
//! rather than merely carried is measured here: the width of the column a block sits in, the
//! air above it, and the runs behind each of its lines. Nine rules in PIPELINE §8 ask those
//! same three questions, and answering them once at the boundary is both cheaper and far
//! easier to check than answering them nine times.

use oc_model::extract::{ImageId, ImageRef};
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
                    line: line.clone(),
                    text: text_of(page, line).to_owned(),
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

/// Every image in the document, in page order, with ids that are unique across the book.
///
/// The backend numbers images **per page** — `ImageId(0)` is the first image of whichever
/// page you asked for — because `PdfDoc::image_bytes(page, id)` uses the id as an index into
/// that page's draw order. That is right for extraction and wrong for a `Figure`, which lives
/// in a document and whose `image` field has to name one picture in the whole book.
///
/// So the renumbering happens here, at the document boundary, and the page-local index stays
/// recoverable: it is the image's position among the images sharing its page, which page
/// order preserves.
pub fn document_images(text: &TextStage) -> Vec<ImageRef> {
    text.pages
        .iter()
        .flat_map(|page| page.images.iter().cloned())
        .enumerate()
        .map(|(index, image)| ImageRef {
            id: ImageId(u32::try_from(index).unwrap_or(u32::MAX)),
            ..image
        })
        .collect()
}
