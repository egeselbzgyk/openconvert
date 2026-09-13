//! The text ledger: every character a stage removed or added, and why (D13.4).
//!
//! The conservation law is the project's central safety property — text does not go missing
//! silently, because every removal has to name a reason and stay inside that reason's
//! budget. The ledger is what makes that checkable rather than aspirational.

use serde::Serialize;

use crate::extract::CharHistogram;

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
    /// The one reason that appears on both sides of the ledger at once, and the case that
    /// makes plain multiset equality fail (PIPELINE §4, ARCHITECTURE §5.4 I-1).
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
    /// Text OCR added. The only reason that *only* adds, and region-scoped by invariant I-6.
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
    /// Whether this reason may put an `Added` entry in the ledger.
    ///
    /// Two do. `Ocr` invents text that was not in the document; `LigatureExpand` turns one
    /// scalar into two and so appears on both sides at once. Stated as a method rather than
    /// left implicit because invariant I-1 balances added against removed, and getting the
    /// side wrong would make the equation hold while the text was lost.
    pub fn may_add(self) -> bool {
        matches!(self, Reason::Ocr | Reason::LigatureExpand)
    }

    /// Whether this reason may put a `Removed` entry in the ledger. Every reason but `Ocr`
    /// does; OCR is Added-only by invariant I-6.
    pub fn may_remove(self) -> bool {
        !matches!(self, Reason::Ocr)
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
            reason.may_remove(),
            "{reason:?} never removes text; use LedgerEntry::added"
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
            reason.may_add(),
            "{reason:?} never adds text; use LedgerEntry::removed"
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

/// The multiset the conservation law is stated over: the non-whitespace scalars of `text`.
///
/// `C(D)` excludes every scalar with the Unicode `White_Space` property (ARCHITECTURE §5.2).
/// That single exclusion is what makes a backend-generated space invisible to the ledger and
/// a soft hyphen — which is *not* whitespace — visible to it.
pub fn c_of(text: &str) -> CharHistogram {
    let mut histogram = CharHistogram::new();
    for ch in text.chars().filter(|ch| !ch.is_whitespace()) {
        histogram.add(ch);
    }
    histogram
}

/// What one stage did to the text, and nothing else.
///
/// A delta rather than the whole ledger because the invariants are stated per stage: the
/// checker is handed exactly the entries that stage produced and can therefore say which
/// stage broke the law rather than that the book no longer balances.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct LedgerDelta {
    entries: Vec<LedgerEntry>,
}

impl LedgerDelta {
    pub fn new(entries: Vec<LedgerEntry>) -> Self {
        Self { entries }
    }

    pub fn push(&mut self, entry: LedgerEntry) {
        self.entries.push(entry);
    }

    pub fn entries(&self) -> &[LedgerEntry] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn into_entries(self) -> Vec<LedgerEntry> {
        self.entries
    }

    /// `chars(Removed_s)` — the multiset this stage says it took out.
    pub fn removed(&self) -> CharHistogram {
        self.side(false)
    }

    /// `chars(Added_s)` — the multiset this stage says it put in.
    pub fn added(&self) -> CharHistogram {
        self.side(true)
    }

    /// How many non-whitespace characters this stage removed under one reason, less what the
    /// same reason added back.
    ///
    /// Netted because a budget bounds text *loss*, and `LigatureExpand` removes one scalar
    /// only to add two in its place: counting the removal alone would charge a book for
    /// spelling `ﬁ` as `fi`. See `docs/DECISIONS_LOG.md`.
    pub fn net_removed(&self, reason: Reason) -> u64 {
        let removed = self.reason_side(reason, false).total();
        let added = self.reason_side(reason, true).total();
        removed.saturating_sub(added)
    }

    fn side(&self, added: bool) -> CharHistogram {
        let mut histogram = CharHistogram::new();
        for entry in self.entries.iter().filter(|e| e.added == added) {
            for ch in entry.text.chars().filter(|ch| !ch.is_whitespace()) {
                histogram.add(ch);
            }
        }
        histogram
    }

    fn reason_side(&self, reason: Reason, added: bool) -> CharHistogram {
        let mut histogram = CharHistogram::new();
        for entry in self
            .entries
            .iter()
            .filter(|e| e.added == added && e.reason == reason)
        {
            for ch in entry.text.chars().filter(|ch| !ch.is_whitespace()) {
                histogram.add(ch);
            }
        }
        histogram
    }

    /// Every reason this stage cited, in enum order and without repeats.
    pub fn reasons(&self) -> Vec<Reason> {
        let mut seen: Vec<Reason> = self.entries.iter().map(|e| e.reason).collect();
        seen.sort_unstable();
        seen.dedup();
        seen
    }
}

/// Whether a stage is allowed to change the text at all (ARCHITECTURE §5.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StageKind {
    /// Plain multiset equality holds across the stage; an empty ledger is the only legal one.
    Conserving,
    /// The stage may remove or add text, within a declared reason set and a budget.
    Budgeted,
}

/// What the invariant checker found after one stage, recorded in the report.
///
/// Kept even when everything held: "I-1 through I-4 were checked and passed after every
/// stage" is the claim the architecture rests on, and a claim with no record is prose.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StageCheck {
    pub stage: &'static str,
    pub kind: StageKind,
    /// Non-whitespace characters this stage removed.
    pub removed_chars: u64,
    /// Non-whitespace characters this stage added.
    pub added_chars: u64,
    /// `|C(D_after)| / |C_0|` — the retention ratio as of this stage.
    pub retention: f32,
}
