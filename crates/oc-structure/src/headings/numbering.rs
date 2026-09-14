//! Heading numbering, in the three target languages (PIPELINE §8.2's table).
//!
//! Numbering does two things. It *refines* a level that size rank guessed — `1.2.3` is a
//! level-three heading whatever size it is set at — and it is a confidence signal in its own
//! right: a book whose headings all match a numbering pattern has told you its scheme, and
//! the escalation predicate for `heading_roles` fires only when coverage is below 100 %.
//!
//! Turkish is matched under Turkish-locale folding and never under invariant lowercasing:
//! `BÖLÜM` lowercases to `bölüm` only if `I` is known to pair with `ı` (R10 §6.3).

use oc_model::lang::LangTag;
use oc_text::fold::fold_key;
use serde::Serialize;

/// What a numbering pattern says a heading is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NumberingKind {
    /// `Part One`, `Teil II`, `Kısım 2` — above a chapter.
    Part,
    /// `Chapter 3`, `Kapitel 3`, `Bölüm 3`.
    Chapter,
    /// `Appendix A`, `Anhang A`, `Ek A` — back matter, at chapter level.
    Appendix,
    /// `1`, `1.2`, `1.2.3` — the level is the number of components.
    Decimal,
    /// A bare roman numeral: a front-matter division or a part title.
    Roman,
}

/// What a heading's numbering said.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Numbering {
    pub kind: NumberingKind,
    /// The number as printed: `"3"`, `"II"`, `"1.2.3"`, `"A"`.
    pub value: String,
    /// The level the pattern implies, 1-based.
    pub level: u8,
}

/// The keyword sets, folded, per language. Ordered so the matcher is deterministic.
///
/// All three languages are tried for every document rather than only the detected one. A
/// German book quoting an English chapter title still has a chapter there, and the cost of
/// the extra comparisons is three string equalities per heading.
const PART_WORDS: [&str; 4] = ["part", "teil", "kisim", "kısım"];
const CHAPTER_WORDS: [&str; 5] = ["chapter", "kapitel", "abschnitt", "bolum", "bölüm"];
const APPENDIX_WORDS: [&str; 3] = ["appendix", "anhang", "ek"];

/// Read a heading's numbering, if it has one.
///
/// Returns the kind, the number as printed, and the level the pattern implies. `None` when
/// nothing matched, which is the common case: most novels number nothing.
pub fn read(text: &str, lang: &LangTag) -> Option<Numbering> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Keyword forms first: `Chapter 3` is a chapter even though `3` is also a decimal.
    if let Some(found) = keyword(trimmed, lang) {
        return Some(found);
    }

    let first = trimmed.split_whitespace().next()?;
    let head = first.trim_end_matches(['.', ')', ':', '\u{2014}', '\u{2013}']);

    if let Some(level) = decimal_level(head) {
        return Some(Numbering {
            kind: NumberingKind::Decimal,
            value: head.to_owned(),
            level,
        });
    }
    if is_roman(head) {
        return Some(Numbering {
            kind: NumberingKind::Roman,
            value: head.to_owned(),
            // A bare roman numeral stands for a part or a front-matter division: both sit at
            // the top of the tree, and nothing in the numeral itself says which.
            level: 1,
        });
    }
    None
}

/// `Chapter 3`, `Anhang A`, `Bölüm II` — a keyword, then its number.
fn keyword(text: &str, lang: &LangTag) -> Option<Numbering> {
    let mut words = text.split_whitespace();
    let first = words.next()?;
    let folded = fold_key(first.trim_end_matches([':', '.']), lang.clone());
    let kind = if PART_WORDS.contains(&folded.as_str()) {
        NumberingKind::Part
    } else if CHAPTER_WORDS.contains(&folded.as_str()) {
        NumberingKind::Chapter
    } else if APPENDIX_WORDS.contains(&folded.as_str()) {
        NumberingKind::Appendix
    } else {
        return None;
    };

    let value = words
        .next()
        .map(|word| {
            word.trim_end_matches(['.', ':', ',', '\u{2014}', '\u{2013}'])
                .to_owned()
        })
        .filter(|word| !word.is_empty())?;
    // A keyword on its own — a section literally titled "Appendix" — still carries the
    // keyword's level, but the *number* has to be a number, a letter or a numeral. "Chapter
    // of Accidents" is not chapter "of".
    if !(value.chars().all(|c| c.is_ascii_digit())
        || is_roman(&value)
        || (value.chars().count() == 1 && value.chars().all(char::is_alphabetic))
        || is_number_word(&value, lang))
    {
        return None;
    }

    Some(Numbering {
        kind,
        value,
        level: match kind {
            // A part is above a chapter, and both are above everything else; XHTML has six
            // levels and a book that has parts spends one of them here.
            NumberingKind::Part => 1,
            _ => 1,
        },
    })
}

/// The spelled-out numbers a chapter title uses: `Chapter One`, `Teil Zwei`, `Bölüm Bir`.
fn is_number_word(value: &str, lang: &LangTag) -> bool {
    const WORDS: [&str; 30] = [
        "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eins",
        "zwei", "drei", "vier", "funf", "fünf", "sechs", "sieben", "acht", "neun", "zehn", "bir",
        "iki", "uc", "üç", "dort", "dört", "bes", "beş", "alti",
    ];
    WORDS.contains(&fold_key(value, lang.clone()).as_str())
}

/// `1`, `1.2`, `1.2.3` — the level is the number of components, capped at the six XHTML has.
fn decimal_level(head: &str) -> Option<u8> {
    let parts: Vec<&str> = head.split('.').filter(|part| !part.is_empty()).collect();
    if parts.is_empty() || !parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit())) {
        return None;
    }
    // A bare number is only a numbering if the heading text shows the dotted form somewhere,
    // or is a single component — `1` opening a heading is a section number far more often
    // than it is a year, and the `>= 2 siblings` guard against `1984 was…` is the *list*
    // rule's (PIPELINE §8.5), not this one's: a heading candidate is already short, set
    // large, and sitting under white space.
    Some(u8::try_from(parts.len()).unwrap_or(1).clamp(1, 6))
}

/// A roman numeral, upper or lower case, of reasonable length.
pub fn is_roman(text: &str) -> bool {
    !text.is_empty()
        && text.chars().count() <= 8
        && text.chars().all(|c| {
            matches!(
                c.to_ascii_lowercase(),
                'i' | 'v' | 'x' | 'l' | 'c' | 'd' | 'm'
            )
        })
}

/// The value of a roman numeral, for the page-label arithmetic book structure does.
pub fn roman_value(text: &str) -> Option<u32> {
    if !is_roman(text) {
        return None;
    }
    let digit = |c: char| -> i64 {
        match c.to_ascii_lowercase() {
            'i' => 1,
            'v' => 5,
            'x' => 10,
            'l' => 50,
            'c' => 100,
            'd' => 500,
            'm' => 1000,
            _ => 0,
        }
    };
    let digits: Vec<i64> = text.chars().map(digit).collect();
    // Signed, and summed before any clamping. `iv` subtracts one *before* adding five, and a
    // running unsigned total saturates that first step at zero and reads it as five.
    let total: i64 = digits
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let next = digits.get(index + 1).copied().unwrap_or_default();
            if *value < next {
                -value
            } else {
                *value
            }
        })
        .sum();
    u32::try_from(total).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keywords_are_read_in_all_three_languages() {
        for (text, lang) in [
            ("Chapter 3", LangTag::EN),
            ("Kapitel 3", LangTag::DE),
            ("Bölüm 3", LangTag::TR),
        ] {
            let found = read(text, &lang).unwrap_or_else(|| panic!("{text} has numbering"));
            assert_eq!(found.kind, NumberingKind::Chapter, "{text}");
            assert_eq!(found.value, "3", "{text}");
            assert_eq!(found.level, 1, "{text}");
        }
        assert_eq!(
            read("Appendix A", &LangTag::EN).map(|n| n.kind),
            Some(NumberingKind::Appendix)
        );
        assert_eq!(
            read("Ek B", &LangTag::TR).map(|n| n.kind),
            Some(NumberingKind::Appendix)
        );
        assert_eq!(
            read("Part One", &LangTag::EN).map(|n| n.kind),
            Some(NumberingKind::Part)
        );
    }

    /// Turkish `BÖLÜM` folds to `bolum` only under Turkish-locale rules; invariant
    /// lowercasing of a dotted capital I is the classic way to lose it (R10 §6.3).
    #[test]
    fn turkish_keywords_fold_under_turkish_rules() {
        assert_eq!(
            read("BÖLÜM 4", &LangTag::TR).map(|n| n.kind),
            Some(NumberingKind::Chapter)
        );
        assert_eq!(
            read("KISIM II", &LangTag::TR).map(|n| n.kind),
            Some(NumberingKind::Part)
        );
    }

    #[test]
    fn a_dotted_number_gives_its_own_depth() {
        assert_eq!(
            read("1 Introduction", &LangTag::EN).map(|n| n.level),
            Some(1)
        );
        assert_eq!(read("1.2 Method", &LangTag::EN).map(|n| n.level), Some(2));
        assert_eq!(read("1.2.3 Detail", &LangTag::EN).map(|n| n.level), Some(3));
        assert_eq!(
            read("1.2.3 Detail", &LangTag::EN).map(|n| n.value),
            Some("1.2.3".to_owned())
        );
    }

    /// The failure this guard exists for: a keyword followed by a word is not a number.
    #[test]
    fn a_keyword_without_a_number_is_not_numbering() {
        assert_eq!(read("Chapter of Accidents", &LangTag::EN), None);
        assert_eq!(read("Parting Words", &LangTag::EN), None);
        assert_eq!(read("An Ordinary Title", &LangTag::EN), None);
    }

    #[test]
    fn roman_numerals_are_read_and_valued() {
        assert_eq!(
            read("IV", &LangTag::EN).map(|n| n.kind),
            Some(NumberingKind::Roman)
        );
        assert_eq!(roman_value("iv"), Some(4));
        assert_eq!(roman_value("XIV"), Some(14));
        assert_eq!(roman_value("mcmxcix"), Some(1999));
        assert_eq!(roman_value("A"), None);
    }
}
