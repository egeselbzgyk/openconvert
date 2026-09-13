//! German compounds, without a dependency (PIPELINE §7 tier T4, plan Phase 3 detail 5, V2 §4).
//!
//! German is the language where dehyphenation goes wrong in both directions at once. It
//! compounds without limit — `Donaudampfschifffahrtsgesellschaft` is a word — so a broken
//! compound that is not rejoined leaves a visible scar in the middle of a noun. And it
//! hyphenates real compounds — `Nord-Süd-Achse`, `E-Mail-Adresse`, `Goethe-Institut` — so a
//! hyphen that is rejoined welds two words into one that no dictionary will ever contain.
//!
//! Two rules, and the first one costs nothing:
//!
//! 1. **An upper-case continuation means the hyphen is real.** German capitalises nouns at
//!    their first letter and nowhere else, so a word broken across a line always continues in
//!    lower case: `Fahr-` / `zeug`, never `Fahr-` / `Zeug`. A capital after the break is
//!    therefore a hyphen the author wrote. This is not a heuristic about frequencies; it is
//!    the orthography, and it needs no lexicon at all.
//! 2. **The compound acceptor**, for everything else. Try each internal split point of the
//!    joined form and accept it when both parts are attested words, allowing the *Fugenlaute*
//!    — the linking morphemes `-s-`, `-es-`, `-n-`, `-en-`, `-er-` that German inserts at a
//!    compound seam (`Bundesbahn` is `Bund` + `-es-` + `Bahn`). V2 §4 endorses this as the
//!    dependency-free fallback; CharSplit, the usual tool, is Python-only.
//!
//! What counts as "attested" is the caller's to supply, and today that is the document's own
//! vocabulary: D15's German frequency list does not exist yet, because the sources the plan
//! named turned out to be CC-BY-SA. The acceptor is written against a predicate rather than
//! against a list so that the list, when it ships, is one argument rather than a rewrite.

/// The linking morphemes German inserts at a compound seam, longest first so that `-es-` is
/// tried before `-s-` and `Bundes|bahn` is not read as `Bunde|s|bahn`.
const FUGENLAUTE: [&str; 5] = ["es", "en", "er", "s", "n"];

/// The shortest a part of a compound may be.
///
/// Three characters: `Bau`, `Weg`, `Uhr`, `Tor` are all German words and all three letters
/// long. Below that the acceptor starts finding compounds in every word it is shown — `ab`,
/// `an`, `in` are all attested, so a two-character floor turns `Anbau` into two words and
/// every seam into a match.
const MIN_PART_CHARS: usize = 3;

/// How a compound was parsed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Split {
    pub left: String,
    /// The linking morpheme, if the seam carried one.
    pub fugen: Option<String>,
    pub right: String,
}

/// Rule 1: does an upper-case continuation mean this hyphen belongs to the word?
///
/// Both sides have to look like nouns — a capital either side of the hyphen — because that is
/// the shape of a real German compound hyphen. `Nord-Süd-Achse` qualifies; a lower-case
/// continuation does not, and goes to the acceptor.
pub fn hyphen_is_real(head: &str, tail: &str) -> bool {
    let capital = |word: &str| {
        word.chars()
            .next()
            .is_some_and(|ch| ch.is_uppercase() || !ch.is_alphabetic())
    };
    capital(head) && tail.chars().next().is_some_and(char::is_uppercase)
}

/// Rule 2: does this word parse as a compound of two attested parts?
///
/// `attested` answers "is this a word" — in v1 that is the document's own vocabulary. The
/// first split that works is returned, scanning from the left, so the parse is deterministic
/// and the longest linking morpheme wins at each seam.
pub fn split_compound(word: &str, attested: impl Fn(&str) -> bool) -> Option<Split> {
    let chars: Vec<char> = word.chars().collect();
    if chars.len() < MIN_PART_CHARS * 2 {
        return None;
    }

    for seam in MIN_PART_CHARS..=chars.len().saturating_sub(MIN_PART_CHARS) {
        let left: String = chars[..seam].iter().collect();
        if !attested(&left) {
            continue;
        }
        let rest: String = chars[seam..].iter().collect();
        if rest.chars().count() >= MIN_PART_CHARS && attested(&rest) {
            return Some(Split {
                left,
                fugen: None,
                right: rest,
            });
        }
        for fugen in FUGENLAUTE {
            let Some(right) = rest.strip_prefix(fugen) else {
                continue;
            };
            if right.chars().count() >= MIN_PART_CHARS && attested(right) {
                return Some(Split {
                    left,
                    fugen: Some(fugen.to_owned()),
                    right: right.to_owned(),
                });
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(list: &[&str]) -> impl Fn(&str) -> bool + use<> {
        let owned: Vec<String> = list.iter().map(|word| word.to_lowercase()).collect();
        move |word: &str| owned.contains(&word.to_lowercase())
    }

    /// The rule that decides the fixture, and it needs no lexicon: German never breaks a word
    /// before a capital, so a capital after the hyphen means the author wrote the hyphen.
    #[test]
    fn an_uppercase_continuation_is_a_real_hyphen() {
        assert!(hyphen_is_real("Nord", "Süd-Achse"));
        assert!(hyphen_is_real("E", "Mail-Adresse"));
        assert!(hyphen_is_real("Goethe", "Institut"));
    }

    /// And a lower-case one is a broken word, which is the ordinary case.
    #[test]
    fn a_lowercase_continuation_is_a_broken_word() {
        assert!(!hyphen_is_real("Fahr", "zeug"));
        assert!(!hyphen_is_real("Eisen", "bahn"));
    }

    #[test]
    fn a_compound_of_two_attested_words_is_accepted() {
        let split = split_compound("Eisenbahn", words(&["eisen", "bahn"]))
            .expect("Eisen + bahn is a compound");
        assert_eq!(split.left, "Eisen");
        assert_eq!(split.right, "bahn");
        assert_eq!(split.fugen, None);
    }

    /// The Fugenlaut: `Bundesbahn` is `Bund` + `-es-` + `Bahn`, and an acceptor that does not
    /// know that finds no compound at all.
    #[test]
    fn a_linking_morpheme_at_the_seam_is_allowed() {
        let split = split_compound("Bundesbahn", words(&["bund", "bahn"]))
            .expect("Bund + es + bahn is a compound");
        assert_eq!(split.left, "Bund");
        assert_eq!(split.fugen.as_deref(), Some("es"));
        assert_eq!(split.right, "bahn");
    }

    /// The longest linking morpheme wins, so `Bundes|bahn` is not read as `Bunde|s|bahn`.
    #[test]
    fn the_longest_linking_morpheme_wins() {
        let split =
            split_compound("Bundesbahn", words(&["bund", "bunde", "bahn"])).expect("a compound");
        assert_eq!(split.left, "Bund");
        assert_eq!(split.fugen.as_deref(), Some("es"));
    }

    #[test]
    fn a_word_with_no_attested_parts_is_not_a_compound() {
        assert_eq!(split_compound("Zarquonblivet", words(&["bahn"])), None);
    }

    /// The floor on part length. `an` and `bau` are both German words, and without a floor
    /// every word with a two-letter prefix becomes a compound.
    #[test]
    fn a_two_letter_part_is_not_a_compound_seam() {
        assert_eq!(split_compound("Anbau", words(&["an", "bau"])), None);
    }
}
