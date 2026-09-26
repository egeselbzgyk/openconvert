//! Task `front_page`: the kind of a page before the first chapter, when the deterministic rules
//! could not say (see [`crate::prompt::v1::front_page`]).
//!
//! The answer is a label and nothing else — a wrong one sets a page's `epub:type` and its styling
//! wrong, and cannot add, remove or move a character. In quality mode it is asked twice, with the
//! kinds in opposite orders, and taken only when both answers name the same kind.

use oc_model::decision::LlmTrace;

use crate::prompt::v1::front_page::{kinds, request, LETTERS};
use crate::session::{Asker, Unasked};

/// The kinds a page can be given: the words of `kinds.txt`, in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PageKind {
    Title,
    HalfTitle,
    Copyright,
    Dedication,
    Epigraph,
    Contents,
    Foreword,
    Preface,
    Introduction,
    Other,
    Body,
}

impl PageKind {
    pub const ALL: [PageKind; 11] = [
        PageKind::Title,
        PageKind::HalfTitle,
        PageKind::Copyright,
        PageKind::Dedication,
        PageKind::Epigraph,
        PageKind::Contents,
        PageKind::Foreword,
        PageKind::Preface,
        PageKind::Introduction,
        PageKind::Other,
        PageKind::Body,
    ];

    /// The word the prompt defines it by.
    pub fn word(self) -> &'static str {
        match self {
            PageKind::Title => "title",
            PageKind::HalfTitle => "halftitle",
            PageKind::Copyright => "copyright",
            PageKind::Dedication => "dedication",
            PageKind::Epigraph => "epigraph",
            PageKind::Contents => "contents",
            PageKind::Foreword => "foreword",
            PageKind::Preface => "preface",
            PageKind::Introduction => "introduction",
            PageKind::Other => "other",
            PageKind::Body => "body",
        }
    }

    fn from_word(word: &str) -> Option<PageKind> {
        PageKind::ALL.into_iter().find(|kind| kind.word() == word)
    }
}

/// The kind an answer names: its first letter, read against the order the options were listed
/// in. `None` for an answer that is no option's letter.
pub fn parse(answer: &str, order: &[&str]) -> Option<PageKind> {
    let letter = answer
        .trim_start_matches(|ch: char| ch.is_whitespace() || ch == '"' || ch == '*' || ch == '(')
        .chars()
        .next()?;
    let index = LETTERS.find(letter.to_ascii_uppercase())?;
    PageKind::from_word(order.get(index)?)
}

/// What asking about one page came to.
#[derive(Clone, Debug, PartialEq)]
pub struct PageAnswer {
    /// The kind the answers named; `None` when they disagreed or one named none.
    pub kind: Option<PageKind>,
    /// Every call made, in order.
    pub traces: Vec<LlmTrace>,
}

/// Ask about one page — once, or with `twice` a second time with the kinds in the opposite
/// order, keeping the kind only if both answers name it. `Err` when the session refused the
/// first question; a refusal of the second leaves the first answer unconfirmed, which is no
/// answer.
pub fn ask(
    asker: &mut dyn Asker,
    page: &str,
    twice: bool,
    max_tokens: u32,
) -> Result<PageAnswer, Unasked> {
    let mut traces = Vec::new();
    let mut kinds_named = Vec::new();
    let orders: &[bool] = if twice { &[false, true] } else { &[false] };
    for reversed in orders {
        let Ok((question, order)) = request(page, *reversed, max_tokens) else {
            return Ok(PageAnswer { kind: None, traces });
        };
        match asker.ask(&question) {
            Ok(asked) => {
                kinds_named.push(parse(&asked.response.text, &order));
                traces.push(asked.trace);
            }
            Err(why) if traces.is_empty() => return Err(why),
            Err(_) => return Ok(PageAnswer { kind: None, traces }),
        }
    }
    let first = kinds_named.first().copied().flatten();
    let kind = first.filter(|kind| kinds_named.iter().all(|named| *named == Some(*kind)));
    Ok(PageAnswer { kind, traces })
}

/// The kinds `kinds.txt` defines, in its order: the list [`PageKind`] must be.
pub fn defined_words() -> Vec<&'static str> {
    kinds().into_iter().map(|(word, _)| word).collect()
}
