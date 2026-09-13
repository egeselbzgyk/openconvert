//! Rows 3.1 and 3.13 of the Phase 3 table: the two segmenters agree on ordinary prose, and
//! the stage that runs them changes no text at all.
//!
//! The agreement test is the phase's cheapest guard and the one that would catch the most:
//! Docstrum and Breuel's whitespace cover read the same page by different means, so a block
//! boundary both of them draw is evidence, and one only Docstrum draws is a flag. On a
//! single-column, first-line-indent book the two should not differ anywhere, which is what
//! makes zero flags on `f01` an assertion rather than a hope.

use oc_core::ledger_check::{check_invariants, ConservationError, ReasonTotals};
use oc_core::stages;
use oc_core::thresholds::T;
use oc_model::extract::{CharHistogram, PageRef};
use oc_model::lang::LangTag;
use oc_model::ledger::{LedgerDelta, LedgerEntry, Reason, StageKind};
use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;
use openconvert::pipeline::{furniture_stage, layout_stage, text_stage, LayoutStage, PageInput};

/// Read a fixture and run `text`, `furniture` and `layout` over it, as a conversion would.
fn layout_of(relative: &str) -> LayoutStage {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "missing fixture {}: {error}; run `cargo run -p xtask -- fixtures`",
            path.display()
        )
    });
    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(&bytes, None).expect("the fixture opens");
    let input: Vec<PageInput> = (0..document.page_count())
        .map(|index| {
            let geometry = document
                .page_geometry(index)
                .expect("the page has geometry");
            PageInput {
                page: PageRef::new(index),
                width_pt: geometry.width_pt(),
                height_pt: geometry.height_pt(),
                glyphs: document
                    .page_glyphs(index)
                    .expect("the page extracts")
                    .glyphs,
            }
        })
        .collect();

    let mut totals = ReasonTotals::default();
    let text = text_stage(&input, &mut totals, &T).expect("text conserves");
    let furniture =
        furniture_stage(&text, LangTag::EN, &mut totals, &T).expect("furniture stays in budget");
    layout_stage(&text, &furniture, &mut totals, &T).expect("layout conserves")
}

/// Row 3.1. Every block Docstrum drew is a block the whitespace cover also drew, to within
/// `layout.block.agreement_iou_min`, and no block on the fixture is flagged.
#[test]
fn blocks_docstrum_and_whitespace_agree_on_f01() {
    let layout = layout_of("../../target/fixtures/f01_prose_single_column.pdf");

    assert!(
        !layout.blocks.is_empty(),
        "the fixture has pages to segment"
    );
    for (index, agreement) in layout.agreement.iter().enumerate() {
        for (block, iou) in agreement.iou.iter().enumerate() {
            assert!(
                *iou >= T.layout.block.agreement_iou_min as f32,
                "page {index} block {block}: IoU {iou} below the agreement floor"
            );
        }
        assert_eq!(
            agreement.low_confidence_count(),
            0,
            "page {index} carries low-confidence blocks: {:?}",
            agreement.iou
        );
    }
}

/// Row 3.13, first half. The stage is Conserving and behaves that way on a real document:
/// no ledger entry, and the checker agrees the text came through whole.
#[test]
fn layout_stage_is_conserving() {
    let layout = layout_of("../../target/fixtures/f01_prose_single_column.pdf");

    assert!(
        layout.delta.is_empty(),
        "layout ledgered something: {:?}",
        layout.delta.entries()
    );
    assert_eq!(layout.check.kind, StageKind::Conserving);
    assert_eq!(layout.check.removed_chars, 0);
    assert_eq!(layout.check.added_chars, 0);
}

/// Row 3.13, second half. "Conserving" is enforced, not documented: a `layout` that ledgered
/// a removal is an error even if the arithmetic would otherwise balance.
#[test]
fn layout_stage_conservation_violation_errors() {
    let mut before = CharHistogram::new();
    for ch in "kept".chars() {
        before.add(ch);
    }
    let mut after = CharHistogram::new();
    for ch in "kep".chars() {
        after.add(ch);
    }
    let delta = LedgerDelta::new(vec![LedgerEntry::removed(
        stages::LAYOUT.name,
        Reason::DecorativeGlyph,
        0,
        (3, 4),
        "t".to_owned(),
    )]);
    let mut totals = ReasonTotals::new(&before);

    let error = check_invariants(&before, &after, &delta, stages::LAYOUT, &mut totals)
        .expect_err("a Conserving stage that removed a character must fail");
    assert!(
        matches!(
            error,
            ConservationError::ConservingStageMutated {
                stage: "layout",
                ..
            }
        ),
        "unexpected error: {error}"
    );
}
