//! The text ledger: every character a stage removed or added, and why (D13.4).
//!
//! The conservation law is the project's central safety property — text does not go missing
//! silently, because every removal has to name a reason and stay inside that reason's
//! budget. The ledger is what makes that checkable rather than aspirational.

use serde::{Deserialize, Serialize};

use crate::extract::CharHistogram;

/// Why a character left the document, or arrived in it.
///
/// **Closed, and deliberately so.** A stage that wants to remove text for a reason not on
/// this list is a stage proposing a new way to lose a reader's book, and that belongs in a
/// decision record before it belongs in code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
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
    /// Three do. `Ocr` invents text that was not in the document; `LigatureExpand` turns one
    /// scalar into two and so appears on both sides at once; `UserOverride` is a heading the user
    /// renamed, whose new text the PDF never printed. Stated as a method rather than left implicit
    /// because invariant I-1 balances added against removed, and getting the side wrong would make
    /// the equation hold while the text was lost.
    pub fn may_add(self) -> bool {
        matches!(
            self,
            Reason::Ocr | Reason::LigatureExpand | Reason::UserOverride
        )
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

/// `C(·)` of text that arrives in pieces — a page's runs, a document's blocks, a ledger's
/// entries — **composed across the joins**.
///
/// This exists because `c_of` alone is not enough once `C` is defined after canonical
/// composition. Composition is a property of a *sequence*, so where the sequence is cut
/// changes the answer: a base and the combining mark after it compose when they are in one
/// string and stay apart when they are in two. Compare a page composed whole against its runs
/// composed one at a time and a single mark split across a run boundary reads as
/// "1 character left and 2 appeared" — which is what four corpus documents did, and the loss
/// was an artefact of the comparison rather than anything the stage had done.
///
/// So every side of every conservation comparison joins first and composes once. That makes
/// the law **granularity-independent**: it cannot matter where a stage happens to cut its
/// text, which is not a property the pipeline should have to remember.
///
/// The parts are joined with **nothing**, and that is the load-bearing choice. Both sides of
/// a comparison have to cut the text the same way or the composition differs again, and the
/// only cut guaranteed to be identical on a glyph stream and on a list of runs is no cut at
/// all. A separator here would reintroduce the very mismatch this function exists to remove:
/// the glyph stream has no separators, so runs joined by one would leave a mark at a run
/// boundary uncomposed on one side and composed on the other.
pub fn c_of_parts<'a>(parts: impl IntoIterator<Item = &'a str>) -> CharHistogram {
    let joined: String = parts.into_iter().collect();
    c_of(&joined)
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
/// `C(D)` is taken **after canonical decomposition (NFD)** — the ruling of 2026-09-20 and an
/// amendment to ARCHITECTURE §5.2 (`docs/DECISIONS_LOG.md`).
///
/// *Why a canonical form at all.* `N` includes NFC, D13.4's `Reason` enum is closed and has
/// no variant for canonical composition, and NFC has **singleton** compositions — U+2126 OHM
/// SIGN becomes U+03A9 GREEK CAPITAL LETTER OMEGA, one scalar for one scalar. Seven corpus
/// documents were refused by I-1 for exactly that, with nothing lost. Unicode's canonical
/// equivalence says the two encodings *are* the same character, so a law that counts them
/// apart is counting encodings rather than text.
///
/// *Why NFD and not NFC.* Composition depends on **adjacency**: `u` followed by U+0308
/// composes to `ü` only when the two are next to each other. But a stage cuts its text where
/// it likes — the glyph stream is one sequence in *draw* order and the runs are many in
/// *reading* order — so a composing law gives two different answers for the same book. That
/// is not hypothetical: it is how `oapen-20-500-12657-116098` was refused, an umlaut drawn as
/// two glyphs and assembled into two runs that are not adjacent.
///
/// Decomposition has no such dependency. It expands each character on its own, and the
/// canonical reordering that follows cannot change a **multiset**, which is what `C` is. So
/// `C` of a text is the same whatever pieces it arrives in, and the law becomes
/// granularity-independent by construction rather than by every stage remembering to cut in
/// the same place.
///
/// It folds exactly what NFC folded — U+2126/U+03A9, `ü`/`u`+◌̈, `İ`/`I`+◌̇, U+1F71/U+03AC —
/// because two canonically equivalent strings have the same decomposition by definition.
///
/// NFD and **not** NFKD: D13.4 forbids compatibility normalisation, because it changes the
/// text. `ﬁ` is not `fi` (that is `LigatureExpand`, which is ledgered) and `²` is not `2`.
/// Turkish `ı` is not `i` and `İ` is not `I`: folding either would be a case fold, which
/// D13.4 forbids outright (R10 §6.3).
///
/// Composing — or here, decomposing — inside `c_of` makes the fault **unrepresentable**: no
/// stage can break I-1 over an encoding difference, because both sides of every comparison
/// pass through this one function. It cannot hide a real loss, because decomposition is a
/// bijection on the text it expands.
pub fn c_of(text: &str) -> CharHistogram {
    use unicode_normalization::UnicodeNormalization;

    let mut histogram = CharHistogram::new();
    for ch in text.nfd().filter(|ch| !ch.is_whitespace()) {
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

    /// The ruling of 2026-09-20, as an invariant over **all of Unicode** rather than as a
    /// fix for the one character that happened to be in the corpus.
    ///
    /// The refusal that raised this was U+2126 OHM SIGN composing to U+03A9 GREEK CAPITAL
    /// LETTER OMEGA. Ohm is one of **1 120 code points** NFC rewrites — 1 002 CJK, 34 Hebrew,
    /// 23 Greek, 17 Tibetan, 8 Devanagari, and the Angstrom and Kelvin signs beside it — and
    /// that is before the far larger population of *sequences*: every accented letter written
    /// as a base plus a combining mark, which is most of German, Turkish, French and
    /// Vietnamese as some producers encode them.
    ///
    /// So the assertion is not three examples. It sweeps every scalar in Unicode and requires
    /// that a character, its canonical decomposition and its canonical composition all have
    /// the same `C` — which is the whole of canonical equivalence, by definition. A fix
    /// fitted to the ohm sign would pass a three-example test and fail this one early.
    #[test]
    fn c_of_is_invariant_under_canonical_equivalence_across_unicode() {
        use unicode_normalization::UnicodeNormalization;

        let mut rewritten = 0u32;
        let mut checked = 0u32;

        for cp in 0u32..0x11_0000 {
            let Some(ch) = char::from_u32(cp) else {
                continue; // a surrogate: not a scalar
            };
            let own = ch.to_string();
            let decomposed: String = own.nfd().collect();
            let composed: String = own.nfc().collect();
            checked += 1;

            // Canonically equivalent encodings are the same text, so they are one `C`.
            assert_eq!(
                c_of(&own),
                c_of(&decomposed),
                "U+{cp:04X}: a character and its canonical decomposition differ under C"
            );
            assert_eq!(
                c_of(&own),
                c_of(&composed),
                "U+{cp:04X}: a character and its canonical composition differ under C"
            );

            if composed != own {
                rewritten += 1;
            }
        }

        // The sweep is not vacuous: NFC really does rewrite a large population, and each one
        // is a document this pipeline would have refused with nothing lost.
        assert!(
            checked > 1_000_000,
            "the sweep covers Unicode, not a sample: {checked}"
        );
        assert!(
            rewritten > 1_000,
            "NFC rewrites a population, not an exception: {rewritten}"
        );
    }

    /// The three languages v1 claims, in both encodings, because this is where a producer's
    /// choice actually bites (R10 §6.3).
    ///
    /// A German book whose umlauts are drawn as `a` + U+0308 and a Turkish one whose `ş` is
    /// `s` + U+0327 are ordinary, not pathological — some producers emit one form and some
    /// the other, and one book can mix them. Greek is here because it carries the ohm sign's
    /// exact shape: U+1F71 is a singleton that rewrites to U+03AC.
    #[test]
    fn c_of_folds_the_encodings_the_three_v1_languages_arrive_in() {
        for (what, left, right) in [
            ("tr dotted capital I", "\u{0130}", "I\u{0307}"),
            ("tr s-cedilla", "\u{015F}", "s\u{0327}"),
            ("tr g-breve", "\u{011F}", "g\u{0306}"),
            ("de a-umlaut", "\u{00E4}", "a\u{0308}"),
            ("de o-umlaut", "\u{00F6}", "o\u{0308}"),
            ("el alpha-tonos", "\u{03AC}", "\u{03B1}\u{0301}"),
            ("el alpha-oxia is the ohm shape", "\u{03AC}", "\u{1F71}"),
        ] {
            assert_eq!(
                c_of(left),
                c_of(right),
                "{what}: the two encodings are the same text"
            );
        }

        // Turkish dotless i has no decomposition and must not acquire one: folding `ı` into
        // `i` would be a case fold, which D13.4 forbids outright (R10 §6.3).
        assert_ne!(
            c_of("\u{0131}"),
            c_of("i"),
            "dotless i is not i; C never case-folds"
        );
        assert_ne!(
            c_of("\u{0130}"),
            c_of("I"),
            "dotted capital I is not I; the dot is a character"
        );
    }

    /// The regression the first NFC fix introduced, as an invariant.
    ///
    /// Composing inside `c_of` closed the ohm-sign class and opened a subtler one: composition
    /// is a property of a *sequence*, so where the text is cut changes the answer. The glyph
    /// stream is one string per page and the runs are many, so a base and the combining mark
    /// after it composed on the input side and stayed apart on the output side — and four
    /// corpus documents reported "1 character left and 2 appeared" for a loss that had not
    /// happened. It was caught by the corpus inventory, not by any test, which is why this
    /// one exists.
    ///
    /// The property: **where the text is cut must not change `C`.**
    #[test]
    fn c_of_does_not_depend_on_where_the_text_was_cut() {
        // A base and its combining mark, split at every position the string allows.
        let whole = "cafe\u{0301} noir";
        for cut in 1..whole.len() {
            if !whole.is_char_boundary(cut) {
                continue;
            }
            let (left, right) = whole.split_at(cut);
            assert_eq!(
                c_of_parts([left, right]),
                c_of(whole),
                "cutting {whole:?} at {cut} changed C"
            );
        }

        // The case that actually happened: the mark opens the second piece, because `text`
        // put it in a run of its own.
        assert_eq!(
            c_of_parts(["cafe", "\u{0301}"]),
            c_of("caf\u{00E9}"),
            "a combining mark at a run boundary is still part of the letter before it"
        );
        // Two scalars, not one: `C` is taken after *de*composition, so `é` is `e` plus
        // its acute on both sides. What matters is that the two encodings agree —
        // the assertion above — not that either of them is short.
        assert_eq!(c_of_parts(["e", "\u{0301}"]).total(), 2);

        // Three-way and empty pieces must not matter either.
        assert_eq!(c_of_parts(["e", "", "\u{0301}"]), c_of("\u{00E9}"));
        assert_eq!(c_of_parts(["\u{0130}"]), c_of_parts(["I", "\u{0307}"]));
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
