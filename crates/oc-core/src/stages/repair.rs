//! The `repair` stage's contract: deterministic structural fixes (PIPELINE §12).

use oc_model::ledger::StageKind;

use super::StageDecl;

/// `repair` is **Conserving**.
///
/// Every repair in the table is structural: an `alt` attribute given a fallback description, a
/// `noteref` link removed from a marker whose text stays exactly where it was, an uncaptioned
/// figure dropped. None of them is a character, which is what lets the stage whose job is to
/// modify an assembled book make the strongest statement the conservation law has.
///
/// PIPELINE §12 words it as "**Conserving**, except `UserOverride`". The exception is not declared
/// here, and deliberately: a `Conserving` stage's ledger must be empty by I-3, so declaring a
/// reason it may never cite would be a contract that contradicts itself. User corrections keyed by
/// `BlockId` arrive with the review UI in Phase 12; the stage becomes `Budgeted` with
/// `UserOverride` in its reason set at that point, in the commit that can also test it.
pub const REPAIR: StageDecl = StageDecl {
    name: "repair",
    kind: StageKind::Conserving,
    reasons: &[],
};
