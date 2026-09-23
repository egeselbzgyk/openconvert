//! PHASE 10: the four AI-assisted decisions, wired into the pipeline — and the deterministic path
//! they sit beside, which must not move.

mod common;

/// Every Typst fixture.
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

/// `ai.enabled = false` is the v1 default and the deterministic path is the product (D17), so
/// wiring the four tasks in must not change a single byte of what a conversion without them
/// writes. The snapshot was taken from the tree **before** Phase 10's first change (Phase 9's merge,
/// `8f045a1`) and is held here: every fixture's container, converted with no AI, hashed.
///
/// A change to this snapshot is a change to the product that the AI work was not allowed to make,
/// and the only acceptable reason for one is a deliberate deterministic change reviewed as such.
#[test]
fn no_ai_output_is_byte_identical_to_the_pre_phase_snapshot() {
    let hashes: std::collections::BTreeMap<&str, String> = FIXTURES
        .iter()
        .map(|stem| {
            let built = common::build(stem);
            (
                *stem,
                common::sha256_hex_of(built.conversion.built.bytes.as_slice()),
            )
        })
        .collect();
    insta::assert_json_snapshot!("no_ai_epub_sha256", hashes);
}

/// Row 10.2. `f07` is a Typst book, and Typst writes an outline: the outline is ground truth for
/// the book's structure, so the `book_structure` predicate abstains and no record of it exists —
/// while the same book's ambiguous indented block *is* escalated and recorded, with AI off.
#[test]
fn no_escalation_when_outline_present() {
    use oc_structure::escalate::{TASK_BOOK_STRUCTURE, TASK_VERSE_QUOTE};

    let built = common::build("f07_verse_and_quote");
    let tasks: Vec<&str> = built.escalations.iter().map(|record| record.task).collect();
    assert!(
        !tasks.contains(&TASK_BOOK_STRUCTURE),
        "an outline exists, so book structure is never escalated: {tasks:?}"
    );
    assert!(
        tasks.contains(&TASK_VERSE_QUOTE),
        "the ambiguous block is recorded whether or not a model is asked: {tasks:?}"
    );
    // With AI off, nothing was asked: every decision is the deterministic path's own.
    assert!(built
        .document
        .decisions
        .iter()
        .all(|decision| decision.llm.is_none() && decision.fallback.is_none()));
}

/// A fixture as `structure` receives it.
fn prepared(stem: &str) -> openconvert::convert::Prepared {
    use oc_pdf::inspect::PdfOpen;
    let bytes = std::fs::read(common::fixture(stem)).unwrap_or_else(|error| {
        panic!("missing fixture {stem}: {error}; run `cargo run -p xtask -- fixtures`")
    });
    let backend = oc_pdf::pdfium::PdfiumBackend::bind().expect("PDFium is vendored");
    let pdf = backend.open(&bytes, None).expect("the fixture opens");
    openconvert::convert::prepare(
        pdf.as_ref(),
        &openconvert::convert::sha256_hex(&bytes),
        &openconvert::convert::ConvertOptions {
            filename: format!("{stem}.pdf"),
            language: Some(oc_model::lang::LangTag::EN),
            preset: oc_model::document::PresetName::Auto,
            epub: common::epub_options(),
        },
        &oc_core::thresholds::T,
    )
    .expect("the fixture prepares")
}

/// Every block a structure output places in the flow, by id.
fn placed_blocks(
    output: &oc_structure::stage::StructureOutput,
) -> std::collections::BTreeSet<oc_model::ids::BlockId> {
    use oc_model::doc::Content;
    fn walk(content: &[Content], out: &mut std::collections::BTreeSet<oc_model::ids::BlockId>) {
        for item in content {
            match item {
                Content::Paragraph(para) => out.extend(para.blocks.iter().copied()),
                Content::Heading(heading) => {
                    out.insert(heading.id);
                }
                Content::Verse(verse) => {
                    out.insert(verse.id);
                }
                Content::Preformatted(pre) => {
                    out.insert(pre.id);
                }
                Content::List(list) => {
                    for item in &list.items {
                        walk(&item.content, out);
                    }
                }
                Content::BlockQuote(inner) | Content::Epigraph(inner) => walk(inner, out),
                Content::Figure(_)
                | Content::Table(_)
                | Content::NoteRefAnchor(_)
                | Content::PageBreak(_)
                | Content::Rule => {}
            }
        }
    }
    let mut out = std::collections::BTreeSet::new();
    for section in &output.sections {
        for section in section.walk() {
            if let Some(heading) = &section.heading {
                out.insert(heading.id);
            }
            walk(&section.content, &mut out);
        }
    }
    out
}

/// Row 10.8 — the RT A8.4 contradiction made executable. Two thousand generated mappings, two
/// hundred over each fixture's own style inventory, each one labelling the **body** cluster
/// `running_head` and every other cluster with any role at all. Applied through the structure
/// stage exactly as an admitted answer is: `C` is unchanged and no block leaves the flow.
///
/// The fixtures' outlines are removed first, so their headings' levels are size rank's and the
/// mappings act on them. Label authority is not deletion authority (D13.5): a `running_head`
/// label is a proposal that only furniture may act on, and furniture has already run. An implementation that let the label
/// drive a deletion fails here on the first case whose body cluster holds a block.
#[test]
fn running_head_label_never_deletes_text() {
    use oc_ai::prompt::v1::heading_roles::{HeadingRole, HeadingRolesAnswer};
    use oc_core::ledger_check::c_of_parts;
    use oc_core::thresholds::T;
    use proptest::prelude::*;
    use proptest::test_runner::{Config, TestRunner};

    const CASES_PER_FIXTURE: u32 = 200;

    for stem in FIXTURES {
        let mut prepared = prepared(stem);
        // Typst writes an outline, and an outline's levels are ground truth no mapping touches.
        // Without it the levels are size rank's — the answer the task stands in for — which is
        // where an edit acts, and so where a deletion would have to happen if one could.
        prepared.structure_input.outline.clear();
        let input = &prepared.structure_input;
        let before = oc_structure::stage::structure(input, &T);
        let c_before = c_of_parts(before.emitted_text().iter().map(String::as_str));
        let blocks_before = placed_blocks(&before);
        let clusters: Vec<u32> = before
            .inventory
            .clusters
            .iter()
            .map(|cluster| cluster.id.0)
            .collect();
        let body = before.inventory.body.map(|body| body.0);
        let heading_clusters: std::collections::BTreeSet<u32> = before
            .headings
            .iter()
            .map(|heading| heading.cluster.0)
            .collect();

        let roles = proptest::collection::vec(
            proptest::sample::select(HeadingRole::ALL.to_vec()),
            clusters.len(),
        );
        let mut runner = TestRunner::new(Config {
            cases: CASES_PER_FIXTURE,
            failure_persistence: None,
            ..Config::default()
        });
        runner
            .run(&roles, |roles| {
                let mut mapping: std::collections::BTreeMap<u32, HeadingRole> =
                    clusters.iter().copied().zip(roles).collect();
                if let Some(body) = body {
                    mapping.insert(body, HeadingRole::RunningHead);
                }
                let answer = HeadingRolesAnswer {
                    clusters: mapping,
                    probes: std::collections::BTreeMap::new(),
                };
                let edit = oc_ai::task::heading_roles::role_edit(&answer, &heading_clusters);
                let edits = oc_structure::stage::StructureEdits {
                    headings: openconvert::ai::heading_edits(&edit),
                    ..Default::default()
                };
                let after = oc_structure::stage::structure_with(input, &T, &edits);

                let c_after = c_of_parts(after.emitted_text().iter().map(String::as_str));
                prop_assert_eq!(&c_after, &c_before, "{}: C changed", stem);
                let blocks_after = placed_blocks(&after);
                prop_assert!(
                    blocks_before.is_subset(&blocks_after),
                    "{}: blocks left the flow: {:?}",
                    stem,
                    blocks_before.difference(&blocks_after).collect::<Vec<_>>()
                );
                Ok(())
            })
            .unwrap_or_else(|failure| panic!("{failure}"));
    }
}
