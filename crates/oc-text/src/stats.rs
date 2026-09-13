//! The suspicious-text statistics: the Gopher/MassiveText family, with the production
//! thresholds from `datatrove` (PIPELINE §4 step 6, R10 §6.18).
//!
//! **These statistics route, they never fix.** That is the whole of their safety argument:
//! a statistic that only flags has zero false-repair risk, whatever its false-positive rate,
//! and the cost of a wrong verdict is a page in the review pane rather than a corrupted book.
//! They are also the free oracle Gate V reuses, and the same repetition thresholds catch
//! generative degeneration if a model is ever put behind them.
//!
//! Nine numbers, and every one of them is a *fraction of something stated*, because a raw
//! count means nothing across a two-page pamphlet and a six-hundred-page novel.

use oc_core::thresholds::Thresholds;
use oc_model::lang::LangTag;
use serde::Serialize;
use std::collections::BTreeMap;

/// What a set of statistics describes.
///
/// The verdict is per page (PIPELINE §4: "Per-page verdict: ok / suspicious / broken"),
/// because a scanned plate in the middle of a clean book must not drag the book's numbers
/// down, and a book-wide figure cannot say which page to look at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Region {
    Page(u32),
    Document,
}

/// How much to trust a region's text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Ok,
    /// Something is off. The page is flagged with the failing statistic named, and a human
    /// decides.
    Suspicious,
    /// The text is not text. On a page `inspect` called `Text`, this is a disagreement
    /// between two independent detectors and is worth a bounded re-ingest (PIPELINE §4).
    Broken,
}

/// The nine statistics, for one region.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct QualityStats {
    pub region: Region,
    /// Lines that are a repeat of an earlier line, over all lines.
    pub dup_line_frac: f32,
    /// The same, over blank-line-separated paragraphs.
    pub dup_para_frac: f32,
    /// The share of the text taken by its most frequent word 2-gram, 3-gram, 4-gram.
    pub top_2gram: f32,
    pub top_3gram: f32,
    pub top_4gram: f32,
    /// Words with no alphabetic character in them, over all words.
    pub non_alpha_word_ratio: f32,
    pub mean_word_len: f32,
    /// U+FFFD and private-use characters, over all non-whitespace characters.
    pub replacement_share: f32,
    /// The share of words found in the language's frequency list.
    ///
    /// `None` when no list is loaded for the language — **not** zero. Zero is the value a page
    /// of glyph indices scores, and a rule that could not tell "no dictionary" from "nothing
    /// matched" would call every Hungarian book broken.
    pub dict_hit_rate: Option<f32>,
}

impl QualityStats {
    /// The routing verdict, and the reason it is conservative.
    ///
    /// `Broken` needs a signal that text is not text at all — replacement characters over the
    /// share `inspect` already uses, or a dictionary that recognises nothing. `Suspicious` is
    /// everything the Gopher thresholds catch: repetition, degenerate word length, a page that
    /// is mostly punctuation. Everything else is `Ok`, including a page with no statistics to
    /// compute, because an empty region is empty rather than broken.
    pub fn verdict(&self, t: &Thresholds) -> Verdict {
        let quality = &t.quality;
        if self.replacement_share > t.pageclass.broken_text_replacement_share as f32 {
            return Verdict::Broken;
        }
        if self
            .dict_hit_rate
            .is_some_and(|rate| rate < t.pageclass.broken_text_dict_hit_min as f32)
        {
            return Verdict::Broken;
        }

        let repetitive = self.dup_line_frac > quality.dup_line_frac as f32
            || self.dup_para_frac > quality.dup_para_frac as f32
            || self.top_2gram > quality.top_2gram_frac as f32
            || self.top_3gram > quality.top_3gram_frac as f32
            || self.top_4gram > quality.top_4gram_frac as f32;
        let degenerate = self.non_alpha_word_ratio > quality.max_non_alpha_words_ratio as f32
            || (self.mean_word_len > 0.0
                && (self.mean_word_len < quality.mean_word_len_min as f32
                    || self.mean_word_len > quality.mean_word_len_max as f32));
        if repetitive || degenerate {
            return Verdict::Suspicious;
        }
        Verdict::Ok
    }
}

/// Compute the nine statistics over one region's text.
///
/// `dict_hit_rate` is passed in rather than computed here: the frequency lists live in
/// [`crate::freq`] and a caller with no list for the language passes `None`, which is a
/// different thing from a hit rate of zero.
pub fn quality_stats(
    text: &str,
    _lang: LangTag,
    region: Region,
    dict_hit_rate: Option<f32>,
) -> QualityStats {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let paragraphs: Vec<String> = text
        .split("\n\n")
        .map(|block| block.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|block| !block.is_empty())
        .collect();
    let words: Vec<&str> = text.split_whitespace().collect();

    QualityStats {
        region,
        dup_line_frac: duplicate_fraction(lines.iter().copied()),
        dup_para_frac: duplicate_fraction(paragraphs.iter().map(String::as_str)),
        top_2gram: top_ngram_share(&words, 2),
        top_3gram: top_ngram_share(&words, 3),
        top_4gram: top_ngram_share(&words, 4),
        non_alpha_word_ratio: ratio(
            words
                .iter()
                .filter(|word| !word.chars().any(char::is_alphabetic))
                .count(),
            words.len(),
        ),
        mean_word_len: mean_word_len(&words),
        replacement_share: replacement_share(text),
        dict_hit_rate,
    }
}

/// The share of items that are a repeat of an earlier item.
///
/// An item seen `k` times contributes `k − 1` duplicates, so a page whose every line is
/// unique scores zero and a page that is one line repeated ten times scores 0.9.
fn duplicate_fraction<'a>(items: impl Iterator<Item = &'a str>) -> f32 {
    let mut seen: BTreeMap<&str, u32> = BTreeMap::new();
    let mut total = 0usize;
    for item in items {
        *seen.entry(item).or_default() += 1;
        total += 1;
    }
    let duplicates: usize = seen
        .values()
        .map(|count| usize::try_from(count.saturating_sub(1)).unwrap_or_default())
        .sum();
    ratio(duplicates, total)
}

/// The share of the text's characters taken by its most frequent word `n`-gram.
///
/// Characters rather than occurrences, as datatrove counts it: a five-word phrase repeated
/// three times has taken more of the page than a two-word one repeated three times, and the
/// threshold is a statement about how much of the page is repetition.
fn top_ngram_share(words: &[&str], n: usize) -> f32 {
    if words.len() < n || n == 0 {
        return 0.0;
    }
    let total_chars: usize = words.iter().map(|word| word.chars().count()).sum();
    if total_chars == 0 {
        return 0.0;
    }

    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for window in words.windows(n) {
        *counts.entry(window.join(" ")).or_default() += 1;
    }
    counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(gram, count)| {
            let chars = gram.chars().filter(|ch| !ch.is_whitespace()).count();
            ratio(chars * count, total_chars)
        })
        .fold(0.0f32, f32::max)
}

fn mean_word_len(words: &[&str]) -> f32 {
    if words.is_empty() {
        return 0.0;
    }
    let total: usize = words.iter().map(|word| word.chars().count()).sum();
    total as f32 / words.len() as f32
}

/// U+FFFD and private-use characters, over all non-whitespace characters.
///
/// The same quantity `inspect` classifies `broken_text` on, computed here over assembled text
/// rather than over the glyph stream — which is what makes the two an *independent* pair of
/// signals rather than one signal read twice (PIPELINE §4).
fn replacement_share(text: &str) -> f32 {
    let mut total = 0usize;
    let mut suspect = 0usize;
    for ch in text.chars().filter(|ch| !ch.is_whitespace()) {
        total += 1;
        if is_undecodable(ch) {
            suspect += 1;
        }
    }
    ratio(suspect, total)
}

/// A character that stands for something that could not be decoded: the replacement
/// character, a private-use code point, or a control character that is not layout.
fn is_undecodable(ch: char) -> bool {
    const PRIVATE_USE: [std::ops::RangeInclusive<u32>; 3] =
        [0xE000..=0xF8FF, 0xF0000..=0xFFFFD, 0x100000..=0x10FFFD];
    if ch == char::REPLACEMENT_CHARACTER {
        return true;
    }
    if ch.is_control() && !matches!(ch, '\t' | '\n' | '\r') {
        return true;
    }
    let code = u32::from(ch);
    PRIVATE_USE.iter().any(|range| range.contains(&code))
}

fn ratio(part: usize, whole: usize) -> f32 {
    if whole == 0 {
        return 0.0;
    }
    part as f32 / whole as f32
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Row 2.18 of the Phase 2 table.
// ---------------------------------------------------------------------------

#[test]
fn quality_stats_match_datatrove_thresholds() {
    // Ten lines, four of which are second occurrences: a duplicate fraction of exactly 0.40,
    // over datatrove's production threshold of 0.30 (R10 §6.18).
    let text = "alpha\nbeta\ngamma\ndelta\nepsilon\nzeta\nalpha\nbeta\ngamma\ndelta\n";
    let stats = quality_stats(text, LangTag::EN, Region::Page(0), None);

    assert!(
        (stats.dup_line_frac - 0.40).abs() < 1e-6,
        "{}",
        stats.dup_line_frac
    );
    assert!(stats.dup_line_frac > oc_core::thresholds::T.quality.dup_line_frac as f32);
    assert_eq!(stats.verdict(&oc_core::thresholds::T), Verdict::Suspicious);
}

#[test]
fn clean_prose_is_ok() {
    let text = "It was a dark and stormy night; the rain fell in torrents.\n\
                The office was quiet, and a single clerk remained at his desk.\n\
                Outside, the harbour lights went out one by one.\n";
    let stats = quality_stats(text, LangTag::EN, Region::Page(0), None);

    assert_eq!(stats.dup_line_frac, 0.0);
    assert_eq!(stats.dup_para_frac, 0.0);
    assert!(stats.mean_word_len > 3.0 && stats.mean_word_len < 10.0);
    assert_eq!(stats.replacement_share, 0.0);
    assert_eq!(stats.dict_hit_rate, None);
    assert_eq!(stats.verdict(&oc_core::thresholds::T), Verdict::Ok);
}

#[test]
fn a_repeated_phrase_shows_up_in_the_top_ngram() {
    // Six of the twelve words are the same pair, said three times.
    let text = "buy now buy now buy now the quick brown fox jumps over\n";
    let stats = quality_stats(text, LangTag::EN, Region::Page(0), None);
    assert!(
        stats.top_2gram > oc_core::thresholds::T.quality.top_2gram_frac as f32,
        "{}",
        stats.top_2gram
    );
}

#[test]
fn replacement_characters_are_counted_as_a_share_of_the_text() {
    let text = "abc\u{FFFD}\u{E000}";
    let stats = quality_stats(text, LangTag::EN, Region::Page(0), None);
    assert!(
        (stats.replacement_share - 0.4).abs() < 1e-6,
        "{}",
        stats.replacement_share
    );
    assert_eq!(stats.verdict(&oc_core::thresholds::T), Verdict::Broken);
}

#[test]
fn a_page_of_glyph_indices_is_broken_by_both_signals() {
    // What a CID font stripped of `/ToUnicode` produces: control characters where letters
    // should be, and no word in any dictionary.
    let text = "\u{1}\u{2}\u{3}\u{4} \u{1}\u{3}\u{5}\u{6}\u{4}";
    let stats = quality_stats(text, LangTag::EN, Region::Page(0), Some(0.0));
    assert_eq!(stats.verdict(&oc_core::thresholds::T), Verdict::Broken);
    assert_eq!(stats.dict_hit_rate, Some(0.0));
}

#[test]
fn empty_text_is_not_broken_it_is_empty() {
    let stats = quality_stats("", LangTag::EN, Region::Page(0), None);
    assert_eq!(stats.dup_line_frac, 0.0);
    assert_eq!(stats.mean_word_len, 0.0);
    assert_eq!(stats.verdict(&oc_core::thresholds::T), Verdict::Ok);
}
