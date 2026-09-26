//! Turning what `layout` and `paragraphs` produced into the view `structure` reads.
//!
//! The boundary is `oc_structure::view::BlockView`, and everything that has to be *measured*
//! rather than merely carried is measured here: the width of the column a block sits in, the
//! air above it, and the runs behind each of its lines. Nine rules in PIPELINE §8 ask those
//! same three questions, and answering them once at the boundary is both cheaper and far
//! easier to check than answering them nine times.

use oc_layout::paragraphs::ParagraphPlan;
use oc_model::extract::{ImageId, ImageRef};
use oc_structure::view::{BlockView, LineView};

use crate::pipeline::{text_of, LayoutStage, TextStage};

/// Build the block views for a laid-out document, in document reading order.
///
/// Document order, not page order: `structure` walks the book. `layout` numbers reading
/// order within a page, so the document order is the pages in order and each page's blocks in
/// their own reading order — which is what the flattening below produces by construction.
///
/// `paragraphs`' decisions ride on the views: where each block's paragraphs start, which block
/// carries on which, and which line breaks were joined. `layout`'s pages already hold the text
/// as `paragraphs` left it, so a line's text here has lost exactly the hyphens it joined.
pub fn block_views(text: &TextStage, layout: &LayoutStage, plan: &ParagraphPlan) -> Vec<BlockView> {
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
                .enumerate()
                .map(|(position, line)| LineView {
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
                    glue: plan.glued_at(block.id, position),
                })
                .map(without_joined_hyphen)
                .collect();

            views.push(BlockView {
                id: block.id,
                page: block.page.index,
                order,
                bbox: block.bbox,
                column: block.column,
                kind_hint: block.kind_hint,
                text: oc_structure::build::join_lines(lines.iter()),
                lines,
                column_width_pt,
                space_above_pt,
                page_height_pt: page.height_pt,
                para_starts: plan.starts.get(&block.id).cloned().unwrap_or_default(),
                continues: plan.continues.get(&block.id).copied(),
            });
            order = order.saturating_add(1);
        }
    }
    views
}

/// A line whose broken word `paragraphs` joined has lost its hyphen from its text; its last run
/// loses it too, so that every structure built from the runs — a table's cells, a note's body —
/// reads the same text the line does.
fn without_joined_hyphen(mut line: LineView) -> LineView {
    if !line.glue {
        return line;
    }
    if let Some(run) = line
        .runs
        .iter_mut()
        .rev()
        .find(|run| !run.text.trim().is_empty())
    {
        let trimmed = run.text.trim_end();
        if let Some(hyphen) = trimmed
            .chars()
            .next_back()
            .filter(|ch| oc_text::dehyphen::LINE_BREAK_HYPHENS.contains(ch))
        {
            run.text = trimmed[..trimmed.len() - hyphen.len_utf8()].to_owned();
        }
    }
    line
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

/// Per document-wide image, in [`document_images`]'s order, its index among its page's images —
/// the number [`oc_pdf::inspect::PdfDoc::image_bytes`] decodes it by.
///
/// Read from the page-local ids `ingest` numbered ([`crate::input::number_images`]), not recovered
/// from positions in the document-wide list: an image OCR replaced is no longer in the list, and
/// the positions of the ones after it on its page moved when it left.
pub fn image_slots(text: &TextStage) -> Vec<ImageId> {
    text.pages
        .iter()
        .flat_map(|page| page.images.iter().map(|image| image.id))
        .collect()
}
