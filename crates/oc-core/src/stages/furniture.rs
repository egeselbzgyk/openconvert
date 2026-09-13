//! The `furniture` stage's contract: running heads, feet, page numbers, watermarks and
//! decorative glyphs, removed before segmentation (PIPELINE §5).

use oc_model::ledger::{Reason, StageKind};

use super::StageDecl;

/// `furniture` is the stage that deletes the most text on purpose, which is why its three
/// principal reasons share one budget (`conservation.budget.furniture`) rather than each
/// getting their own: a detector that mislabels a header as a page number should not thereby
/// gain a second allowance.
pub const FURNITURE: StageDecl = StageDecl {
    name: "furniture",
    kind: StageKind::Budgeted,
    reasons: &[
        Reason::RunningHeader,
        Reason::RunningFooter,
        Reason::PageNumber,
        Reason::Watermark,
        Reason::DecorativeGlyph,
    ],
};
