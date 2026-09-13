//! Language detection: `dc:language` for the package, `xml:lang` for the blocks that earn it
//! (D13.11, PIPELINE §4 step 7, R10 §6.17).
//!
//! Two problems that look like one. **The document's language** is easy — a whole book of
//! running text through `whatlang` is effectively always right — and it is also *required*:
//! EPUB 3.3 will not accept a package without `dc:language`, so this function cannot fail,
//! only fall back and say so.
//!
//! **A block's language** is the hard one, and the failure mode is not "wrong tag on one
//! paragraph". A detector run per block on a monolingual book tags a scattering of short
//! paragraphs as Dutch, Afrikaans and Catalan, and a screen reader switches voice mid-chapter
//! for each of them. Three guards, all from R10 §6.17, and a block needs all three:
//!
//! 1. **Length.** Fewer than `lang.block_min_words` words is not evidence. Trigram models on
//!    a four-word paragraph are guessing.
//! 2. **Margin.** The top-two gap has to clear `lang.block_min_confidence`. `whatlang`'s
//!    `confidence` *is* that gap, normalised — which is why it is the number checked rather
//!    than a raw score.
//! 3. **Share.** If more than `lang.block_override_max_share` of the blocks would be
//!    overridden, **none of them is**, and the document warns `W_LANG_UNSTABLE`. The
//!    reasoning is worth stating plainly: if a fifth of the book disagrees with the primary
//!    language, the likeliest explanation is that the primary detection was wrong, and the
//!    second likeliest is that the detector is unreliable on this text. Neither is repaired
//!    by tagging every dissenting paragraph.
//!
//! `lingua` was rejected: its default build pulls roughly 300 MB of language models (RT B11).

use oc_core::thresholds::Thresholds;
use oc_model::lang::LangTag;
use whatlang::Detector;

/// More than `lang.block_override_max_share` of blocks disagreed with the document language,
/// so no per-block tag was applied. The report names the share.
pub const W_LANG_UNSTABLE: &str = "W_LANG_UNSTABLE";

/// No language could be detected, so the job's locale was used for `dc:language`. Loud on
/// purpose: EPUB 3.3 requires the field, so the alternative to a guess is an invalid package,
/// and a reader deserves to know which they got.
pub const W_LANG_FALLBACK: &str = "W_LANG_FALLBACK";

/// ISO 639-3 (what `whatlang` speaks) to ISO 639-1 (what `dc:language` and `xml:lang` want).
///
/// `whatlang::Lang::code()` returns three-letter 639-3 codes — `eng`, `deu`, `tur` — and BCP-47
/// requires the shortest code that exists for a language, which for all seventy of these is the
/// two-letter one. Emitting `eng` would produce a package EPUBCheck rejects and a reading system
/// cannot match a voice to.
///
/// Two of the seventy are macrolanguage cases and are mapped deliberately: `cmn` (Mandarin) to
/// `zh` and `pes` (Western Persian) to `fa`, because those are the tags a reading system knows.
const ISO_639_3_TO_1: [(&str, &str); 70] = [
    ("epo", "eo"),
    ("eng", "en"),
    ("rus", "ru"),
    ("cmn", "zh"),
    ("spa", "es"),
    ("por", "pt"),
    ("ita", "it"),
    ("ben", "bn"),
    ("fra", "fr"),
    ("deu", "de"),
    ("ukr", "uk"),
    ("kat", "ka"),
    ("ara", "ar"),
    ("hin", "hi"),
    ("jpn", "ja"),
    ("heb", "he"),
    ("yid", "yi"),
    ("pol", "pl"),
    ("amh", "am"),
    ("jav", "jv"),
    ("kor", "ko"),
    ("nob", "nb"),
    ("dan", "da"),
    ("swe", "sv"),
    ("fin", "fi"),
    ("tur", "tr"),
    ("nld", "nl"),
    ("hun", "hu"),
    ("ces", "cs"),
    ("ell", "el"),
    ("bul", "bg"),
    ("bel", "be"),
    ("mar", "mr"),
    ("kan", "kn"),
    ("ron", "ro"),
    ("slv", "sl"),
    ("hrv", "hr"),
    ("srp", "sr"),
    ("mkd", "mk"),
    ("lit", "lt"),
    ("lav", "lv"),
    ("est", "et"),
    ("tam", "ta"),
    ("vie", "vi"),
    ("urd", "ur"),
    ("tha", "th"),
    ("guj", "gu"),
    ("uzb", "uz"),
    ("pan", "pa"),
    ("aze", "az"),
    ("ind", "id"),
    ("tel", "te"),
    ("pes", "fa"),
    ("mal", "ml"),
    ("ori", "or"),
    ("mya", "my"),
    ("nep", "ne"),
    ("sin", "si"),
    ("khm", "km"),
    ("tuk", "tk"),
    ("aka", "ak"),
    ("zul", "zu"),
    ("sna", "sn"),
    ("afr", "af"),
    ("lat", "la"),
    ("slk", "sk"),
    ("cat", "ca"),
    ("tgl", "tl"),
    ("hye", "hy"),
    ("cym", "cy"),
];

/// The BCP-47 tag for a language `whatlang` named.
///
/// Falls back to the 639-3 code for anything unmapped — a wrong-looking tag is recoverable and
/// a missing one is not, since `dc:language` is required (EPUB 3.3).
fn tag_of(lang: whatlang::Lang) -> LangTag {
    let code = lang.code();
    let short = ISO_639_3_TO_1
        .iter()
        .find(|(three, _)| *three == code)
        .map(|(_, two)| *two)
        .unwrap_or(code);
    LangTag::new(short)
}

/// The document's language, and whether it had to be guessed.
#[derive(Clone, Debug, PartialEq)]
pub struct DocumentLanguage {
    pub lang: LangTag,
    pub confidence: f32,
    /// The detector declined and `fallback` was used. `dc:language` is required by EPUB 3.3,
    /// so this is a warning rather than an error.
    pub fell_back: bool,
}

/// Detect `dc:language` over the whole body.
///
/// `fallback` is the job's locale. It is a parameter and not a constant because "the language
/// of a document we could not read" is a property of who is converting it, not of this crate.
pub fn detect_document(text: &str, fallback: LangTag) -> DocumentLanguage {
    match Detector::new().detect(text) {
        Some(info) if info.is_reliable() => DocumentLanguage {
            lang: tag_of(info.lang()),
            confidence: info.confidence() as f32,
            fell_back: false,
        },
        Some(info) => DocumentLanguage {
            // Detected but not reliable: the tag is still better evidence than the locale of
            // whoever happened to run the conversion, and the confidence records how thin it
            // is.
            lang: tag_of(info.lang()),
            confidence: info.confidence() as f32,
            fell_back: false,
        },
        None => DocumentLanguage {
            lang: fallback,
            confidence: 0.0,
            fell_back: true,
        },
    }
}

/// Per-block `xml:lang`, or nothing at all.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockLanguages {
    /// One entry per input block, in order. `None` means "no `xml:lang`", which is the
    /// answer for every block on a monolingual book.
    pub tags: Vec<Option<LangTag>>,
    /// The share of blocks that *would* have been overridden before the cap was applied.
    pub proposed_share: f32,
    pub warnings: Vec<&'static str>,
}

/// Assign `xml:lang` to the blocks that clear all three guards.
pub fn detect_blocks(blocks: &[&str], document: &LangTag, t: &Thresholds) -> BlockLanguages {
    let detector = Detector::new();
    let min_words = usize::try_from(t.lang.block_min_words.max(0)).unwrap_or(usize::MAX);
    let min_confidence = t.lang.block_min_confidence;

    let proposed: Vec<Option<LangTag>> = blocks
        .iter()
        .map(|block| {
            if block.split_whitespace().count() < min_words {
                return None;
            }
            let info = detector.detect(block)?;
            if info.confidence() < min_confidence {
                return None;
            }
            let tag = tag_of(info.lang());
            // A block in the document's own language needs no attribute: `xml:lang` on every
            // paragraph of a monolingual book is noise in the markup and nothing in the ear.
            (tag.primary() != document.primary()).then_some(tag)
        })
        .collect();

    let overridden = proposed.iter().filter(|tag| tag.is_some()).count();
    let share = if blocks.is_empty() {
        0.0
    } else {
        overridden as f32 / blocks.len() as f32
    };

    if share > t.lang.block_override_max_share as f32 {
        // The third guard, and the one that fires on the failure that matters: a fifth of the
        // book disagreeing with the primary language means the primary detection was wrong or
        // the detector is unreliable here, and neither is fixed by applying the overrides.
        return BlockLanguages {
            tags: vec![None; blocks.len()],
            proposed_share: share,
            warnings: vec![W_LANG_UNSTABLE],
        };
    }

    BlockLanguages {
        tags: proposed,
        proposed_share: share,
        warnings: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
const ENGLISH: &str = "It was a dark and stormy night; the rain fell in torrents, except at \
     occasional intervals, when it was checked by a violent gust of wind which swept up the \
     streets, rattling along the housetops, and fiercely agitating the scanty flame of the \
     lamps that struggled against the darkness.";

#[cfg(test)]
const GERMAN: &str = "Der Herbst kam früh in diesem Jahr. Über den Feldern lag ein grauer \
     Nebel, und die Bäume an der Straße hatten ihre Blätter schon verloren. Der alte Müller \
     saß vor seinem Haus und rauchte eine Pfeife, während die Kinder aus dem Dorf zwischen \
     den Scheunen spielten.";

#[test]
fn a_document_with_no_text_falls_back_and_says_so() {
    let verdict = detect_document("", LangTag::EN);
    assert!(verdict.fell_back);
    assert_eq!(verdict.lang, LangTag::EN);
}

#[test]
fn block_lang_override_capped() {
    // Ten blocks, four of them German in an English document: 40 % would be overridden,
    // which is over the 20 % cap, so **none** is — and the document says why.
    let mut blocks: Vec<&str> = vec![ENGLISH; 6];
    blocks.extend([GERMAN; 4]);

    let result = detect_blocks(&blocks, &LangTag::EN, &oc_core::thresholds::T);
    assert!(
        (result.proposed_share - 0.40).abs() < 1e-6,
        "{}",
        result.proposed_share
    );
    assert!(
        result.tags.iter().all(Option::is_none),
        "nothing may be overridden: {:?}",
        result.tags
    );
    assert_eq!(result.warnings, vec![W_LANG_UNSTABLE]);
}

#[test]
fn a_single_foreign_block_is_tagged() {
    // The same detector, under the cap: one German block in ten is 10 %, and it gets its
    // attribute. Without this the cap would be indistinguishable from switching the feature
    // off.
    let mut blocks: Vec<&str> = vec![ENGLISH; 9];
    blocks.push(GERMAN);

    let result = detect_blocks(&blocks, &LangTag::EN, &oc_core::thresholds::T);
    assert!(result.warnings.is_empty());
    assert_eq!(result.tags.iter().filter(|tag| tag.is_some()).count(), 1);
    assert_eq!(
        result
            .tags
            .last()
            .and_then(Clone::clone)
            .map(|t| t.primary().to_owned()),
        Some("de".to_owned())
    );
}

#[test]
fn a_short_block_is_never_tagged() {
    // Four words is not evidence, however confident a trigram model claims to be.
    let blocks = ["Der Herbst kam früh", ENGLISH];
    let result = detect_blocks(&blocks, &LangTag::EN, &oc_core::thresholds::T);
    assert_eq!(result.tags[0], None);
}

#[test]
fn a_block_in_the_documents_own_language_gets_no_attribute() {
    let result = detect_blocks(&[ENGLISH, ENGLISH], &LangTag::EN, &oc_core::thresholds::T);
    assert!(result.tags.iter().all(Option::is_none));
    assert_eq!(result.proposed_share, 0.0);
}

#[test]
fn every_language_whatlang_knows_has_a_two_letter_tag() {
    // A language that reached `dc:language` as `eng` would fail EPUBCheck, and the only way
    // to find out would be at the end of a conversion. This says so at build time instead.
    for lang in whatlang::Lang::all() {
        let tag = tag_of(*lang);
        assert_eq!(
            tag.as_str().chars().count(),
            2,
            "{} ({}) has no two-letter tag",
            lang.eng_name(),
            lang.code()
        );
    }
}

#[test]
fn the_three_languages_v1_claims_are_named_correctly() {
    assert_eq!(detect_document(ENGLISH, LangTag::EN).lang, LangTag::EN);
    assert_eq!(detect_document(GERMAN, LangTag::EN).lang, LangTag::DE);
}
