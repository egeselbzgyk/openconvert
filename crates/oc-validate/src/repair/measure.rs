//! `M = (fatal, error, warning)`, the measure the repair loop terminates on (D13.7, RT A10.1).
//!
//! The whole termination argument is one sentence: a repair is applied only if it strictly
//! decreases a well-founded measure, so the loop runs at most `|messages|` times. The cap of three
//! is a safety bound and **not** the argument — a loop that terminated only because someone wrote
//! `3` would be a loop nobody could reason about.
//!
//! Lexicographic, and in that order, because trading one fatal for two warnings is progress and
//! trading one warning for two fatals is not.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use crate::tier1::{Finding, Severity};

/// How bad a container is, in the only terms the loop compares.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct Measure {
    pub fatal: u32,
    pub error: u32,
    pub warning: u32,
}

impl Measure {
    /// Count a finding list.
    pub fn of(findings: &[Finding]) -> Self {
        let mut measure = Self::default();
        for finding in findings {
            match finding.severity {
                Severity::Fatal => measure.fatal += 1,
                Severity::Error => measure.error += 1,
                Severity::Warning => measure.warning += 1,
            }
        }
        measure
    }

    /// Nothing left to repair.
    pub fn is_zero(&self) -> bool {
        *self == Self::default()
    }

    /// Whether anything at or above `Error` remains — what decides `report.status == "invalid"`.
    pub fn has_errors(&self) -> bool {
        self.fatal > 0 || self.error > 0
    }
}

/// Lexicographic on `(fatal, error, warning)`.
///
/// Hand-written rather than derived, so that the field order carrying the meaning is stated where
/// a reader will look for it: a derived `Ord` would mean the same thing and would mean it by
/// accident of declaration order.
impl Ord for Measure {
    fn cmp(&self, other: &Self) -> Ordering {
        self.fatal
            .cmp(&other.fatal)
            .then_with(|| self.error.cmp(&other.error))
            .then_with(|| self.warning.cmp(&other.warning))
    }
}

impl PartialOrd for Measure {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Every message id in a finding list, sorted and without repeats.
///
/// The second half of the progress rule: a repair must strictly decrease `M` **and** introduce no
/// message id that was absent before. Strict decrease alone would accept trading two `RSC-005`s for
/// one `OPF-014` — fewer messages, a new class of defect, and a loop that can walk sideways
/// through the message space forever.
pub fn message_ids(findings: &[Finding]) -> BTreeSet<&'static str> {
    findings.iter().map(|finding| finding.id).collect()
}

/// The ids in `after` that are not in `before`.
pub fn new_ids<'a>(before: &BTreeSet<&'a str>, after: &BTreeSet<&'a str>) -> Vec<&'a str> {
    after.difference(before).copied().collect()
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). The measure's own properties; the loop's use
// of it is rows 6.4-6.6.
// ---------------------------------------------------------------------------

#[cfg(test)]
fn finding(id: &'static str, severity: Severity) -> Finding {
    Finding {
        id,
        severity,
        location: String::new(),
        detail: String::new(),
    }
}

/// Lexicographic, and in the stated order. One fatal is worse than any number of errors, and one
/// error is worse than any number of warnings — which is what makes "trade a fatal for two
/// warnings" progress and the reverse not.
#[test]
fn the_measure_is_lexicographic_in_severity_order() {
    let one_fatal = Measure {
        fatal: 1,
        error: 0,
        warning: 0,
    };
    let many_errors = Measure {
        fatal: 0,
        error: 99,
        warning: 99,
    };
    assert!(many_errors < one_fatal);

    let one_error = Measure {
        fatal: 0,
        error: 1,
        warning: 0,
    };
    let many_warnings = Measure {
        fatal: 0,
        error: 0,
        warning: 99,
    };
    assert!(many_warnings < one_error);

    assert!(Measure::default().is_zero());
    assert!(!Measure::default().has_errors());
    assert!(one_error.has_errors());
    assert!(!many_warnings.has_errors());
}

#[test]
fn the_measure_counts_a_finding_list_by_severity() {
    let findings = vec![
        finding("PKG-008", Severity::Fatal),
        finding("RSC-005", Severity::Error),
        finding("RSC-005", Severity::Error),
        finding("ACC-001", Severity::Warning),
    ];
    assert_eq!(
        Measure::of(&findings),
        Measure {
            fatal: 1,
            error: 2,
            warning: 1
        }
    );
}

/// The id set is what "no new message id" is stated over, and it is a *set*: two `RSC-005`s
/// becoming one is progress under the measure and introduces nothing new.
#[test]
fn a_new_message_id_is_the_difference_of_two_id_sets() {
    let before = message_ids(&[
        finding("RSC-005", Severity::Error),
        finding("RSC-005", Severity::Error),
    ]);
    let after = message_ids(&[
        finding("RSC-005", Severity::Error),
        finding("OPF-003", Severity::Error),
    ]);

    assert_eq!(new_ids(&before, &after), vec!["OPF-003"]);
    assert!(new_ids(&after, &before).is_empty());
}
