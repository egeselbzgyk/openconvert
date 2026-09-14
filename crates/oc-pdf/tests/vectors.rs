//! Vector regions, and the one question the pipeline asks of them: which paths are rules.
//!
//! Two consumers depend on the answer and both of them are structural rather than cosmetic —
//! the footnote separator (PIPELINE §8.3) and the table lattice (§8.7) — so a path
//! misclassified here is a footnote that does not link or a table that becomes an image.
//! The predicate is two conditions and this is what pins both of them down.

use oc_model::extract::VectorRegion;
use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;

fn vectors(bytes: &[u8], page: u32) -> Vec<VectorRegion> {
    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(bytes, None).expect("the fixture opens");
    document.page_vectors(page).expect("the page extracts")
}

/// h24 draws one filled hairline per page, 60 pt long and half a point thick, and nothing
/// else vector. It comes back as exactly one region, and that region is a rule.
#[test]
fn a_filled_hairline_is_read_as_a_horizontal_rule() {
    let bytes = oc_testkit::handmade::h24_footnote_symbol_cycle();
    for page in 0..2 {
        let regions = vectors(&bytes, page);
        assert_eq!(
            regions.len(),
            1,
            "page {page} draws one path and should report one region"
        );
        let rule = &regions[0];
        assert!(rule.is_rule, "a 60 pt by 0.5 pt filled bar is a rule");
        assert!(rule.is_horizontal());
        // 60 pt long, within the tolerance of a box PDFium measured rather than one we wrote.
        assert!(
            (rule.rule_length() - 60.0).abs() < 1.0,
            "rule length was {}",
            rule.rule_length()
        );
    }
}

/// A page that draws no paths reports no regions — the footnote detector's "no separator"
/// case has to be an empty vector rather than a region with `is_rule = false`.
#[test]
fn a_page_with_no_paths_has_no_vector_regions() {
    let bytes = oc_testkit::handmade::h01_two_glyphs();
    assert!(vectors(&bytes, 0).is_empty());
}

/// The predicate is thin **and** long, not either. A square block is neither; a page-tall
/// hairline is thin but is not a separator; a short thick bar is long-ish but not thin.
///
/// Built through the same filled-rectangle path a real producer uses, so what is asserted is
/// the extractor's reading of a drawn box rather than the arithmetic of `is_rule` in
/// isolation.
#[test]
fn a_block_is_not_a_rule_however_it_is_proportioned() {
    let bytes = oc_testkit::handmade::filled_boxes(&[
        // Long and thin: a rule.
        [60.0, 300.0, 200.0, 300.5],
        // Square: not a rule at any size.
        [60.0, 400.0, 120.0, 460.0],
        // Thin but short: 4 pt long at 1 pt thick is 4:1, under the aspect floor.
        [60.0, 500.0, 64.0, 501.0],
        // Long but thick: a 10 pt bar is a block of colour, not a line.
        [60.0, 600.0, 300.0, 610.0],
    ]);
    let regions = vectors(&bytes, 0);
    assert_eq!(regions.len(), 4, "four paths, four regions, in draw order");
    assert_eq!(
        regions.iter().map(|r| r.is_rule).collect::<Vec<_>>(),
        vec![true, false, false, false]
    );
}
