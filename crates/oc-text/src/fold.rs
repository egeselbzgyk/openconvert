//! Turkish-aware folding keys — the **only** place in the pipeline where case changes
//! (D13.4, R1 §A.11 #10, R10 §6.3).
//!
//! A folding key is for comparison and lookup: does this band line repeat across pages, is
//! this word in the frequency list, are these two heading strings the same string. It is
//! never emitted. Emitted text keeps the document's own case, which is what the property
//! test in this module asserts over arbitrary input.
//!
//! The locale parameter is not a nicety. Turkish has four i's in two pairs — `İ`/`i` and
//! `I`/`ı` — and invariant folding pairs them the Latin way, so `ISPARTA` folds to `isparta`
//! (a word with a letter Turkish does not have there) and `İSTANBUL` to `i̇stanbul` (with a
//! stray combining dot). Both then fail to match the lower-case form printed on the next
//! page, and a running head goes undetected.

use compact_str::CompactString;
use oc_model::lang::LangTag;
use unicode_normalization::UnicodeNormalization;

/// LATIN CAPITAL LETTER I WITH DOT ABOVE — the capital of Turkish `i`.
const DOTTED_CAPITAL_I: char = '\u{0130}';
/// LATIN SMALL LETTER DOTLESS I — the lower case of Turkish `I`.
const DOTLESS_SMALL_I: char = '\u{0131}';

/// A comparison key for `text` under `lang`: composed, lower-cased in the language's own
/// locale, and nothing else.
///
/// Simple lower-casing rather than full Unicode case folding, so `ß` stays `ß` and does not
/// become `ss`. Full folding would merge two spellings a German book distinguishes, and the
/// keys are compared against each other rather than against an external corpus, so the
/// weaker rule is the safer one.
pub fn fold_key(text: &str, lang: LangTag) -> CompactString {
    let turkic = lang.is_turkic_i();
    let mut out = CompactString::default();
    for ch in text.nfc() {
        match ch {
            DOTTED_CAPITAL_I if turkic => out.push('i'),
            'I' if turkic => out.push(DOTLESS_SMALL_I),
            _ => {
                for lower in ch.to_lowercase() {
                    out.push(lower);
                }
            }
        }
    }
    // Lower-casing `İ` outside a Turkic locale yields `i` + COMBINING DOT ABOVE, which is
    // not composed; compose again so two spellings of one key are one key.
    out.nfc().collect()
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Rows 2.6 and 2.7 of the
// Phase 2 table.
// ---------------------------------------------------------------------------

#[test]
fn fold_key_is_turkish_aware() {
    // The dotted/dotless i is simultaneously an encoding hazard, an OCR confusion pair and
    // a folding trap (R10 §6.3). Turkish has four i's and the locale decides which pairs
    // with which: İ↔i and I↔ı.
    assert_eq!(fold_key("İSTANBUL", LangTag::TR), "istanbul");
    assert_eq!(fold_key("ISPARTA", LangTag::TR), "ısparta");

    // Invariant folding gets both of them wrong, which is the point of the parameter.
    assert_ne!(fold_key("İSTANBUL", LangTag::EN), "istanbul");
    assert_ne!(fold_key("ISPARTA", LangTag::EN), "ısparta");
    assert_eq!(fold_key("ISPARTA", LangTag::EN), "isparta");

    // And it is a round trip for text already in the target case.
    assert_eq!(fold_key("istanbul", LangTag::TR), "istanbul");
    assert_eq!(fold_key("ısparta", LangTag::TR), "ısparta");
}

#[test]
fn fold_key_lowercases_the_rest_of_the_world_normally() {
    assert_eq!(fold_key("Grüße", LangTag::DE), "grüße");
    assert_eq!(fold_key("MOBY-DICK", LangTag::EN), "moby-dick");
    // Azerbaijani shares Turkish's i's.
    assert_eq!(fold_key("İL", LangTag::new("az")), "il");
}

#[test]
fn fold_key_is_nfc() {
    // e + COMBINING ACUTE folds to the composed form, so two spellings of the same word
    // produce the same key.
    assert_eq!(
        fold_key("E\u{0301}TE", LangTag::EN),
        fold_key("\u{00E9}te", LangTag::EN)
    );
}

#[cfg(test)]
proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(10_000))]

    /// `fold_key` is the only place in the pipeline where casing happens. Emitted text —
    /// what `N` produces and what reaches the EPUB — keeps the document's own case, for
    /// every input (RT C1, ARCHITECTURE §5.1).
    #[test]
    fn text_is_never_case_folded_in_output(text in "[^\\x{00AD}\\x{FB00}-\\x{FB06}]{0,64}") {
        let mut ledger = oc_model::ledger::LedgerDelta::default();
        let out = crate::normalize::normalize(&text, &mut ledger, crate::normalize::LedgerSite {
            stage: "text",
            page: 0,
            char_offset: 0,
        });
        let composed: String = unicode_normalization::UnicodeNormalization::nfc(text.as_str())
            .collect();
        proptest::prop_assert_eq!(out.as_str(), composed.as_str());
        proptest::prop_assert!(ledger.is_empty());
    }
}
