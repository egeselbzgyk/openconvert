//! Phase 6: the structural validator over real conversions.
//!
//! I-7 is the one invariant stated end to end — from `C_0`, the multiset extraction produced,
//! to `C(EPUB)`, the multiset a reader can see — and the only one that would notice a chapter
//! that never reached the container. Everything between the two is checked per stage; this is
//! the claim that the composition of those stages did not lose the book.
//!
//! Measured against the **archive**, resolved through the package document's spine, for the same
//! reason Tier 1 reads the archive: an emitter checked against its own account of what it wrote
//! is checked against nothing.

mod common;

/// Every fixture the builder produces.
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

/// Row 6.1, and acceptance criterion A6.1. `C(EPUB) ⊎ Removed_all == C_0 ⊎ Added_all` for every
/// fixture, measured over the zip rather than over the emitter's output.
#[test]
fn i7_holds_end_to_end_on_all_fixtures() {
    for stem in FIXTURES {
        let built = common::build(stem);
        let chars = oc_validate::epub_chars(&built.built.bytes)
            .unwrap_or_else(|error| panic!("{stem}: {error}"));
        let result = oc_validate::check_i7(&chars, &built.document.ledger);

        assert!(
            result.holds(),
            "{stem}: I-7 fails — {} characters unaccounted for, {} unexplained additions; \
             missing {:?}",
            result.missing.total(),
            result.extra.total(),
            result.missing.iter().take(12).collect::<Vec<_>>()
        );
    }
}

/// The archive and the emitter agree about `C(EPUB)`. They are two different measurements — one
/// parses the zip's spine, the other the `Emitted` files the builder kept — and the `epub`
/// stage's own conservation check uses the second. If they ever disagree the bug is in the zip
/// writer, which no other test in the project would see.
#[test]
fn the_archive_and_the_emitter_agree_about_the_text() {
    use oc_model::ledger::c_of;

    for stem in FIXTURES {
        let built = common::build(stem);
        let from_archive = oc_validate::epub_chars(&built.built.bytes)
            .unwrap_or_else(|error| panic!("{stem}: {error}"));

        let mut from_emitter = oc_model::extract::CharHistogram::new();
        for file in &built.built.emitted.files {
            let text = oc_epub::textcontent::body_text(&file.markup).expect("the emitter's own");
            from_emitter = from_emitter.union(&c_of(&text));
        }

        assert_eq!(
            from_archive, from_emitter,
            "{stem}: the zip and the emitter disagree about the book's text"
        );
    }
}

/// The measured retention of every fixture, recorded rather than asserted against a bound.
///
/// `retention = |C(EPUB)| / |C_0|` counts ledgered furniture removal as loss, because `C_0` is
/// the pdfium text layer and a running head is part of it. A book whose furniture is at the 4 %
/// budget (ARCHITECTURE §5.5) therefore retains 0.96 and is entirely correct, which is why
/// `validate.min_char_retention` is a **flag** in this phase and not a gate — R6 §11 words it
/// that way too ("flag < 98 %"). The snapshot is the regression artefact: a number that moves
/// means a stage started losing or keeping text it did not before.
#[test]
fn retention_per_fixture_is_recorded() {
    let floor = oc_core::thresholds::T.validate.min_char_retention;
    let mut lines = Vec::new();

    for stem in FIXTURES {
        let built = common::build(stem);
        let chars = oc_validate::epub_chars(&built.built.bytes)
            .unwrap_or_else(|error| panic!("{stem}: {error}"));
        let result = oc_validate::check_i7(&chars, &built.document.ledger);
        let flagged = oc_validate::structural::retention_warnings(&result, floor);

        lines.push(format!(
            "{stem}  retention {:.4}  |C_0| {}  |C(EPUB)| {}  flagged {}",
            result.retention(),
            result.c0_chars,
            result.epub_chars,
            if flagged.is_empty() { "no" } else { "yes" }
        ));
    }

    insta::assert_snapshot!("retention_per_fixture", lines.join("\n"));
}

/// The whole structural report over every fixture, recorded. This is the measurement the phase
/// exists to make: it is the first time the project has said, in one place and for every fixture,
/// how much of the book arrived, whether the heading tree is sane, and whether anything is in the
/// container twice.
#[test]
fn the_structural_report_on_every_fixture_is_recorded() {
    use oc_core::thresholds::T;

    let mut lines = Vec::new();
    for stem in FIXTURES {
        let built = common::build(stem);
        let tier1 = oc_validate::validate_tier1(
            &built.built.bytes,
            &oc_validate::Expectations {
                images: Some(built.extracted_images),
            },
        );
        let pages = u32::try_from(built.document.page_breaks.len()).unwrap_or(u32::MAX);
        let report = oc_validate::structural::validate_structural(
            &built.document,
            &built.built.bytes,
            &tier1,
            pages,
            &T,
        )
        .unwrap_or_else(|error| panic!("{stem}: {error}"));

        lines.push(format!(
            "{stem}\n  i7 {}  retention {:.4}  parity {}  bijection {}  hrefs {}\n  \
             h1 {}  skips {}  monotone {:?}  plausible {:?}\n  \
             blocks {}  duplicates {}  dup_para_frac {:.4}  top_3gram {:.4}\n  warnings {:?}",
            report.i7.holds(),
            report.retention,
            report.image_parity,
            report.note_bijection,
            report.hrefs_resolve,
            report.heading_sanity.h1_count,
            report.heading_sanity.level_skips.len(),
            report.heading_sanity.monotone_with_pages,
            report.heading_sanity.h1_count_plausible,
            report.duplicates.blocks,
            report.duplicates.duplicates,
            report.quality.dup_para_frac,
            report.quality.top_3gram,
            report
                .warnings
                .iter()
                .map(|warning| warning.code)
                .collect::<Vec<_>>()
        ));
    }

    insta::assert_snapshot!("structural_report_per_fixture", lines.join("\n"));
}

/// Acceptance criterion A6.1's other half, and the one the repair loop depends on: every fixture's
/// structural report holds. I-7, image parity, the note bijection, resolving hrefs and a sane
/// heading tree are the five the report calls a conjunction, and a fixture that fails one of them
/// is an emitter bug — which is the premise of the zero-fire-rate gate.
#[test]
fn the_structural_report_holds_on_every_fixture() {
    use oc_core::thresholds::T;

    for stem in FIXTURES {
        let built = common::build(stem);
        let tier1 = oc_validate::validate_tier1(
            &built.built.bytes,
            &oc_validate::Expectations {
                images: Some(built.extracted_images),
            },
        );
        let pages = u32::try_from(built.document.page_breaks.len()).unwrap_or(u32::MAX);
        let report = oc_validate::structural::validate_structural(
            &built.document,
            &built.built.bytes,
            &tier1,
            pages,
            &T,
        )
        .unwrap_or_else(|error| panic!("{stem}: {error}"));

        assert!(
            report.holds(),
            "{stem}: the structural report does not hold — i7 {}, parity {}, bijection {}, \
             hrefs {}, heading sanity {:?}",
            report.i7.holds(),
            report.image_parity,
            report.note_bijection,
            report.hrefs_resolve,
            report.heading_sanity
        );
    }
}
