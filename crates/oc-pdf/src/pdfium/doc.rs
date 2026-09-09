//! A PDF opened through PDFium, and the per-page counters `inspect` reads from it.
//!
//! Counters only. No text is assembled and no `Glyph` is materialised here — that is Phase
//! 1's ingestion, and doing it now would mean building the expensive half of the pipeline to
//! answer a cheap question.

use pdfium_render::prelude::{
    PdfDocument, PdfPageObjectCommon, PdfPageObjectType, PdfPageObjectsCommon,
    PdfPageTextRenderMode, Pdfium,
};

use crate::classify::{PageCharStats, PageImageStats};
use crate::error::PdfError;
use crate::geom::{PageGeometry, PdfRect, Rotate};
use crate::inspect::{DocMetadata, PdfDoc};

/// Unicode private-use areas. A subset font with no `ToUnicode` map lands here rather than
/// producing U+FFFD, so both are counted towards the broken-text share (D13.10).
const PUA_BMP: std::ops::RangeInclusive<u32> = 0xE000..=0xF8FF;
const PUA_PLANE_15: std::ops::RangeInclusive<u32> = 0xF_0000..=0xF_FFFD;
const PUA_PLANE_16: std::ops::RangeInclusive<u32> = 0x10_0000..=0x10_FFFD;

/// The marker Tesseract leaves in the font name of an OCR text layer (R3 §4).
const GLYPHLESS_FONT_MARKER: &str = "GlyphLessFont";

/// Fully transparent fill: drawn, but not shown.
const INVISIBLE_ALPHA: u8 = 0;

/// A document PDFium holds open, plus the metadata read once at open time.
pub struct PdfiumDoc {
    document: PdfDocument<'static>,
    metadata: DocMetadata,
}

impl PdfiumDoc {
    /// Open a document from memory.
    ///
    /// The bytes are copied into the document because PDFium reads lazily and the caller's
    /// buffer would otherwise have to outlive it — an ownership puzzle that buys nothing at
    /// the sizes involved.
    pub(crate) fn open(
        pdfium: &'static Pdfium,
        bytes: &[u8],
        password: Option<&str>,
    ) -> Result<Self, PdfError> {
        let document = pdfium
            .load_pdf_from_byte_vec(bytes.to_vec(), password)
            .map_err(|source| PdfError::Open {
                message: source.to_string(),
            })?;
        let metadata = read_metadata(&document, bytes);
        Ok(Self { document, metadata })
    }
}

impl PdfDoc for PdfiumDoc {
    fn page_count(&self) -> u32 {
        // PDFium reports the count as a signed integer; a negative count is not a thing.
        u32::try_from(self.document.pages().len()).unwrap_or_default()
    }

    fn doc_info(&self) -> DocMetadata {
        self.metadata.clone()
    }

    fn page_geometry(&self, index: u32) -> Result<PageGeometry, PdfError> {
        let page = self.page(index)?;
        let boundaries = page.boundaries();
        // The crop box is what is displayed; a document that omits it falls back to the
        // media box, which is what PDFium reports in that case anyway.
        let crop = boundaries
            .crop()
            .or_else(|_| boundaries.media())
            .map_err(|source| PdfError::Page {
                index,
                message: source.to_string(),
            })?
            .bounds;
        let rotate = Rotate::from_degrees(page.rotation().map(degrees).unwrap_or_default())
            .ok_or_else(|| PdfError::Page {
                index,
                message: "page /Rotate is not a multiple of 90".to_owned(),
            })?;
        Ok(PageGeometry::new(
            PdfRect {
                llx: crop.left().value,
                lly: crop.bottom().value,
                urx: crop.right().value,
                ury: crop.top().value,
            },
            rotate,
        ))
    }

    fn page_char_stats(&self, index: u32) -> Result<PageCharStats, PdfError> {
        let page = self.page(index)?;
        let text = page.text().map_err(|source| PdfError::Page {
            index,
            message: source.to_string(),
        })?;

        let mut stats = PageCharStats::default();
        for character in text.chars().iter() {
            // Synthesised spaces are PDFium's reconstruction, not the document's content, so
            // they are excluded from every count (D3).
            if character.is_generated().unwrap_or(false) {
                continue;
            }
            if character.font_name().contains(GLYPHLESS_FONT_MARKER) {
                stats.glyphless_font = true;
            }

            let invisible = matches!(
                character.render_mode(),
                Ok(PdfPageTextRenderMode::Invisible | PdfPageTextRenderMode::InvisibleClipping)
            ) || character
                .fill_color()
                .is_ok_and(|colour| colour.alpha() == INVISIBLE_ALPHA);

            if invisible {
                stats.invisible += 1;
                continue;
            }
            stats.visible += 1;

            match character.unicode_char() {
                Some(char::REPLACEMENT_CHARACTER) => stats.replacement += 1,
                Some(c) if is_private_use(c) => stats.pua += 1,
                _ => {}
            }
        }
        Ok(stats)
    }

    fn page_image_stats(&self, index: u32) -> Result<PageImageStats, PdfError> {
        let page = self.page(index)?;
        let page_area = page.width().value * page.height().value;

        let mut stats = PageImageStats::default();
        let mut covered = 0.0f32;
        for object in page.objects().iter() {
            if object.object_type() != PdfPageObjectType::Image {
                continue;
            }
            stats.count += 1;
            if let Ok(bounds) = object.bounds() {
                let width = (bounds.right().value - bounds.left().value).abs();
                let height = (bounds.top().value - bounds.bottom().value).abs();
                // Summed, not unioned: two overlapping images would over-count, but the ratio
                // is only ever compared against a threshold and is clamped to 1 below.
                covered += width * height;
            }
        }
        stats.covered_area_ratio = if page_area > 0.0 {
            (covered / page_area).clamp(0.0, 1.0)
        } else {
            0.0
        };
        Ok(stats)
    }
}

impl PdfiumDoc {
    fn page(&self, index: u32) -> Result<pdfium_render::prelude::PdfPage<'_>, PdfError> {
        let page_index = i32::try_from(index).map_err(|_| PdfError::Page {
            index,
            message: "page index does not fit in a PDF page number".to_owned(),
        })?;
        self.document
            .pages()
            .get(page_index)
            .map_err(|source| PdfError::Page {
                index,
                message: source.to_string(),
            })
    }
}

fn degrees(rotation: pdfium_render::prelude::PdfPageRenderRotation) -> i32 {
    use pdfium_render::prelude::PdfPageRenderRotation as R;
    match rotation {
        R::None => 0,
        R::Degrees90 => 90,
        R::Degrees180 => 180,
        R::Degrees270 => 270,
    }
}

fn is_private_use(c: char) -> bool {
    let code = u32::from(c);
    PUA_BMP.contains(&code) || PUA_PLANE_15.contains(&code) || PUA_PLANE_16.contains(&code)
}

/// Read the document metadata `inspect` reports.
///
/// The struct-tree check is a byte search rather than a parse: PDFium exposes no predicate
/// for it, and `inspect` only needs to know whether one is present, not what is in it.
/// Phase 1 reads the tree properly through `lopdf` when it needs the hints inside.
fn read_metadata(document: &PdfDocument<'_>, bytes: &[u8]) -> DocMetadata {
    use pdfium_render::prelude::PdfDocumentMetadataTagType as Tag;

    let metadata = document.metadata();
    let tag = |tag: Tag| -> Option<String> {
        metadata
            .get(tag)
            .map(|entry| entry.value().to_owned())
            .filter(|value| !value.trim().is_empty())
    };

    DocMetadata {
        producer: tag(Tag::Producer),
        creator: tag(Tag::Creator),
        title: tag(Tag::Title),
        author: tag(Tag::Author),
        // An encrypted document that opened is one whose user password was empty or supplied;
        // the permission flags do not block conversion (D13.11).
        encrypted: find_bytes(bytes, b"/Encrypt"),
        has_struct_tree: find_bytes(bytes, b"/StructTreeRoot"),
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}
