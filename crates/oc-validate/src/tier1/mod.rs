//! Tier 1: the internal validator, always on, no external process (D6).
//!
//! Two jobs. The first is the generic one — OCF structure, OPF metadata, manifest↔spine
//! integrity, XHTML well-formedness — which covers the **RSC-005 / RSC-012 / OPF-014 /
//! PKG-007** classes that generated EPUBs trip most (R5 §B6). The second is the one no generic
//! validator has, because it is about *books made from PDFs*: the `noteref`↔`footnote`
//! bijection, page-list resolution, non-empty alt text, no scripts, no remote resources, no
//! entity declarations, and image-count parity with extraction (RT A6).
//!
//! Every check reads the **archive**, never the emitter's intentions. A validator that asked
//! the emitter what it wrote would agree with it about everything, including the bugs — which
//! is the whole reason Tier 1 exists rather than a set of assertions inside `oc-epub`.
//!
//! Tier 1's coverage is a measured number, not a claim: `xtask epubcheck-parity` runs it over
//! EPUBCheck's own public test corpus and records per-message-id parity in
//! `docs/TIER1_PARITY.md` (RT A6.2). The number may start low; it may never silently regress.

mod book;
mod ocf;
pub(crate) mod opf;
mod xhtml;

use std::collections::BTreeMap;

use oc_epub::EpubBytes;

/// How bad a finding is.
///
/// The same three levels the repair loop's lexicographic measure `M = (fatal, error, warning)`
/// is built on (D13.7), so a report can be compared against the one before it without
/// translation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Warning,
    Error,
    Fatal,
}

/// One thing wrong with a container.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Finding {
    /// The message id. EPUBCheck's own where the check has a counterpart there, so that the
    /// repair table and the parity measurement key on one vocabulary; `OC-…` where it does not.
    pub id: &'static str,
    pub severity: Severity,
    /// Where in the container: a path, optionally with a fragment.
    pub location: String,
    pub detail: String,
}

impl Finding {
    fn new(
        id: &'static str,
        severity: Severity,
        location: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            id,
            severity,
            location: location.into(),
            detail: detail.into(),
        }
    }
}

/// What Tier 1 found, and what it looked at.
///
/// `checked` is not decoration. "Tier 1 passed" means nothing unless it also says what ran:
/// a container so broken that the package document could not be parsed skips every check that
/// reads it, and a report that did not say so would read as a clean bill of health.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct Tier1Report {
    pub findings: Vec<Finding>,
    pub checked: Vec<&'static str>,
}

impl Tier1Report {
    /// Whether nothing worse than a warning was found.
    pub fn is_valid(&self) -> bool {
        !self
            .findings
            .iter()
            .any(|finding| finding.severity >= Severity::Error)
    }

    /// How many findings there are at or above `Error`.
    pub fn error_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.severity >= Severity::Error)
            .count()
    }

    /// Whether a message id was reported.
    pub fn has(&self, id: &str) -> bool {
        self.findings.iter().any(|finding| finding.id == id)
    }

    /// Every message id reported, sorted and without repeats — what the parity measurement
    /// compares against EPUBCheck's own.
    pub fn message_ids(&self) -> Vec<&'static str> {
        let mut ids: Vec<&'static str> = self.findings.iter().map(|finding| finding.id).collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    fn push(&mut self, finding: Finding) {
        self.findings.push(finding);
    }

    fn ran(&mut self, check: &'static str) {
        self.checked.push(check);
    }
}

/// What the pipeline knows that the container cannot say for itself.
#[derive(Clone, Copy, Debug, Default)]
pub struct Expectations {
    /// How many images `ingest` extracted and the emitter was asked to carry.
    ///
    /// Marker loses about 14 % of images on some documents with no error and no log line
    /// (R1 §A.6). Count in, count out, assert.
    pub images: Option<u32>,
}

/// Run every Tier-1 check over a container.
pub fn validate_tier1(epub: &EpubBytes, expected: &Expectations) -> Tier1Report {
    let mut report = Tier1Report::default();

    // The archive first, because every other check needs the files out of it. A container that
    // will not open is one fatal finding and the end of the run: continuing would report the
    // absence of a package document as a second, unrelated problem.
    let entries: BTreeMap<String, Vec<u8>> = match oc_epub::read_entries(epub) {
        Ok(entries) => entries,
        Err(error) => {
            report.push(Finding::new(
                "PKG-008",
                Severity::Fatal,
                "",
                format!("the container is not a readable zip: {error}"),
            ));
            return report;
        }
    };

    ocf::check(epub, &entries, &mut report);

    let Some(package) = opf::check(&entries, &mut report) else {
        return report;
    };

    xhtml::check(&entries, &package, &mut report);
    book::check(&entries, &package, expected, &mut report);

    report
}
