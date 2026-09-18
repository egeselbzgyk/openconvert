//! Every numeric constant the pipeline uses, generated from `thresholds.toml` (D17).
//!
//! Code reads thresholds through [`T`], never by string key, so a renamed or deleted
//! threshold is a compile error rather than a silent default. [`PROVENANCE`] carries the
//! `source` and `evidence` of every entry and is emitted into the conversion report, so a
//! user can see which numbers were provisional at conversion time.

use std::fmt;

include!(concat!(env!("OUT_DIR"), "/thresholds_generated.rs"));

/// `source` values that carry an obligation to revisit the number (D17).
const PROVISIONAL: &str = "provisional";

/// What is wrong with one entry's provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LintProblem {
    /// A `provisional` entry with no owner: nobody is on the hook to revisit it.
    MissingOwner,
    /// A `provisional` entry with no `review_by` date at all.
    MissingReviewBy,
    /// A `provisional` entry whose `review_by` has passed. The number is now unowned in
    /// practice even though someone's name is on it.
    ReviewExpired { review_by: String },
}

impl fmt::Display for LintProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LintProblem::MissingOwner => f.write_str("source = \"provisional\" but owner is empty"),
            LintProblem::MissingReviewBy => {
                f.write_str("source = \"provisional\" but there is no review_by")
            }
            LintProblem::ReviewExpired { review_by } => {
                write!(
                    f,
                    "review_by {review_by} has passed; re-derive the value or move the date"
                )
            }
        }
    }
}

/// One entry that fails D17's provenance rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintFinding {
    /// The dotted key, e.g. `layout.furniture.band_ratio`.
    pub key: String,
    pub problem: LintProblem,
}

/// Check `thresholds.toml` against D17: a `provisional` entry needs an owner and a
/// `review_by` that has not passed. `binary`, `published` and `calibrated` entries need
/// neither (§0.7).
///
/// `today` is passed in as an ISO `YYYY-MM-DD` date rather than read from a clock, so the
/// rule is a pure function and the caller decides what "now" means. ISO dates compare
/// correctly as strings, which is why no date type is needed here.
///
/// Returns the findings, or `Err` if the file is not parseable TOML. An empty vector means
/// the file passes.
pub fn lint(toml_text: &str, today: &str) -> Result<Vec<LintFinding>, toml::de::Error> {
    let root: toml::Value = toml::from_str(toml_text)?;
    let mut findings = Vec::new();
    if let Some(table) = root.as_table() {
        lint_table(table, &[], today, &mut findings);
    }
    Ok(findings)
}

fn lint_table(table: &toml::Table, path: &[&str], today: &str, findings: &mut Vec<LintFinding>) {
    for (key, value) in table {
        let Some(child) = value.as_table() else {
            continue;
        };
        let mut child_path = path.to_vec();
        child_path.push(key);

        if !child.contains_key("value") {
            lint_table(child, &child_path, today, findings);
            continue;
        }
        if child.get("source").and_then(toml::Value::as_str) != Some(PROVISIONAL) {
            continue;
        }

        let dotted = child_path.join(".");
        let owner = child
            .get("owner")
            .and_then(toml::Value::as_str)
            .unwrap_or_default();
        if owner.trim().is_empty() {
            findings.push(LintFinding {
                key: dotted.clone(),
                problem: LintProblem::MissingOwner,
            });
            continue;
        }
        match child.get("review_by").and_then(toml::Value::as_str) {
            None => findings.push(LintFinding {
                key: dotted,
                problem: LintProblem::MissingReviewBy,
            }),
            Some(review_by) if review_by <= today => findings.push(LintFinding {
                key: dotted,
                problem: LintProblem::ReviewExpired {
                    review_by: review_by.to_owned(),
                },
            }),
            Some(_) => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Test names are exactly the ones
// listed in the Phase 0 "Tests to write FIRST" table, rows 0.5 and 0.6.
// ---------------------------------------------------------------------------

/// The file the constants are generated from, as text, so the tests can re-parse it at
/// runtime and compare against what the build script produced.
#[cfg(test)]
const THRESHOLDS_TOML: &str = include_str!("../../../thresholds.toml");

#[test]
fn every_provisional_has_owner_and_future_review() {
    use crate::thresholds::{lint, LintProblem};

    let today = time::OffsetDateTime::now_utc().date().to_string();
    let findings = lint(THRESHOLDS_TOML, &today).expect("thresholds.toml parses");

    assert!(
        findings.is_empty(),
        "thresholds.toml has {} entries that fail D17's provenance rule (today is {today}):\n{}",
        findings.len(),
        findings
            .iter()
            .map(|f| format!("  {} — {}", f.key, f.problem))
            .collect::<Vec<_>>()
            .join("\n")
    );

    // The rule itself is checked against inputs that must fail, so a lint that silently
    // stopped finding anything cannot pass this test.
    let missing_owner = r#"
[demo.no_owner]
value = 1
source = "provisional"
evidence = "none"
owner = ""
review_by = "2999-01-01"
"#;
    let expired = r#"
[demo.expired]
value = 1
source = "provisional"
evidence = "none"
owner = "maintainer"
review_by = "2000-01-01"
"#;
    let no_review = r#"
[demo.no_review]
value = 1
source = "provisional"
evidence = "none"
owner = "maintainer"
"#;
    let published_needs_neither = r#"
[demo.published]
value = 1
source = "published"
evidence = "R2 §B.3"
owner = "maintainer"
review_by = "2000-01-01"
"#;

    assert!(matches!(
        lint(missing_owner, &today).expect("parses").as_slice(),
        [f] if f.key == "demo.no_owner" && matches!(f.problem, LintProblem::MissingOwner)
    ));
    assert!(matches!(
        lint(expired, &today).expect("parses").as_slice(),
        [f] if f.key == "demo.expired" && matches!(f.problem, LintProblem::ReviewExpired { .. })
    ));
    assert!(matches!(
        lint(no_review, &today).expect("parses").as_slice(),
        [f] if f.key == "demo.no_review" && matches!(f.problem, LintProblem::MissingReviewBy)
    ));
    assert!(
        lint(published_needs_neither, &today)
            .expect("parses")
            .is_empty(),
        "review_by is ignored for a non-provisional entry (§0.7)"
    );
}

#[test]
fn generated_constants_match_toml() {
    use crate::thresholds::{PROVENANCE, T};

    // The anchored value named in the Phase 0 test table.
    assert_eq!(T.layout.furniture.band_ratio, 0.08);

    // And it — plus one of each other type the file contains — equals the TOML re-read at
    // runtime, which is what catches a code-generation bug rather than a typo in the file.
    let toml: toml::Value = toml::from_str(THRESHOLDS_TOML).expect("thresholds.toml parses");
    let entry = |path: &str| -> &toml::Value {
        let mut node = &toml;
        for segment in path.split('.') {
            node = node
                .get(segment)
                .unwrap_or_else(|| panic!("no entry {path}"));
        }
        node.get("value")
            .unwrap_or_else(|| panic!("{path} has no value"))
    };

    assert_eq!(
        T.layout.furniture.band_ratio,
        entry("layout.furniture.band_ratio")
            .as_float()
            .expect("float")
    );
    assert_eq!(
        T.limits.max_memory_bytes,
        entry("limits.max_memory_bytes")
            .as_integer()
            .expect("integer")
    );
    assert_eq!(T.ai.enabled, entry("ai.enabled").as_bool().expect("bool"));
    assert_eq!(
        T.repair.require_strict_decrease,
        entry("repair.require_strict_decrease")
            .as_bool()
            .expect("bool")
    );

    // v1 ships with the LLM off (D17 / RT A7).
    assert!(!T.ai.enabled);

    // Provenance covers every entry in the file, keyed by its dotted path.
    let file_entries = count_entries(&toml);
    assert_eq!(PROVENANCE.len(), file_entries);
    let band = PROVENANCE
        .iter()
        .find(|entry| entry.key == "layout.furniture.band_ratio")
        .expect("band_ratio is in PROVENANCE");
    assert_eq!(band.source, "published");
    assert!(band.evidence.contains("R2 §B.4"), "{}", band.evidence);

    // Owner and `review_by` travel too, because the report's promise is that a user can see which
    // numbers were provisional *and who owes a better one* (D17).
    let provisional = PROVENANCE
        .iter()
        .find(|entry| entry.key == "validate.min_char_retention")
        .expect("min_char_retention is in PROVENANCE");
    assert_eq!(provisional.source, "provisional");
    assert_eq!(provisional.owner, "maintainer");
    assert_eq!(provisional.review_by, "2027-06-30");
}

#[cfg(test)]
fn count_entries(node: &toml::Value) -> usize {
    match node.as_table() {
        None => 0,
        Some(table) if table.contains_key("value") => 1,
        Some(table) => table.values().map(count_entries).sum(),
    }
}
