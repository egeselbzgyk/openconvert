//! Row 2.19 of the Phase 2 table: `dc:language` on the three languages v1 claims.
//!
//! One page of ordinary prose each, through extraction, assembly and `whatlang`. The
//! fixtures are `f01` (English), `f04` (German) and `f05` (Turkish); the German and Turkish
//! pages were written for this test rather than quoted, so nothing third-party is
//! redistributed, and they are ordinary prose rather than sentences chosen to be easy — the
//! umlauts and the ß on one, the vowel harmony and the agglutinated suffixes on the other,
//! which is what a trigram model actually keys on.

use oc_core::thresholds::T;
use oc_model::extract::PageRef;
use oc_model::lang::LangTag;
use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;
use oc_text::lang::detect_document;
use oc_text::lines::assemble_lines;
use oc_text::words::assemble_runs;

/// Every page of a fixture, assembled into one string of body text.
fn body_text(relative: &str) -> String {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "missing fixture {}: {error}; run `cargo run -p xtask -- fixtures`",
            path.display()
        )
    });
    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(&bytes, None).expect("the fixture opens");

    let mut out = String::new();
    for index in 0..document.page_count() {
        let page = document.page_glyphs(index).expect("the page extracts");
        let assembly = assemble_runs(&page.glyphs, PageRef::new(index), &T);
        for line in assemble_lines(&assembly.runs, &T) {
            for id in &line.runs {
                if let Some(run) = assembly.runs.get(id.0 as usize) {
                    out.push_str(&run.text);
                }
            }
            out.push('\n');
        }
    }
    out
}

#[test]
fn language_detected_en_de_tr() {
    for (fixture, expected) in [
        (
            "../../target/fixtures/f01_prose_single_column.pdf",
            LangTag::EN,
        ),
        ("../../target/fixtures/f04_german_prose.pdf", LangTag::DE),
        ("../../target/fixtures/f05_turkish_prose.pdf", LangTag::TR),
    ] {
        let text = body_text(fixture);
        let verdict = detect_document(&text, LangTag::EN);
        assert_eq!(verdict.lang, expected, "{fixture}: {verdict:?}");
        assert!(!verdict.fell_back, "{fixture} should not need the fallback");
        // Two letters, not three: `whatlang` speaks ISO 639-3 and `dc:language` wants
        // BCP-47, and an `eng` in the package is an EPUBCheck failure at the end of a
        // conversion.
        assert_eq!(verdict.lang.as_str().chars().count(), 2);
    }
}

#[test]
fn the_turkish_page_keeps_its_dotted_and_dotless_i() {
    // The reason Turkish is a fixture and not a line in a unit test: `ı`, `İ`, `ğ` and `ş`
    // have to survive extraction as themselves. If they arrive folded, transliterated or
    // replaced, `whatlang` may still say Turkish and every lookup key in the pipeline is
    // wrong.
    let text = body_text("../../target/fixtures/f05_turkish_prose.pdf");
    for ch in ['ı', 'İ', 'ğ', 'ş', 'ç', 'ö', 'ü'] {
        assert!(text.contains(ch), "the page lost {ch:?}");
    }
    assert!(!text.contains('\u{FFFD}'), "something failed to decode");
}
