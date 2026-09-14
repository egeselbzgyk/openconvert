//! The `document` stage's contract: the assembled book (PIPELINE §9).

use oc_model::ledger::StageKind;

use super::StageDecl;

/// `document` is **Conserving**, and there is nothing here for it to be otherwise about.
///
/// The stage inserts page breaks into the flow, classifies the book, resolves the preset and
/// closes every cross-reference. Not one of those is a character: a `PageBreak` carries a
/// *label* — the printed page number `furniture` recovered — and a label is outside `C` by
/// definition (ARCHITECTURE §5.2). That is exactly what makes "remove the page number from the
/// flow, keep it as a page-list label" a clean `Removed{PageNumber}` in `furniture` rather than
/// a paradox here.
pub const DOCUMENT: StageDecl = StageDecl {
    name: "document",
    kind: StageKind::Conserving,
    reasons: &[],
};
