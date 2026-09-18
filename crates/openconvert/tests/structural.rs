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
