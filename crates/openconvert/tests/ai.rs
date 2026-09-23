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
