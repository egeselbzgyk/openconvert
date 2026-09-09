//! Glyph extraction: the Stage-1 extraction layer (IMPLEMENTATION_PLAN Phase 1).
//!
//! Every character PDFium reports, with all thirteen signals D3 verified, plus the
//! ingestion filter of Phase 1 detail 2. Nothing here merges or rewrites text: filtering is
//! removal, every removal is ledgered, and normalisation is Phase 2's.

use oc_model::extract::{CharHistogram, FontId, FontInfo, Glyph};
use oc_model::ledger::LedgerEntry;

use crate::classify::{PageCharStats, PageClass};

/// The stage name every ledger entry from this module carries.
pub const STAGE: &str = "ingest";

/// One page, extracted.
#[derive(Clone, Debug)]
pub struct PageGlyphs {
    /// The glyphs that survived ingestion, in the backend's order.
    pub glyphs: Vec<Glyph>,
    /// The fonts those glyphs point at, indexed by [`FontId`].
    pub fonts: Vec<FontInfo>,
    /// What ingestion removed, and why.
    pub removed: Vec<LedgerEntry>,
    /// `C_raw`: the characters the document contains, counted before any interpretation and
    /// after the backend's own filtering (D13.4). Synthesised spaces are not in it - they
    /// are not in the document.
    pub c_raw: CharHistogram,
    /// The counters page classification runs on.
    pub stats: PageCharStats,
    pub class: PageClass,
    pub class_confidence: f32,
}

impl PageGlyphs {
    pub fn font(&self, id: FontId) -> Option<&FontInfo> {
        self.fonts.get(usize::from(id.0))
    }
}

/// Build the family key a heading cluster groups on: subset prefix, style suffix and case
/// removed (Phase 4 needs `ABCDEF+Minion-Bold` and `Minion-Regular` to land together).
pub fn family_key(name: &str) -> String {
    const SUBSET_PREFIX_LEN: usize = 7; // six uppercase letters and a `+`
    const STYLE_SUFFIXES: [&str; 10] = [
        "regular", "bold", "italic", "oblique", "light", "medium", "semibold", "book", "roman",
        "black",
    ];

    let without_subset = match name.as_bytes().get(SUBSET_PREFIX_LEN - 1) {
        Some(b'+') => &name[SUBSET_PREFIX_LEN..],
        _ => name,
    };
    let mut key = without_subset.to_lowercase();
    // Suffixes are stripped repeatedly: "Minion-BoldItalic" is two of them.
    loop {
        let trimmed = key.trim_end_matches(['-', '_', ' ', ',']);
        let Some(stripped) = STYLE_SUFFIXES
            .iter()
            .find_map(|suffix| trimmed.strip_suffix(suffix))
        else {
            key = trimmed.to_owned();
            break;
        };
        key = stripped.to_owned();
    }
    key.trim_end_matches(['-', '_', ' ', ',']).to_owned()
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Rows 1.1–1.4 of the Phase 1 table.
// ---------------------------------------------------------------------------

#[cfg(test)]
fn handmade(name: &str) -> std::path::PathBuf {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/fixtures/handmade")
        .join(format!("{name}.pdf"));
    assert!(
        path.is_file(),
        "missing fixture {}; run `cargo run -p xtask -- handmade-fixtures`",
        path.display()
    );
    path
}

#[cfg(test)]
fn ingest(name: &str) -> crate::glyphs::PageGlyphs {
    use crate::inspect::PdfOpen;
    use crate::pdfium::PdfiumBackend;

    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let bytes = std::fs::read(handmade(name)).expect("the fixture is readable");
    let document = backend.open(&bytes, None).expect("the fixture opens");
    document.page_glyphs(0).expect("page 0 extracts")
}

#[test]
fn glyphs_carry_all_verified_signals() {
    use oc_testkit::handmade::{FIXTURE_FONT_SIZE_PT, FIXTURE_ORIGIN};

    let page = ingest("h01_two_glyphs");
    assert_eq!(page.glyphs.len(), 2, "{:?}", page.glyphs);

    let first = &page.glyphs[0];
    assert_eq!(first.ch, 'A');
    assert_eq!(page.glyphs[1].ch, 'B');

    // Geometry: a real glyph has area, and the loose box is never tighter than the tight one.
    assert!(first.bbox.x1 > first.bbox.x0, "{:?}", first.bbox);
    assert!(first.bbox.y1 > first.bbox.y0, "{:?}", first.bbox);
    assert!(first.loose_bbox.x1 - first.loose_bbox.x0 >= first.bbox.x1 - first.bbox.x0);

    // The origin is the baseline point, in the normalised space: x unchanged, y measured
    // down from the top of an 800 pt page.
    assert!(
        (first.origin.0 - FIXTURE_ORIGIN.0).abs() < 0.5,
        "{:?}",
        first.origin
    );
    assert!(
        (first.origin.1 - (800.0 - FIXTURE_ORIGIN.1)).abs() < 0.5,
        "{:?}",
        first.origin
    );

    // The remaining signals, none of them a placeholder.
    assert!(
        (first.size_pt - FIXTURE_FONT_SIZE_PT).abs() < 0.01,
        "{}",
        first.size_pt
    );
    assert_eq!(first.render_mode, 0, "filled, the default");
    assert_eq!(first.fill[3], u8::MAX, "opaque");
    assert!(!first.generated);
    assert!(!first.hyphen_flag);
    assert!(first.angle_deg.abs() < 0.01);
    assert!(first.weight > 0, "a real font reports a weight");

    // The font is interned once and both glyphs point at it.
    assert_eq!(first.font, page.glyphs[1].font);
    let font = page.font(first.font).expect("the font is in the table");
    assert!(font.name.contains("Helvetica"), "{}", font.name);
    assert!(!font.type3);

    // Nothing was filtered, so nothing is ledgered, and C_raw is the two characters.
    assert!(page.removed.is_empty(), "{:?}", page.removed);
    assert_eq!(page.c_raw.total(), 2);
    assert_eq!(page.c_raw.count('A'), 1);
    assert_eq!(page.c_raw.count('B'), 1);
}

#[test]
fn generated_spaces_are_dropped_and_ledgered() {
    use oc_model::ledger::Reason;

    let page = ingest("h06_generated_space");

    // PDFium synthesises a space between the two words; it is not in the document.
    assert_eq!(
        page.glyphs.iter().map(|g| g.ch).collect::<String>(),
        "onetwo",
        "the synthesised space must not reach the IR"
    );
    assert!(page.glyphs.iter().all(|g| !g.generated));

    let generated: Vec<_> = page
        .removed
        .iter()
        .filter(|entry| entry.reason == Reason::GeneratedSpace)
        .collect();
    assert_eq!(generated.len(), 1, "{:?}", page.removed);
    assert_eq!(generated[0].text, " ");

    // `C_raw` counts what the document contains, and a synthesised space is not that. It is
    // whitespace besides, which sits outside `C` either way (D13.4).
    assert_eq!(page.c_raw.total(), 6, "one/two, no space");
    assert_eq!(page.c_raw.count(' '), 0);
}

#[test]
fn invisible_render_mode_3_is_not_visible_text() {
    use oc_model::ledger::Reason;

    let page = ingest("h05_invisible_layer");

    // Every glyph is render mode 3 over a full-page image: this is an OCR sandwich, and the
    // layer is kept with low confidence rather than discarded (D13.10).
    assert_eq!(
        page.class,
        crate::classify::PageClass::OcrSandwich,
        "{:?}",
        page.class
    );
    assert_eq!(page.stats.visible, 0);
    assert_eq!(page.stats.invisible, 26);
    assert!(page.stats.glyphless_font, "the OCR font marks itself");

    assert_eq!(page.glyphs.len(), 26, "the OCR layer is kept, not dropped");
    assert!(page.glyphs.iter().all(|g| g.render_mode == 3));
    assert!(
        !page.removed.iter().any(|e| e.reason == Reason::HiddenText),
        "render mode 3 on a sandwich page is the text, not hidden text: {:?}",
        page.removed
    );
    assert_eq!(page.c_raw.total(), 26);
}

#[test]
fn overdraw_duplicate_glyphs_are_deduped() {
    use oc_model::ledger::Reason;

    // h07 draws the same glyph twice, 0.2 pt apart - how a producer fakes bold (R1 §D.6 #2).
    let page = ingest("h07_overdraw_duplicate");

    assert_eq!(
        page.glyphs.len(),
        1,
        "the duplicate must not survive: {:?}",
        page.glyphs
    );
    assert_eq!(page.glyphs[0].ch, 'A');
    assert_eq!(page.c_raw.total(), 1);

    // The dedup is PDFium's, measured and written up in docs/DECISIONS_LOG.md: its text page
    // collapses identical overlapping glyphs before we ever see them, at separations an order
    // of magnitude beyond the 0.35 pt rule the plan specifies. So there is no ledger entry to
    // make - we removed nothing - and the plan's "C_raw has 2, C_0 has 1" cannot hold here.
    assert!(
        !page
            .removed
            .iter()
            .any(|e| e.reason == Reason::OverdrawDedup),
        "we did not remove it; PDFium did: {:?}",
        page.removed
    );

    // Two *different* overlapping glyphs are both kept, at the same 0.2 pt separation. That
    // is the property that makes the text page safe to build `C_raw` on: PDFium collapses
    // duplicates, never distinct characters, so nothing a reader would miss is lost.
    let pair = ingest("h08_overlap_distinct");
    assert_eq!(pair.glyphs.iter().map(|g| g.ch).collect::<String>(), "AB");
    assert_eq!(pair.c_raw.total(), 2);
    assert!(pair.removed.is_empty(), "{:?}", pair.removed);
}
