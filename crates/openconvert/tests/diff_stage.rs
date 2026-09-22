//! PHASE 7.5 rows 7.5.1 … 7.5.5: the conservation diagnostic and the claim invariant.
//!
//! Two questions, and they are not the same one. `diff-stage` asks *what left*, which is the
//! diagnostic; the claim invariant asks *who was supposed to be carrying it*, which is the
//! fix. The first makes a defect findable in a minute instead of an afternoon, and the second
//! makes the defect class unable to recur.

use oc_core::conservation_diff::{diff, Unit};
use oc_core::ledger_check::ReasonTotals;
use oc_core::thresholds::T;
use oc_model::lang::LangTag;
use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;
use oc_structure::claims::{Claim, ClaimKind, Claimant, Claims};
use oc_structure::meta::{InfoDict, MetaSources};
use oc_structure::stage::{StructureInput, StructureOutput};
use openconvert::pipeline::{body_runs, furniture_stage, layout_stage, text_stage};
use openconvert::structure_input::{block_views, document_images};

/// The ten Typst fixtures, which are the documents `structure` is known to conserve on.
const FIXTURES: &[&str] = &[
    "../../target/fixtures/f01_prose_single_column.pdf",
    "../../target/fixtures/f02_two_column.pdf",
    "../../target/fixtures/f03_image_only.pdf",
    "../../target/fixtures/f04_german_prose.pdf",
    "../../target/fixtures/f05_turkish_prose.pdf",
    "../../target/fixtures/f06_hyphenation_de.pdf",
    "../../target/fixtures/f07_verse_and_quote.pdf",
    "../../target/fixtures/f08_footnotes.pdf",
    "../../target/fixtures/f09_novel_structure.pdf",
    "../../target/fixtures/f10_lists_and_table.pdf",
];

/// Run everything up to `structure`, without the conservation check, and return both sides.
fn structure_of(relative: &str) -> (Vec<Unit>, StructureOutput) {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "missing fixture {}: {error}; run `cargo run -p xtask -- fixtures`",
            path.display()
        )
    });
    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(&bytes, None).expect("the fixture opens");
    let pdf = document.as_ref();
    let input = openconvert::input::page_inputs(pdf).expect("every page extracts");

    let mut totals = ReasonTotals::default();
    let text = text_stage(&input, &mut totals, &T).expect("text conserves");
    let furniture =
        furniture_stage(&text, LangTag::EN, &mut totals, &T).expect("furniture stays in budget");
    let layout = layout_stage(&text, &furniture, &mut totals, &T).expect("layout conserves");

    let images = document_images(&text);
    let hashes = openconvert::convert::image_hashes(pdf, &images);
    let blocks = block_views(&text, &layout);
    let stage_input = StructureInput {
        blocks: blocks.clone(),
        runs: body_runs(&text, &furniture),
        fonts: text.fonts.clone(),
        images,
        image_hashes: hashes,
        vectors: (0..pdf.page_count())
            .filter_map(|page| pdf.page_vectors(page).ok())
            .flatten()
            .collect(),
        outline: pdf.outline(),
        labels: furniture.labels.clone(),
        drop_caps: layout.drop_caps.iter().flatten().cloned().collect(),
        page_count: pdf.page_count(),
        meta: MetaSources {
            xmp: pdf.xmp(),
            info: InfoDict {
                title: None,
                author: None,
            },
            filename: relative.to_owned(),
            source_sha256: "0f0f0f".to_owned(),
            language: LangTag::EN,
        },
        lang: LangTag::EN,
    };
    let output = oc_structure::stage::structure(&stage_input, &T);

    let units = blocks
        .iter()
        .map(|block| {
            let unit = Unit::new(
                format!("block {}", block.id.as_str()),
                Some(block.page),
                block.text.clone(),
            );
            match output.claims.claimant_of(block.id) {
                Some(claimant) => unit.claimed_by(claimant.label()),
                None => unit,
            }
        })
        .collect();
    (units, output)
}

fn units_of(texts: Vec<String>) -> Vec<Unit> {
    texts
        .into_iter()
        .map(|text| Unit::new("out", None, text))
        .collect()
}

/// Row 7.5.1. The whole point of the command: a refusal that says "644 characters left" and
/// nothing else costs an afternoon of binary search over page prefixes. This one says which
/// block, on which page, and which structure took it.
#[test]
fn diff_stage_names_every_character_that_left() {
    let input = vec![
        Unit::new("block A", Some(0), "the paragraph that survives"),
        Unit::new("block B", Some(41), "the footnote body that does not").claimed_by("note n0"),
    ];
    let output = units_of(vec!["the paragraph that survives".to_owned()]);

    let result = diff(&input, &output);

    // Exactly the removed characters, and no others.
    let expected = oc_model::ledger::c_of("the footnote body that does not");
    assert_eq!(result.lost, expected);
    assert!(result.appeared.is_empty());

    // Attributed to their block, and to the claimant that took it.
    assert_eq!(result.unaccounted.len(), 1);
    assert_eq!(result.unaccounted[0].label, "block B");
    assert_eq!(result.unaccounted[0].page, Some(41));
    assert_eq!(result.unaccounted[0].claimed_by.as_deref(), Some("note n0"));
}

/// Row 7.5.2. The other direction. `structure` emitting a table's cells *and* leaving the
/// same lines in the flow is the shape this catches — the defect *AI Engineering* exhibits,
/// 21 526 characters emitted twice.
#[test]
fn diff_stage_names_every_character_that_appeared() {
    let input = vec![Unit::new("block A", Some(0), "row one cell one")];
    let output = units_of(vec![
        "row one cell one".to_owned(),
        "row one cell one".to_owned(),
    ]);

    let result = diff(&input, &output);

    assert!(result.lost.is_empty());
    assert_eq!(result.appeared, oc_model::ledger::c_of("row one cell one"));
    assert!(!result.balances());
}

/// Row 7.5.3. The diagnostic must be quiet where there is nothing to say, or nobody will read
/// it where there is. Run against the reachable output, not the declared one: a fixture whose
/// containers are all reachable is the only kind that should be here.
#[test]
fn diff_stage_is_silent_on_a_conserving_stage() {
    for name in FIXTURES {
        let (input, output) = structure_of(name);
        let result = diff(&input, &units_of(output.reachable_text()));

        assert!(
            result.lost.is_empty() && result.appeared.is_empty(),
            "{name}: lost {} appeared {}; unaccounted {:?}",
            result.lost.total(),
            result.appeared.total(),
            result
                .unaccounted
                .iter()
                .map(|unit| (&unit.label, &unit.claimed_by))
                .collect::<Vec<_>>()
        );
    }
}

/// Row 7.5.4. The claim invariant, stated directly: a claimant that does not emit what it
/// claimed fails, and the failure names it.
///
/// This is the assumption `structure` made silently for four stages' worth of code — a block
/// in `taken` is dropped from the flow because *something* will re-emit it — and nothing
/// checked it.
#[test]
fn a_claimed_block_must_be_accounted_for_by_its_claimant() {
    let block = oc_model::ids::BlockId::derive(
        0,
        oc_model::geom::Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 10.0,
            y1: 10.0,
        },
        "a claimed block",
    );
    let mut claims = Claims::new();
    claims.push(Claim {
        block,
        page: 0,
        by: Claimant::new(ClaimKind::List, "l0"),
        text: "a claimed block".to_owned(),
    });

    let input = vec![Unit::new(
        format!("block {}", block.as_str()),
        Some(0),
        "a claimed block",
    )
    .claimed_by("list l0")];
    // The claimant emitted nothing.
    let result = diff(&input, &units_of(vec![]));

    assert_eq!(result.unaccounted.len(), 1);
    assert_eq!(
        result.unaccounted[0].claimed_by.as_deref(),
        Some("list l0"),
        "the failure names the claimant, not only the block"
    );
    assert!(claims.contains(block));
}

/// Row 7.5.5. A fifth claimant cannot be forgotten.
///
/// `ClaimKind` is the enum every claimant must name itself through, and there is no way into
/// `Claims` that does not go through it. The test is the exhaustive match: adding a variant
/// and not adding it here fails to compile, which is the only kind of reminder that works.
#[test]
fn every_claimant_is_covered_by_the_invariant() {
    let all = [
        ClaimKind::Note,
        ClaimKind::Table,
        ClaimKind::List,
        ClaimKind::Caption,
    ];
    for kind in all {
        // Exhaustive: a new variant makes this match fail to compile.
        let named = match kind {
            ClaimKind::Note => "note",
            ClaimKind::Table => "table",
            ClaimKind::List => "list",
            ClaimKind::Caption => "caption",
        };
        assert_eq!(kind.as_str(), named);
        assert_eq!(Claimant::unnamed(kind).kind(), named);
    }
    assert_eq!(all.len(), 4, "a fifth claimant needs a row in this list");
}

/// The defect this phase opened with, stated as an invariant rather than as a book.
///
/// `emitted_text` counts every container the stage built. `reachable_text` counts only the
/// ones the flow points at. When they disagree, the stage balances against its own bookkeeping
/// and the reader gets a shorter book — a loss the conservation law cannot see, which is the
/// exact failure mode the law exists to prevent.
#[test]
fn every_container_the_conservation_check_counts_is_reachable_from_the_flow() {
    for name in FIXTURES {
        let (_, output) = structure_of(name);

        let mut declared = oc_model::extract::CharHistogram::new();
        for text in output.emitted_text() {
            declared = declared.union(&oc_model::ledger::c_of(&text));
        }
        let mut reachable = oc_model::extract::CharHistogram::new();
        for text in output.reachable_text() {
            reachable = reachable.union(&oc_model::ledger::c_of(&text));
        }

        assert_eq!(
            declared.difference(&reachable).total(),
            0,
            "{name}: {} characters sit in a container the flow does not reach",
            declared.difference(&reachable).total()
        );
    }
}

/// `structure/lost/orphaned-claimant`, as an invariant.
///
/// A structure that takes a block out of the flow must itself be in the book, or the text it
/// took is gone and nothing downstream can see it. 73 % of all measured corpus loss was this:
/// lists that claimed blocks and were never placed, because their emission was triggered by a
/// predicate other than their own claims. Emission is now derived from the claims — a list is
/// placed where its first taken line is — so this should hold by construction.
#[test]
fn no_claimant_is_orphaned() {
    for name in FIXTURES {
        let (_, output) = structure_of(name);
        let orphaned: Vec<String> = output
            .orphaned_claims()
            .iter()
            .map(|claim| format!("{} by {}", claim.block.as_str(), claim.by.label()))
            .collect();
        assert!(
            orphaned.is_empty(),
            "{name}: claimed by a structure the book never reaches: {orphaned:?}"
        );
    }
}

/// `structure/appeared/contested-claim`, as an invariant — stated as what is actually
/// required, which is not what was first written.
///
/// The first version asserted that no *block* has two owners, and it failed on `f08`: one
/// footnote block holding two notes, split correctly between them. Sharing a block is not the
/// defect. The defect is **the same characters owned twice** — a list item that is also a
/// table row, one note body handed to four notes — because whatever is owned twice is emitted
/// twice. 44 of 95 corpus documents had it, in all six strata. The detectors are now built in
/// precedence order, each from what the ones before it left, and each claim records only the
/// text its claimant took.
#[test]
fn no_text_has_two_owners() {
    for name in FIXTURES {
        let (input, output) = structure_of(name);
        let text_of = |block: oc_model::ids::BlockId| -> String {
            input
                .iter()
                .find(|unit| unit.label == format!("block {}", block.as_str()))
                .map(|unit| unit.text.clone())
                .unwrap_or_default()
        };
        let overclaimed: Vec<String> = output
            .claims
            .overclaimed(text_of)
            .iter()
            .map(|(block, who)| {
                let names: Vec<String> = who.iter().map(|by| by.label()).collect();
                format!("{} by {}", block.as_str(), names.join(" + "))
            })
            .collect();
        assert!(
            overclaimed.is_empty(),
            "{name}: text owned by more than one structure: {overclaimed:?}"
        );
    }
}
