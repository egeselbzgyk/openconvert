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
use oc_model::layout::{BlockKindHint, ParagraphConvention};
use oc_model::ledger::{LedgerDelta, LedgerEntry, Reason, StageKind};
use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;
use openconvert::pipeline::{
    block_text, furniture_stage, layout_stage, paragraphs_stage, text_stage, LayoutStage,
    PageInput, ParagraphStage,
};

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
                images: document.page_images(index).unwrap_or_default(),
            }
        })
        .collect();

    let mut totals = ReasonTotals::default();
    let text = text_stage(&input, &mut totals, &T).expect("text conserves");
    let furniture =
        furniture_stage(&text, LangTag::EN, &mut totals, &T).expect("furniture stays in budget");
    layout_stage(&text, &furniture, &mut totals, &T).expect("layout conserves")
}

/// The same, carried one stage further: paragraphs, dehyphenated, under I-5.
fn paragraphs_of(relative: &str, lang: LangTag) -> ParagraphStage {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("missing fixture {}: {error}", path.display()));
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
                images: document.page_images(index).unwrap_or_default(),
            }
        })
        .collect();

    let mut totals = ReasonTotals::default();
    let text = text_stage(&input, &mut totals, &T).expect("text conserves");
    let furniture =
        furniture_stage(&text, lang.clone(), &mut totals, &T).expect("furniture stays in budget");
    let layout = layout_stage(&text, &furniture, &mut totals, &T).expect("layout conserves");
    paragraphs_stage(&layout, lang, &mut totals, &T).expect("paragraphs stays in budget")
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

/// The reading-order text of a page: every block's lines, in order, one string per block.
fn blocks_text(layout: &LayoutStage, page: usize) -> Vec<String> {
    layout.blocks[page]
        .iter()
        .map(|block| block_text(block, &layout.pages[page]))
        .collect()
}

/// Where a phrase first appears in a page's reading order, by block index.
fn position_of(blocks: &[String], needle: &str) -> usize {
    blocks
        .iter()
        .position(|text| text.contains(needle))
        .unwrap_or_else(|| panic!("{needle:?} is not on the page: {blocks:#?}"))
}

/// Row 3.2, and acceptance A3.1. The whole left column precedes the right one — the failure
/// this test exists for is the classic interleave, where a naive top-to-bottom sort reads one
/// line of each column in turn and produces two unrelated sentences spliced together.
#[test]
fn two_column_reading_order_is_left_then_right() {
    let layout = layout_of("../../target/fixtures/f02_two_column.pdf");
    let page = blocks_text(&layout, 0);

    assert!(
        position_of(&page, "The left column continues")
            < position_of(&page, "The right column begins"),
        "reading order interleaves the columns: {page:#?}"
    );
    assert_eq!(
        layout.columns[0].count(),
        2,
        "the fixture is two columns: {:?}",
        layout.columns[0].gutters
    );
}

/// Row 3.3. The floating title spans both columns, so it cannot be cut with them: it is
/// pre-masked, kept whole, and put back at the top of the page.
#[test]
fn floating_title_is_premasked_not_split() {
    let layout = layout_of("../../target/fixtures/f02_two_column.pdf");
    let page = blocks_text(&layout, 0);

    let title = "On the Measurement of Columns";
    assert_eq!(
        page.iter().filter(|text| text.contains(title)).count(),
        1,
        "the title is one block: {page:#?}"
    );
    assert_eq!(
        position_of(&page, title),
        0,
        "the title is first in reading order: {page:#?}"
    );
    assert_eq!(
        layout.blocks[0][0].kind_hint,
        BlockKindHint::FloatingTitle,
        "the title is pre-masked, not sorted with the columns"
    );
}

/// Row 3.5. A page can carry a valley as deep and as tall as a gutter and still be one
/// column, and nothing on the page says which it is. The evidence is between the pages: read
/// as two columns the text stops running on at every boundary, and read as one it never does.
#[test]
fn cross_page_continuity_downgrades_column_count() {
    let layout = layout_of("../../corpus/fixtures/handmade/h22_false_gutter.pdf");

    assert!(
        layout.column_retries > 0,
        "the two-column hypothesis was never questioned"
    );
    for (index, columns) in layout.columns.iter().enumerate() {
        assert_eq!(
            columns.count(),
            1,
            "page {index} kept its false gutter: {:?}",
            columns.gutters
        );
    }
    assert!(
        layout.continuity.rate() >= 0.9,
        "continuity after the downgrade is {} over {} boundaries",
        layout.continuity.rate(),
        layout.continuity.boundaries
    );
}

/// And the control: the fixture really does look like two columns before the check runs, so
/// the downgrade is a decision rather than a detector that never fires.
#[test]
fn the_false_gutter_is_found_before_continuity_rejects_it() {
    let layout = layout_of("../../corpus/fixtures/handmade/h22_false_gutter.pdf");
    assert!(layout.continuity.boundaries >= 4, "a rate needs samples");

    // Page 0's ink, projected as `layout` projects it, with the continuity check taken away.
    let ink: Vec<oc_model::geom::Rect> = layout.pages[0]
        .lines
        .iter()
        .flat_map(|line| line.segments.iter().map(|segment| segment.bbox))
        .collect();
    let em = oc_layout::columns::median_height(
        &layout.pages[0]
            .lines
            .iter()
            .map(|line| line.bbox())
            .collect::<Vec<_>>(),
    );
    let unchecked = oc_layout::columns::detect_columns(&ink, em, usize::MAX, &T);
    assert_eq!(
        unchecked.count(),
        2,
        "the fixture is supposed to look like two columns: {unchecked:?}"
    );
}

/// Row 3.6. `f01` indents its first lines, and the book-level convention has to see that from
/// the book rather than from any one page.
#[test]
fn paragraph_convention_indent_detected() {
    let stage = paragraphs_of(
        "../../target/fixtures/f01_prose_single_column.pdf",
        LangTag::EN,
    );

    assert_eq!(stage.convention, ParagraphConvention::FirstLineIndent);

    let page0: Vec<&oc_model::layout::Para> = stage
        .paragraphs
        .iter()
        .filter(|para| para.pages.0 == 0)
        .collect();
    let texts: Vec<&str> = page0.iter().map(|para| para.text.as_str()).collect();
    assert_eq!(
        page0.len(),
        4,
        "page 0 is a heading and three paragraphs: {texts:#?}"
    );
    assert!(
        texts[0].starts_with("Chapter 3"),
        "the heading is its own paragraph here; `structure` gives it its role in Phase 4"
    );
    assert!(texts[1].starts_with("It was a dark and stormy night"));
    assert!(texts[2].starts_with("The office was quiet"));
    assert!(texts[3].starts_with("Outside, the harbour lights"));
}

/// Row 3.7. A paragraph interrupted by a page break is one paragraph, and the word broken
/// across the same break is one word.
///
/// Both halves matter and they fail together: a merge that leaves `pipe- line` in the text is
/// as wrong as two paragraphs. The join is settled by the document's own vocabulary — the
/// first sentence of the fixture uses `pipeline` — so this test is about the merge and the
/// join, not about the tier that would otherwise have to guess.
#[test]
fn paragraph_merges_across_page_break() {
    let stage = paragraphs_of(
        "../../corpus/fixtures/handmade/h23_paragraph_across_pages.pdf",
        LangTag::EN,
    );

    let merged: Vec<&oc_model::layout::Para> = stage
        .paragraphs
        .iter()
        .filter(|para| para.is_merged())
        .collect();
    assert_eq!(
        merged.len(),
        1,
        "exactly one paragraph spans the break: {:#?}",
        stage
            .paragraphs
            .iter()
            .map(|para| (para.pages, para.text.clone()))
            .collect::<Vec<_>>()
    );
    assert_eq!(merged[0].pages, (0, 1));
    assert!(
        merged[0].text.contains("pipeline"),
        "the broken word was not rejoined: {:?}",
        merged[0].text
    );
    assert!(
        !merged[0].text.contains("pipe-"),
        "the hyphen survived the join: {:?}",
        merged[0].text
    );
}

/// And the ledger says so: exactly one character left, it was a hyphen, and it left under the
/// one reason this stage is allowed to cite.
#[test]
fn dehyphenation_is_ledgered_one_hyphen_at_a_time() {
    let stage = paragraphs_of(
        "../../corpus/fixtures/handmade/h23_paragraph_across_pages.pdf",
        LangTag::EN,
    );

    let entries = stage.delta.entries();
    assert_eq!(entries.len(), 1, "{entries:#?}");
    assert_eq!(entries[0].reason, Reason::Dehyphenate);
    assert_eq!(entries[0].text, "-");
    assert!(!entries[0].added);
    assert_eq!(entries[0].stage, "paragraphs");
    assert_eq!(stage.check.removed_chars, 1);
    assert_eq!(stage.check.added_chars, 0);
}

/// The in-document lexicon is what decided it, and it is built from the book rather than
/// from a dictionary: a document that never says `pipeline` keeps its hyphen.
#[test]
fn the_lexicon_is_built_from_the_document() {
    let stage = paragraphs_of(
        "../../corpus/fixtures/handmade/h23_paragraph_across_pages.pdf",
        LangTag::EN,
    );
    assert!(!stage.lexicon.is_empty());
    assert!(stage.lexicon.joined_count("pipe", "line") >= 1);
}

/// Row 3.9. A German compound broken at its own hyphen is not rejoined, and one broken
/// inside a word is.
///
/// Both directions in one fixture, because getting either one right alone is easy and the
/// pair is what the language actually demands: `Nord-Süd-Achse` must survive, and
/// `Eisen-` / `bahn` must not.
#[test]
fn dehyphenate_keeps_german_real_hyphen() {
    let stage = paragraphs_of("../../target/fixtures/f06_hyphenation_de.pdf", LangTag::DE);
    let text = stage
        .paragraphs
        .iter()
        .map(|para| para.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        text.contains("Nord-Süd-Achse"),
        "the compound's own hyphen was eaten: {text}"
    );
    assert!(
        !text.contains("NordSüd"),
        "the halves were welded together: {text}"
    );
    assert!(
        text.contains("Eisenbahn"),
        "a word broken inside itself was not rejoined: {text}"
    );
    assert!(
        !text.contains("Eisen- bahn"),
        "the line break survived the join: {text}"
    );
}

/// And the ledger agrees about which of the two was the removal.
#[test]
fn the_german_fixture_removes_exactly_one_hyphen() {
    let stage = paragraphs_of("../../target/fixtures/f06_hyphenation_de.pdf", LangTag::DE);
    let entries = stage.delta.entries();
    assert_eq!(
        entries.len(),
        1,
        "one join, one entry, and the kept hyphen has nothing to record: {entries:#?}"
    );
    assert_eq!(entries[0].reason, Reason::Dehyphenate);
    assert_eq!(entries[0].text, "-");
}

/// PIPELINE §6 step 5. An image-only page still has its image in the flow: "drop nothing yet"
/// is the rule for the whole stage, and a figure that never reaches the flow is a figure that
/// never reaches the book.
#[test]
fn an_image_only_page_keeps_its_image_in_the_flow() {
    let layout = layout_of("../../target/fixtures/f03_image_only.pdf");

    let anchored: usize = layout.anchors.iter().map(Vec::len).sum();
    assert!(
        anchored >= 1,
        "the fixture's image was not anchored: {:?}",
        layout.anchors
    );
}
