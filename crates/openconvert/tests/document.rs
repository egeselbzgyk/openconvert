//! Phase 5: what `document` makes of a structured book (PIPELINE §9).
//!
//! The stage is small and three of its four outputs are load-bearing for the EPUB: the page
//! breaks become `page-list` entries, the classification picks the preset, and closure is the
//! precondition every later stage assumes. Each of those is checked here against a real
//! fixture rather than a constructed tree, because the failures worth catching — a page whose
//! every block was taken by a table, a chapter heading that opens a page — only occur in a
//! document something actually laid out.

mod common;

use oc_model::doc::Content;
use oc_model::document::{first_block, walk_content, DocClass, PresetName};
use oc_model::ledger::StageKind;

/// Every fixture the builder produces, so that a document-level postcondition is checked
/// against all of them rather than against the one that happened to be convenient.
const FIXTURES: [&str; 10] = [
    "f01_prose_single_column",
    "f02_two_column",
    "f03_image_only",
    "f04_german_prose",
    "f05_turkish_prose",
    "f06_hyphenation_de",
    "f07_verse_and_quote",
    "f08_footnotes",
    "f09_novel_structure",
    "f10_lists_and_table",
];

/// Closure is the precondition everything after this stage assumes. A `Content::Figure(7)` in
/// the flow of a document carrying six figures becomes an `<img src>` naming a file the
/// manifest does not list — EPUBCheck's `RSC-007` — and the emitter has no way to notice,
/// because by then the reference is a string.
#[test]
fn the_document_closes_every_reference_it_makes() {
    for stem in FIXTURES {
        let converted = common::convert(stem);
        assert!(
            converted.document.dangling_references().is_empty(),
            "{stem}: {:?}",
            converted.document.dangling_references()
        );
    }
}

/// A page break is a place in the flow *and* an entry in the page list, and the two have to be
/// the same thing. One `Content::PageBreak` per `PageBreak`, each naming a page the book has,
/// in ascending order and each page once — anything else is a `page-list` that either skips a
/// page or offers the reader two places to land on one.
#[test]
fn page_breaks_open_every_page_the_flow_reaches() {
    for stem in FIXTURES {
        let converted = common::convert(stem);
        let document = &converted.document;

        let in_flow: Vec<_> = document
            .walk()
            .into_iter()
            .flat_map(|section| walk_content(&section.content))
            .filter_map(|content| match content {
                Content::PageBreak(id) => Some(*id),
                _ => None,
            })
            .collect();
        let declared: Vec<_> = document.page_breaks.iter().map(|brk| brk.id).collect();
        assert_eq!(in_flow, declared, "{stem}: flow and page list disagree");

        let pages: Vec<u32> = document
            .page_breaks
            .iter()
            .map(|brk| brk.page.index)
            .collect();
        assert!(
            pages.windows(2).all(|pair| pair[0] < pair[1]),
            "{stem}: page breaks are not in ascending page order: {pages:?}"
        );
    }
}

/// `furniture` removed the printed folios and recovered them as labels; this is the stage
/// that puts them back where a citation can reach them. A `page-list` entry whose label were
/// the page *index* would be wrong for every book that paginates its front matter in roman —
/// which is most of them — so the label has to be the one that was printed.
///
/// `f01` is the fixture, not `f09`: `f09` restarts at arabic 1 after roman front matter and
/// `furniture` recovers no label at all from it, because a numbering-system change splits the
/// folio group in two and neither half reaches `layout.furniture.repetition_ratio_min` over
/// the whole book. That is a gap in `furniture`, recorded in `docs/DECISIONS_LOG.md`, and
/// asserting the document stage against it would be asserting the wrong stage.
#[test]
fn page_breaks_carry_the_printed_labels_furniture_recovered() {
    let converted = common::convert("f01_prose_single_column");
    let labels: Vec<Option<&str>> = converted
        .document
        .page_breaks
        .iter()
        .map(|brk| brk.page.label.as_deref())
        .collect();

    assert_eq!(
        labels,
        vec![Some("1"), Some("2")],
        "the printed folio reaches the page break that opens its page"
    );
}

/// A chapter that opens a page must break *before* its heading. The heading is not in the
/// section's content list, so a walk that only looked at content would attach the break to the
/// first paragraph and "go to page 57" would land past the title of the chapter that starts
/// there. The break is recorded at the head of the content list and `epub` lifts it above the
/// heading.
#[test]
fn a_section_that_opens_a_page_breaks_before_its_heading() {
    let converted = common::convert("f09_novel_structure");
    let document = &converted.document;

    let mut checked = 0;
    for section in document.walk() {
        let Some(heading) = &section.heading else {
            continue;
        };
        let Some(brk) = document
            .page_breaks
            .iter()
            .find(|brk| brk.before_block == heading.id)
        else {
            continue;
        };
        assert_eq!(
            section.content.first(),
            Some(&Content::PageBreak(brk.id)),
            "a break anchored on {:?} is not at the head of its section",
            heading.text()
        );
        checked += 1;
    }
    assert!(
        checked > 0,
        "f09 opens at least one chapter on a page of its own"
    );
}

/// The anchor of a page break has to be a block that is *in the flow*, not merely a block of
/// the page: a block a table or a note took is nowhere the reader can be sent, and a
/// `page-list` href pointing at one resolves to nothing.
#[test]
fn every_page_break_anchors_on_a_block_the_flow_still_carries() {
    for stem in FIXTURES {
        let converted = common::convert(stem);
        let document = &converted.document;

        let mut anchors: Vec<_> = document
            .walk()
            .into_iter()
            .flat_map(|section| {
                let heading = section.heading.iter().map(|heading| heading.id);
                let flow = walk_content(&section.content)
                    .into_iter()
                    .filter_map(first_block);
                heading.chain(flow).collect::<Vec<_>>()
            })
            .collect();
        anchors.sort_unstable();

        for brk in &document.page_breaks {
            assert!(
                anchors.binary_search(&brk.before_block).is_ok(),
                "{stem}: page break {:?} anchors on a block the flow does not carry",
                brk.id
            );
        }
    }
}

/// The classification picks the preset, which is a partial override map over every threshold
/// the rest of the pipeline reads (D13.11) — so getting it wrong retunes the book. `f01` is
/// one column of prose and `f02` is two columns of the same.
#[test]
fn a_book_is_classified_by_what_the_pipeline_measured() {
    let prose = common::convert("f01_prose_single_column");
    assert_eq!(prose.document.classification, DocClass::BookProse);
    assert_eq!(prose.document.presets, PresetName::Novel);

    let two_column = common::convert("f02_two_column");
    assert_eq!(
        two_column.document.classification,
        DocClass::AcademicMulticolumn
    );
    assert_eq!(two_column.document.presets, PresetName::Academic);

    // `auto` is an instruction and never survives into a document.
    for stem in FIXTURES {
        let converted = common::convert(stem);
        assert_ne!(
            converted.document.presets,
            PresetName::Auto,
            "{stem} was converted with \"pick one\""
        );
    }
}

/// The stage is declared Conserving, and the check that says so runs on every conversion
/// rather than in this test. What this test pins is that the check *was made* and recorded:
/// a stage whose result never reached the ledger would satisfy the law silently.
#[test]
fn document_records_a_conserving_check_in_the_ledger() {
    let converted = common::convert("f09_novel_structure");
    let checks = &converted.document.ledger.per_stage_checks;

    assert!(
        checks.iter().any(|check| check.stage == "structure"
            && check.kind == StageKind::Conserving
            && check.removed_chars == 0),
        "the document carries every earlier stage's check: {checks:?}"
    );
    assert!(
        !converted.document.ledger.c_0.is_empty(),
        "the retention denominator is opened before the document is assembled"
    );
}
