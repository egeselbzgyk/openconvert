//! What each pipeline stage is statically allowed to do to the text (ARCHITECTURE §5.3).
//!
//! The declarations live here, next to the checker that enforces them, rather than on the
//! stage implementations themselves. A stage that could widen its own reason set could
//! authorise its own deletions, and the whole point of I-2 is that it cannot.

use oc_model::ledger::{Reason, StageKind};

pub mod document;
pub mod epub;
pub mod furniture;
pub mod ingest;
pub mod layout;
pub mod paragraphs;
pub mod repair;
pub mod structure;
pub mod text;
pub mod validate;

pub use document::{DOCUMENT, DOCUMENT_CORRECTED};
pub use epub::EPUB;
pub use furniture::FURNITURE;
pub use ingest::INGEST;
pub use layout::LAYOUT;
pub use paragraphs::PARAGRAPHS;
pub use repair::REPAIR;
pub use structure::STRUCTURE;
pub use text::TEXT;
pub use validate::VALIDATE;

/// One stage's static contract: its name, whether it may change the text, and the closed set
/// of reasons it may cite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StageDecl {
    /// The stage name, as it appears in events, `--dump-stage` and the ledger.
    pub name: &'static str,
    pub kind: StageKind,
    pub reasons: &'static [Reason],
}

impl StageDecl {
    pub fn declares(&self, reason: Reason) -> bool {
        self.reasons.contains(&reason)
    }
}
