//! Reading an open PDF into the shape the post-`ingest` stages take.
//!
//! One function, and it is here rather than repeated at each call site because the shape has
//! now changed twice — once when images were added and once when the font table was — and
//! each time it changed in six places, five of which were tests that then read the document
//! twice per page to fill the new field.

use oc_model::extract::PageRef;
use oc_pdf::error::PdfError;
use oc_pdf::inspect::PdfDoc;

use crate::pipeline::PageInput;

/// Read every page of an open document into a [`PageInput`].
///
/// Glyphs and the font table come from one call per page, because they are one call: the
/// backend interns the fonts while it walks the glyphs, and asking twice walks twice.
///
/// Images are best-effort. A page whose images cannot be read still has its text, and
/// refusing the book over an image is the wrong trade — the same judgement `page_vectors`
/// makes about a path it cannot measure.
pub fn page_inputs(document: &dyn PdfDoc) -> Result<Vec<PageInput>, PdfError> {
    (0..document.page_count())
        .map(|index| {
            let glyphs = document.page_glyphs(index)?;
            let geometry = document.page_geometry(index)?;
            Ok(PageInput {
                page: PageRef::new(index),
                width_pt: geometry.width_pt(),
                height_pt: geometry.height_pt(),
                glyphs: glyphs.glyphs,
                fonts: glyphs.fonts,
                images: document.page_images(index).unwrap_or_default(),
            })
        })
        .collect()
}
