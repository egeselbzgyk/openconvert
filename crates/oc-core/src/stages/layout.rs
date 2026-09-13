//! The `layout` stage's contract: blocks, columns and reading order (PIPELINE §6).

use oc_model::ledger::StageKind;

use super::StageDecl;

/// `layout` is **Conserving**, and this is the stage where that is worth the most.
///
/// Segmentation and ordering rearrange text; they never change it. Saying so statically means
/// I-3 reduces to plain multiset equality across the stage, so a block-grouping bug that drops
/// a line — the classic way a layout analyser loses a paragraph — is caught by the same check
/// that runs everywhere else, with no budget to hide inside.
pub const LAYOUT: StageDecl = StageDecl {
    name: "layout",
    kind: StageKind::Conserving,
    reasons: &[],
};
