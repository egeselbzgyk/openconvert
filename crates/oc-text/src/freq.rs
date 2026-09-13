//! Word-frequency lists, linked into the binary as data (D15, RT B10, PLAN Phase 2 detail 5).
//!
//! One question, asked a lot: **is this a word in this language?** The answer feeds the
//! dictionary hit rate, which is the second of the two independent signals that call a page
//! `broken_text` — the first being the share of characters that decode to nothing. Two
//! signals rather than one because they fail differently: a CID font stripped of `/ToUnicode`
//! trips the first, and a font whose encoding is *wrong* rather than missing decodes cleanly
//! to letters that spell nothing, and only trips the second.
//!
//! **No hunspell, no igerman98, no `zspell`** (D15). igerman98 is GPL/LGPL; a hunspell
//! dictionary is a spell-checker's affix machinery, which is far more than a membership test
//! and comes with whoever compiled it's licence attached.
//!
//! The blob is a sorted string table with an offset index, built by
//! `eval/src/oc_eval/generate/wordfreq.py` and committed alongside a manifest naming every
//! source it read and that source's licence. Little-endian throughout:
//!
//! ```text
//! offset  size            meaning
//! 0       8               b"OCFREQ1\0"
//! 8       8               language tag, ASCII, NUL-padded
//! 16      4               word count N
//! 20      4 * (N + 1)     byte offsets into the word table; offsets[0] == 0
//! ...     offsets[N]      the words, folded, sorted by byte order, concatenated
//! ```
//!
//! No FST, no perfect hash, no crate. The operation is "does this byte string appear in a
//! sorted list", the list is a couple of hundred kilobytes, and a format anyone can read with
//! a hex editor is one nobody has to trust.
//!
//! ## Only English ships today
//!
//! PLAN Phase 2 detail 5 names Standard Ebooks, DTA plain text and Wikisource-TR as "CC0/PD".
//! Standard Ebooks is CC0 and English ships. DTA and Wikisource host public-domain *works*
//! under **CC-BY-SA transcriptions**, which is not CC0 and is not on D15's allow-list for a
//! shipped artefact — a question for `DECISIONS.md`, not for this module. Until it is
//! answered, [`dict_hit_rate`] returns `None` for German and Turkish, and `None` is not zero:
//! zero is what a page of glyph indices scores, and the rules that consume it abstain rather
//! than call every German book broken.

use oc_model::lang::LangTag;

use crate::fold::fold_key;

const MAGIC: &[u8; 8] = b"OCFREQ1\0";
const LANG_FIELD_BYTES: usize = 8;
const HEADER_BYTES: usize = MAGIC.len() + LANG_FIELD_BYTES + 4;
const OFFSET_BYTES: usize = 4;

/// The lists compiled in, by primary subtag.
///
/// A list is added here by hand rather than discovered by a build script, because adding one
/// is a licence decision and a decision should be visible in a diff.
static LISTS: &[(&str, &[u8])] = &[("en", include_bytes!("freq/en.bin"))];

/// Tokens shorter than this are not evidence either way — they are as common in real prose as
/// in a page of glyph indices. Matches `MIN_WORD_CHARS` in the generator.
const MIN_WORD_CHARS: usize = 2;

/// Whether a frequency list is compiled in for this language.
pub fn have_list(lang: &LangTag) -> bool {
    list_bytes(lang).is_some()
}

/// The languages with a list, in the order they are linked.
pub fn languages() -> Vec<LangTag> {
    LISTS
        .iter()
        .map(|(tag, _)| LangTag::new(tag))
        .collect::<Vec<_>>()
}

/// How many words the language's list holds. `None` when there is no list.
pub fn word_count(lang: &LangTag) -> Option<u32> {
    let bytes = list_bytes(lang)?;
    read_u32(bytes, MAGIC.len() + LANG_FIELD_BYTES)
}

/// Whether `word` is in the language's list. `None` when there is no list.
///
/// `word` is folded before lookup with the same rule the generator folded the list with, so a
/// Turkish `ISPARTA` finds `ısparta` and an English `The` finds `the`.
pub fn contains(word: &str, lang: &LangTag) -> Option<bool> {
    let bytes = list_bytes(lang)?;
    let needle = fold_key(word, lang.clone());
    Some(binary_search(bytes, needle.as_bytes()))
}

/// What one whitespace-separated token is worth to the hit rate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Token {
    /// A word to look up, folded.
    Word(String),
    /// Characters that were meant to be letters and did not decode — a control character
    /// where a CID font's `/ToUnicode` used to be, a U+FFFD, a private-use point. It counts
    /// in the denominator and never hits, because "this failed to decode" is exactly the
    /// thing the signal is looking for.
    Undecodable,
    /// A number, a bare punctuation mark, a single letter. Not evidence either way, and
    /// counted in neither part of the ratio: a page of dates is not a broken page.
    NotEvidence,
}

/// Split a text into the tokens the hit rate is computed over.
///
/// Whitespace-separated, then classified — rather than "runs of letters", which would make a
/// page of glyph indices produce *no tokens at all* and the rate unmeasurable on the one input
/// the signal exists for.
pub fn tokens_of(text: &str, lang: &LangTag) -> Vec<Token> {
    text.split_whitespace()
        .map(|token| {
            if token.chars().any(crate::stats::is_undecodable) {
                return Token::Undecodable;
            }
            let letters: String = token.chars().filter(|ch| is_word_char(*ch)).collect();
            if letters.chars().filter(|ch| ch.is_alphabetic()).count() < MIN_WORD_CHARS {
                return Token::NotEvidence;
            }
            Token::Word(fold_key(letters.trim_matches(is_word_edge), lang.clone()).into_string())
        })
        .collect()
}

/// The share of a text's words that the language's list recognises.
///
/// `None` when there is no list for the language, and that is load-bearing: `Some(0.0)` says
/// "nothing here is a word", which is what a page of glyph indices scores and what
/// `broken_text` fires on. Returning zero for a language nobody built a list for would call
/// every Hungarian book broken.
///
/// `None` also when the text holds no countable token — a ratio over an empty denominator is
/// not zero, it is unmeasured.
pub fn dict_hit_rate(text: &str, lang: &LangTag) -> Option<f32> {
    let bytes = list_bytes(lang)?;

    let mut total = 0usize;
    let mut hits = 0usize;
    for token in tokens_of(text, lang) {
        match token {
            Token::NotEvidence => {}
            Token::Undecodable => total += 1,
            Token::Word(word) => {
                total += 1;
                if binary_search(bytes, word.as_bytes()) {
                    hits += 1;
                }
            }
        }
    }
    if total == 0 {
        return None;
    }
    Some(hits as f32 / total as f32)
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphabetic() || is_word_edge(ch)
}

/// Characters that join a word but are not one: an apostrophe or a hyphen inside `don't` and
/// `Nord-Süd`, and nothing at the edges.
fn is_word_edge(ch: char) -> bool {
    matches!(ch, '\'' | '\u{2019}' | '-')
}

fn list_bytes(lang: &LangTag) -> Option<&'static [u8]> {
    let primary = lang.primary();
    LISTS
        .iter()
        .find(|(tag, _)| *tag == primary)
        .map(|(_, bytes)| *bytes)
}

/// Is `needle` in the sorted table? A plain binary search over the offset index.
fn binary_search(bytes: &[u8], needle: &[u8]) -> bool {
    let Some(count) = read_u32(bytes, MAGIC.len() + LANG_FIELD_BYTES) else {
        return false;
    };
    let count = count as usize;
    if count == 0 {
        return false;
    }
    let table = HEADER_BYTES + OFFSET_BYTES * (count + 1);

    let (mut low, mut high) = (0usize, count);
    while low < high {
        let middle = low + (high - low) / 2;
        let Some(word) = word_at(bytes, table, middle) else {
            return false;
        };
        match word.cmp(needle) {
            std::cmp::Ordering::Less => low = middle + 1,
            std::cmp::Ordering::Greater => high = middle,
            std::cmp::Ordering::Equal => return true,
        }
    }
    false
}

fn word_at(bytes: &[u8], table: usize, index: usize) -> Option<&[u8]> {
    let start = read_u32(bytes, HEADER_BYTES + OFFSET_BYTES * index)? as usize;
    let end = read_u32(bytes, HEADER_BYTES + OFFSET_BYTES * (index + 1))? as usize;
    bytes.get(table.checked_add(start)?..table.checked_add(end)?)
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    let slice = bytes.get(at..at.checked_add(4)?)?;
    let array: [u8; 4] = slice.try_into().ok()?;
    Some(u32::from_le_bytes(array))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn the_english_blob_is_well_formed() {
    let english = LangTag::EN;
    assert!(have_list(&english));
    let count = word_count(&english).expect("english ships");
    assert!(count > 5_000, "{count} words is too few to judge a page on");

    // Sorted by bytes and deduplicated, which is what the binary search assumes.
    let bytes = list_bytes(&english).expect("english ships");
    let table = HEADER_BYTES + OFFSET_BYTES * (count as usize + 1);
    let mut previous: &[u8] = b"";
    for index in 0..count as usize {
        let word = word_at(bytes, table, index).expect("every offset resolves");
        assert!(
            previous < word,
            "not sorted at {index}: {:?} then {:?}",
            String::from_utf8_lossy(previous),
            String::from_utf8_lossy(word)
        );
        assert!(std::str::from_utf8(word).is_ok(), "not UTF-8 at {index}");
        previous = word;
    }
}

#[test]
fn common_words_are_in_and_glyph_indices_are_not() {
    let english = LangTag::EN;
    for word in ["the", "and", "night", "harbour", "quiet", "The", "NIGHT"] {
        assert_eq!(contains(word, &english), Some(true), "{word}");
    }
    for word in ["zzxq", "qqqqq", "\u{1}\u{2}\u{3}"] {
        assert_eq!(contains(word, &english), Some(false), "{word:?}");
    }
}

#[test]
fn a_language_with_no_list_answers_none_not_zero() {
    // The whole point of the Option. See the module note on DTA and Wikisource-TR.
    assert_eq!(
        dict_hit_rate("Zucker und Salz im Wasser", &LangTag::DE),
        None
    );
    assert_eq!(contains("zucker", &LangTag::DE), None);
    assert!(!have_list(&LangTag::TR));
}

#[test]
fn prose_scores_high_and_glyph_indices_score_zero() {
    let english = LangTag::EN;
    let prose = "It was a dark and stormy night; the rain fell in torrents, except at \
                 occasional intervals, when it was checked by a violent gust of wind.";
    let rate = dict_hit_rate(prose, &english).expect("english ships");
    assert!(rate > 0.9, "{rate}");

    // A CID font stripped of `/ToUnicode`: control characters where letters should be. They
    // are not "not words", they are words that failed to decode, so they count and they miss.
    let indices = "\u{1}\u{2}\u{3}\u{4} \u{1}\u{3}\u{5}\u{6}\u{4} \u{2}\u{5}\u{1}";
    assert_eq!(dict_hit_rate(indices, &english), Some(0.0));

    // Letters that spell nothing are the case this signal exists for: they decode cleanly,
    // so the replacement share says nothing, and only the dictionary knows.
    let mojibake = "qxzj vbkq wjxz mqpv zxqj kbvn";
    let rate = dict_hit_rate(mojibake, &english).expect("english ships");
    assert_eq!(rate, 0.0);
}

#[test]
fn an_empty_text_is_unmeasured_not_zero() {
    assert_eq!(dict_hit_rate("", &LangTag::EN), None);
    assert_eq!(dict_hit_rate("   \n  ", &LangTag::EN), None);
    // A page of dates and page numbers is not a broken page, and it is not a measured one
    // either: none of those tokens is evidence about whether the text decoded.
    assert_eq!(dict_hit_rate("1996 42 — 7 ( ) 1,200", &LangTag::EN), None);
}

#[test]
fn tokens_are_classified_before_they_are_counted() {
    let english = LangTag::EN;
    assert_eq!(
        tokens_of("night, 1996 \u{FFFD}\u{FFFD} a don't", &english),
        vec![
            Token::Word("night".to_owned()),
            Token::NotEvidence,
            Token::Undecodable,
            Token::NotEvidence,
            Token::Word("don't".to_owned()),
        ]
    );
}
