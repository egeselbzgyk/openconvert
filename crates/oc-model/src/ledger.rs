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

/// The multiset the conservation law is stated over: the non-whitespace scalars of `text`,
/// **after canonical composition**.
///
/// Two exclusions, and each one buys something.
///
/// `C(D)` excludes every scalar with the Unicode `White_Space` property (ARCHITECTURE §5.2).
/// That is what makes a backend-generated space invisible to the ledger and a soft hyphen —
/// which is *not* whitespace — visible to it.
///
/// `C(D)` is taken **after NFC**, which is the ruling of 2026-09-20 and an amendment to
/// ARCHITECTURE §5.2 (`docs/DECISIONS_LOG.md`). `N` includes NFC, D13.4's `Reason` enum is
/// closed and has no variant for canonical composition, and NFC has *singleton* compositions
/// — U+2126 OHM SIGN becomes U+03A9 GREEK CAPITAL LETTER OMEGA, one scalar for one scalar.
/// Seven corpus documents were refused by I-1 for exactly that, with nothing lost. Unicode's
/// own canonical equivalence says the two are the same character; a law that counts them
/// apart is counting encodings rather than text.
///
/// Composing here rather than ledgering there makes the fault **unrepresentable**: no stage
/// can break I-1 by composing, at any point, ever, because the two sides of the comparison
/// are composed by the same function. It cannot hide a real loss — NFC is a bijection on the
/// text it composes, so a character that actually vanishes still vanishes.
///
/// NFC and **not** NFKC: D13.4 forbids NFKC, because compatibility decomposition does change
/// the text (`ﬁ` and `fi` are not the same characters, and `²` is not `2`). Only canonical
/// equivalence is folded.
pub fn c_of(text: &str) -> CharHistogram {
    use unicode_normalization::UnicodeNormalization;

    let mut histogram = CharHistogram::new();
    for ch in text.nfc().filter(|ch| !ch.is_whitespace()) {
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
            histogram = histogram.union(&c_of(&entry.text));
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
            histogram = histogram.union(&c_of(&entry.text));
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

/// The whole ledger of one conversion: every entry, both baselines, and the per-stage
/// invariant results (IR_SKETCH).
///
/// [`LedgerDelta`] is what a stage produces and what the checker reads; this is what the
/// finished [`Document`](crate::document::Document) carries, and it is the record the report
/// and invariant I-7 are written against. Keeping the two apart is what lets the checker say
/// *which* stage broke the law rather than that the book no longer balances.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Ledger {
    pub entries: Vec<LedgerEntry>,
    /// `C_raw` — the multiset as extraction produced it, before normalisation `N`.
    pub c_raw: CharHistogram,
    /// `C_0` — the retention denominator: after `N`, overdraw dedup and OCR-layer dedup
    /// (ARCHITECTURE §5.2).
    pub c_0: CharHistogram,
    pub per_stage_checks: Vec<StageCheck>,
}

impl Ledger {
    /// Fold one stage's delta and its check into the document ledger, in stage order.
    pub fn push_stage(&mut self, delta: &LedgerDelta, check: StageCheck) {
        self.entries.extend(delta.entries().iter().cloned());
        self.per_stage_checks.push(check);
    }

    /// Everything every stage removed, as one multiset — the `Removed_all` of invariant I-7.
    pub fn removed_all(&self) -> CharHistogram {
        self.side(false)
    }

    /// Everything every stage added — the `Added_all` of invariant I-7.
    pub fn added_all(&self) -> CharHistogram {
        self.side(true)
    }

    fn side(&self, added: bool) -> CharHistogram {
        let mut histogram = CharHistogram::new();
        for entry in self.entries.iter().filter(|e| e.added == added) {
            histogram = histogram.union(&c_of(&entry.text));
        }
        histogram
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

#[cfg(test)]
mod c_of_tests {
    use super::*;

    /// The ruling of 2026-09-20, as an invariant rather than as a fix.
    ///
    /// Seven corpus documents were refused by I-1 at the `text` stage because `N`'s NFC
    /// component rewrites U+2126 OHM SIGN to U+03A9 GREEK CAPITAL LETTER OMEGA — one scalar
    /// for one scalar, so the refusal read "N characters left and N appeared", and D13.4's
    /// closed `Reason` enum had nothing honest to record it as.
    ///
    /// Composing inside `c_of` makes that unrepresentable: no stage can break I-1 by
    /// composing, because both sides of every comparison are composed by this function.
    #[test]
    fn c_of_is_invariant_under_canonical_composition() {
        // Singletons: one scalar in, one scalar out. These are the ones that produced equal
        // counts on both sides of the refusal and therefore looked like a substitution.
        for (decomposed, composed) in [
            ("\u{2126}", "\u{03A9}"), // OHM SIGN -> GREEK CAPITAL LETTER OMEGA
            ("\u{212B}", "\u{00C5}"), // ANGSTROM SIGN -> LATIN CAPITAL LETTER A WITH RING
            ("\u{212A}", "\u{004B}"), // KELVIN SIGN -> LATIN CAPITAL LETTER K
        ] {
            assert_eq!(
                c_of(decomposed),
                c_of(composed),
                "canonically equivalent text is the same text: {decomposed:?} vs {composed:?}"
            );
        }

        // Sequences: a base and its combining mark compose to one scalar. Unequal counts, so
        // this shape was always visible to I-1 — and it must be folded for the same reason.
        assert_eq!(
            c_of("e\u{0301}crit"),
            c_of("\u{00E9}crit"),
            "a combining acute and a precomposed e-acute are the same word"
        );
        assert_eq!(c_of("e\u{0301}").total(), 1, "composed, then counted");
    }

    /// The other half of D13.4, and the reason the fold is NFC and not NFKC.
    ///
    /// Compatibility decomposition *does* change the text, so folding it would let a real
    /// difference through the law. `expand_ligatures` is a ledgered transform with its own
    /// `Reason` precisely because `ﬁ` and `fi` are not the same characters.
    #[test]
    fn c_of_does_not_fold_compatibility_equivalents() {
        assert_ne!(
            c_of("\u{FB01}re"),
            c_of("fire"),
            "the fi ligature is not f followed by i; LigatureExpand ledgers that transform"
        );
        assert_ne!(
            c_of("\u{00B2}"),
            c_of("2"),
            "superscript two is not two; D13.4 forbids NFKC for this reason"
        );
    }

    /// Composition must not become a way to lose a character.
    #[test]
    fn c_of_still_sees_a_character_that_actually_vanished() {
        let before = c_of("\u{2126} resistance");
        let after = c_of("resistance");

        assert_eq!(
            before.difference(&after),
            c_of("\u{03A9}"),
            "the omega is gone and the law says so, in its composed form"
        );
    }
}
