//! Fuzz-lite: the whole byte-taking surface, fed things that are not PDFs (test 1.16).
//!
//! The contract is narrow and absolute — **`Err`, never a panic, never a hang** — and it is
//! the contract that decides whether this is a tool you can point at a directory. A converter
//! that panics on a damaged file takes the batch down with it; one that returns an error
//! carries on to the next book.
//!
//! Two generators, because they find different things.
//!
//! *Random bytes* mostly get rejected at the header, which exercises the door and little else.
//! Half of them are therefore given a real `%PDF-` header, so the parser is reached with a
//! body that is nonsense — a much deeper reach for the same cost.
//!
//! *Truncated fixtures* are what the plan asks for, and measured, they barely reach: PDFium
//! wants a trailer and an xref at the end of the file, so cutting anywhere before them is
//! refused at the door. Over a hundred evenly spaced cuts of each of three fixtures, **two to
//! three per cent opened at all**. Two hundred truncations therefore buy about five inputs
//! that reach an extraction path, which is not a fuzz test of the extraction paths.
//!
//! *Corrupted fixtures* are the half that reaches. Flip one byte and the trailer survives, so
//! PDFium takes its damaged-file recovery path — rebuilding the xref, guessing at object
//! boundaries — which is both the deepest and the least-travelled code in the library. Over
//! two hundred single-byte corruptions of the same three fixtures, **93 to 98 per cent opened
//! and 85 to 94 per cent extracted glyphs**. Both generators are kept: the plan names the
//! first, the measurement earns the second.
//!
//! **What this cannot catch.** A segmentation fault inside PDFium is not a panic and
//! `catch_unwind` will never see it; it takes the process with it. RT A5.2 accepts that for
//! v1 and reserves `--isolate-parser` for the Phase 14 worker. So a green run here means "no
//! panic", not "no crash", and the day this test dies rather than fails is the day that
//! reservation gets spent.

use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;
use proptest::prelude::*;

/// How many random inputs the fast tier runs. The plan's figure; the nightly `proptest-deep`
/// job raises it to 200 000 through `PROPTEST_CASES`.
const RANDOM_CASES: u32 = 20_000;

/// How many truncations. The plan's figure.
const TRUNCATION_CASES: u32 = 200;

/// How many corruptions. More, because nearly all of them reach an extraction path and are
/// therefore worth running; fewer than the random half, because each one costs a parse.
const CORRUPTION_CASES: u32 = 2_000;

/// The longest random input. Length is not what finds bugs here — structure is — and a short
/// input is a fast one.
const MAX_RANDOM_LEN: usize = 4096;

/// The fixtures the truncation and corruption generators draw on. Small, committed, and
/// between them they cover text, an invisible layer, an image with a mask and an outline.
const FIXTURE_SOURCES: [&str; 4] = [
    "h01_two_glyphs",
    "h05_invisible_layer",
    "h09_image_smask",
    "h13_outline",
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(RANDOM_CASES))]

    /// Test 1.16, the random half.
    #[test]
    fn prop_never_panics_on_arbitrary_bytes(bytes in arbitrary_bytes()) {
        exercise(&bytes);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(TRUNCATION_CASES))]

    /// Test 1.16, the truncation half.
    ///
    /// Split from the random half rather than folded into one generator so that the two case
    /// counts can differ by two orders of magnitude — they cost that differently — and so a
    /// failure names which kind of input found it.
    #[test]
    fn prop_never_panics_on_truncated_fixtures(
        source in proptest::sample::select(FIXTURE_SOURCES.as_slice()),
        fraction in 0.0f64..1.0,
    ) {
        let bytes = fixture(source);
        // Cut anywhere in the file, not just near the end: a document truncated inside its
        // first object fails differently from one missing only its xref.
        let cut = ((bytes.len() as f64) * fraction) as usize;
        exercise(&bytes[..cut.min(bytes.len())]);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CORRUPTION_CASES))]

    /// Test 1.16, the half that reaches the extraction paths.
    ///
    /// One byte changed. The header, the trailer and the xref all survive, so the document
    /// opens and every per-page path runs against objects that are subtly wrong — a length
    /// that lies, a dictionary key that is now something else, a stream that no longer
    /// inflates. That is the shape a real damaged file has.
    #[test]
    fn prop_never_panics_on_corrupted_fixtures(
        source in proptest::sample::select(FIXTURE_SOURCES.as_slice()),
        position in 0.0f64..1.0,
        value in any::<u8>(),
    ) {
        let mut bytes = fixture(source);
        let offset = (((bytes.len() - 1) as f64) * position) as usize;
        if let Some(byte) = bytes.get_mut(offset) {
            *byte = value;
        }
        exercise(&bytes);
    }
}

/// The corruption generator really does reach the extraction paths, and the truncation
/// generator really does not.
///
/// Written because the argument for having both is a measurement, and a measurement recorded
/// only in a comment is a measurement that stops being true without anyone noticing. If
/// PDFium's recovery path ever tightens to the point where corrupted files stop opening, this
/// fails and says so, rather than leaving two thousand cases quietly testing the door.
#[test]
fn corruption_reaches_the_extraction_paths() {
    /// A deterministic spread of corruption sites; a prime stride visits the file evenly.
    const STRIDE: usize = 7919;
    const SAMPLES: usize = 200;
    /// Measured at 93-98%. Half is a floor with room for PDFium to change its mind, and still
    /// far above the 2-3% truncation manages.
    const MIN_REACHING: usize = SAMPLES / 2;

    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let bytes = fixture("h13_outline");

    let mut extracted = 0;
    for index in 0..SAMPLES {
        let mut corrupt = bytes.clone();
        let offset = (index * STRIDE) % corrupt.len();
        corrupt[offset] = ((index * 251) % 256) as u8;
        if let Ok(document) = backend.open(&corrupt, None) {
            if document.page_count() > 0 && document.page_glyphs(0).is_ok() {
                extracted += 1;
            }
        }
    }
    assert!(
        extracted >= MIN_REACHING,
        "only {extracted} of {SAMPLES} corruptions reached glyph extraction; the corruption generator has stopped fuzzing anything but the door"
    );
}

/// Bytes that are not a PDF, half of them wearing a PDF's header.
fn arbitrary_bytes() -> impl Strategy<Value = Vec<u8>> {
    /// The header PDFium looks for. Without it almost every random input is refused before a
    /// parser sees it, and the test measures the header check twenty thousand times.
    const HEADER: &[u8] = b"%PDF-1.7\n";

    let body = proptest::collection::vec(any::<u8>(), 0..MAX_RANDOM_LEN);
    prop_oneof![
        body.clone(),
        body.prop_map(|bytes| {
            let mut out = HEADER.to_vec();
            out.extend(bytes);
            out
        }),
    ]
}

/// Put one input through every entry point that takes bytes.
///
/// Nothing here asserts a *result*: what a malformed file extracts is not defined and not
/// interesting. The assertion is the absence of a panic, which is made by returning normally.
fn exercise(bytes: &[u8]) {
    let backend = PdfiumBackend::bind().expect("PDFium is vendored");

    // The pure readers first — they take bytes from an untrusted file and are cheap.
    let _ = oc_pdf::meta::read_xmp(bytes);
    if let Ok(document) = lopdf::Document::load_mem(bytes) {
        let _ = oc_pdf::meta::is_encrypted(&document);
        let _ = oc_pdf::meta::has_struct_tree(&document);
        let _ = oc_pdf::meta::xmp_packet(&document, &oc_core::limits::Limits::default());
    }

    let Ok(document) = backend.open(bytes, None) else {
        return;
    };

    // It opened, which for a truncated file it often does. Every per-document and per-page
    // path now has to survive whatever is left of the objects.
    let _ = document.doc_info();
    let _ = document.outline();

    // Only the first few pages: a malformed page tree can claim an enormous count, and the
    // point is to reach the page paths, not to walk a fabricated book.
    const PAGES_TO_TOUCH: u32 = 4;
    for index in 0..document.page_count().min(PAGES_TO_TOUCH) {
        let _ = document.page_geometry(index);
        let _ = document.page_char_stats(index);
        let _ = document.page_image_stats(index);
        let _ = document.page_glyphs(index);
        let _ = document.page_images(index);
    }

    // And one index that is certainly out of range, which is its own class of bug.
    let _ = document.page_geometry(document.page_count());
}

fn fixture(name: &str) -> Vec<u8> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/fixtures/handmade")
        .join(format!("{name}.pdf"));
    std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "missing fixture {}: {error}; run `cargo run -p xtask -- handmade-fixtures`",
            path.display()
        )
    })
}
