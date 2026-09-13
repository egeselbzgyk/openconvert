//! The `text` stage's contract: glyphs to runs and lines, with normalisation `N` applied
//! exactly once (PIPELINE §4).

use oc_model::ledger::{Reason, StageKind};

use super::StageDecl;

/// `text` is Budgeted over exactly the two things `N` does: it strips soft hyphens and it
/// expands ligatures. Anything else it removed would be text going missing (IR_SKETCH,
/// "Stage kinds").
pub const TEXT: StageDecl = StageDecl {
    name: "text",
    kind: StageKind::Budgeted,
    reasons: &[Reason::SoftHyphen, Reason::LigatureExpand],
};
