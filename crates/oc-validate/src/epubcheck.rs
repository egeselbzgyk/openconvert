//! Tier 2: EPUBCheck, run as a subprocess (D6).
//!
//! EPUBCheck is authoritative and it is Java. Bundling a JRE in the base install would cost
//! more than the whole rest of the application, so it is never on the default conversion path:
//! it is a hard CI gate, and in-app it arrives with the optional validation pack.
//!
//! This module only *runs* it and reads its JSON. The jar is never downloaded from here —
//! `oc-validate` has no network dependency and `cargo deny` enforces that (D13.9); fetching is
//! `xtask fetch-epubcheck`'s, with the SHA-256 pinned.

use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

/// One message EPUBCheck reported.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    #[serde(rename = "ID", default)]
    pub id: String,
    #[serde(rename = "severity", default)]
    pub severity: String,
    #[serde(rename = "message", default)]
    pub message: String,
}

/// What EPUBCheck said about one file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpubCheckReport {
    pub errors: Vec<Message>,
    pub warnings: Vec<Message>,
    /// Every message id reported, sorted and deduplicated — what the Tier-1 parity measurement
    /// compares against.
    pub message_ids: Vec<String>,
}

/// Why EPUBCheck could not be run or read.
#[derive(Debug, thiserror::Error)]
pub enum EpubCheckError {
    #[error("could not run java: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("epubcheck produced no readable JSON: {0}")]
    Output(String),
}

/// Run EPUBCheck over one file and read its JSON report.
///
/// `--failonwarnings` is deliberately not passed. The gate is `epubcheck.max_errors = 0`, and
/// EPUBCheck's warnings include things a PDF-derived book cannot help — an `.opf` with no
/// `dc:date`, a missing cover — which would turn a release gate into a nagging one.
pub fn run(jar: &Path, epub: &Path) -> Result<EpubCheckReport, EpubCheckError> {
    let output = Command::new(java())
        .arg("-jar")
        .arg(jar)
        .arg("--json")
        .arg("-")
        .arg("--quiet")
        .arg(epub)
        .output()?;

    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    parse(&text)
}

/// The JVM to use: `JAVA_HOME` when the environment names one, `java` otherwise.
fn java() -> std::ffi::OsString {
    match std::env::var_os("JAVA_HOME") {
        Some(home) => {
            let mut path = std::path::PathBuf::from(home);
            path.push("bin");
            path.push("java");
            path.into_os_string()
        }
        None => "java".into(),
    }
}

/// Read EPUBCheck's `--json` report.
///
/// Only the message list is taken. EPUBCheck's JSON carries a publication summary and a
/// per-file breakdown as well, and reading those would tie this module to a schema that moves
/// between releases for no gain: the gate is a count of errors and the parity measurement is a
/// set of ids.
pub fn parse(text: &str) -> Result<EpubCheckReport, EpubCheckError> {
    let value: serde_json::Value = serde_json::from_str(text.trim())
        .map_err(|error| EpubCheckError::Output(error.to_string()))?;

    let messages: Vec<Message> = value
        .get("messages")
        .and_then(|messages| serde_json::from_value(messages.clone()).ok())
        .unwrap_or_default();

    let errors: Vec<Message> = messages
        .iter()
        .filter(|message| is_at_least_error(&message.severity))
        .cloned()
        .collect();
    let warnings: Vec<Message> = messages
        .iter()
        .filter(|message| message.severity.eq_ignore_ascii_case("WARNING"))
        .cloned()
        .collect();

    let mut message_ids: Vec<String> = messages
        .iter()
        .map(|message| message.id.clone())
        .filter(|id| !id.is_empty())
        .collect();
    message_ids.sort();
    message_ids.dedup();

    Ok(EpubCheckReport {
        errors,
        warnings,
        message_ids,
    })
}

/// Whether a severity counts against `epubcheck.max_errors`.
fn is_at_least_error(severity: &str) -> bool {
    severity.eq_ignore_ascii_case("ERROR") || severity.eq_ignore_ascii_case("FATAL")
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2)
// ---------------------------------------------------------------------------

/// The reader is the part that can be tested without a JVM, and it is the part that would
/// silently return "zero errors" if the schema moved — which would turn the release gate off
/// without anything failing.
#[test]
fn a_report_with_errors_is_not_read_as_a_clean_one() {
    let json = r#"{
      "messages": [
        {"ID": "RSC-005", "severity": "ERROR", "message": "malformed"},
        {"ID": "OPF-003", "severity": "WARNING", "message": "unused"},
        {"ID": "PKG-007", "severity": "FATAL", "message": "mimetype"}
      ]
    }"#;

    let report = parse(json).expect("the report reads");
    assert_eq!(report.errors.len(), 2, "FATAL counts as an error");
    assert_eq!(report.warnings.len(), 1);
    assert_eq!(report.message_ids, vec!["OPF-003", "PKG-007", "RSC-005"]);
}

/// A clean run reports nothing, and a report that is not JSON at all is an error rather than
/// an empty message list — the difference between "EPUBCheck found nothing" and "EPUBCheck did
/// not run" is the whole value of the gate.
#[test]
fn an_unreadable_report_is_an_error_and_not_an_empty_one() {
    assert!(parse(r#"{"messages": []}"#)
        .expect("reads")
        .errors
        .is_empty());
    assert!(parse("Exception in thread \"main\"").is_err());
}
