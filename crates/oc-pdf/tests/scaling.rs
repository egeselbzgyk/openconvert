//! Extraction cost per page must not depend on how many pages there are.
//!
//! This is a regression test for an algorithmic bug, not a benchmark. `page_images` used to
//! ask `lopdf` for the document's page map on every page, and `Document::get_pages` walks the
//! whole page tree and builds a map each time it is called — so the per-page cost grew with
//! the book. Measured before the fix: 78 µs/page over 50 pages, 564 µs/page over 400. That is
//! O(n²), and at the three-thousand-page limit it would have put ingestion minutes past
//! acceptance criterion A1.6's budget of 0.15 s/page.
//!
//! The assertion is a **ratio**, not a duration. An absolute timing test on a shared CI runner
//! measures the runner; a ratio between two counts in the same process cancels most of that
//! out, and the bound is loose enough that only a change in complexity can trip it.

use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;

/// The two document sizes compared. Eight times apart, so quadratic growth shows up as an
/// eightfold rise in per-page cost and linear growth as none.
const SMALL: usize = 100;
const LARGE: usize = 800;

/// How much worse the large document's per-page cost may be.
///
/// Measured after the fix: about 0.9, i.e. slightly *better*, because fixed costs amortise.
/// Measured before it: 7.2. Four is far above the noise and far below the bug.
const MAX_RATIO: f64 = 4.0;

#[test]
fn per_page_extraction_cost_does_not_grow_with_page_count() {
    let backend = PdfiumBackend::bind().expect("PDFium is vendored");

    let per_page = |count: usize| -> f64 {
        let bytes = oc_testkit::handmade::many_pages(count);
        let document = backend.open(&bytes, None).expect("the fixture opens");
        // Warm: the first page pays for anything cached lazily, and this test is about the
        // slope rather than the intercept.
        let _ = document.page_images(0);

        let start = std::time::Instant::now();
        for index in 0..document.page_count() {
            document.page_images(index).expect("the page extracts");
        }
        start.elapsed().as_secs_f64() / count as f64
    };

    let small = per_page(SMALL);
    let large = per_page(LARGE);
    assert!(
        small > 0.0,
        "the small run took no measurable time, so the ratio below means nothing"
    );

    let ratio = large / small;
    assert!(
        ratio < MAX_RATIO,
        "per-page cost rose {ratio:.1}x from {SMALL} to {LARGE} pages \
         ({small:.6}s to {large:.6}s per page); extraction has gone superlinear again"
    );
}
