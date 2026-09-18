//! The `validate` stage's contract: prove the container is what we think it is (PIPELINE §11).

use oc_model::ledger::StageKind;

use super::StageDecl;

/// `validate` is **Conserving**, and trivially so: the stage is read-only.
///
/// It is declared all the same, because the ledger's per-stage record is the evidence that
/// "I-1 through I-4 were checked after every stage" is a fact rather than prose (ARCHITECTURE
/// §5.4). A stage missing from the record would be a stage nobody had checked.
pub const VALIDATE: StageDecl = StageDecl {
    name: "validate",
    kind: StageKind::Conserving,
    reasons: &[],
};
