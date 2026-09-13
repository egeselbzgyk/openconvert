//! The `paragraphs` stage's contract: lines into paragraphs, and dehyphenation (PIPELINE §7).

use oc_model::ledger::{Reason, StageKind};

use super::StageDecl;

/// `paragraphs` is **Budgeted over exactly one reason**, and the narrowness is the point.
///
/// Reconstructing paragraphs changes no text at all: it decides where one ends and the next
/// begins. The single thing this stage may remove is a hyphen at a line break, under
/// `Dehyphenate` and inside `conservation.budget.dehyphenate`. Any other reason appearing here
/// is text going missing during an operation that has no business losing any, and I-2 makes
/// that an error rather than a judgement call.
pub const PARAGRAPHS: StageDecl = StageDecl {
    name: "paragraphs",
    kind: StageKind::Budgeted,
    reasons: &[Reason::Dehyphenate],
};
