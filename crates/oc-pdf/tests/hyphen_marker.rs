//! PDFium's line-break hyphen marker, decoded at extraction (Phase 2 carried debt).
//!
//! PDFium does not report the hyphen a producer draws at a line break as a hyphen. It
//! reports **U+0002**, a control character, and sets `is_hyphen()` on it. Phase 1 measured
//! this against `pdftotext` (item 1.11) and left it standing, because Phase 1's job was to
//! report what the backend says. It cannot stand into Phase 2: `C_raw` is supposed to be
//! "the multiset of scalars the document contains", and a document contains no U+0002 —
//! nor does any reader see one. Every run text built from those glyphs would carry a
//! control character into the EPUB.
//!
//! Decoding the marker is extraction's job, not a transformation of the text, so it happens
//! in the backend and never reaches the ledger. What the decode cannot recover is whether
//! the source encoded that hyphen as U+002D or as U+00AD: PDFium collapses both to the same
//! marker. It resolves to U+002D, which is what the page prints either way; the soft/hard
//! distinction is a dehyphenation input and stays open (see PROGRESS.md).

use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;

fn fixture(relative: &str) -> Vec<u8> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "missing fixture {}: {error}; run `cargo run -p xtask -- fixtures`",
            path.display()
        )
    })
}

/// Every glyph of every page of a fixture.
fn all_glyphs(relative: &str) -> Vec<oc_model::extract::Glyph> {
    let bytes = fixture(relative);
    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(&bytes, None).expect("the fixture opens");
    (0..document.page_count())
        .flat_map(|index| {
            document
                .page_glyphs(index)
                .expect("the page extracts")
                .glyphs
        })
        .collect()
}

#[test]
fn a_line_break_hyphen_is_a_hyphen_not_a_control_character() {
    // f02 breaks `projec-tion` and `reading-order` across lines; the doc comment on
    // `is_undecodable_control` records both, measured in Phase 1.
    let glyphs = all_glyphs("../../target/fixtures/f02_two_column.pdf");
    let flagged: Vec<char> = glyphs
        .iter()
        .filter(|glyph| glyph.hyphen_flag)
        .map(|glyph| glyph.ch)
        .collect();

    assert!(
        flagged.len() >= 2,
        "f02 should carry at least two line-break hyphens, found {flagged:?}"
    );
    assert!(
        flagged.iter().all(|ch| *ch == '-'),
        "a flagged hyphen must arrive as U+002D, found {flagged:?}"
    );
}

#[test]
fn no_extracted_glyph_is_an_undecodable_control() {
    for name in [
        "../../target/fixtures/f01_prose_single_column.pdf",
        "../../target/fixtures/f02_two_column.pdf",
    ] {
        for glyph in all_glyphs(name) {
            assert!(
                !glyph.ch.is_control(),
                "{name} yielded U+{:04X}, a control character, at {:?}",
                u32::from(glyph.ch),
                glyph.origin
            );
        }
    }
}

#[test]
fn c_raw_counts_the_hyphen_the_page_prints() {
    let bytes = fixture("../../target/fixtures/f02_two_column.pdf");
    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(&bytes, None).expect("the fixture opens");
    let page = document.page_glyphs(0).expect("the page extracts");
    assert_eq!(
        page.c_raw.count('\u{0002}'),
        0,
        "C_raw must not contain PDFium's marker"
    );
    assert!(
        page.c_raw.count('-') > 0,
        "C_raw must contain the hyphens the page prints"
    );
}
