//! Reading an open PDF into the shape the post-`ingest` stages take.
//!
//! One function, and it is here rather than repeated at each call site because the shape has
//! now changed twice — once when images were added and once when the font table was — and
//! each time it changed in six places, five of which were tests that then read the document
//! twice per page to fill the new field.

use oc_model::extract::{ImageId, ImageRef, PageRef};
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
                class: glyphs.class,
                glyphs: glyphs.glyphs,
                fonts: glyphs.fonts,
                images: number_images(document.page_images(index).unwrap_or_default()),
                ocr_runs: Vec::new(),
            })
        })
        .collect()
}

/// Number a page's images by their position among the page's images in draw order — the index
/// [`PdfDoc::image_bytes`] takes (its own documentation says so) — rather than by the backend's
/// position among all page objects.
///
/// The pipeline used to recover that index later, as the image's position among the page's images
/// in the document-wide list. That holds only while no image leaves the list, and OCR removes the
/// ones whose text it read (PHASE 13 detail 9): after that, every later image on the page would
/// have decoded as its predecessor.
pub fn number_images(images: Vec<ImageRef>) -> Vec<ImageRef> {
    images
        .into_iter()
        .enumerate()
        .map(|(position, image)| ImageRef {
            id: ImageId(u32::try_from(position).unwrap_or(u32::MAX)),
            ..image
        })
        .collect()
}
