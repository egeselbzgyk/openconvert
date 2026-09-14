//! Phase 5 rows 5.17 and 5.18: the EPUBCheck gate, and the parity number that must not fall.
//!
//! Row 5.17 needs a JVM and a jar, so it is behind the `epubcheck` cargo feature rather than
//! an ignore attribute — a feature is something CI turns on and a developer can turn on,
//! while an ignored test is one nobody ever runs again. `cargo run -p xtask -- fetch-epubcheck` puts the
//! jar where this looks for it.

mod common;

/// The fixtures the gate runs over.
#[cfg(feature = "epubcheck")]
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

/// Row 5.17, and acceptance criterion A5.1. EPUBCheck is authoritative, and
/// `epubcheck.max_errors = 0` is a binary threshold: this is the one gate in the phase that is
/// not our own opinion of our own output.
#[cfg(feature = "epubcheck")]
#[test]
fn epubcheck_zero_errors_on_all_fixtures() {
    let jar = jar();
    let directory = std::env::temp_dir().join(format!("oc-epubcheck-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("the scratch directory is made");

    let mut failures = Vec::new();
    for stem in FIXTURES {
        let built = common::build(stem);
        let path = directory.join(format!("{stem}.epub"));
        std::fs::write(&path, built.built.bytes.as_slice()).expect("the EPUB writes");

        let report = oc_validate::epubcheck::run(&jar, &path).expect("epubcheck runs");
        let allowed = usize::try_from(oc_core::thresholds::T.epubcheck.max_errors).unwrap_or(0);
        if report.errors.len() > allowed {
            failures.push(format!(
                "{stem}: {} error(s): {:#?}",
                report.errors.len(),
                report.errors
            ));
        }
    }

    let _ = std::fs::remove_dir_all(&directory);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Where `xtask fetch-epubcheck` leaves the jar, or wherever `OC_EPUBCHECK_JAR` says.
#[cfg(feature = "epubcheck")]
fn jar() -> std::path::PathBuf {
    if let Some(path) = std::env::var_os("OC_EPUBCHECK_JAR") {
        return std::path::PathBuf::from(path);
    }
    let vendored = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor/epubcheck/epubcheck-5.3.0/epubcheck.jar");
    assert!(
        vendored.is_file(),
        "no epubcheck jar at {}; run `cargo run -p xtask -- fetch-epubcheck` \
         or set OC_EPUBCHECK_JAR",
        vendored.display()
    );
    vendored
}

/// Row 5.18. The measurement itself is a CI job — four hundred JVM launches is not a unit test
/// — and `xtask epubcheck-parity --check` is what compares it. What this asserts is that the
/// gate has something to read: a `TIER1_PARITY.md` with no number in it would make the
/// non-decreasing check pass on anything, silently, which is exactly the failure RT A6.2 is
/// about.
#[test]
fn tier1_parity_does_not_regress() {
    let path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/TIER1_PARITY.md");
    let markdown = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "{}: {error}; run `cargo run -p xtask -- epubcheck-parity`",
            path.display()
        )
    });

    let (caught, total) = parity_line(&markdown)
        .unwrap_or_else(|| panic!("{} records no PARITY line", path.display()));

    assert!(total > 0, "the corpus the number came from was not empty");
    assert!(caught <= total);
    assert!(
        markdown.contains("| id | EPUBCheck reported | Tier 1 agreed |"),
        "the per-message-id table is what makes the gap a work item rather than a number"
    );
}

/// The `PARITY: caught/total` line.
///
/// Parsed here rather than by calling into `xtask`, which would drag Typst and a PDF renderer
/// into this test binary for four lines of string handling. `xtask::epubcheck_parity::recorded`
/// is the authority — it is what the gate uses — and `epubcheck_parity`'s own round-trip test
/// is what keeps the two spellings honest.
fn parity_line(markdown: &str) -> Option<(usize, usize)> {
    let line = markdown.lines().find(|line| line.starts_with("PARITY: "))?;
    let fraction = line.trim_start_matches("PARITY: ").split(' ').next()?;
    let (caught, total) = fraction.split_once('/')?;
    Some((caught.parse().ok()?, total.parse().ok()?))
}
