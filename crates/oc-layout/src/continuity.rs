//! Cross-page continuity: does the last line of page *n* continue into the first line of
//! page *n+1*? (PIPELINE §6 step 4, R10 §6.5.)
//!
//! R10 calls this "the highest-value deterministic signal in the whole pipeline, and it costs
//! nothing", and the reason is that it is a *check on the column hypothesis by its
//! consequences*. Column detection can be wrong in a way that no amount of looking at the
//! page will reveal — a table, a hanging-indent bibliography and a two-column spread all put a
//! tall empty band down the middle — but a wrong hypothesis reorders the page, and a reordered
//! page stops flowing into the next one. So the page order is tested by whether the book still
//! reads like a book.
//!
//! The proxy is deliberately cheap: the page-final line ends without terminal punctuation
//! **and** the page-initial line starts lower-case. Neither half is conclusive on its own and
//! together they are not proof; what makes them usable is that they are applied to a *rate*
//! over the whole document and compared between hypotheses, never to one boundary in
//! isolation.

use unicode_properties::{GeneralCategory, UnicodeGeneralCategory};

/// Sentence-final punctuation, in the scripts v1 claims. A page that ends on one of these has
/// finished its sentence, so the next page starting a new one is no evidence either way.
const TERMINAL: [char; 8] = ['.', '!', '?', '…', '。', '！', '？', '؟'];

/// How the document read under one column hypothesis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Continuity {
    /// Page boundaries examined: one fewer than the number of pages with text.
    pub boundaries: u32,
    /// Boundaries where the text ran on.
    pub held: u32,
}

impl Continuity {
    /// The share of boundaries where the text ran on. A document with no boundary to examine
    /// is perfectly continuous, because nothing has been shown otherwise — the caller's
    /// comparison between hypotheses is what makes that safe.
    pub fn rate(&self) -> f32 {
        if self.boundaries == 0 {
            return 1.0;
        }
        self.held as f32 / self.boundaries as f32
    }

    /// The share of boundaries where it did not.
    pub fn break_rate(&self) -> f32 {
        1.0 - self.rate()
    }
}

/// Measure continuity over a document, given each page's text in reading order.
pub fn continuity(pages: &[Vec<String>]) -> Continuity {
    let flowing: Vec<&Vec<String>> = pages
        .iter()
        .filter(|page| page.iter().any(|text| !text.trim().is_empty()))
        .collect();

    let mut measured = Continuity::default();
    for pair in flowing.windows(2) {
        let Some(last) = pair[0].iter().rev().find(|text| !text.trim().is_empty()) else {
            continue;
        };
        let Some(first) = pair[1].iter().find(|text| !text.trim().is_empty()) else {
            continue;
        };
        measured.boundaries += 1;
        if runs_on(last, first) {
            measured.held += 1;
        }
    }
    measured
}

/// Whether one page's last text runs on into the next page's first.
pub fn runs_on(last: &str, first: &str) -> bool {
    // A sentence ends inside its closing quote or bracket as well: `…the end.”` has finished.
    let unfinished = last
        .trim_end()
        .trim_end_matches(CLOSERS)
        .chars()
        .next_back()
        .is_some_and(|ch| !TERMINAL.contains(&ch));
    // Lower case continues a sentence; so does a letter of a script that has no case at all —
    // Han, Kana, Arabic, Hebrew, Devanagari — where the test cannot be put the other way.
    let continues = first.trim_start().chars().next().is_some_and(|ch| {
        ch.general_category() == GeneralCategory::LowercaseLetter
            || ch.general_category() == GeneralCategory::OtherLetter
    });
    unfinished && continues
}

/// The closing quotes and brackets a sentence may end inside of.
const CLOSERS: [char; 10] = [
    '"', '\'', '\u{201D}', '\u{2019}', '\u{00BB}', '\u{00AB}', ')', ']', '}', '\u{300D}',
];

#[cfg(test)]
mod tests {
    use super::*;

    fn page(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|line| (*line).to_owned()).collect()
    }

    #[test]
    fn a_sentence_that_crosses_a_page_boundary_is_continuous() {
        let pages = vec![
            page(&["the first page ends in the middle of a"]),
            page(&["sentence, and the second picks it up."]),
        ];
        assert_eq!(continuity(&pages).rate(), 1.0);
    }

    #[test]
    fn a_page_that_ends_its_sentence_is_not_evidence_of_continuity() {
        let pages = vec![
            page(&["the first page ends here."]),
            page(&["A new sentence begins."]),
        ];
        let measured = continuity(&pages);
        assert_eq!(measured.boundaries, 1);
        assert_eq!(measured.held, 0);
    }

    /// Both halves are needed: a page ending mid-sentence followed by a capital is a chapter
    /// break, not a run-on.
    #[test]
    fn an_uppercase_start_breaks_continuity_even_without_punctuation() {
        let pages = vec![
            page(&["the first page ends without a full stop"]),
            page(&["Chapter Two"]),
        ];
        assert_eq!(continuity(&pages).held, 0);
    }

    #[test]
    fn a_single_page_has_no_boundary_to_measure() {
        let measured = continuity(&[page(&["alone."])]);
        assert_eq!(measured.boundaries, 0);
        assert_eq!(measured.rate(), 1.0);
    }

    /// An empty page contributes nothing rather than breaking the chain either way.
    #[test]
    fn a_blank_page_is_skipped_not_counted_as_a_break() {
        let pages = vec![
            page(&["the first page ends in the middle of a"]),
            page(&["   "]),
            page(&["sentence, and the third picks it up."]),
        ];
        let measured = continuity(&pages);
        assert_eq!(measured.boundaries, 1);
        assert_eq!(measured.held, 1);
    }
}

#[cfg(test)]
mod script_tests {
    use super::*;

    /// A sentence that ends inside its closing quote has ended.
    #[test]
    fn a_sentence_closed_inside_a_quote_is_finished() {
        assert!(!runs_on(
            "and then she said, \u{201C}Go.\u{201D}",
            "and he went"
        ));
        assert!(!runs_on("(as he had said.)", "and then"));
        assert!(runs_on("and then she said, \u{201C}Go", "on, go\u{201D}"));
    }

    /// A caseless script continues a sentence with an ordinary letter.
    #[test]
    fn a_caseless_script_can_run_on() {
        assert!(runs_on(
            "\u{3053}\u{308C}\u{306F}",
            "\u{65E5}\u{672C}\u{8A9E}"
        ));
        assert!(!runs_on("\u{3053}\u{308C}\u{3002}", "\u{65E5}\u{672C}"));
    }
}
