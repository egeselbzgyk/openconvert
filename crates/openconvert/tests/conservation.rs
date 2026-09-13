//! Row 2.15 of the Phase 2 table: the conservation law across the two stages that break it.
//!
//! `text` and `furniture` are the first two `Budgeted` stages in the pipeline, and between
//! them they exercise every shape invariant I-1 has to cover: a removal with nothing added
//! (a soft hyphen), an addition paired with a removal under one reason (a ligature), a
//! removal of whole lines (furniture), and whitespace appearing from nowhere (an inferred
//! space) which must **not** appear in the ledger because `C` does not count it.
//!
//! Two halves, as the plan's table asks. The fixtures run the real checker, so I-2, I-3 and
//! I-4 are checked with them. The property half generates two hundred documents and asserts
//! I-1 directly: a random page of glyphs can easily be four-fifths running head, and I-4
//! failing on a document nobody would ever convert would say nothing about conservation.

use oc_core::ledger_check::{c_of, ConservationError, ReasonTotals};
use oc_core::thresholds::T;
use oc_model::extract::{CharHistogram, FontId, Glyph, PageRef};
use oc_model::geom::Rect;
use oc_model::lang::LangTag;
use oc_model::ledger::LedgerDelta;
use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;
use openconvert::pipeline::{
    furniture_stage, glyph_chars, line_chars, run_chars, text_stage, PageInput,
};
use proptest::prelude::*;

fn read(path: &str) -> Vec<PageInput> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("missing fixture {}: {error}", path.display()));
    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(&bytes, None).expect("the fixture opens");
    (0..document.page_count())
        .map(|index| PageInput {
            page: PageRef::new(index),
            width_pt: document
                .page_geometry(index)
                .expect("the page has geometry")
                .width_pt(),
            height_pt: document
                .page_geometry(index)
                .expect("the page has geometry")
                .height_pt(),
            glyphs: document
                .page_glyphs(index)
                .expect("the page extracts")
                .glyphs,
            images: document.page_images(index).unwrap_or_default(),
        })
        .collect()
}

/// I-1, stated the way ARCHITECTURE §5.4 states it.
fn balances(before: &CharHistogram, after: &CharHistogram, delta: &LedgerDelta) -> bool {
    before.union(&delta.added()) == after.union(&delta.removed())
}

#[test]
fn conservation_i1_holds_across_text_and_furniture() {
    for fixture in [
        "../../target/fixtures/f01_prose_single_column.pdf",
        "../../target/fixtures/f02_two_column.pdf",
    ] {
        let input = read(fixture);
        let mut totals = ReasonTotals::default();

        let before_text = glyph_chars(&input);
        let text = text_stage(&input, &mut totals, &T)
            .unwrap_or_else(|error| panic!("{fixture}: text broke the law: {error}"));
        assert!(
            balances(&before_text, &run_chars(&text.pages), &text.delta),
            "{fixture}: text does not balance"
        );

        let before_furniture = run_chars(&text.pages);
        let furniture = furniture_stage(&text, LangTag::EN, &mut totals, &T)
            .unwrap_or_else(|error| panic!("{fixture}: furniture broke the law: {error}"));
        assert!(
            balances(
                &before_furniture,
                &line_chars(&furniture.pages),
                &furniture.delta
            ),
            "{fixture}: furniture does not balance"
        );

        // And the stages did something, so the assertion is not vacuous.
        assert!(text.c_0.total() > 0, "{fixture}: no text survived");
        assert!(
            !furniture.delta.is_empty(),
            "{fixture}: no furniture was found, so nothing was checked"
        );
    }
}

#[test]
fn a_ligature_reaches_the_flow_expanded_and_balanced() {
    // h04 encodes `ﬁn` properly: byte 200 with a `/ToUnicode` CMap saying U+FB01.
    //
    // PDFium expands it anyway. R2 §B.8 says it does not — "U+FB00–FB06 arrive in the glyph
    // stream and must be mapped explicitly" — and on `chromium/7881` the glyph stream carries
    // `f`, `i`, `n`, with the CMap in the file and read. Measured, recorded in
    // docs/DECISIONS_LOG.md, and it does not change what this stage must do: `N` keeps its
    // ligature table, because the contract is about the text, not about which component of
    // the pipeline happened to expand it.
    //
    // So this test asserts the part that holds either way, which is the part that matters:
    // the word arrives whole and the ledger balances, whether one entity or two did the work.
    let input = read("../../corpus/fixtures/handmade/h04_ligature_fi.pdf");
    let mut totals = ReasonTotals::default();
    let before = glyph_chars(&input);
    let text = text_stage(&input, &mut totals, &T).expect("h04 conserves");

    assert!(balances(&before, &run_chars(&text.pages), &text.delta));
    assert_eq!(text.check.removed_chars, text.check.added_chars.min(1));
    let flow: String = text
        .pages
        .iter()
        .flat_map(|page| page.runs.iter())
        .map(|run| run.text.as_str())
        .collect();
    assert_eq!(flow, "fin");
    assert!(
        !flow.contains('\u{FB01}'),
        "no ligature may reach the output"
    );
}

#[test]
fn a_budget_breach_stops_the_stage() {
    // Not a conversion anyone would run: four pages that are nothing but a running head, so
    // furniture takes every character there is. The point is that the law reports it rather
    // than the book quietly emerging empty.
    let input = read("../../corpus/fixtures/handmade/h20_recto_verso.pdf");
    let mut totals = ReasonTotals::default();
    let text = text_stage(&input, &mut totals, &T).expect("text conserves");

    // h20's heads are 8 of its 20 body characters per page — well over the 4 % furniture
    // budget, and the checker says so by name.
    let error = furniture_stage(&text, LangTag::EN, &mut totals, &T)
        .expect_err("removing a third of the text must breach the furniture budget");
    match error {
        ConservationError::BudgetExceeded { stage, group, .. } => {
            assert_eq!(stage, "furniture");
            assert_eq!(group, "furniture");
        }
        other => panic!("expected a budget breach, got {other}"),
    }
}

/// A page of glyphs: `count` characters drawn along `lines` baselines.
fn page_of(page: u32, lines: usize, per_line: usize, alphabet: &[char]) -> PageInput {
    const SIZE_PT: f32 = 10.0;
    const ADVANCE_PT: f32 = 6.0;
    const LEADING_PT: f32 = 20.0;
    const TOP_PT: f32 = 20.0;

    let mut glyphs = Vec::new();
    for line in 0..lines {
        let baseline = TOP_PT + LEADING_PT * line as f32;
        for column in 0..per_line {
            let index = (line * per_line + column) % alphabet.len().max(1);
            let x = 20.0 + ADVANCE_PT * column as f32;
            let bbox = Rect {
                x0: x,
                y0: baseline - SIZE_PT,
                x1: x + ADVANCE_PT,
                y1: baseline,
            };
            glyphs.push(Glyph {
                ch: alphabet.get(index).copied().unwrap_or('a'),
                bbox,
                loose_bbox: bbox,
                origin: (x, baseline),
                font: FontId(0),
                size_pt: SIZE_PT,
                weight: 400,
                italic: false,
                render_mode: 0,
                fill: [0, 0, 0, 255],
                generated: false,
                hyphen_flag: false,
                angle_deg: 0.0,
            });
        }
    }
    PageInput {
        page: PageRef::new(page),
        width_pt: 600.0,
        height_pt: 800.0,
        glyphs,
        images: Vec::new(),
    }
}

proptest! {
    // Two hundred documents, as the plan's table asks.
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// The property half of test 2.15.
    ///
    /// The alphabet is chosen to hit the awkward cases on purpose: `ﬁ` and `ﬂ` are the paired
    /// Removed + Added that plain multiset equality cannot express, and U+00AD is the removal
    /// with no addition. A naive `N` that expanded ligatures without ledgering them passes
    /// every unit test in `oc-text` and fails here, which is the RT A1 finding.
    #[test]
    fn conservation_i1_holds_over_generated_documents(
        pages in 1usize..4,
        lines in 1usize..6,
        per_line in 1usize..12,
        alphabet in proptest::collection::vec(
            proptest::sample::select(vec![
                'a', 'e', 'n', 'r', 's', '-', '1', '2',
                '\u{FB01}', '\u{FB02}', '\u{00AD}', 'İ', 'ß',
            ]),
            1..8,
        ),
    ) {
        let input: Vec<PageInput> = (0..pages)
            .map(|index| page_of(index as u32, lines, per_line, &alphabet))
            .collect();

        let mut totals = ReasonTotals::default();
        let before_text = glyph_chars(&input);
        // The budget checks are not the subject here: a page of pure running head breaches
        // them by construction, and I-1 is what the row is named for.
        let text = match text_stage(&input, &mut totals, &T) {
            Ok(text) => text,
            Err(ConservationError::BudgetExceeded { .. }) => return Ok(()),
            Err(error) => return Err(TestCaseError::fail(format!("text: {error}"))),
        };
        prop_assert!(balances(&before_text, &run_chars(&text.pages), &text.delta));

        let before_furniture = run_chars(&text.pages);
        let furniture = match furniture_stage(&text, LangTag::EN, &mut totals, &T) {
            Ok(furniture) => furniture,
            Err(ConservationError::BudgetExceeded { .. }) => return Ok(()),
            Err(error) => return Err(TestCaseError::fail(format!("furniture: {error}"))),
        };
        prop_assert!(balances(
            &before_furniture,
            &line_chars(&furniture.pages),
            &furniture.delta
        ));
    }

    /// The whitespace half of the same claim: an inferred space is outside `C`, so it must
    /// never reach the ledger, and it must not unbalance the equation either.
    #[test]
    fn an_inferred_space_is_never_ledgered(gap_pt in 0.0f32..40.0) {
        let mut page = page_of(0, 1, 2, &['a', 'b']);
        // Push the second glyph away from the first by `gap_pt`.
        if let Some(second) = page.glyphs.get_mut(1) {
            second.origin.0 += gap_pt;
            second.bbox.x0 += gap_pt;
            second.bbox.x1 += gap_pt;
            second.loose_bbox.x0 += gap_pt;
            second.loose_bbox.x1 += gap_pt;
        }
        let input = vec![page];

        let mut totals = ReasonTotals::default();
        let before = glyph_chars(&input);
        let text = text_stage(&input, &mut totals, &T).map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert!(text.delta.is_empty(), "{:?}", text.delta.entries());
        prop_assert_eq!(run_chars(&text.pages), before);
        prop_assert_eq!(c_of("ab"), run_chars(&text.pages));
    }
}
