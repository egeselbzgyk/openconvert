//! A PDF opened through PDFium, and the per-page counters `inspect` reads from it.
//!
//! Counters only. No text is assembled and no `Glyph` is materialised here — that is Phase
//! 1's ingestion, and doing it now would mean building the expensive half of the pipeline to
//! answer a cheap question.

use pdfium_render::prelude::{
    PdfDocument, PdfPageObjectCommon, PdfPageObjectType, PdfPageObjectsCommon,
    PdfPageTextRenderMode, Pdfium,
};

use crate::classify::{classify_page, PageCharStats, PageClass, PageImageStats};
use crate::error::PdfError;
use crate::geom::{PageGeometry, PdfRect, Rotate};
use crate::glyphs::{family_key, PageGlyphs, STAGE};
use crate::inspect::{DocMetadata, PdfDoc};
use oc_model::extract::{CharHistogram, FontId, FontInfo, Glyph};
use oc_model::ledger::{LedgerEntry, Reason};

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
                Some(c) if is_undecodable_control(c, &character) => stats.control += 1,
                _ => {}
            }
        }
        Ok(stats)
    }

    fn page_glyphs(&self, index: u32) -> Result<PageGlyphs, PdfError> {
        self.page_glyphs_impl(index)
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

impl PdfiumDoc {
    /// Extract one page into the Stage-1 layer (Phase 1 details 1-3).
    ///
    /// Filtering here is removal, and every removal is ledgered. What is *not* removed
    /// matters as much: an OCR sandwich's invisible layer is the page's only text, so
    /// render mode 3 is dropped as `HiddenText` on every page except that one.
    pub fn page_glyphs_impl(&self, index: u32) -> Result<PageGlyphs, PdfError> {
        let page = self.page(index)?;
        let geometry = self.page_geometry(index)?;
        let images = self.page_image_stats(index)?;
        let text = page.text().map_err(|source| PdfError::Page {
            index,
            message: source.to_string(),
        })?;

        // Classification comes first, because it decides whether render mode 3 is the page's
        // text or text hidden on it.
        let stats = self.page_char_stats(index)?;
        let (class, class_confidence) =
            classify_page(&stats, &images, None, &oc_core::thresholds::T);
        let sandwich = class == PageClass::OcrSandwich;

        let mut fonts: Vec<FontInfo> = Vec::new();
        let mut glyphs = Vec::new();
        let mut removed = Vec::new();
        let mut c_raw = CharHistogram::new();

        for (position, character) in text.chars().iter().enumerate() {
            let position = u32::try_from(position).unwrap_or(u32::MAX);
            let span = (position, position.saturating_add(1));
            let ch = character
                .unicode_char()
                .unwrap_or(char::REPLACEMENT_CHARACTER);

            // A synthesised space is the backend's reconstruction, not the document's
            // content, so it never enters `C_raw` (D3, Phase 1 detail 2).
            if character.is_generated().unwrap_or(false) {
                removed.push(LedgerEntry::removed(
                    STAGE,
                    Reason::GeneratedSpace,
                    index,
                    span,
                    ch.to_string(),
                ));
                continue;
            }

            let render_mode = render_mode_code(&character);
            let fill = fill_rgba(&character);
            let invisible = render_mode == RENDER_MODE_INVISIBLE || fill[3] == INVISIBLE_ALPHA;
            if invisible && !sandwich {
                // Rendered but not visible on a page that is not a sandwich: hidden text.
                removed.push(LedgerEntry::removed(
                    STAGE,
                    Reason::HiddenText,
                    index,
                    span,
                    ch.to_string(),
                ));
                continue;
            }

            let tight = character.tight_bounds().map_err(|source| PdfError::Page {
                index,
                message: source.to_string(),
            })?;
            let loose = character.loose_bounds().unwrap_or(tight);
            let origin = character.origin();

            // Wholly outside the visible page is geometric absence, not a rendering choice.
            let bbox = geometry.normalise(pdf_rect(&tight));
            if outside_page(&geometry, &bbox) {
                removed.push(LedgerEntry::removed(
                    STAGE,
                    Reason::ClippedOffPage,
                    index,
                    span,
                    ch.to_string(),
                ));
                continue;
            }

            let font = intern_font(&mut fonts, &character);
            let (ox, oy) = match origin {
                Ok((x, y)) => geometry.normalise_point(x.value, y.value),
                // A glyph PDFium cannot place has no origin worth inventing; the tight box
                // is still known, so the baseline is taken from its foot.
                Err(_) => (bbox.x0, bbox.y1),
            };

            c_raw.add(ch);
            glyphs.push(Glyph {
                ch,
                bbox,
                loose_bbox: geometry.normalise(pdf_rect(&loose)),
                origin: (ox, oy),
                font,
                size_pt: character.scaled_font_size().value,
                weight: font_weight(&character),
                italic: character.font_is_italic(),
                render_mode,
                fill,
                generated: false,
                hyphen_flag: character.is_hyphen().unwrap_or(false),
                angle_deg: character.angle_degrees().unwrap_or_default(),
            });
        }

        Ok(PageGlyphs {
            glyphs,
            fonts,
            removed,
            c_raw,
            stats,
            class,
            class_confidence,
        })
    }
}

/// PDF text rendering mode 3.
const RENDER_MODE_INVISIBLE: u8 = 3;

fn render_mode_code(character: &pdfium_render::prelude::PdfPageTextChar<'_>) -> u8 {
    use pdfium_render::prelude::PdfPageTextRenderMode as Mode;
    match character.render_mode() {
        Ok(Mode::FilledUnstroked) => 0,
        Ok(Mode::StrokedUnfilled) => 1,
        Ok(Mode::FilledThenStroked) => 2,
        Ok(Mode::Invisible) => RENDER_MODE_INVISIBLE,
        Ok(Mode::FilledUnstrokedClipping) => 4,
        Ok(Mode::StrokedUnfilledClipping) => 5,
        Ok(Mode::FilledThenStrokedClipping) => 6,
        Ok(Mode::InvisibleClipping) => 7,
        // An unrecognised mode is reported as filled: assuming a glyph is visible keeps it
        // in the document, and losing text is the failure that matters.
        Ok(Mode::Unknown) | Err(_) => 0,
    }
}

fn fill_rgba(character: &pdfium_render::prelude::PdfPageTextChar<'_>) -> [u8; 4] {
    match character.fill_color() {
        Ok(colour) => [colour.red(), colour.green(), colour.blue(), colour.alpha()],
        // Same reasoning as the render mode: an unreadable colour is treated as opaque black
        // rather than as invisible.
        Err(_) => [0, 0, 0, u8::MAX],
    }
}

fn pdf_rect(rect: &pdfium_render::prelude::PdfRect) -> crate::geom::PdfRect {
    crate::geom::PdfRect {
        llx: rect.left().value,
        lly: rect.bottom().value,
        urx: rect.right().value,
        ury: rect.top().value,
    }
}

/// A font's weight on the usual 100-900 scale.
///
/// PDFium reports "unknown" for fonts that do not declare one, which is most of the
/// standard fourteen. Normal is the honest stand-in: it is what a reader renders them at,
/// and a zero would read as "thinner than hairline" to every later stage.
fn font_weight(character: &pdfium_render::prelude::PdfPageTextChar<'_>) -> u16 {
    use pdfium_render::prelude::PdfFontWeight as W;
    const NORMAL: u16 = 400;
    match character.font_weight() {
        Some(W::Weight100) => 100,
        Some(W::Weight200) => 200,
        Some(W::Weight300) => 300,
        Some(W::Weight400Normal) => NORMAL,
        Some(W::Weight500) => 500,
        Some(W::Weight600) => 600,
        Some(W::Weight700Bold) => 700,
        Some(W::Weight800) => 800,
        Some(W::Weight900) => 900,
        // PDFium reports 0 for a font with no descriptor, which is every one of the
        // standard fourteen. Zero would read as "thinner than hairline" to a heading
        // clusterer, so it is normal here too.
        Some(W::Custom(0)) | None => NORMAL,
        Some(W::Custom(value)) => u16::try_from(value).unwrap_or(u16::MAX),
    }
}

/// Whether a normalised rect lies wholly outside the page.
fn outside_page(geometry: &crate::geom::PageGeometry, rect: &oc_model::geom::Rect) -> bool {
    let (w, h) = (geometry.width_pt(), geometry.height_pt());
    rect.x1 <= 0.0 || rect.y1 <= 0.0 || rect.x0 >= w || rect.y0 >= h
}

/// Intern a character's font, returning its index.
fn intern_font(
    fonts: &mut Vec<FontInfo>,
    character: &pdfium_render::prelude::PdfPageTextChar<'_>,
) -> FontId {
    let name = character.font_name();
    if let Some(existing) = fonts.iter().position(|f| f.name == name) {
        return FontId(u16::try_from(existing).unwrap_or_default());
    }
    let id = FontId(u16::try_from(fonts.len()).unwrap_or(u16::MAX));
    let lowered = name.to_lowercase();
    fonts.push(FontInfo {
        family_key: family_key(&name),
        // Heuristics over the name, which is all PDFium exposes here; Phase 2 refines them
        // from the font descriptor through `lopdf` when it needs more.
        serif: !lowered.contains("sans")
            && !lowered.contains("arial")
            && !lowered.contains("helvetica"),
        fixed_pitch: lowered.contains("mono") || lowered.contains("courier"),
        symbolic: lowered.contains("symbol") || lowered.contains("dingbat"),
        type3: lowered.contains("type3"),
        embedded: name.as_bytes().get(6) == Some(&b'+'),
        name,
        id,
    });
    id
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

/// The three control characters a correctly-extracted text page legitimately carries.
const TAB: u32 = 0x09;
const LINE_FEED: u32 = 0x0A;
const CARRIAGE_RETURN: u32 = 0x0D;

/// A control character that stands for a code nothing could map to Unicode.
///
/// Two exclusions, both measured on the fixtures rather than assumed:
///
/// - Tab, line feed and carriage return reach a text page as structure — PDFium inserts them
///   between lines and columns — rather than as content.
/// - PDFium marks a hyphen at a line break with **U+0002** and sets `is_hyphen()` on it.
///   `f02` carries two, at `projec-tion` and `reading-order`; the stripped-`/ToUnicode` `f01`
///   carries 653 controls and not one of them is flagged. So the flag separates the two
///   meanings exactly, and a hyphenated page is not charged for its own hyphens.
fn is_undecodable_control(
    c: char,
    character: &pdfium_render::prelude::PdfPageTextChar<'_>,
) -> bool {
    let code = u32::from(c);
    c.is_control()
        && code != TAB
        && code != LINE_FEED
        && code != CARRIAGE_RETURN
        && !character.is_hyphen().unwrap_or(false)
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
