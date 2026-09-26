//! Task `front_page`, version 1: what kind of page one of the pages before the first chapter is.
//!
//! Asked as a **decision**: the page is the state, the kinds are lettered options, and the answer
//! is one letter — the interface of the System-1 decision models (Tev1-4B, the bundled default)
//! and one any instruction-following model can answer. A hand-labelled set of 31 opening pages
//! in three languages (2026-09-26) put Tev1-4B at 30 of 31 this way, with the page cut to its
//! first `llm.front_page_max_chars` characters; the chat model it replaced was at 11 with
//! one-word answers.

use serde::Serialize;

use crate::prompt::render::{self, RenderError};
use crate::prompt::Artifacts;
use crate::provider::{LlmRequest, Purpose};

pub const ARTIFACTS: Artifacts = Artifacts {
    purpose: Purpose::FrontPage,
    system: include_str!("../../../prompts/front_page/v1/system.md"),
    user_template: include_str!("../../../prompts/front_page/v1/user.tmpl"),
    grammar: include_str!("../../../prompts/front_page/v1/grammar.gbnf"),
    schema: include_str!("../../../prompts/front_page/v1/schema.json"),
};

/// The kinds and what each means, one `word: definition` per line.
pub const KINDS: &str = include_str!("../../../prompts/front_page/v1/kinds.txt");

/// The question the decision asks.
pub const INSTRUCTION: &str = include_str!("../../../prompts/front_page/v1/instruction.txt");

/// The letters options are labelled by, in order.
pub const LETTERS: &str = "ABCDEFGHIJK";

#[derive(Serialize)]
struct Option_<'a> {
    label: String,
    key: &'a str,
    description: &'a str,
}

#[derive(Serialize)]
struct Payload<'a> {
    state: &'a str,
    question: &'a str,
    options: Vec<Option_<'a>>,
}

/// The kinds as `(word, definition)`, in `kinds.txt`'s order.
pub fn kinds() -> Vec<(&'static str, &'static str)> {
    KINDS
        .lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(word, definition)| (word.trim(), definition.trim()))
        .filter(|(word, _)| !word.is_empty())
        .collect()
}

/// The decision about one page, and the kind each letter stands for in it. `reversed` lists
/// the kinds last to first: a model that picks by position rather than by meaning answers the
/// two orders differently.
pub fn request(
    page: &str,
    reversed: bool,
    max_tokens: u32,
) -> Result<(LlmRequest, Vec<&'static str>), RenderError> {
    let mut kinds = kinds();
    if reversed {
        kinds.reverse();
    }
    let options: Vec<Option_<'_>> = kinds
        .iter()
        .zip(LETTERS.chars())
        .map(|((word, definition), letter)| Option_ {
            label: letter.to_string(),
            key: word,
            description: definition,
        })
        .collect();
    let payload = Payload {
        state: page,
        question: INSTRUCTION.trim(),
        options,
    };
    let payload =
        serde_json::to_string(&payload).map_err(|error| RenderError::Json(error.to_string()))?;
    let user = render::fill(ARTIFACTS.user_template, &[("payload", &payload)])?;
    let order = kinds.into_iter().map(|(word, _)| word).collect();
    Ok((ARTIFACTS.request(user, max_tokens), order))
}
