//! The deterministic tiers, in the order PIPELINE §7 gives them.

use oc_model::confidence::Signal;
use oc_model::lang::LangTag;

use crate::compound_de;
use crate::freq;

use super::{joined, Decision, DocLexicon, HyphenAction, Tier};

/// **T2 — candidacy.** Is this line break even a broken word?
///
/// The general rule is that the next line starts lower-case: `pipe-` / `line` is a broken
/// word, `Nord-` / `Süd-Achse` is not, because nobody breaks a word before a capital. German
/// is the exception that proves it — every noun is capitalised, so `Fahr-` / `Zeug` is a
/// perfectly ordinary broken `Fahrzeug` — so for German an upper-case continuation stays a
/// candidate and the later tiers decide it on evidence instead.
///
/// Either way a digit on either side rules it out: `2019-` / `2020` is a range, and nothing
/// in this module should ever be joining numbers.
pub fn is_candidate(head: &str, tail: &str, capitalises_nouns: bool) -> bool {
    let Some(first) = tail.chars().next() else {
        return false;
    };
    if head.chars().any(|ch| ch.is_numeric()) || tail.chars().any(|ch| ch.is_numeric()) {
        return false;
    }
    if first.is_lowercase() {
        return true;
    }
    capitalises_nouns
}

/// **T3 — the in-document lexicon.** The book's own vocabulary, which is stronger evidence
/// than any external dictionary and costs nothing (PIPELINE §7).
///
/// One refusal worth naming: when *both* forms occur in the document, the tier abstains
/// rather than taking the larger count. A book that discusses both `pipeline` and
/// `pipe-line` has told us nothing about this break, and a three-to-two majority is not
/// evidence.
pub fn in_document(head: &str, tail: &str, doc: &DocLexicon) -> Option<Decision> {
    let joined_count = doc.joined_count(head, tail);
    let hyphenated_count = doc.hyphenated_count(head, tail);
    let signals = vec![
        Signal::new("indoc_joined", joined_count as f32),
        Signal::new("indoc_hyphenated", hyphenated_count as f32),
    ];
    match (joined_count, hyphenated_count) {
        (0, 0) => None,
        (joined, hyphenated) if joined > 0 && hyphenated > 0 => None,
        (joined, _) if joined > 0 => {
            Some(Decision::new(HyphenAction::Join, Tier::InDocument, signals))
        }
        _ => Some(Decision::new(HyphenAction::Keep, Tier::InDocument, signals)),
    }
}

/// **The suspended compound.** `Ein-` / `und Ausgang` is not a broken word: the hyphen is
/// left hanging on purpose and the word after it is a conjunction. Which words those are is
/// read from the book — the words it writes after a hyphen hanging mid-line, and the handful
/// it uses most, which in every language are its function words. Joining `Einund` would be a
/// silent corruption; keeping the hyphen, at worst, a visible one.
pub fn suspended(
    head: &str,
    tail: &str,
    doc: &DocLexicon,
    frequent_rank: usize,
) -> Option<Decision> {
    let _ = head;
    let follows = doc.follows_suspended(tail);
    let frequent = doc.is_frequent(tail, frequent_rank);
    if !(follows || frequent) {
        return None;
    }
    Some(Decision::new(
        HyphenAction::Keep,
        Tier::InDocument,
        vec![
            Signal::new("follows_suspended", f32::from(u8::from(follows))),
            Signal::new("frequent_tail", f32::from(u8::from(frequent))),
        ],
    ))
}

/// **The book's own stems.** When the joined form is nowhere else in the book but a word
/// starting with the head and the first letters of the tail is, the break is inside a word the
/// book uses: the stem is attested, and only the ending is new. The inflecting and
/// compounding languages' case, found without knowing which language it is.
pub fn stem(
    head: &str,
    tail: &str,
    doc: &DocLexicon,
    min_stem: usize,
    tail_chars: usize,
) -> Option<Decision> {
    // A book that spells this very word both ways has said nothing about the break (T3
    // abstained), and a stem search would only find the joined spelling again.
    if contested(head, tail, doc) {
        return None;
    }
    let lead: String = tail.chars().take(tail_chars).collect();
    let stem = format!("{head}{lead}");
    if stem.chars().count() < min_stem
        || lead.chars().count() < tail_chars.min(tail.chars().count())
    {
        return None;
    }
    let count = doc.stem_count(&stem);
    (count > 0).then(|| {
        Decision::new(
            HyphenAction::Join,
            Tier::InDocument,
            vec![Signal::new("indoc_stem", count as f32)],
        )
    })
}

/// **The book's own hyphenation style.** A book that writes almost no compound with a
/// lower-case second part — no `well-known`, no `self-evident` — does not have one broken at
/// the end of a line either: a line-final hyphen before a lower-case word is the typesetter's.
/// Measured on the book, so it is a property of the book rather than of its language; English
/// prose writes such compounds often enough that the rule stays out of its way.
pub fn book_style(
    head: &str,
    tail: &str,
    doc: &DocLexicon,
    max_rate: f64,
    min_words: u64,
) -> Option<Decision> {
    // A style is a property of a book, and a handful of words is not one.
    if contested(head, tail, doc) || doc.word_count() < min_words {
        return None;
    }
    let rate = doc.lower_compound_rate();
    (rate <= max_rate).then(|| {
        Decision::new(
            HyphenAction::Join,
            Tier::InDocument,
            vec![Signal::new("lower_compound_rate", rate as f32)],
        )
    })
}

/// Whether the book spells this word both ways, which is the case T3 declines.
fn contested(head: &str, tail: &str, doc: &DocLexicon) -> bool {
    doc.joined_count(head, tail) > 0 || doc.hyphenated_count(head, tail) > 0
}

/// **T4, German first.** The orthography and the compound acceptor (`compound_de`).
///
/// Before the frequency list, because it answers a question the list cannot: German has no
/// shipped list yet (D15), and even with one the capital after the hyphen is decisive on its
/// own. What the acceptor needs is somewhere to look words up, and in v1 that is the
/// document's own vocabulary — which is why the lexicon is passed here and not only to T3.
pub fn german(head: &str, tail: &str, doc: &DocLexicon) -> Option<Decision> {
    if compound_de::hyphen_is_real(head, tail) {
        return Some(Decision::new(
            HyphenAction::Keep,
            Tier::Lexicon,
            vec![Signal::new("de_capital_after_hyphen", 1.0)],
        ));
    }
    let split = compound_de::split_compound(&joined(head, tail), |word| doc.plain_count(word) > 0)?;
    Some(Decision::new(
        HyphenAction::Join,
        Tier::Lexicon,
        vec![
            Signal::new("de_compound_parts", 2.0),
            Signal::new(
                "de_compound_fugen",
                f32::from(u8::from(split.fugen.is_some())),
            ),
        ],
    ))
}

/// **T4 — the language frequency list.**
///
/// Two questions, and they are not symmetrical. If the joined form is a word of the language,
/// that is evidence to join. If *both halves* are independently attested words, that is
/// evidence to keep — `Goethe-Institut`, `E-Mail-Adresse`, `Nord-Süd-Achse` — and it is the
/// rule that saves German compounds from being welded together.
///
/// Where the list has nothing to say, so does this tier. `freq::contains` returns `None` for a
/// language with no shipped list, and `None` is not `false`: Turkish and German have no list
/// yet (D15), so for those two this tier abstains entirely rather than reporting that every
/// word is unknown.
pub fn lexicon(head: &str, tail: &str, lang: &LangTag) -> Option<Decision> {
    let whole = freq::contains(&joined(head, tail), lang)?;
    let left = freq::contains(head, lang)?;
    let right = freq::contains(tail, lang)?;
    let signals = vec![
        Signal::new("lex_joined", f32::from(u8::from(whole))),
        Signal::new("lex_head", f32::from(u8::from(left))),
        Signal::new("lex_tail", f32::from(u8::from(right))),
    ];

    // Both halves attested and the join is not a word: two words with a hyphen between them.
    if left && right && !whole {
        return Some(Decision::new(HyphenAction::Keep, Tier::Lexicon, signals));
    }
    if whole && !(left && right) {
        return Some(Decision::new(HyphenAction::Join, Tier::Lexicon, signals));
    }
    None
}
