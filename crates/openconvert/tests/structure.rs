//! Phase 4: what `structure` makes of a laid-out book.
//!
//! Row 4.3 is here rather than in `oc-structure` because it is a claim about a *real book*:
//! that the character-weighted mode of `f01`'s styles is its body text and that exactly one
//! other style is a heading candidate. The unit tests in `oc-structure` pin the histogram's
//! behaviour on constructed input; this one pins it on a document Typst set.

use oc_core::ledger_check::ReasonTotals;
use oc_core::thresholds::T;
use oc_model::lang::LangTag;
use oc_model::text::Run;
use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;
use oc_structure::headings::cluster::cluster_styles;
use openconvert::pipeline::{body_runs, furniture_stage, text_stage, TextStage};

/// Read a fixture and run `text` and `furniture` over it, returning the body flow's runs
/// alongside the document's font table.
fn body_of(relative: &str) -> (Vec<Run>, TextStage) {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "missing fixture {}: {error}; run `cargo run -p xtask -- fixtures`",
            path.display()
        )
    });
    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(&bytes, None).expect("the fixture opens");
    let input = openconvert::input::page_inputs(document.as_ref()).expect("every page extracts");

    let mut totals = ReasonTotals::default();
    let text = text_stage(&input, &mut totals, &T).expect("text conserves");
    let furniture =
        furniture_stage(&text, LangTag::EN, &mut totals, &T).expect("furniture stays in budget");
    (body_runs(&text, &furniture), text)
}

/// Row 4.3. `f01` is 10 pt Libertinus body with one 1.4 em bold `#heading(level: 1)`.
///
/// Two assertions, and the second is the one that matters: a clustering that found *two*
/// heading candidates on a document with one heading would put a spurious level into every
/// book set with a title page, and one that found none would throw the fast path's anchor
/// away.
#[test]
fn style_clusters_identify_body_mode() {
    let (runs, text) = body_of("../../target/fixtures/f01_prose_single_column.pdf");
    let inventory = cluster_styles(&runs, &text.fonts, &T);

    assert!(
        inventory.valid,
        "an ordinary prose book has a valid inventory: {:?}",
        inventory.warnings
    );
    let body = inventory.body_cluster().expect("f01 has body text");
    assert!(
        f64::from(body.char_share(inventory.total_chars)) >= T.inventory.min_body_char_share,
        "body held {} of {} characters",
        body.char_count,
        inventory.total_chars
    );

    let candidates = inventory.candidates(&T);
    assert_eq!(
        candidates.len(),
        1,
        "f01 sets exactly one style above body: {:?}",
        inventory
            .clusters
            .iter()
            .map(|c| (c.id, c.size_pt, c.weight, c.char_count))
            .collect::<Vec<_>>()
    );

    // And that style is the one "Chapter 3" is set in.
    let heading = inventory
        .clusters
        .iter()
        .find(|cluster| Some(cluster.id) == candidates.first().copied())
        .expect("the candidate is a cluster");
    assert!(
        heading.examples.iter().any(|text| text.contains("Chapter")),
        "the candidate cluster's examples were {:?}",
        heading.examples
    );
    assert!(heading.size_pt > body.size_pt);
}
