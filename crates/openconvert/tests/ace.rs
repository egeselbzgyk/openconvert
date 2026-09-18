//! Tier 3: Ace by DAISY over every fixture, behind the `ace` cargo feature.
//!
//! Ace is a Node application, so the gate is a feature rather than an ignore attribute — a feature
//! is something CI turns on and a developer can turn on, while an ignored test is one nobody ever
//! runs again (IMPLEMENTATION_PLAN §0.2). The nightly `ace-a11y` job installs `@daisy/ace` and turns
//! it on.
//!
//! Both halves of PIPELINE §11's gate are asserted: zero serious violations, and every required
//! accessibility metadata field present. The second is the one a converter can get wrong silently —
//! a book with no `schema:accessMode` passes every structural check and leaves a screen-reader user
//! unable to decide about it before opening it.

mod common;

/// The fixtures the gate runs over.
#[cfg(feature = "ace")]
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

/// Zero serious violations, and the metadata complete, on every fixture (D6 Tier 3, PIPELINE §11).
#[cfg(feature = "ace")]
#[test]
fn ace_zero_serious_violations_on_all_fixtures() {
    let command = ace_command();
    let directory = std::env::temp_dir().join(format!("oc-ace-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("the scratch directory is made");

    let max_serious = u64::try_from(oc_core::thresholds::T.ace.max_serious_violations).unwrap_or(0);
    let mut failures = Vec::new();

    for stem in FIXTURES {
        let built = common::build(stem);
        let path = directory.join(format!("{stem}.epub"));
        std::fs::write(&path, built.built.bytes.as_slice()).expect("the EPUB writes");
        let out_dir = directory.join(stem);
        std::fs::create_dir_all(&out_dir).expect("the report directory is made");

        let report = oc_validate::ace::run(&command, &path, &out_dir).expect("ace runs");
        if !report.passes(max_serious) {
            failures.push(format!(
                "{stem}: {} serious violation(s) {:#?}; missing metadata {:?}",
                report.serious().len(),
                report.serious(),
                report.missing_metadata
            ));
        }
    }

    let _ = std::fs::remove_dir_all(&directory);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// `OC_ACE` when the environment names an executable, `ace` on the PATH otherwise.
#[cfg(feature = "ace")]
fn ace_command() -> std::path::PathBuf {
    std::env::var_os("OC_ACE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("ace"))
}

/// The metadata half of the gate, without Ace.
///
/// Ace reads these properties out of the package document, so what the gate really asserts about them
/// is that **the emitter writes them** — and that is checkable here, on every fixture, with no Node
/// installed. Without this the metadata claim would only ever be tested in a nightly job.
#[test]
fn every_fixture_carries_the_accessibility_metadata_ace_requires() {
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

    for stem in FIXTURES {
        let built = common::build(stem);
        let opf = built.text_file("content.opf");
        for required in oc_validate::ace::REQUIRED_METADATA {
            assert!(
                opf.contains(&format!("property=\"{required}\"")),
                "{stem}: the package document has no {required}"
            );
        }
    }
}
