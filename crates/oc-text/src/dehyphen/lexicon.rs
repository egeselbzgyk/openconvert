//! The in-document lexicon: what this book's own vocabulary says (PIPELINE §7 tier T3).
//!
//! Calibre's trick, and the strongest cheap evidence there is. A book about pipelines
//! contains the word `pipeline` somewhere other than at the line break in question, and a book
//! about the Nord-Süd-Achse contains `Nord-Süd-Achse` with its hyphen. No external dictionary
//! knows either, because neither is a fact about the language — they are facts about *this
//! document*, which is exactly the level the question is asked at.
//!
//! What goes in: every word of the document as the paragraphs were assembled, folded with the
//! document's own locale rule so that a sentence-initial `Pipeline` answers for `pipeline`
//! and a Turkish `ISPARTA` for `ısparta`. Hyphenated forms go in separately, whole, because
//! the question they answer is a different one.
//!
//! What must *not* go in: the broken pieces themselves. They are two tokens at the line
//! break — `pipe-` and `line` — and a lexicon that counted them as words would answer "yes,
//! the hyphenated form occurs elsewhere" about the very occurrence being asked about.

use std::collections::BTreeMap;

use compact_str::CompactString;
use oc_model::lang::LangTag;

use crate::fold::fold_key;

use super::{is_word_char, LINE_BREAK_HYPHENS};

/// The words of one document, folded, with how often each occurs.
#[derive(Clone, Debug)]
pub struct DocLexicon {
    /// Unhyphenated words.
    plain: BTreeMap<CompactString, u32>,
    /// Words containing an interior hyphen, held whole.
    hyphenated: BTreeMap<CompactString, u32>,
    /// Hyphenated words whose second part starts lower-case: the compounds a book writes with
    /// a hyphen, as opposed to a name joined to a name.
    lower_compounds: u32,
    /// Words met mid-sentence, and how many of them start with a capital: a book that
    /// capitalises its nouns — German does — capitalises a quarter of them.
    mid_sentence: u32,
    mid_sentence_capitals: u32,
    /// Words that follow a hyphen left hanging mid-line — `Ein- und Ausgang`, `pre- and
    /// post-war` — whichever language's conjunction that is.
    after_suspended: BTreeMap<CompactString, u32>,
    /// The locale the keys were folded with. Carried rather than passed at lookup time,
    /// because a lookup folded differently from the build asks for a key nobody wrote — and
    /// Turkish folds its dotted and dotless i its own way, so that is a real failure and not
    /// a hypothetical one.
    lang: LangTag,
}

impl Default for DocLexicon {
    fn default() -> Self {
        Self {
            plain: BTreeMap::new(),
            hyphenated: BTreeMap::new(),
            lower_compounds: 0,
            mid_sentence: 0,
            mid_sentence_capitals: 0,
            after_suspended: BTreeMap::new(),
            lang: LangTag::EN,
        }
    }
}

impl DocLexicon {
    /// Build a lexicon from the document's text.
    ///
    /// A token that *ends* with a hyphen is skipped entirely: it is a line break, not a word,
    /// and the pieces of it are not evidence about themselves.
    pub fn build<I, S>(texts: I, lang: &LangTag) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut lexicon = Self {
            lang: lang.clone(),
            ..Self::default()
        };
        for text in texts {
            let tokens: Vec<&str> = text.as_ref().split_whitespace().collect();
            for (position, token) in tokens.iter().enumerate() {
                let word: String = token
                    .chars()
                    .skip_while(|ch| !is_word_char(*ch))
                    .take_while(|ch| is_word_char(*ch))
                    .collect();
                if word.is_empty() {
                    continue;
                }
                let sentence_start = position == 0
                    || tokens[position - 1]
                        .chars()
                        .next_back()
                        .is_some_and(ends_a_sentence);
                if !sentence_start {
                    if let Some(first) = word.chars().next().filter(|ch| ch.is_alphabetic()) {
                        lexicon.mid_sentence += 1;
                        if first.is_uppercase() {
                            lexicon.mid_sentence_capitals += 1;
                        }
                    }
                }
                if word
                    .chars()
                    .next_back()
                    .is_some_and(|ch| LINE_BREAK_HYPHENS.contains(&ch))
                {
                    // A hyphen left hanging with more of the line after it is a suspended
                    // compound, and the word after it is what suspends one. At the end of a
                    // line it is a line break, which says nothing.
                    if let Some(next) = tokens.get(position + 1) {
                        let follower: String = next
                            .chars()
                            .skip_while(|ch| !is_word_char(*ch))
                            .take_while(|ch| ch.is_alphanumeric())
                            .collect();
                        if !follower.is_empty() {
                            *lexicon
                                .after_suspended
                                .entry(fold_key(&follower, lang.clone()))
                                .or_insert(0) += 1;
                        }
                    }
                    continue;
                }
                let key = fold_key(&word, lang.clone());
                let interior = word
                    .chars()
                    .skip(1)
                    .take(word.chars().count().saturating_sub(2))
                    .any(|ch| LINE_BREAK_HYPHENS.contains(&ch));
                if interior {
                    let after = word
                        .split(LINE_BREAK_HYPHENS)
                        .nth(1)
                        .and_then(|part| part.chars().next());
                    if after.is_some_and(char::is_lowercase) {
                        lexicon.lower_compounds += 1;
                    }
                }
                let table = if interior {
                    &mut lexicon.hyphenated
                } else {
                    &mut lexicon.plain
                };
                *table.entry(key).or_insert(0) += 1;
            }
        }
        lexicon
    }

    /// How often the joined form occurs in the document.
    pub fn joined_count(&self, head: &str, tail: &str) -> u32 {
        self.count(&super::joined(head, tail), false)
    }

    /// How often the hyphenated form occurs.
    pub fn hyphenated_count(&self, head: &str, tail: &str) -> u32 {
        self.count(&super::hyphenated(head, tail), true)
    }

    /// How often an unhyphenated word occurs in the document.
    ///
    /// The attestation predicate the German compound acceptor is given: "is this a word" is
    /// answered by the book, because D15's German frequency list does not exist yet.
    pub fn plain_count(&self, word: &str) -> u32 {
        self.count(word, false)
    }

    /// How many words of the document start with `stem` — the evidence a word the book
    /// inflects is a word of it, when the exact form at the break occurs nowhere else.
    ///
    /// An agglutinating language writes `uyandırırız` once and `uyandır`, `uyandırdı`,
    /// `uyandırmak` elsewhere; German and English do the same with compounds and suffixes.
    pub fn stem_count(&self, stem: &str) -> u32 {
        let key = fold_key(stem, self.lang.clone());
        if key.is_empty() {
            return 0;
        }
        self.plain
            .range(key.clone()..)
            .take_while(|(word, _)| word.starts_with(key.as_str()))
            .map(|(_, count)| *count)
            .sum()
    }

    /// How many words the document contributed, counting repeats.
    pub fn word_count(&self) -> u64 {
        self.plain
            .values()
            .map(|count| u64::from(*count))
            .sum::<u64>()
            + self
                .hyphenated
                .values()
                .map(|count| u64::from(*count))
                .sum::<u64>()
    }

    /// Hyphenated compounds with a lower-case second part, per word of the document: how
    /// readily this book writes a word with a hyphen in it.
    pub fn lower_compound_rate(&self) -> f64 {
        let words = self.word_count();
        if words == 0 {
            return 0.0;
        }
        f64::from(self.lower_compounds) / words as f64
    }

    /// The share of words met mid-sentence that start with a capital. A book that capitalises
    /// its nouns — German, whatever the book says its language is — is far above one that
    /// capitalises only its names, and that is what decides whether a capital after a
    /// line-break hyphen can still be a broken word.
    pub fn mid_sentence_capital_rate(&self) -> f64 {
        if self.mid_sentence == 0 {
            return 0.0;
        }
        f64::from(self.mid_sentence_capitals) / f64::from(self.mid_sentence)
    }

    /// Whether a word is one the document writes after a suspended hyphen.
    pub fn follows_suspended(&self, word: &str) -> bool {
        self.after_suspended
            .contains_key(&fold_key(word, self.lang.clone()))
    }

    /// Whether a word is among the `rank` most frequent in the document: a function word,
    /// whatever the language.
    pub fn is_frequent(&self, word: &str, rank: usize) -> bool {
        let key = fold_key(word, self.lang.clone());
        let Some(count) = self.plain.get(&key).copied() else {
            return false;
        };
        let above = self.plain.values().filter(|other| **other > count).count();
        above < rank
    }

    /// The locale this lexicon was folded with.
    pub fn lang(&self) -> &LangTag {
        &self.lang
    }

    /// How many distinct words the document contributed. Used by the report, and by tests
    /// that want to know the lexicon was actually built.
    pub fn len(&self) -> usize {
        self.plain.len() + self.hyphenated.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn count(&self, word: &str, hyphenated: bool) -> u32 {
        let key = fold_key(word, self.lang.clone());
        let table = if hyphenated {
            &self.hyphenated
        } else {
            &self.plain
        };
        table.get(&key).copied().unwrap_or(0)
    }
}

/// Whether a token ending in this character ends a sentence, so the next word may be capitalised
/// for that reason alone. Script-free: every script's sentence-final marks and closing quotes.
fn ends_a_sentence(ch: char) -> bool {
    matches!(
        ch,
        '.' | '!'
            | '?'
            | '\u{2026}'
            | ':'
            | '"'
            | '\u{201D}'
            | '\u{00BB}'
            | '\u{00AB}'
            | '\u{201C}'
            | '\u{201E}'
            | '\u{3002}'
            | '\u{FF01}'
            | '\u{FF1F}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_documents_own_words_answer_for_it() {
        let lexicon = DocLexicon::build(["the pipeline is long", "Pipeline again"], &LangTag::EN);
        assert_eq!(lexicon.joined_count("pipe", "line"), 2);
        assert_eq!(lexicon.hyphenated_count("pipe", "line"), 0);
    }

    #[test]
    fn a_hyphenated_word_is_held_whole() {
        let lexicon = DocLexicon::build(["die Nord-Süd-Achse führt"], &LangTag::DE);
        assert_eq!(lexicon.hyphenated_count("Nord", "Süd-Achse"), 1);
        assert_eq!(lexicon.joined_count("Nord", "Süd-Achse"), 0);
    }

    /// The trap this lexicon exists to avoid: the broken pieces are not evidence about
    /// themselves, so a token ending in a hyphen contributes nothing.
    #[test]
    fn a_line_break_is_not_a_word() {
        let lexicon = DocLexicon::build(["a broken pipe-", "line follows"], &LangTag::EN);
        assert_eq!(lexicon.joined_count("pipe", "line"), 0);
        assert_eq!(lexicon.hyphenated_count("pipe", "line"), 0);
    }

    #[test]
    fn punctuation_is_stripped_from_the_ends_of_words() {
        let lexicon = DocLexicon::build(["\"pipeline,\" he said."], &LangTag::EN);
        assert_eq!(lexicon.joined_count("pipe", "line"), 1);
    }

    /// Turkish folds its dotted and dotless i its own way, and the lexicon has to fold the
    /// same way the lookup does or a capitalised word answers for nothing.
    #[test]
    fn folding_is_the_documents_own() {
        let lexicon = DocLexicon::build(["ISPARTA ilinde"], &LangTag::TR);
        assert_eq!(lexicon.joined_count("ıspar", "ta"), 1);
    }
}
