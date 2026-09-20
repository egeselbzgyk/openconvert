//! Metamorphic invariants: transformations of a PDF that must not change what it contains.
//!
//! Rows 1.5–1.7 of the Phase 1 table. Each one takes a fixture, changes something a reader
//! is entitled to change — the page's `/Rotate`, its CropBox, the order the drawing
//! operators happen to appear in — and asserts the extracted characters are the same.
//!
//! These are worth more than their line count. R1 §D.6 #1 documents a shipping tool that
//! silently dropped body text on pages whose CropBox did not start at the origin, and the
//! reason it shipped is that no fixture had an offset CropBox: every unit test agreed with
//! every other unit test because they all made the same assumption. A metamorphic test does
//! not need to know what the right answer is, only that two routes to the answer must agree,
//! which is exactly the kind of bug a table of expected values cannot catch. Test 1.5 found
//! one on its first run — see the rotation note below.
//!
//! They live in `tests/` rather than beside the code because they exercise the crate the way
//! the pipeline does — open, extract, compare — and need nothing private to do it.

use oc_pdf::geom::PageGeometry;
use oc_pdf::glyphs::PageGlyphs;
use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;
use oc_testkit::handmade::{LINE_ALPHABET, LINE_GLYPH_COUNT};
use proptest::prelude::*;

/// The four values `/Rotate` may take. A PDF may write any multiple of 90; a reader
/// normalises it, and so does `PageGeometry`.
const ROTATIONS: [i32; 4] = [0, 90, 180, 270];

/// The CropBox shift test 1.6 applies, in points — the offset R1 §D.6 #1's bug needed.
const CROP_SHIFT_PT: (f32, f32) = (50.0, 50.0);

/// How many proptest cases the two property tests run.
///
/// The rotation domain has exactly four values and the permutation domain has 8! = 40 320,
/// so neither wants proptest's default 256: every case opens a document through PDFium, and
/// running the same four rotations two hundred times measures nothing new. The nightly
/// `proptest-deep` job raises this through `PROPTEST_CASES`.
const CASES: u32 = 24;

/// Rects may sit this far outside the page box before it counts as escaping it — the same
/// tolerance `oc_pdf::geom` normalises with, since this is float arithmetic on a box whose
/// corners came out of the file.
const INSIDE_PAGE_TOLERANCE_PT: f32 = 1.0;

/// Baselines closer together than this are one line of text.
///
/// A 12 pt body face leads at 14–16 pt, so anything within a point of another baseline is
/// the same line by a wide margin. The grouping below needs a threshold rather than equality
/// because the same glyph, extracted from the same page under two different `/Rotate`
/// values, arrives with a baseline that differs in the fourth decimal.
const SAME_LINE_PT: f32 = 1.0;

// ---------------------------------------------------------------------------
// 1.5 — rotation
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    /// `/Rotate` is a display instruction. It turns the page — and with it every glyph on
    /// the page — but it does not touch the content stream, so it cannot change which
    /// characters the page draws or the sequence they read in.
    ///
    /// **Measured, and not what the plan assumed.** PDFium's own character order is *not*
    /// rotation-invariant: at `/Rotate 90` its text page hands back the lines of `f01` in a
    /// different order, while 180 and 270 leave the order alone (`docs/DECISIONS_LOG.md`).
    /// So the invariant is asserted where it is actually true — over the characters put into
    /// reading order — and that is the stronger test anyway, because it also proves
    /// `PageGeometry::normalise` maps each rotation to the right axis: un-rotating the
    /// origins of a page whose rotation we had mapped wrongly would not line them up.
    ///
    /// The multiset is asserted too, separately and first. It is the conservation quantity
    /// (D13.4), it is what R1 §D.6 #1's bug destroyed, and a failure there means text was
    /// lost rather than merely reordered — worth telling apart in the failure message.
    #[test]
    fn prop_rotate_invariance_of_extracted_text(rotation in 0usize..ROTATIONS.len()) {
        let source = fixture("../../target/fixtures/f01_prose_single_column.pdf");
        let expected = extract_all(&source, ROTATIONS[0]);

        // Not a vacuous pass: `f01` is two pages of prose, so an extraction that returned
        // nothing would make both sides agree on nothing and prove nothing.
        prop_assert!(
            expected.iter().map(|page| page.reading_order.len()).sum::<usize>()
                > LINE_GLYPH_COUNT,
            "f01 extracted almost nothing, so the comparison below is empty"
        );

        let degrees = ROTATIONS[rotation];
        let rotated = oc_testkit::mutate::rotate(&source, degrees)
            .expect("f01 is a well-formed PDF");
        let got = extract_all(&rotated, degrees);

        prop_assert_eq!(
            got.iter().map(|page| page.sorted.clone()).collect::<Vec<_>>(),
            expected.iter().map(|page| page.sorted.clone()).collect::<Vec<_>>(),
            "/Rotate {} changed which characters the page contains",
            degrees
        );
        prop_assert_eq!(
            got.iter().map(|page| page.reading_order.clone()).collect::<Vec<_>>(),
            expected.iter().map(|page| page.reading_order.clone()).collect::<Vec<_>>(),
            "/Rotate {} changed the order they read in",
            degrees
        );
    }
}

// ---------------------------------------------------------------------------
// 1.6 — CropBox offset
// ---------------------------------------------------------------------------

/// A CropBox is a window on the page, not a change to it. Shifting it must leave every
/// character present and every rect inside the page box it now describes (R1 §D.6 #1).
///
/// All three routes to the offset are checked, because they fail differently. `h03` is built
/// with the offset from the start, so it catches a builder that never writes one. The
/// mutation applies it to `h01` after the fact, so it catches an extractor that measures from
/// the MediaBox where it should measure from the CropBox — the bug itself. And the committed
/// `h01__cropbox_offset.pdf` is what `cargo xtask mutations` wrote, so it catches a recipe
/// that has drifted from the file every other tool reads.
#[test]
fn cropbox_offset_does_not_lose_text() {
    let plain_bytes = fixture("../../corpus/fixtures/handmade/h01_two_glyphs.pdf");
    let plain = ingest(&plain_bytes);
    assert_eq!(text(&plain), "AB", "the unshifted baseline");

    let mutated =
        oc_testkit::mutate::cropbox_offset(&plain_bytes, CROP_SHIFT_PT.0, CROP_SHIFT_PT.1)
            .expect("h01 is a well-formed PDF");
    let built = fixture("../../corpus/fixtures/handmade/h03_cropbox_offset.pdf");
    let committed = fixture("../../corpus/fixtures/mutations/h01__cropbox_offset.pdf");

    for (label, bytes) in [
        ("h03, built offset", &built),
        ("h01, mutated", &mutated),
        ("h01__cropbox_offset.pdf", &committed),
    ] {
        let page = ingest(bytes);
        assert_eq!(text(&page), "AB", "{label} lost text");
        assert_eq!(page.c_raw, plain.c_raw, "{label} changed C_raw");
        assert_inside_page(label, &page, bytes);
    }
}

// ---------------------------------------------------------------------------
// 1.7 — content-stream operator order
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    /// Nothing obliges a producer to draw a line left to right. Justification, kerning
    /// corrections and font switching all reorder the operators, and a page that emits its
    /// glyphs back to front is valid PDF that renders identically.
    ///
    /// So extraction may not depend on the operator order: put into reading order, the
    /// characters must come back as the line reads, whichever order they were drawn in.
    #[test]
    fn prop_content_stream_reorder_invariance(order in permutation()) {
        let page = ingest(&oc_testkit::handmade::line_of_glyphs(&order));
        prop_assert_eq!(reading_order(placed(&page)), LINE_ALPHABET);
    }
}

/// A permutation of the glyph slots: which slot each successive drawing operator fills.
fn permutation() -> impl Strategy<Value = Vec<usize>> {
    Just((0..LINE_GLYPH_COUNT).collect::<Vec<usize>>()).prop_shuffle()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// One page, reduced to the two forms the rotation test compares.
struct PageText {
    /// Every character, in code-point order: the multiset, stated as a string.
    sorted: String,
    /// Every character, in reading order.
    reading_order: String,
}

fn fixture(relative: &str) -> Vec<u8> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "missing fixture {}: {error}; run `cargo run -p xtask -- fixtures` and \
             `cargo run -p xtask -- handmade-fixtures`",
            path.display()
        )
    })
}

fn ingest(bytes: &[u8]) -> PageGlyphs {
    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(bytes, None).expect("the fixture opens");
    document.page_glyphs(0).expect("the page extracts")
}

/// Every page of a document whose pages carry `/Rotate degrees`, reduced to comparable text.
fn extract_all(bytes: &[u8], degrees: i32) -> Vec<PageText> {
    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(bytes, None).expect("the fixture opens");
    (0..document.page_count())
        .map(|index| {
            let page = document.page_glyphs(index).expect("the page extracts");
            let geometry = document
                .page_geometry(index)
                .expect("the page has geometry");

            let mut characters: Vec<char> = page.glyphs.iter().map(|glyph| glyph.ch).collect();
            characters.sort_unstable();

            PageText {
                sorted: characters.into_iter().collect(),
                reading_order: reading_order(unrotated(&page, &geometry, degrees)),
            }
        })
        .collect()
}

/// The characters in the order they were drawn.
fn text(page: &PageGlyphs) -> String {
    page.glyphs.iter().map(|glyph| glyph.ch).collect()
}

/// Each glyph as `(x, y, ch)` in the page's own normalised space.
fn placed(page: &PageGlyphs) -> Vec<(f32, f32, char)> {
    page.glyphs
        .iter()
        .map(|glyph| (glyph.origin.0, glyph.origin.1, glyph.ch))
        .collect()
}

/// Each glyph as `(x, y, ch)`, mapped back out of the page's display rotation.
///
/// `PageGeometry::normalise` puts a glyph where a reader sees it, which for a rotated page
/// means a line of text runs down the screen rather than across it. Comparing two rotations
/// therefore needs one shared frame, and the natural one is the unrotated page: this undoes
/// the quarter-turns, so that all four rotations describe the same layout again.
///
/// The inverse of a clockwise quarter-turn on a `w × h` page — the page dimensions *after*
/// the turn, which is what the geometry reports — is what each arm below writes out.
fn unrotated(page: &PageGlyphs, geometry: &PageGeometry, degrees: i32) -> Vec<(f32, f32, char)> {
    let (w, h) = (geometry.width_pt(), geometry.height_pt());
    page.glyphs
        .iter()
        .map(|glyph| {
            let (x, y) = glyph.origin;
            let (x0, y0) = match degrees.rem_euclid(360) {
                90 => (y, w - x),
                180 => (w - x, h - y),
                270 => (h - y, x),
                _ => (x, y),
            };
            (x0, y0, glyph.ch)
        })
        .collect()
}

/// Positioned characters, read the way a page is read: down it, then across each line.
///
/// Lines are found by single-linkage grouping on the baseline rather than by rounding to a
/// grid, because rounding has an edge — two baselines a ten-thousandth of a point apart can
/// land in different buckets — and that edge is exactly where a rotated page's arithmetic
/// puts them. Grouping on the gap has no such boundary at any realistic leading.
fn reading_order(mut placed: Vec<(f32, f32, char)>) -> String {
    placed.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.total_cmp(&b.0)));

    let mut out = String::new();
    let mut line: Vec<(f32, f32, char)> = Vec::new();
    for item in placed {
        if line
            .last()
            .is_some_and(|last| (item.1 - last.1).abs() > SAME_LINE_PT)
        {
            flush_line(&mut line, &mut out);
        }
        line.push(item);
    }
    flush_line(&mut line, &mut out);
    out
}

fn flush_line(line: &mut Vec<(f32, f32, char)>, out: &mut String) {
    line.sort_by(|a, b| a.0.total_cmp(&b.0));
    out.extend(line.iter().map(|item| item.2));
    line.clear();
}

/// Every glyph's boxes lie inside the page the geometry describes.
fn assert_inside_page(label: &str, page: &PageGlyphs, bytes: &[u8]) {
    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(bytes, None).expect("the fixture opens");
    let geometry = document.page_geometry(0).expect("page 0 has geometry");
    let (width, height) = (geometry.width_pt(), geometry.height_pt());
    let tolerance = INSIDE_PAGE_TOLERANCE_PT;

    for glyph in &page.glyphs {
        for (which, rect) in [("tight", glyph.bbox), ("loose", glyph.loose_bbox)] {
            assert!(
                rect.x0 >= -tolerance
                    && rect.y0 >= -tolerance
                    && rect.x1 <= width + tolerance
                    && rect.y1 <= height + tolerance,
                "{label}: {which} box of {:?} is {rect:?}, outside the {width} x {height} page",
                glyph.ch
            );
        }
    }
}

/// `C_raw` is `C(·)` of what extraction produced, and `C(·)` has exactly one definition.
///
/// The backend used to build this histogram one glyph at a time. That is indistinguishable
/// from `c_of` until a document draws a base and a combining mark as two glyphs — and then
/// `C_raw` is in a different normal form from every other histogram in the pipeline, because
/// ARCHITECTURE §5.2 takes `C(·)` after canonical composition (amended 2026-09-20). Nothing
/// divides by `C_raw` today; `report.json` prints it and the retention ratio is one change
/// away from using it.
///
/// So the assertion is not "the number is right" but "it came from the one function".
#[test]
fn c_raw_is_c_of_the_extracted_text_and_not_a_second_count() {
    for name in [
        "../../target/fixtures/f01_prose_single_column.pdf",
        "../../target/fixtures/f04_german_prose.pdf",
        "../../target/fixtures/f05_turkish_prose.pdf",
        "../../corpus/fixtures/handmade/h01_two_glyphs.pdf",
    ] {
        let page = ingest(&fixture(name));
        let glyph_text: String = page.glyphs.iter().map(|glyph| glyph.ch).collect();

        assert_eq!(
            page.c_raw,
            oc_model::ledger::c_of(&glyph_text),
            "{name}: C_raw disagrees with C(·) of the glyphs it was built from"
        );
    }
}
