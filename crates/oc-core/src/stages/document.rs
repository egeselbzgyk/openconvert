//! The `document` stage's contract: the assembled book (PIPELINE §9).

use oc_model::ledger::{Reason, StageKind};

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

/// `document` when the user supplied corrections: **Conserving, except `UserOverride`**
/// (PIPELINE §9 step 7, ARCHITECTURE §4.7).
///
/// A heading the user renamed is text the book did not print, and the text it replaces is gone:
/// both are ledgered as `UserOverride`, the one reason a person rather than a rule cites. The
/// stage runs under this contract only when an `overrides.json` was given, so a conversion without
/// one is held to the stronger statement above.
pub const DOCUMENT_CORRECTED: StageDecl = StageDecl {
    name: "document",
    kind: StageKind::Budgeted,
    reasons: &[Reason::UserOverride],
};
