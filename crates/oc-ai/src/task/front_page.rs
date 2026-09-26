//! Task `front_page`: the kind of a page before the first chapter, when the deterministic rules
//! could not say (quality mode only; see [`crate::prompt::v1::front_page`]).
//!
//! The answer is a label and nothing else — a wrong one sets a page's `epub:type` and its styling
//! wrong, and cannot add, remove or move a character. It is asked twice, with the kinds in
//! opposite orders, and taken only when both reasoned answers name the same kind.

use oc_model::decision::LlmTrace;

use crate::prompt::v1::front_page::{request, KINDS};
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

/// The kind a reasoned answer ends on: the word after its last `ANSWER:`, whatever case and
/// emphasis the model wrapped it in. `None` for an answer that names no kind of the list.
pub fn parse(answer: &str) -> Option<PageKind> {
    const MARK: &str = "answer:";
    let lower = answer.to_lowercase();
    let at = lower.rfind(MARK)?;
    let word: String = lower[at + MARK.len()..]
        .trim_start_matches(|ch: char| ch.is_whitespace() || ch == '*' || ch == '`' || ch == '"')
        .chars()
        .take_while(|ch| ch.is_alphabetic() || *ch == '-')
        .filter(|ch| *ch != '-')
        .collect();
    PageKind::from_word(&word)
}

/// What asking about one page came to.
#[derive(Clone, Debug, PartialEq)]
pub struct PageAnswer {
    /// The kind both answers named; `None` when they disagreed or either named none.
    pub kind: Option<PageKind>,
    /// Every call made, in order.
    pub traces: Vec<LlmTrace>,
}

/// Ask about one page: twice, the kinds in opposite orders, and keep the kind only if both
/// answers name it. `Err` when the session refused the first question; a refusal of the second
/// leaves the first answer unconfirmed, which is no answer.
pub fn ask(
    asker: &mut dyn Asker,
    page: &str,
    number: u32,
    max_tokens: u32,
) -> Result<PageAnswer, Unasked> {
    let mut traces = Vec::new();
    let mut kinds = Vec::new();
    for reversed in [false, true] {
        let Ok(question) = request(page, number, reversed, max_tokens) else {
            return Ok(PageAnswer { kind: None, traces });
        };
        match asker.ask(&question) {
            Ok(asked) => {
                kinds.push(parse(&asked.response.text));
                traces.push(asked.trace);
            }
            Err(why) if traces.is_empty() => return Err(why),
            Err(_) => return Ok(PageAnswer { kind: None, traces }),
        }
    }
    let kind = match kinds.as_slice() {
        [Some(first), Some(second)] if first == second => Some(*first),
        _ => None,
    };
    Ok(PageAnswer { kind, traces })
}

/// The kinds `kinds.txt` defines, in its order: the list [`PageKind`] must be.
pub fn defined_words() -> Vec<&'static str> {
    KINDS
        .lines()
        .filter_map(|line| line.split_once(':').map(|(word, _)| word.trim()))
        .filter(|word| !word.is_empty())
        .collect()
}
