//! The `epub` stage's contract: the container (PIPELINE §10).

use oc_model::ledger::StageKind;

use super::StageDecl;

/// `epub` is **Conserving**, and D13.4 names it: "XHTML serialization" is on the list of
/// operations that may not change one character.
///
/// It is the last stage that could lose text and the first whose output a reader opens, which
/// is why the check is worth making here even though nothing downstream reads the result. What
/// the emitter needs to *say* for itself — alt text on a figure, a label on a page break, an
/// accessibility summary — goes into an attribute or into package metadata, and both are
/// outside `C` (ARCHITECTURE §5.2). That is not a loophole: it is the same rule that makes
/// "remove the printed folio from the flow, keep it as a page-list label" a clean
/// `Removed{PageNumber}` in `furniture` rather than a paradox.
pub const EPUB: StageDecl = StageDecl {
    name: "epub",
    kind: StageKind::Conserving,
    reasons: &[],
};
