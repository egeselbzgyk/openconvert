//! The extraction layer as canonical JSON: `dump-stage ingest` (Phase 1 detail 9).
//!
//! This is the debugging surface for everything Phase 1 built. When a book converts wrongly,
//! the first question is always "what did we actually extract?", and the only useful answer is
//! the whole extraction layer, in a form that diffs — which is what canonical JSON is for
//! (D13.3): sorted keys, geometry at two decimals, NFC, `ir_version` first.
//!
//! **It is written per page.** RT B4 puts a real book's extraction layer at tens of megabytes,
//! so nothing here builds the whole document in memory: the header is written, then each page
//! as it is extracted, then the footer. The types below are therefore per-page, and the
//! document-level frame is assembled by the writer rather than by a struct.

use serde::Serialize;

use crate::classify::PageClass;
use crate::error::PdfError;
use crate::inspect::PdfDoc;

/// The stage name this dump belongs to. One of the twelve fixed stage names.
pub const STAGE: &str = "ingest";

/// The schema tag every dump carries, so a consumer can refuse a shape it does not know.
const SCHEMA: &str = "openconvert.dump.ingest/1";

/// What the document as a whole contributes: everything that is not per-page.
#[derive(Clone, Debug, Serialize)]
pub struct DumpHeader {
    pub schema: &'static str,
    pub stage: &'static str,
    /// First in the output, by D13.3 — a reader has to know the version before the shape.
    pub ir_version: u32,
    pub pages: u32,
    pub encrypted: bool,
    pub has_struct_tree: bool,
    pub permissions: crate::encrypt::Permissions,
    pub outline: Vec<oc_model::extract::OutlineEntry>,
}

/// One page's extraction layer.
#[derive(Clone, Debug, Serialize)]
pub struct DumpPage {
    pub index: u32,
    pub width_pt: f32,
    pub height_pt: f32,
    pub rotate: u16,
    pub class: PageClass,
    pub class_confidence: f32,
    /// `C_raw`: the conservation baseline for this page (D13.4).
    pub c_raw: oc_model::extract::CharHistogram,
    pub fonts: Vec<oc_model::extract::FontInfo>,
    pub glyphs: Vec<oc_model::extract::Glyph>,
    pub images: Vec<oc_model::extract::ImageRef>,
    /// What ingestion removed from this page, and why.
    pub removed: Vec<oc_model::ledger::LedgerEntry>,
}

/// Build the document-level header.
pub fn header(document: &dyn PdfDoc) -> DumpHeader {
    let info = document.doc_info();
    DumpHeader {
        schema: SCHEMA,
        stage: STAGE,
        ir_version: oc_model::IR_VERSION,
        pages: document.page_count(),
        encrypted: info.encrypted,
        has_struct_tree: info.has_struct_tree,
        permissions: info.permissions,
        outline: document.outline(),
    }
}

/// Extract one page into its dump form.
pub fn page(document: &dyn PdfDoc, index: u32) -> Result<DumpPage, PdfError> {
    let geometry = document.page_geometry(index)?;
    let glyphs = document.page_glyphs(index)?;
    Ok(DumpPage {
        index,
        width_pt: geometry.width_pt(),
        height_pt: geometry.height_pt(),
        rotate: geometry.rotate_degrees(),
        class: glyphs.class,
        class_confidence: glyphs.class_confidence,
        c_raw: glyphs.c_raw,
        fonts: glyphs.fonts,
        glyphs: glyphs.glyphs,
        images: document.page_images(index)?,
        removed: glyphs.removed,
    })
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Row 1.17 of the Phase 1 table.
// ---------------------------------------------------------------------------

/// Test 1.17.
///
/// A two-glyph fixture, so the snapshot is a thing a person can read in a diff — which is the
/// whole point of having one. Everything a real book's dump contains is present here in
/// miniature: geometry, a font table, glyphs with all thirteen signals, `C_raw`, and the
/// empty collections that prove the fields exist.
#[test]
fn dump_stage_ingest_snapshot_h01() {
    use crate::inspect::PdfOpen;
    use crate::pdfium::PdfiumBackend;

    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/fixtures/handmade/h01_two_glyphs.pdf");
    let bytes = std::fs::read(path).expect("h01 is committed");

    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(&bytes, None).expect("the fixture opens");

    let header = header(document.as_ref());
    let page = page(document.as_ref(), 0).expect("page 0 extracts");

    // Canonical, not `serde_json`'s default: sorted keys, geometry rounded, `ir_version`
    // first (D13.3). The snapshot is of that exact text, so a change to the canonical form is
    // a change to this snapshot, which is what makes the form testable at all.
    let rendered = format!(
        "{}\n{}",
        oc_model::canonical::to_canonical_json(&header).expect("the header is canonical"),
        oc_model::canonical::to_canonical_json(&page).expect("the page is canonical")
    );
    insta::assert_snapshot!(rendered);
}

/// The histogram serialises as the multiset it is, not as the array it is stored in.
///
/// Asserted separately because a regression here is invisible in the snapshot above — 128
/// zeroes look like noise, and noise is exactly what gets skimmed past in a review.
#[test]
fn char_histogram_serialises_only_present_characters() {
    let mut histogram = oc_model::extract::CharHistogram::new();
    histogram.add_str("aab\u{e9}");

    let json = oc_model::canonical::to_canonical_json(&histogram).expect("canonical");
    assert_eq!(json, r#"{"a":2,"b":1,"é":1}"#);
}
