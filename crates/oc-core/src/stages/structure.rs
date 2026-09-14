//! The `structure` stage's contract: semantic roles (PIPELINE §8).

use oc_model::ledger::StageKind;

use super::StageDecl;

/// `structure` is **Conserving**, and PIPELINE §8 says why in one sentence: every operation
/// here is a label.
///
/// Saying that statically is what forces the stage's shape. A block's text has to land in
/// exactly one place — a section's content, a note's body, a table's cells, a figure's
/// caption — because anything counted twice or not at all fails plain multiset equality. It
/// is also why a list marker stays inside its item's text: `Reason` is a closed set of
/// fifteen variants and none of them is "a list marker", so a stage that dropped one would be
/// removing text it cannot account for.
pub const STRUCTURE: StageDecl = StageDecl {
    name: "structure",
    kind: StageKind::Conserving,
    reasons: &[],
};
