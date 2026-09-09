//! The text ledger: every character a stage removed or added, and why (D13.4).
//!
//! The conservation law is the project's central safety property — text does not go missing
//! silently, because every removal has to name a reason and stay inside that reason's
//! budget. The ledger is what makes that checkable rather than aspirational.

use serde::Serialize;

/// Why a character left the document, or arrived in it.
///
/// **Closed, and deliberately so.** A stage that wants to remove text for a reason not on
/// this list is a stage proposing a new way to lose a reader's book, and that belongs in a
/// decision record before it belongs in code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    /// A soft hyphen, U+00AD, removed by normalisation `N`.
    SoftHyphen,
    /// A ligature expanded into its components — removes one character, adds two or three.
    LigatureExpand,
    /// A space the backend synthesised, which the document does not contain.
    GeneratedSpace,
    RunningHeader,
    RunningFooter,
    PageNumber,
    /// The same glyph drawn twice, as producers fake bold.
    OverdrawDedup,
    /// An OCR layer repeating text that is already visible.
    OcrLayerDuplicate,
    /// A hyphen removed when a word broken across lines was rejoined.
    Dehyphenate,
    /// Text OCR added. The only reason that adds rather than removes, and region-scoped by
    /// invariant I-6.
    Ocr,
    DecorativeGlyph,
    Watermark,
    /// Geometrically absent: outside the CropBox, or removed by a clipping path.
    ClippedOffPage,
    /// Rendered but not visible: render mode 3 on a page that is not an OCR sandwich, or a
    /// fill colour indistinguishable from the local background.
    HiddenText,
    UserOverride,
}

impl Reason {
    /// Whether this reason adds text rather than removing it.
    ///
    /// Only OCR does. Stated as a method rather than left implicit because invariant I-1
    /// balances added against removed, and getting the side wrong would make the equation
    /// hold while the text was lost.
    pub fn adds(self) -> bool {
        matches!(self, Reason::Ocr)
    }
}

/// One removal or addition, with enough context to find it in the document.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LedgerEntry {
    /// The stage that did it.
    pub stage: &'static str,
    pub reason: Reason,
    /// Zero-based page index.
    pub page: u32,
    /// The character range within the page's extracted sequence.
    pub span: (u32, u32),
    /// The text itself, so a report can quote what went.
    pub text: String,
    /// `true` when the text was added rather than removed.
    pub added: bool,
}

impl LedgerEntry {
    /// A removal.
    pub fn removed(
        stage: &'static str,
        reason: Reason,
        page: u32,
        span: (u32, u32),
        text: String,
    ) -> Self {
        debug_assert!(
            !reason.adds(),
            "{reason:?} adds text; use LedgerEntry::added"
        );
        Self {
            stage,
            reason,
            page,
            span,
            text,
            added: false,
        }
    }

    /// An addition. Only OCR makes these.
    pub fn added(
        stage: &'static str,
        reason: Reason,
        page: u32,
        span: (u32, u32),
        text: String,
    ) -> Self {
        debug_assert!(
            reason.adds(),
            "{reason:?} removes text; use LedgerEntry::removed"
        );
        Self {
            stage,
            reason,
            page,
            span,
            text,
            added: true,
        }
    }
}
