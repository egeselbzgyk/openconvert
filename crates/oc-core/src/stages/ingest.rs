//! The `ingest` stage's contract: glyphs, images, vector regions and — where routing demands it —
//! OCR runs, all in the one normalised space (PIPELINE §3).

use oc_model::ledger::{Reason, StageKind};

use super::StageDecl;

/// `ingest` is Budgeted over the four removal classes of extraction, the OCR-layer dedup, and
/// `Ocr` — which it owns, because OCR creates runs and only `ingest` creates runs (ratified notes
/// N-2 and N-3; IR_SKETCH, "Stage kinds"). `Ocr` is the only reason in the system that only adds,
/// and invariant I-6 scopes it to regions with no pre-existing text (N-1).
pub const INGEST: StageDecl = StageDecl {
    name: "ingest",
    kind: StageKind::Budgeted,
    reasons: &[
        Reason::GeneratedSpace,
        Reason::ClippedOffPage,
        Reason::HiddenText,
        Reason::OverdrawDedup,
        Reason::OcrLayerDuplicate,
        Reason::Ocr,
    ],
};
