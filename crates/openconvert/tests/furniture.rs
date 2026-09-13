//! Furniture detection over real PDFs — rows 2.10 to 2.14 of the Phase 2 table.
//!
//! Running heads, feet and page numbers are the highest-confidence verdict in the whole
//! matrix (R10 §6.6: a CPU-only deterministic pipeline scores 92.8 on this category, a 3B VLM
//! 32.1), and also the top text-loss risk: Calibre's blunt pixel rule eats the only line on an
//! atypical page and Marker deletes body text outright (R1 §C.2 #7, §C.3). Every one of these
//! five tests is a case where the naive rule deletes something it should not.

use oc_core::thresholds::T;
use oc_layout::furniture::{apply_furniture, detect_furniture, PageLines};
use oc_model::extract::PageRef;
use oc_model::lang::LangTag;
use oc_model::ledger::Reason;
use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;
use oc_testkit::handmade::{CONSTANT_BAND_NUMBER, ONE_OFF_HEAD, RECTO_HEAD, VERSO_HEAD};
use oc_text::lines::assemble_lines;
use oc_text::words::assemble_runs;

/// Read a document, assemble every page, and describe it the way `furniture` wants it.
fn page_lines(path: &str) -> Vec<PageLines> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "missing fixture {}: {error}; run `cargo run -p xtask -- fixtures` and \
             `cargo run -p xtask -- handmade-fixtures`",
            path.display()
        )
    });
    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(&bytes, None).expect("the fixture opens");
    (0..document.page_count())
        .map(|index| {
            let glyphs = document.page_glyphs(index).expect("the page extracts");
            let geometry = document
                .page_geometry(index)
                .expect("the page has geometry");
            let assembly = assemble_runs(&glyphs.glyphs, PageRef::new(index), &T);
            let lines = assemble_lines(&assembly.runs, &T);
            PageLines::from_runs(
                PageRef::new(index),
                geometry.height_pt(),
                &assembly.runs,
                &lines,
            )
        })
        .collect()
}

/// Every line that survived, page by page.
fn surviving_text(pages: &[PageLines]) -> Vec<Vec<String>> {
    pages
        .iter()
        .map(|page| page.lines.iter().map(|line| line.text.clone()).collect())
        .collect()
}

fn run(path: &str) -> (Vec<PageLines>, oc_layout::furniture::FurnitureOutcome) {
    let pages = page_lines(path);
    let verdicts = detect_furniture(&pages, LangTag::EN, &T);
    let outcome = apply_furniture(&pages, &verdicts);
    (pages, outcome)
}

#[test]
fn furniture_removes_repeating_header_f01() {
    let (_, outcome) = run("../../target/fixtures/f01_prose_single_column.pdf");

    // The assertion f01.assert.json already carries: `{"kind":"text_absent",
    // "text":"The Test Book"}`, "running header removed on both pages".
    let flow = surviving_text(&outcome.pages).concat().join("\n");
    assert!(
        !flow.contains("The Test Book"),
        "the running header survived:\n{flow}"
    );
    assert!(
        flow.contains("It was a dark and stormy night"),
        "the body did not:\n{flow}"
    );

    let headers: Vec<&str> = outcome
        .delta
        .entries()
        .iter()
        .filter(|entry| entry.reason == Reason::RunningHeader)
        .map(|entry| entry.text.as_str())
        .collect();
    assert_eq!(
        headers,
        vec!["The Test Book", "The Test Book"],
        "{headers:?}"
    );
}

#[test]
fn furniture_detects_page_numbers_by_progression() {
    let (_, outcome) = run("../../target/fixtures/f01_prose_single_column.pdf");

    let numbers: Vec<&str> = outcome
        .delta
        .entries()
        .iter()
        .filter(|entry| entry.reason == Reason::PageNumber)
        .map(|entry| entry.text.as_str())
        .collect();
    assert_eq!(numbers, vec!["1", "2"], "{numbers:?}");

    // The value is not lost, it moves: `PageRef.label` becomes the `page-list` nav target,
    // and nav text is outside `C`, which is what makes this a clean removal rather than a
    // paradox (ARCHITECTURE §5.2).
    assert_eq!(
        outcome.labels,
        vec![Some("1".to_owned()), Some("2".to_owned())]
    );
}

#[test]
fn furniture_keeps_chapter_number_that_is_not_a_progression() {
    let (_, outcome) = run("../../corpus/fixtures/handmade/h19_constant_band_number.pdf");

    for (index, page) in surviving_text(&outcome.pages).iter().enumerate() {
        assert!(
            page.iter().any(|line| line == CONSTANT_BAND_NUMBER),
            "page {index} lost its chapter number: {page:?}"
        );
    }
    assert!(
        outcome.delta.is_empty(),
        "nothing should have been removed: {:?}",
        outcome.delta.entries()
    );
    assert!(outcome.labels.iter().all(Option::is_none));
}

#[test]
fn furniture_respects_parity() {
    let (_, outcome) = run("../../corpus/fixtures/handmade/h20_recto_verso.pdf");

    let flow = surviving_text(&outcome.pages);
    let joined = flow.concat().join("\n");
    assert!(
        !joined.contains(VERSO_HEAD),
        "verso head survived:\n{joined}"
    );
    assert!(
        !joined.contains(RECTO_HEAD),
        "recto head survived:\n{joined}"
    );

    // Three of six pages is a ratio of 0.5 — inside the grey zone, where a parity-blind
    // detector abstains and keeps both heads. Split by parity each is three of three.
    let removed: Vec<&str> = outcome
        .delta
        .entries()
        .iter()
        .map(|entry| entry.text.as_str())
        .collect();
    assert_eq!(removed.iter().filter(|t| **t == VERSO_HEAD).count(), 3);
    assert_eq!(removed.iter().filter(|t| **t == RECTO_HEAD).count(), 3);

    // The band line that appears once is not furniture, whichever way it is counted.
    assert!(
        joined.contains(ONE_OFF_HEAD),
        "the one-off band line was deleted:\n{joined}"
    );
    for page in &flow {
        assert!(
            page.iter().any(|line| line.starts_with("Body text page")),
            "a page lost its body: {page:?}"
        );
    }
}

#[test]
fn furniture_never_removes_sole_page_content() {
    let (pages, outcome) = run("../../corpus/fixtures/handmade/h21_band_is_sole_content.pdf");
    let flow = surviving_text(&outcome.pages);

    // The head goes from the three pages that have a body …
    for page in flow.iter().take(3) {
        assert!(!page.iter().any(|line| line == VERSO_HEAD), "{page:?}");
    }
    // … and stays on the page that has nothing else.
    let last = flow.last().expect("four pages");
    assert_eq!(last, &vec![VERSO_HEAD.to_owned()]);
    assert_eq!(pages.len(), 4);

    // No page was emptied.
    assert!(flow.iter().all(|page| !page.is_empty()), "{flow:?}");
}
