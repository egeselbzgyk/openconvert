//! The static message-id → repair table (D13.7, PIPELINE §12, R10 §6.19).
//!
//! **A table, not an inference.** The set of message ids is finite, documented and versioned, so a
//! lookup is 100 % accurate on the ids it covers, instant, unit-testable and reviewable. A model
//! here would put nondeterminism into the one layer that has to be trustworthy and would
//! occasionally repair a valid EPUB into an invalid one.
//!
//! **An unmapped id is never guessed.** It is logged verbatim, surfaced to the user, and counted:
//! the table's coverage is a quality metric and that is the point, not an embarrassment.
//!
//! **The table is deliberately small, and it should stay small.** Every repair that fires is a bug
//! in our emitter (ARCHITECTURE §7.3), so the corpus-wide fire rate is a release gate with target
//! zero. Writing thirty speculative repairs for ids this emitter has never produced would be thirty
//! untested code paths with nothing to test them against; what is here is the set for which a
//! *structural, character-conserving* fix exists and a `Document` defect could still produce it.
//! Everything else is `WarnUser` — named, reported, not guessed at.

use crate::tier1::Finding;

/// What the table says to do about a message id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Remedy {
    /// A deterministic structural edit to the `Document`, followed by a re-emit.
    AutoFix(Fix),
    /// The defect is real and no structural fix exists that would not change the book. Reported to
    /// the user with the id verbatim.
    WarnUser,
}

/// The repairs v1 can make. Each is **Conserving**: not one of them changes the character content
/// of the book (PIPELINE §12).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Fix {
    /// Give a figure whose `alt` is empty the fallback description.
    ///
    /// Conserving because `alt` is an attribute value and attribute text is outside `C`
    /// (ARCHITECTURE §5.2). An empty `alt` is EPUB's marker for a decorative image, so a
    /// content image carrying one is a claim about the book that is false.
    FigureAlt,
    /// Turn a note reference whose target does not exist into plain text.
    ///
    /// Conserving because the marker's *text* stays exactly where it was: what is removed is the
    /// `noteref`, which is a link and not a character. The alternative — inventing the note it
    /// points at — would put text in the book that the PDF does not contain.
    DemoteDanglingNoteref,
    /// Drop a figure that nothing in the flow references.
    ///
    /// Conserving only when the figure carries no caption, and the fix is conditioned on that: a
    /// caption is text. An unreferenced manifest item is EPUBCheck's `OPF-003`, and an image
    /// anchored to a block that no section carries is how the emitter can produce one.
    DropUnreferencedFigure,
}

impl Fix {
    /// The id this repair is logged and counted under, for the fire-rate metric.
    pub fn id(self) -> &'static str {
        match self {
            Fix::FigureAlt => "fix.figure_alt",
            Fix::DemoteDanglingNoteref => "fix.demote_dangling_noteref",
            Fix::DropUnreferencedFigure => "fix.drop_unreferenced_figure",
        }
    }
}

/// The table itself: message id → remedy.
///
/// Sorted by id, and asserted to be, so that a reader can find an entry and a reviewer can see the
/// whole of it at once.
const TABLE: [(&str, Remedy); 9] = [
    // Accessibility metadata is missing or an image has no description. Alt text is outside `C`.
    ("ACC-001", Remedy::AutoFix(Fix::FigureAlt)),
    // Image count in did not equal image count out. Nothing in the document can put back an image
    // the extractor did not produce.
    ("OC-IMAGE-PARITY", Remedy::WarnUser),
    // A footnote nothing refers to. A detection failure rather than a serialisation defect: the
    // only structural fixes are to drop the note (text) or to invent a reference (a lie).
    ("OC-NOTE-BIJECTION", Remedy::WarnUser),
    // A manifest item nothing references.
    ("OPF-003", Remedy::AutoFix(Fix::DropUnreferencedFigure)),
    // Required package metadata is missing. The emitter writes all four unconditionally, so this
    // is a defect in the emitter and not something to patch around.
    ("OPF-014", Remedy::WarnUser),
    // The mimetype entry is wrong, or the container is not a readable zip: both are the zip
    // writer, and re-emitting through the same writer would produce the same bytes.
    ("PKG-007", Remedy::WarnUser),
    ("PKG-008", Remedy::WarnUser),
    // Not well-formed XML. The typed builder makes this unreachable by construction (D5) and a
    // textual patch to XML we could not parse is exactly the untyped fix ARCHITECTURE §7.3 rejects.
    ("RSC-005", Remedy::WarnUser),
    // A fragment reference that resolves to nothing. Where the reference is a note marker the fix
    // is structural; where it is a nav or page-list target it is the emitter's, and the fix is
    // conditioned on finding the note (see `plan`).
    ("RSC-012", Remedy::AutoFix(Fix::DemoteDanglingNoteref)),
];

/// What the table says about one message id, or `None` when it says nothing.
pub fn remedy(id: &str) -> Option<Remedy> {
    TABLE
        .iter()
        .find(|(entry, _)| *entry == id)
        .map(|(_, remedy)| *remedy)
}

/// Every id the table covers, in order — what the coverage metric is measured against.
pub fn covered_ids() -> Vec<&'static str> {
    TABLE.iter().map(|(id, _)| *id).collect()
}

/// One repair the planner decided to attempt.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct RepairAction {
    pub fix: Fix,
    /// The message id that asked for it.
    pub message_id: &'static str,
    /// `(file, node)` as the finding reported it — the key confluence is stated over.
    pub location: String,
}

/// What one iteration will attempt.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct RepairPlan {
    pub actions: Vec<RepairAction>,
    /// Ids the table covers with `WarnUser`: real defects with no safe structural fix.
    pub warn_only: Vec<&'static str>,
    /// Ids the table says nothing about. Never guessed at (R10 §6.19).
    pub unmapped: Vec<String>,
}

impl RepairPlan {
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}

/// Plan one iteration's repairs.
///
/// Three rules, all from ARCHITECTURE §7.2, and all of them about reproducibility rather than
/// effectiveness:
///
/// - **Fixed total order.** Findings are sorted by `(severity descending, message id, location)`
///   before anything is planned, so the plan does not depend on the order a validator happened to
///   report in.
/// - **At most one repair per `(file, node)` per iteration.** Two repairs racing for one node is
///   the only way this loop could be non-confluent, and the planner is where it is prevented.
/// - **Unmapped ids are collected, not skipped silently.** The caller turns them into
///   `W_UNMAPPED_VALIDATION_ID`.
pub fn plan_repairs(findings: &[Finding]) -> RepairPlan {
    let mut ordered: Vec<&Finding> = findings.iter().collect();
    ordered.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| a.id.cmp(b.id))
            .then_with(|| a.location.cmp(&b.location))
    });

    let mut plan = RepairPlan::default();
    let mut claimed: Vec<String> = Vec::new();

    for finding in ordered {
        match remedy(finding.id) {
            Some(Remedy::AutoFix(fix)) => {
                if claimed.contains(&finding.location) {
                    continue;
                }
                claimed.push(finding.location.clone());
                plan.actions.push(RepairAction {
                    fix,
                    message_id: finding.id,
                    location: finding.location.clone(),
                });
            }
            Some(Remedy::WarnUser) => {
                if !plan.warn_only.contains(&finding.id) {
                    plan.warn_only.push(finding.id);
                }
            }
            None => {
                let id = finding.id.to_owned();
                if !plan.unmapped.contains(&id) {
                    plan.unmapped.push(id);
                }
            }
        }
    }

    plan
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Rows 6.7 and 6.9 of the Phase 6 table, plus
// the table's own invariants.
// ---------------------------------------------------------------------------

#[cfg(test)]
use crate::tier1::Severity;

#[cfg(test)]
fn finding(id: &'static str, severity: Severity, location: &str) -> Finding {
    Finding {
        id,
        severity,
        location: location.to_owned(),
        detail: String::new(),
    }
}

/// The table is sorted and has no repeated id. Both are properties a reader relies on and neither
/// is enforced by the type.
#[test]
fn the_table_is_sorted_and_names_each_id_once() {
    let ids = covered_ids();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted, "the table is not in id order");

    let mut deduped = sorted.clone();
    deduped.dedup();
    assert_eq!(deduped.len(), ids.len(), "an id appears twice");
}

/// Row 6.7. Two repairs targeting one node in one iteration: only the first applies. This is the
/// whole of the confluence argument — two repairs can never race for a node, so the loop's result
/// does not depend on which ran first.
#[test]
fn repair_at_most_one_per_file_node() {
    let node = "text/c0001.xhtml#n1";
    let plan = plan_repairs(&[
        finding("RSC-012", Severity::Error, node),
        finding("ACC-001", Severity::Warning, node),
        finding("ACC-001", Severity::Warning, "text/c0002.xhtml#f1"),
    ]);

    assert_eq!(plan.actions.len(), 2, "{plan:?}");
    assert_eq!(plan.actions[0].location, node);
    assert_eq!(
        plan.actions[0].message_id, "RSC-012",
        "the higher severity claims the node: {:?}",
        plan.actions
    );
    assert_eq!(plan.actions[1].location, "text/c0002.xhtml#f1");
}

/// The order is total and fixed: severity descending, then id, then location. A plan that depended
/// on the order a validator reported in would make the loop irreproducible.
#[test]
fn the_plan_is_in_severity_then_id_then_location_order() {
    let findings = vec![
        finding("ACC-001", Severity::Warning, "b"),
        finding("ACC-001", Severity::Warning, "a"),
        finding("RSC-012", Severity::Error, "c"),
    ];
    let forward = plan_repairs(&findings);

    let mut reversed = findings;
    reversed.reverse();
    let backward = plan_repairs(&reversed);

    assert_eq!(forward, backward, "the plan depends on the input order");
    let locations: Vec<&str> = forward
        .actions
        .iter()
        .map(|action| action.location.as_str())
        .collect();
    assert_eq!(locations, vec!["c", "a", "b"]);
}

/// Row 6.9's planner half. An id the table says nothing about produces no action and is collected
/// verbatim, never guessed at. The loop turns the list into `W_UNMAPPED_VALIDATION_ID`.
#[test]
fn an_unmapped_id_produces_no_action_and_is_collected() {
    let plan = plan_repairs(&[
        finding("CSS-008", Severity::Error, "style.css"),
        finding("MED-003", Severity::Error, "text/c0001.xhtml"),
        finding("CSS-008", Severity::Error, "style.css"),
    ]);

    assert!(plan.actions.is_empty(), "{plan:?}");
    assert_eq!(plan.unmapped, vec!["CSS-008", "MED-003"]);
    assert!(plan.warn_only.is_empty());
}

/// A covered id with no safe structural fix is a `WarnUser`, which is a different thing from
/// unmapped: the defect is understood and the answer is that no repair may be made.
#[test]
fn a_warn_only_id_is_not_an_unmapped_one() {
    let plan = plan_repairs(&[finding(
        "OC-NOTE-BIJECTION",
        Severity::Error,
        "text/c0001.xhtml#n7",
    )]);

    assert!(plan.actions.is_empty());
    assert!(plan.unmapped.is_empty());
    assert_eq!(plan.warn_only, vec!["OC-NOTE-BIJECTION"]);
}

/// Every `AutoFix` in the table names a fix whose id is distinct, because the fire-rate metric is
/// per repair id and two fixes sharing one would be counted as one.
#[test]
fn every_fix_has_its_own_id() {
    let fixes = [
        Fix::FigureAlt,
        Fix::DemoteDanglingNoteref,
        Fix::DropUnreferencedFigure,
    ];
    let mut ids: Vec<&str> = fixes.iter().map(|fix| fix.id()).collect();
    ids.sort_unstable();
    let mut deduped = ids.clone();
    deduped.dedup();
    assert_eq!(ids, deduped);

    // And every `AutoFix` the table carries is one of them, so the metric covers the table.
    for id in covered_ids() {
        if let Some(Remedy::AutoFix(fix)) = remedy(id) {
            assert!(fixes.contains(&fix), "{id} names a fix the metric does not");
        }
    }
}
