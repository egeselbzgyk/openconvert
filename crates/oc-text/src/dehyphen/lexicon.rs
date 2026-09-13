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
            for token in text.as_ref().split_whitespace() {
                let word: String = token
                    .chars()
                    .skip_while(|ch| !is_word_char(*ch))
                    .take_while(|ch| is_word_char(*ch))
                    .collect();
                if word.is_empty() {
                    continue;
                }
                if word
                    .chars()
                    .next_back()
                    .is_some_and(|ch| LINE_BREAK_HYPHENS.contains(&ch))
                {
                    continue;
                }
                let key = fold_key(&word, lang.clone());
                let interior = word
                    .chars()
                    .skip(1)
                    .take(word.chars().count().saturating_sub(2))
                    .any(|ch| LINE_BREAK_HYPHENS.contains(&ch));
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
