//! Phase 1's acceptance criteria, where a named test does not already carry them.
//!
//! A1.1 is the one that needs its own test. The rest are covered: A1.2 by the metamorphic
//! rotation test, A1.3 by the pixel-bomb tests, A1.4 by the broken-text mutation, A1.5 by
//! fuzz-lite, and A1.6 by measurement recorded in `docs/DECISIONS_LOG.md` — a wall-clock
//! assertion in the fast tier would measure the runner, which `tests/scaling.rs` explains.

use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;

/// Every fixture A1.1 names, by where it lives.
const COMPILED: [&str; 3] = [
    "f01_prose_single_column",
    "f02_two_column",
    "f03_image_only",
];
const HANDMADE: [&str; 8] = [
    "h01_two_glyphs",
    "h02_rotate_90",
    "h03_cropbox_offset",
    "h04_ligature_fi",
    "h05_invisible_layer",
    "h06_generated_space",
    "h13_outline",
    "h15_indexed_colour",
];

/// A1.1 — every glyph carries all thirteen signals, and no field is a default placeholder.
///
/// "Not a placeholder" is the hard half. A struct of thirteen fields will happily come back
/// full of zeroes, and every individual assertion on it would pass. So each signal is checked
/// against what it *cannot* be if it was really read: a glyph has area, a font size is not
/// zero, a weight is not zero, a fill is not fully transparent unless the glyph is invisible,
/// and the font table is populated rather than empty.
#[test]
fn every_glyph_carries_thirteen_real_signals() {
    let backend = PdfiumBackend::bind().expect("PDFium is vendored");

    let mut checked = 0usize;
    for (source, name) in COMPILED
        .iter()
        .map(|n| ("../../target/fixtures", *n))
        .chain(
            HANDMADE
                .iter()
                .map(|n| ("../../corpus/fixtures/handmade", *n)),
        )
    {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(source)
            .join(format!("{name}.pdf"));
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|error| panic!("missing fixture {}: {error}", path.display()));

        let document = backend.open(&bytes, None).expect("the fixture opens");
        for index in 0..document.page_count() {
            let page = document.page_glyphs(index).expect("the page extracts");
            for glyph in &page.glyphs {
                let where_ = format!("{name} page {index} glyph {:?}", glyph.ch);

                // Geometry: a glyph that was really measured has area, and its advance box is
                // never tighter than its ink.
                assert!(glyph.bbox.x1 > glyph.bbox.x0, "{where_}: empty bbox");
                assert!(glyph.bbox.y1 > glyph.bbox.y0, "{where_}: empty bbox");
                assert!(
                    glyph.loose_bbox.x1 - glyph.loose_bbox.x0
                        >= glyph.bbox.x1 - glyph.bbox.x0 - 0.01,
                    "{where_}: loose box tighter than tight box"
                );

                // Typography.
                assert!(glyph.size_pt > 0.0, "{where_}: zero font size");
                assert!(glyph.weight > 0, "{where_}: zero weight");
                assert!(glyph.angle_deg.is_finite(), "{where_}: non-finite angle");

                // The font is interned and real, not an index into an empty table.
                let font = page.font(glyph.font).unwrap_or_else(|| {
                    panic!("{where_}: font {:?} is not in the table", glyph.font)
                });
                assert!(!font.name.is_empty(), "{where_}: nameless font");
                assert!(!font.family_key.is_empty(), "{where_}: no family key");

                // Colour: an opaque glyph is the norm, and a transparent one is only allowed
                // where the page is an OCR sandwich, which is where invisible text belongs.
                if glyph.fill[3] == 0 {
                    assert_eq!(
                        page.class,
                        oc_pdf::classify::PageClass::OcrSandwich,
                        "{where_}: transparent fill outside an OCR sandwich"
                    );
                }

                // Filtering already dropped these, so seeing one means the filter did not run.
                assert!(
                    !glyph.generated,
                    "{where_}: a synthesised glyph reached the IR"
                );

                checked += 1;
            }
        }
    }

    // Not vacuous: if the fixtures stopped producing glyphs this would pass over nothing.
    assert!(checked > 1000, "only {checked} glyphs checked");
}
