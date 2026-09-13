//! Normalisation `N`, applied exactly once, at extraction (D13.4, ARCHITECTURE §5.1).
//!
//! ```text
//! N = NFC ∘ strip(U+00AD) ∘ expand_ligatures(U+FB00..U+FB06) ∘ NFC
//! ```
//!
//! ARCHITECTURE §5.1 writes it with three steps. The fourth — the leading `NFC` is the one
//! it names, the trailing one is added here — exists because stripping a soft hyphen can put
//! a base character next to a combining mark that was not adjacent to it before
//! (`e U+00AD U+0301`). Without composing again the output would not be NFC, which PIPELINE
//! §4 requires of every `Run.text`, and `N` would not be idempotent, which test 2.3 requires
//! of it. Composing an already-composed string is a no-op, so the fourth step costs a
//! quick-check and buys both properties.
//!
//! Three things `N` deliberately does **not** do:
//!
//! - **NFKC.** Forbidden pipeline-wide. It maps `¹` → `1` and destroys the superscript
//!   footnote signal in the same pass (R2 §B.8).
//! - **Case folding.** Never, on emitted text. Comparison keys are [`crate::fold`]'s job.
//! - **Anything to the hyphens.** Dehyphenation is Phase 3's, under its own reason and its
//!   own invariant I-5.

use compact_str::CompactString;
use oc_model::ledger::{LedgerDelta, LedgerEntry, Reason};
use unicode_normalization::UnicodeNormalization;

/// The soft hyphen: a discretionary break the document does not print, and not whitespace,
/// so removing it genuinely changes `C` and has to be declared.
const SOFT_HYPHEN: char = '\u{00AD}';

/// The Latin ligature block, U+FB00..=U+FB06, and what each expands to.
///
/// PDFium does not expand these — they arrive in the glyph stream as single scalars (R2
/// §B.8) — so the table is explicit rather than delegated to a Unicode normalisation form:
/// the form that would expand them is NFKC, and NFKC is banned.
///
/// U+FB05 is LATIN SMALL LIGATURE LONG S T. Its Unicode decomposition is `ſ` + `t`, and the
/// long s is an orthographic variant of `s`, so it expands to `st` — not to `ft`, which is
/// the classic long-s misreading and would turn `beſt` into `beft`. See
/// `docs/DECISIONS_LOG.md`.
const LIGATURES: [(char, &str); 7] = [
    ('\u{FB00}', "ff"),
    ('\u{FB01}', "fi"),
    ('\u{FB02}', "fl"),
    ('\u{FB03}', "ffi"),
    ('\u{FB04}', "ffl"),
    ('\u{FB05}', "st"),
    ('\u{FB06}', "st"),
];

/// Where a string being normalised sits, so a ledger entry can point back at it.
///
/// `char_offset` is the string's start within the page's extracted character sequence, which
/// is the coordinate `LedgerEntry::span` is stated in: a caller normalising a page one run at
/// a time passes the running offset, and a caller normalising a whole page passes zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LedgerSite {
    /// The stage doing the normalising. `text`, always, in the pipeline as specified; taken
    /// as a parameter rather than hard-coded so a test can say who it is pretending to be.
    pub stage: &'static str,
    pub page: u32,
    pub char_offset: u32,
}

/// Apply `N` to one string, recording what it removed and added.
///
/// Spans are in characters, not bytes. A `Removed` span indexes the **input** sequence
/// (offset by `at.char_offset`); an `Added` span indexes the **output**. They have to be
/// different coordinates: the whole point of a ligature expansion is that one position in
/// the input becomes two in the output.
pub fn normalize(text: &str, ledger: &mut LedgerDelta, at: LedgerSite) -> CompactString {
    let out = normalize_recording(text, Some((ledger, at)));
    debug_assert_eq!(
        normalize_recording(&out, None),
        out,
        "N is not idempotent on {text:?}"
    );
    out
}

/// `N` with no ledger — the pure transform, used by the idempotence assertion and by any
/// caller that only wants the string.
///
/// Callers inside the pipeline must use [`normalize`]: a removal that is not ledgered is
/// text going missing silently, which is the one thing the conservation law exists to stop.
pub fn normalized(text: &str) -> CompactString {
    normalize_recording(text, None)
}

fn normalize_recording(
    text: &str,
    mut record: Option<(&mut LedgerDelta, LedgerSite)>,
) -> CompactString {
    // Step 1: compose. Everything after this reasons about composed characters, so a
    // ligature or a soft hyphen is found once rather than in each of its encodings.
    let composed: CompactString = text.nfc().collect();

    let mut out = CompactString::default();
    let mut out_chars: u32 = 0;
    let mut removed_any = false;

    for (index, ch) in composed.chars().enumerate() {
        let source = index as u32;

        // Step 2: strip soft hyphens.
        if ch == SOFT_HYPHEN {
            removed_any = true;
            if let Some((ledger, at)) = record.as_mut() {
                ledger.push(LedgerEntry::removed(
                    at.stage,
                    Reason::SoftHyphen,
                    at.page,
                    (at.char_offset + source, at.char_offset + source + 1),
                    ch.to_string(),
                ));
            }
            continue;
        }

        // Step 3: expand ligatures, ledgering both sides. One scalar becomes two or three,
        // which is the case plain multiset equality cannot express and I-1 can.
        if let Some(expansion) = expansion_of(ch) {
            if let Some((ledger, at)) = record.as_mut() {
                ledger.push(LedgerEntry::removed(
                    at.stage,
                    Reason::LigatureExpand,
                    at.page,
                    (at.char_offset + source, at.char_offset + source + 1),
                    ch.to_string(),
                ));
                let width = expansion.chars().count() as u32;
                ledger.push(LedgerEntry::added(
                    at.stage,
                    Reason::LigatureExpand,
                    at.page,
                    (out_chars, out_chars + width),
                    expansion.to_owned(),
                ));
            }
            out.push_str(expansion);
            out_chars += expansion.chars().count() as u32;
            continue;
        }

        out.push(ch);
        out_chars += 1;
    }

    // Step 4: compose again, but only if a strip could have brought a base and a mark
    // together. Expansions cannot: every expansion is ASCII letters, which compose with
    // nothing.
    if removed_any {
        return out.nfc().collect();
    }
    out
}

fn expansion_of(ch: char) -> Option<&'static str> {
    LIGATURES
        .iter()
        .find(|(ligature, _)| *ligature == ch)
        .map(|(_, expansion)| *expansion)
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Rows 2.1–2.4 of the Phase 2
// table.
// ---------------------------------------------------------------------------

/// The site every test normalises at: page 0, from the start of its character sequence.
#[cfg(test)]
const AT: LedgerSite = LedgerSite {
    stage: "text",
    page: 0,
    char_offset: 0,
};

#[test]
fn normalize_expands_ligatures_and_ledgers_both_sides() {
    let mut ledger = LedgerDelta::default();
    let out = normalize("\u{FB01}re", &mut ledger, AT);
    assert_eq!(out, "fire");

    let entries = ledger.entries();
    assert_eq!(entries.len(), 2, "one Removed and one Added: {entries:?}");

    let removed = &entries[0];
    assert_eq!(removed.reason, Reason::LigatureExpand);
    assert!(!removed.added);
    assert_eq!(removed.text, "\u{FB01}");
    assert_eq!(removed.span, (0, 1));

    let added = &entries[1];
    assert_eq!(added.reason, Reason::LigatureExpand);
    assert!(added.added);
    assert_eq!(added.text, "fi");
    assert_eq!(added.span, (0, 2));
}

#[test]
fn normalize_strips_soft_hyphen_with_reason() {
    let mut ledger = LedgerDelta::default();
    let out = normalize("Zu\u{00AD}cker", &mut ledger, AT);
    assert_eq!(out, "Zucker");

    let entries = ledger.entries();
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_eq!(entries[0].reason, Reason::SoftHyphen);
    assert!(!entries[0].added);
    assert_eq!(entries[0].text, "\u{00AD}");
    assert_eq!(entries[0].span, (2, 3));
}

#[test]
fn normalize_never_applies_nfkc() {
    // NFKC maps ¹ to 1 and ½ to 1⁄2, and the first of those destroys the superscript
    // footnote signal in the same pass that "cleans up" the text (R2 §B.8, RT C1).
    let mut ledger = LedgerDelta::default();
    assert_eq!(normalize("\u{00B9}", &mut ledger, AT), "\u{00B9}");
    assert_eq!(normalize("\u{00BD}", &mut ledger, AT), "\u{00BD}");
    assert_eq!(normalize("\u{FF21}", &mut ledger, AT), "\u{FF21}");
    assert!(ledger.is_empty(), "{:?}", ledger.entries());
}

#[test]
fn normalize_composes_to_nfc() {
    let mut ledger = LedgerDelta::default();
    // e + COMBINING ACUTE ACCENT is one character in NFC.
    assert_eq!(normalize("e\u{0301}", &mut ledger, AT), "\u{00E9}");
    // Stripping a soft hyphen can bring a base and a mark together; the result must still
    // be NFC, which is why `N` composes after it strips as well as before.
    assert_eq!(normalize("e\u{00AD}\u{0301}", &mut ledger, AT), "\u{00E9}");
}

#[test]
fn normalize_expands_the_whole_ligature_block() {
    let ledger = LedgerDelta::default();
    for (ligature, expansion) in [
        ('\u{FB00}', "ff"),
        ('\u{FB01}', "fi"),
        ('\u{FB02}', "fl"),
        ('\u{FB03}', "ffi"),
        ('\u{FB04}', "ffl"),
        ('\u{FB05}', "st"),
        ('\u{FB06}', "st"),
    ] {
        let mut one = LedgerDelta::default();
        assert_eq!(
            normalize(&ligature.to_string(), &mut one, AT),
            expansion,
            "U+{:04X}",
            u32::from(ligature)
        );
    }
    assert!(ledger.is_empty());
}

#[cfg(test)]
proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(10_000))]

    /// `N` is applied exactly once, at extraction — so it had better not matter if a bug
    /// ever applies it twice, and any later stage that does can be caught by comparing.
    #[test]
    fn normalize_is_idempotent(text in ".{0,64}") {
        let mut first_ledger = LedgerDelta::default();
        let once = normalize(&text, &mut first_ledger, AT);
        let mut second_ledger = LedgerDelta::default();
        let twice = normalize(&once, &mut second_ledger, AT);
        proptest::prop_assert_eq!(&once, &twice);
        proptest::prop_assert!(second_ledger.is_empty(), "{:?}", second_ledger.entries());
    }

    /// Case is never touched, anywhere, for any input (RT C1; the emitted-text half of
    /// test 2.7).
    #[test]
    fn normalize_never_changes_case(text in "[A-Za-zİıŞşĞğÄäÖöÜüß]{0,64}") {
        let mut ledger = LedgerDelta::default();
        let out = normalize(&text, &mut ledger, AT);
        let expected: String = text.nfc().collect();
        proptest::prop_assert_eq!(out.as_str(), expected.as_str());
    }
}
