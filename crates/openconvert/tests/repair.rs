//! Phase 6: the validate→repair loop on the real path.
//!
//! Row 6.10 is the release gate, and it is a gate whose *passing* is the claim: **every repair that
//! fires is a bug in our emitter** (ARCHITECTURE §7.3), so a non-zero fire rate opens an issue
//! against `oc-epub` and not against the repair table. Reframed that way, "zero EPUBCheck errors on
//! the corpus" stops being a validation goal and becomes a generator-correctness goal — the only
//! version of it that scales for one maintainer.

mod common;

use oc_core::thresholds::T;
use oc_validate::repair::RepairStatus;

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

/// Row 6.10, and acceptance criterion A6.2. Zero repairs fire on the fixture corpus, against
/// `repair.corpus_fire_rate_max = 0`.
#[test]
fn repair_fire_rate_is_zero_on_corpus() {
    let bound = T.repair.corpus_fire_rate_max;
    let mut fired = 0u32;

    for stem in FIXTURES {
        let built = common::build(stem);
        assert_eq!(
            built.repair.status,
            RepairStatus::Clean,
            "{stem}: the loop did something — {:?}, remaining {:?}",
            built.repair.status,
            built
                .repair
                .remaining
                .iter()
                .map(|finding| finding.id)
                .collect::<Vec<_>>()
        );
        fired += built.repair.fire_count();
    }

    assert!(
        f64::from(fired) <= bound,
        "{fired} repairs fired across the fixture corpus, against a bound of {bound}. \
         Every one of them is an emitter bug: open an issue against oc-epub, not against the \
         repair table (ARCHITECTURE §7.3)."
    );
}

/// The loop leaves the document alone when it has nothing to do, which is what makes the container
/// a clean conversion produces byte-identical to the one Phase 5 produced. A loop that re-emitted
/// from a document it had touched would break row 5.4 and the cross-OS gate with it.
#[test]
fn a_clean_conversion_is_the_same_bytes_the_emitter_produced() {
    for stem in FIXTURES {
        let built = common::build(stem);
        let direct = oc_epub::build_epub(&built.document, &Vec::new(), &common::epub_options());

        // Images are not passed here, so only the fixtures without them can be compared byte for
        // byte; for the rest the claim is that the loop changed nothing about the document.
        if let Ok(direct) = direct {
            if built.extracted_images == 0 {
                assert_eq!(
                    direct.bytes, built.built.bytes,
                    "{stem}: the loop's container is not the emitter's"
                );
            }
        }
        assert_eq!(built.repair.status, RepairStatus::Clean, "{stem}");
        assert!(built.repair.log.is_empty(), "{stem}");
    }
}

/// Both new stages are in the ledger, in PIPELINE's order, and both are Conserving.
///
/// `repair`'s check is the one that earns its keep: it compares `C` of the document the loop was
/// given against `C` of the document it settled on with an empty ledger, so a repair that changed
/// one character of the book fails I-1 and the conversion stops. That is PIPELINE §12's "none of
/// them may change the character content of the book", stated as an invariant.
#[test]
fn the_ledger_records_validate_and_repair_as_conserving_stages() {
    use oc_model::ledger::StageKind;

    for stem in FIXTURES {
        let built = common::build(stem);
        let stages: Vec<&str> = built
            .document
            .ledger
            .per_stage_checks
            .iter()
            .map(|check| check.stage)
            .collect();

        assert_eq!(
            stages,
            vec![
                "text",
                "furniture",
                "layout",
                "structure",
                "document",
                "epub",
                "validate",
                "repair",
            ],
            "{stem}: the ledger is not in stage order"
        );

        for name in ["validate", "repair"] {
            let check = built
                .document
                .ledger
                .per_stage_checks
                .iter()
                .find(|check| check.stage == name)
                .unwrap_or_else(|| panic!("{stem}: no {name} check"));
            assert_eq!(check.kind, StageKind::Conserving, "{stem}: {name}");
            assert_eq!(check.removed_chars, 0, "{stem}: {name}");
            assert_eq!(check.added_chars, 0, "{stem}: {name}");
        }
    }
}

/// The structural and repair findings reach `Document.warnings`, which is where the report's issue
/// list comes from. Phase 5 left `BuiltEpub::warnings` carrying codes that nothing attached to the
/// document; they are attached now.
#[test]
fn the_documents_warnings_carry_what_validation_found() {
    let built = common::build("f01_prose_single_column");
    let codes: Vec<&str> = built
        .document
        .warnings
        .iter()
        .map(|warning| warning.code)
        .collect();

    assert!(
        codes.contains(&oc_validate::structural::W_LOW_RETENTION),
        "f01 retains 0.9687 and the warning is the report's: {codes:?}"
    );
    // And nothing the loop did, because it did nothing.
    assert!(
        !codes.contains(&oc_validate::repair::W_REPAIR_FIRED),
        "{codes:?}"
    );
    assert!(
        !codes.contains(&oc_validate::repair::W_EPUB_INVALID),
        "{codes:?}"
    );
}
