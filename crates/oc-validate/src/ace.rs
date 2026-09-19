//! Tier 3: Ace by DAISY, run as a subprocess (D6, PIPELINE §11).
//!
//! Ace checks what EPUBCheck does not: whether the book is *usable*, by the rules WCAG and EPUB
//! Accessibility 1.1 state. It is a Node application, so it is in CI and never on the default
//! conversion path — the same argument D6 makes about EPUBCheck being Java, for the same reason.
//!
//! The gate has two halves and PIPELINE §11 states both: **zero serious violations, plus all
//! required accessibility metadata fields present.** The second half is not a violation Ace fails a
//! book for — a book with no `schema:accessMode` is "unknown", not "failing" — but Ace does compute
//! it, in the `a11y-metadata` block, partitioned into `present`, `missing` and `empty`. That list is
//! what this module reads, so the two halves of the gate come from the same source. A book whose
//! accessibility metadata is absent is a book a reader with a screen reader cannot decide about
//! before opening it, which is the whole purpose of the metadata.
//!
//! **What Ace found on its first real run** (nightly CI 35430065404), all three fixed in `oc-epub`
//! rather than excused here: no `pageBreakSource` on a book that publishes page numbers
//! (`epub-pagesource`, *serious*); `schema:accessModeSufficient` withheld from every book that had
//! no images, the condition inverted (`metadata-accessmodesufficient`); and `<section
//! epub:type="chapter">` with no `role="doc-chapter"`, on every content document
//! (`epub-type-has-matching-role`). A gate whose first run finds three real defects is a gate worth
//! having.
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
    /// Ace ran and left no report behind. Carries its exit status and **its own output**, because
    /// the first version of this error said only "EOF while parsing a value at line 1 column 0" —
    /// which is true, useless, and cost a nightly CI cycle to get past. When a subprocess fails,
    /// what it said is the entire diagnosis.
    #[error(
        "ace exited with {status} and wrote no report to {report_path}\n\
         --- ace stderr ---\n{stderr}\n--- ace stdout ---\n{stdout}"
    )]
    NoReport {
        status: String,
        report_path: String,
        stdout: String,
        stderr: String,
    },
    #[error("ace produced no readable JSON: {0}")]
    Output(String),
}

/// Run Ace over one container and read its report.
///
/// `command` is the executable to run — `ace` on a PATH, or a path to one — because Ace is installed
/// by the CI job and its location is the job's business, not this module's.
///
/// `--force` is passed so that an `--outdir` which already exists is overwritten rather than
/// refused, and `--silent` is **not**: Ace's own diagnosis is the only useful thing to have when it
/// fails, and suppressing it is how a subprocess failure becomes unreadable.
pub fn run(command: &Path, epub: &Path, out_dir: &Path) -> Result<AceReport, AceError> {
    let invoke = |program: &Path| {
        Command::new(program)
            .arg("--outdir")
            .arg(out_dir)
            .arg("--force")
            .arg(epub)
            .output()
    };

    let output = match invoke(command) {
        Ok(output) => output,
        // npm installs a global `ace` as a `.cmd` shim on Windows, and `Command::new` searches
        // `PATHEXT` for `.exe` but never resolves a shim by its bare name. The CI job runs on
        // Linux, where `ace` is a symlink and the first attempt succeeds — but a maintainer
        // debugging this gate is as likely to be on Windows, and "program not found" for a program
        // that is plainly on the PATH is a wasted afternoon.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut shim = command.as_os_str().to_os_string();
            shim.push(".cmd");
            invoke(Path::new(&shim)).or(Err(error))?
        }
        Err(error) => return Err(error.into()),
    };

    // Ace writes `report.json` into `--outdir`; the file is authoritative. When it is not there,
    // the failure is Ace's and what Ace said about it travels with the error.
    let report_path = out_dir.join("report.json");
    let text = match std::fs::read_to_string(&report_path) {
        Ok(text) => text,
        Err(_) => {
            return Err(AceError::NoReport {
                status: output.status.to_string(),
                report_path: report_path.display().to_string(),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            })
        }
    };
    parse(&text)
}

/// Read Ace's `report.json`.
///
/// Two things are taken and nothing else: the violation list under `assertions`, and the
/// **`a11y-metadata`** block — which is Ace's own answer to the metadata question, partitioned into
/// `present`, `missing` and `empty`. Ace's report also carries an outline, an image inventory and a
/// properties table, and reading those would tie this module to a schema that moves between
/// releases for no gain.
///
/// **This read `data.metadata` first, and that key does not exist.** Every required field came back
/// missing on every book, and the unit tests agreed because their fixture JSON was written from the
/// same guess rather than from a report Ace had produced. The fixture below is a real one, trimmed.
/// A parser for somebody else's format has to be tested against their output, not against one's
/// reading of their documentation.
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

    // `present` is Ace's list, and `empty` is deliberately not folded into it: a property declared
    // with an empty value is present in the markup and absent in every sense a reader cares about.
    let metadata = value.get("a11y-metadata");
    let present: BTreeSet<&str> = metadata
        .and_then(|block| block.get("present"))
        .and_then(serde_json::Value::as_array)
        .map(|names| names.iter().filter_map(serde_json::Value::as_str).collect())
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

/// A **real** Ace report, trimmed to the keys the parser reads.
///
/// Copied from `report.json` as Ace 1.3 wrote it, not composed from its documentation. The first
/// version of this constant was composed, put the metadata under `data.metadata`, and so tested the
/// parser against the same misreading the parser was built on — which is how a gate ends up
/// reporting every required field missing on every book and nothing failing (nightly CI 35430065404).
///
/// The `a11y:certifier*` and `dcterms:conformsTo` entries in `missing` are Ace's, and they are
/// *correct*: this converter deliberately certifies nothing. They are here so the fixture exercises
/// the case where `missing` is non-empty and the gate must still pass.
#[cfg(test)]
const REPORT: &str = r#"{
  "assertions": [
    {
      "earl:testSubject": {"url": "content.opf"},
      "assertions": [
        {
          "earl:test": {
            "dct:title": "epub-pagesource",
            "earl:impact": "serious",
            "dct:isPartOf": null
          },
          "earl:result": {"earl:outcome": "fail"}
        },
        {
          "earl:test": {
            "dct:title": "metadata-accessmodesufficient",
            "earl:impact": "moderate",
            "dct:isPartOf": null
          },
          "earl:result": {"earl:outcome": "fail"}
        }
      ]
    },
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
            "dct:title": "html-has-lang",
            "earl:impact": "serious",
            "dct:isPartOf": "WCAG2A"
          },
          "earl:result": {"earl:outcome": "pass"}
        }
      ]
    }
  ],
  "a11y-metadata": {
    "missing": [
      "a11y:certifiedBy",
      "a11y:certifierCredential",
      "a11y:certifierReport",
      "dcterms:conformsTo"
    ],
    "empty": [],
    "present": [
      "schema:accessMode",
      "schema:accessibilityFeature",
      "schema:accessibilityHazard",
      "schema:accessibilitySummary",
      "schema:accessModeSufficient"
    ]
  }
}"#;

/// A `critical` violation counts against the gate, and a rule that *passed* is not a violation.
///
/// A check written against the word "serious" alone would pass a book with a critical violation in
/// it, which is the wrong way round.
#[test]
fn a_critical_violation_counts_as_a_serious_one() {
    let report = parse(REPORT).expect("the report reads");

    assert_eq!(
        report.violations.len(),
        3,
        "two on the package document and one on the content document; the passing rule is not a \
         violation: {:?}",
        report.violations
    );

    let serious: Vec<&str> = report
        .serious()
        .iter()
        .map(|violation| violation.rule.as_str())
        .collect();
    assert_eq!(serious, vec!["epub-pagesource", "image-alt"]);
    assert!(!report.passes(0));
    assert!(report.passes(2), "the bound decides, not the word");
}

/// The metadata half of the gate, read from **Ace's own** `a11y-metadata.present` list.
///
/// PIPELINE §11 asks for "zero serious violations **plus** all required accessibility metadata
/// fields present", and a book with no `schema:accessMode` is not a violation Ace reports — it is a
/// book a screen-reader user cannot decide about before opening it.
///
/// The fixture's `missing` list is non-empty and the gate still passes on it, deliberately: the
/// entries there are `a11y:certifiedBy` and friends, which this converter declines to claim.
#[test]
fn missing_accessibility_metadata_fails_the_gate_on_its_own() {
    let complete = parse(REPORT).expect("reads");
    assert!(
        complete.missing_metadata.is_empty(),
        "Ace lists all four as present: {complete:?}"
    );

    // Ace moves a property it did not find from `present` to `missing`, which is the shape the
    // emitter bug produced: `schema:accessModeSufficient` was withheld from every book with no
    // images.
    let stripped = REPORT.replace("\"schema:accessibilitySummary\",\n      ", "");
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
        r#"{"assertions": [], "a11y-metadata": {"missing": [], "empty": [], "present": [
             "schema:accessMode", "schema:accessibilityFeature",
             "schema:accessibilityHazard", "schema:accessibilitySummary"]}}"#,
    )
    .expect("reads");
    assert!(clean.violations.is_empty());
    assert!(clean.passes(0));

    assert!(parse("Error: Cannot find module '@daisy/ace'").is_err());
    // And an empty report is not a pass: no `a11y-metadata` block means nothing is known to be
    // present, and "nothing is known" must never read as "everything is fine".
    let empty = parse("{}").expect("reads");
    assert_eq!(empty.missing_metadata.len(), REQUIRED_METADATA.len());
    assert!(!empty.passes(0));
}

/// A property Ace found but found *empty* is not present. A declared-and-blank
/// `schema:accessibilitySummary` satisfies a checker that only looks for the element and tells a
/// reader nothing, which is the failure this distinction exists to keep.
#[test]
fn a_declared_but_empty_property_does_not_count_as_present() {
    let report = parse(
        r#"{"assertions": [], "a11y-metadata": {
             "missing": [], "empty": ["schema:accessibilitySummary"],
             "present": ["schema:accessMode", "schema:accessibilityFeature",
                         "schema:accessibilityHazard"]}}"#,
    )
    .expect("reads");

    assert_eq!(report.missing_metadata, vec!["schema:accessibilitySummary"]);
    assert!(!report.passes(0));
}
