//! Tier 3: Ace by DAISY, run as a subprocess (D6, PIPELINE §11).
//!
//! Ace checks what EPUBCheck does not: whether the book is *usable*, by the rules WCAG and EPUB
//! Accessibility 1.1 state. It is a Node application, so it is in CI and never on the default
//! conversion path — the same argument D6 makes about EPUBCheck being Java, for the same reason.
//!
//! The gate has two halves and PIPELINE §11 states both: **zero serious violations, plus all
//! required accessibility metadata fields present.** The second half is not a violation Ace reports
//! by default — a book with no `schema:accessMode` is "unknown", not "failing" — so it is checked
//! here against the set EPUB Accessibility 1.1 requires. A book whose accessibility metadata is
//! absent is a book a reader with a screen reader cannot decide about before opening it, which is
//! the whole purpose of the metadata.
//!
//! This module only *runs* Ace and reads its JSON. Nothing here downloads anything: `oc-validate`
//! has no network dependency and `cargo deny` enforces that (D13.9).

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

/// One violation Ace reported.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Violation {
    /// The rule's id, e.g. `image-alt` or `epub-type-has-matching-role`.
    pub rule: String,
    /// `minor`, `moderate`, `serious` or `critical`, as `axe-core` grades it.
    pub impact: String,
    /// Which specification the rule comes from, when Ace says.
    pub kind: String,
}

/// The accessibility metadata EPUB Accessibility 1.1 requires of a publication.
///
/// `dcterms:conformsTo` is deliberately absent: in 1.1 that property is a *claim* of conformance to
/// a named WCAG level, and a converter that asserted one on its own output would be certifying
/// itself. `oc-epub`'s own documentation makes the same point where it declines to emit it.
pub const REQUIRED_METADATA: [&str; 4] = [
    "schema:accessMode",
    "schema:accessibilityFeature",
    "schema:accessibilityHazard",
    "schema:accessibilitySummary",
];

/// What Ace said, plus the metadata half of the gate.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AceReport {
    pub violations: Vec<Violation>,
    /// The required metadata properties the publication does **not** carry, sorted.
    pub missing_metadata: Vec<String>,
}

impl AceReport {
    /// Violations Ace graded `serious` or `critical` — what `ace.max_serious_violations` bounds.
    ///
    /// `critical` counts. A gate written against the word "serious" alone would pass a book with a
    /// critical violation in it, which is the wrong way round.
    pub fn serious(&self) -> Vec<&Violation> {
        self.violations
            .iter()
            .filter(|violation| is_serious(&violation.impact))
            .collect()
    }

    /// Whether both halves of PIPELINE §11's gate hold.
    pub fn passes(&self, max_serious: u64) -> bool {
        self.serious().len() as u64 <= max_serious && self.missing_metadata.is_empty()
    }
}

/// Why Ace could not be run or read.
#[derive(Debug, thiserror::Error)]
pub enum AceError {
    #[error("could not run ace: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("ace produced no readable JSON: {0}")]
    Output(String),
}

/// Run Ace over one container and read its report.
///
/// `command` is the executable to run — `ace` on a PATH, or a path to one — because Ace is installed
/// by the CI job and its location is the job's business, not this module's.
pub fn run(command: &Path, epub: &Path, out_dir: &Path) -> Result<AceReport, AceError> {
    let output = Command::new(command)
        .arg("--outdir")
        .arg(out_dir)
        .arg("--silent")
        .arg(epub)
        .output()?;

    // Ace writes `report.json` into `--outdir` and prints a summary; the file is authoritative.
    let report_path = out_dir.join("report.json");
    let text = match std::fs::read_to_string(&report_path) {
        Ok(text) => text,
        Err(_) => String::from_utf8_lossy(&output.stdout).into_owned(),
    };
    parse(&text)
}

/// Read Ace's `report.json`.
///
/// Two things are taken and nothing else: the violation list, and the `earl:assertions`-adjacent
/// metadata block Ace copies out of the package document. Ace's report also carries an outline, a
/// per-file image inventory and a data table, and reading those would tie this module to a schema
/// that moves between releases for no gain.
pub fn parse(text: &str) -> Result<AceReport, AceError> {
    let value: serde_json::Value =
        serde_json::from_str(text.trim()).map_err(|error| AceError::Output(error.to_string()))?;

    let violations: Vec<Violation> = value
        .get("assertions")
        .and_then(serde_json::Value::as_array)
        .map(|assertions| {
            assertions
                .iter()
                .filter_map(|assertion| assertion.get("assertions"))
                .filter_map(serde_json::Value::as_array)
                .flatten()
                .filter_map(violation)
                .collect()
        })
        .unwrap_or_default();

    let present: BTreeSet<String> = value
        .get("data")
        .and_then(|data| data.get("metadata"))
        .and_then(serde_json::Value::as_object)
        .map(|metadata| metadata.keys().cloned().collect())
        .unwrap_or_default();
    let missing_metadata: Vec<String> = REQUIRED_METADATA
        .iter()
        .filter(|required| !present.contains(**required))
        .map(|required| (*required).to_owned())
        .collect();

    Ok(AceReport {
        violations,
        missing_metadata,
    })
}

/// One assertion, as a violation, when it carries the fields that make it one.
fn violation(assertion: &serde_json::Value) -> Option<Violation> {
    let test = assertion.get("earl:test")?;
    let rule = test
        .get("dct:title")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let impact = test
        .get("earl:impact")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let kind = test
        .get("dct:isPartOf")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();

    // `earl:result` / `earl:outcome` is `fail` for a violation and `pass` for a rule that held.
    let outcome = assertion
        .get("earl:result")
        .and_then(|result| result.get("earl:outcome"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("fail");
    if !outcome.eq_ignore_ascii_case("fail") {
        return None;
    }

    Some(Violation { rule, impact, kind })
}

fn is_serious(impact: &str) -> bool {
    impact.eq_ignore_ascii_case("serious") || impact.eq_ignore_ascii_case("critical")
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). The reader, which is the part that can be
// tested without Node — and the part that would silently report "zero violations" if the schema
// moved, turning the gate off without anything failing.
// ---------------------------------------------------------------------------

#[cfg(test)]
const REPORT: &str = r#"{
  "assertions": [
    {
      "earl:testSubject": {"url": "text/c0001.xhtml"},
      "assertions": [
        {
          "earl:test": {
            "dct:title": "image-alt",
            "earl:impact": "critical",
            "dct:isPartOf": "WCAG2AA"
          },
          "earl:result": {"earl:outcome": "fail"}
        },
        {
          "earl:test": {
            "dct:title": "landmark-one-main",
            "earl:impact": "moderate",
            "dct:isPartOf": "best-practice"
          },
          "earl:result": {"earl:outcome": "fail"}
        },
        {
          "earl:test": {
            "dct:title": "html-has-lang",
            "earl:impact": "serious",
            "dct:isPartOf": "WCAG2A"
          },
          "earl:result": {"earl:outcome": "pass"}
        }
      ]
    }
  ],
  "data": {
    "metadata": {
      "schema:accessMode": ["textual"],
      "schema:accessibilityFeature": ["readingOrder"],
      "schema:accessibilityHazard": ["none"],
      "schema:accessibilitySummary": ["Converted from PDF by OpenConvert."]
    }
  }
}"#;

/// A `critical` violation counts against the gate. A check written against the word "serious"
/// alone would pass a book with a critical violation in it, which is the wrong way round.
#[test]
fn a_critical_violation_counts_as_a_serious_one() {
    let report = parse(REPORT).expect("the report reads");

    assert_eq!(
        report.violations.len(),
        2,
        "a passing rule is not a violation"
    );
    let serious = report.serious();
    assert_eq!(serious.len(), 1, "{serious:?}");
    assert_eq!(serious[0].rule, "image-alt");
    assert_eq!(serious[0].impact, "critical");
    assert!(!report.passes(0));
    assert!(report.passes(1), "the bound is what decides, not the word");
}

/// The metadata half of the gate. PIPELINE §11 asks for "zero serious violations **plus** all
/// required accessibility metadata fields present", and a book with no `schema:accessMode` is not a
/// violation Ace reports — it is a book a screen-reader user cannot decide about before opening it.
#[test]
fn missing_accessibility_metadata_fails_the_gate_on_its_own() {
    let complete = parse(REPORT).expect("reads");
    assert!(complete.missing_metadata.is_empty(), "{complete:?}");

    let stripped = REPORT.replace(
        "\"schema:accessibilitySummary\"",
        "\"schema:somethingElse\"",
    );
    let report = parse(&stripped).expect("reads");
    assert_eq!(report.missing_metadata, vec!["schema:accessibilitySummary"]);
    assert!(
        !report.passes(u64::MAX),
        "no violation bound excuses absent metadata"
    );
}

/// A clean run reports nothing, and a report that is not JSON at all is an error rather than an
/// empty violation list — the difference between "Ace found nothing" and "Ace did not run" is the
/// whole value of the gate.
#[test]
fn an_unreadable_report_is_an_error_and_not_an_empty_one() {
    let clean = parse(
        r#"{"assertions": [], "data": {"metadata": {
             "schema:accessMode": ["textual"],
             "schema:accessibilityFeature": ["readingOrder"],
             "schema:accessibilityHazard": ["none"],
             "schema:accessibilitySummary": ["x"]}}}"#,
    )
    .expect("reads");
    assert!(clean.violations.is_empty());
    assert!(clean.passes(0));

    assert!(parse("Error: Cannot find module '@daisy/ace'").is_err());
    // And an empty report is not a pass: no metadata block means every required field is missing.
    let empty = parse("{}").expect("reads");
    assert_eq!(empty.missing_metadata.len(), REQUIRED_METADATA.len());
    assert!(!empty.passes(0));
}
