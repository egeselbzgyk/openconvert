//! Row 2.22 of the Phase 2 table: the two `broken_text` signals, both firing.
//!
//! `inspect` calls a page `broken_text` on either of two signals (D13.10): the share of
//! characters that decode to nothing, and the share of words a dictionary recognises. They
//! are worth having as a pair only if they are genuinely independent — one reads the glyph
//! stream, the other reads assembled words against a frequency list — and Phase 1 could only
//! ever exercise the first, because the second had no list to read.
//!
//! `f01__strip_tounicode.pdf` is `f01` with every `/ToUnicode` CMap deleted: the same page,
//! the same glyphs, and nothing to map their codes to Unicode with. It is the canonical
//! failure this classification exists for, and both signals must see it.

use oc_core::thresholds::T;
use oc_model::extract::PageRef;
use oc_model::lang::LangTag;
use oc_pdf::classify::{classify_page, PageClass};
use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;
use oc_text::freq::dict_hit_rate;
use oc_text::lines::assemble_lines;
use oc_text::words::assemble_runs;

/// Extract page zero, assemble its text, and answer both signals for it.
fn signals(relative: &str) -> (f32, Option<f32>, PageClass) {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "missing fixture {}: {error}; run `cargo run -p xtask -- fixtures` and \
             `cargo run -p xtask -- mutations`",
            path.display()
        )
    });
    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(&bytes, None).expect("the fixture opens");
    let page = document.page_glyphs(0).expect("the page extracts");

    let assembly = assemble_runs(&page.glyphs, PageRef::new(0), &T);
    let lines = assemble_lines(&assembly.runs, &T);
    let text: String = lines
        .iter()
        .map(|line| {
            line.runs
                .iter()
                .filter_map(|id| assembly.runs.get(id.0 as usize))
                .map(|run| run.text.as_str())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    let hit_rate = dict_hit_rate(&text, &LangTag::EN);
    let undecodable = f32::from(
        u16::try_from(page.stats.replacement + page.stats.pua + page.stats.control)
            .unwrap_or(u16::MAX),
    );
    let share = undecodable / page.stats.visible.max(1) as f32;
    let (class, _) = classify_page(&page.stats, &Default::default(), hit_rate, &T);
    (share, hit_rate, class)
}

#[test]
fn dict_hit_rate_feeds_broken_text_classification() {
    let (share, hit_rate, class) =
        signals("../../corpus/fixtures/mutations/f01__strip_tounicode.pdf");

    // Signal one: characters that decode to nothing.
    assert!(
        share >= T.pageclass.broken_text_replacement_share as f32,
        "undecodable share was {share}"
    );
    // Signal two, and the one this row exists for: words that spell nothing.
    let hit_rate = hit_rate.expect("English has a frequency list, so the rate is measurable");
    assert!(
        hit_rate < T.pageclass.broken_text_dict_hit_min as f32,
        "dictionary hit rate was {hit_rate}"
    );
    assert_eq!(class, PageClass::BrokenText);
}

#[test]
fn the_unmutated_fixture_passes_both_signals() {
    // The other half of the claim: a signal that fires on everything is not a signal.
    let (share, hit_rate, class) = signals("../../target/fixtures/f01_prose_single_column.pdf");

    assert!(
        share < T.pageclass.broken_text_replacement_share as f32,
        "undecodable share was {share}"
    );
    let hit_rate = hit_rate.expect("English has a frequency list");
    assert!(
        hit_rate >= T.pageclass.broken_text_dict_hit_min as f32,
        "dictionary hit rate was {hit_rate}"
    );
    assert_eq!(class, PageClass::Text);
}
