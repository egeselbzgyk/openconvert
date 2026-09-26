//! Task `front_page`, version 1: what kind of page one of the pages before the first chapter is.
//!
//! Asked in quality mode only, and in free text: the model reasons in a few sentences and ends
//! on `ANSWER: <kind>`. A smoke test against the bundled model (2026-09-26) put constrained
//! one-word answers at chance on these pages — a dedication called a title page, body pages
//! called contents — while a reasoned answer named the title page, copyright page, dedication
//! and contents correctly. So there is no grammar and no schema: the answer's shape is checked
//! by [`crate::task::front_page::parse`], and a reasoned answer is only taken when two of them,
//! asked with the kinds in opposite orders, agree.

use crate::prompt::render::{self, RenderError};
use crate::prompt::Artifacts;
use crate::provider::{LlmRequest, Purpose};

pub const ARTIFACTS: Artifacts = Artifacts {
    purpose: Purpose::FrontPage,
    system: include_str!("../../../prompts/front_page/v1/system.md"),
    user_template: include_str!("../../../prompts/front_page/v1/user.tmpl"),
    // Free text: the model reasons before it answers (`LlmRequest::is_free_text`).
    grammar: "",
    schema: "",
};

/// The kinds and what each means, one `word: definition` per line.
pub const KINDS: &str = include_str!("../../../prompts/front_page/v1/kinds.txt");

/// What the model is told to do with the page.
pub const INSTRUCTION: &str = include_str!("../../../prompts/front_page/v1/instruction.txt");

/// The question about one page. `reversed` lists the kinds last to first: a model that picks by
/// position rather than by meaning answers the two orders differently, and is not taken.
pub fn request(
    page: &str,
    number: u32,
    reversed: bool,
    max_tokens: u32,
) -> Result<LlmRequest, RenderError> {
    let mut kinds: Vec<&str> = KINDS
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    if reversed {
        kinds.reverse();
    }
    let user = render::fill(
        ARTIFACTS.user_template,
        &[
            ("kinds", &kinds.join("\n")),
            ("number", &number.to_string()),
            ("page", page),
            ("instruction", INSTRUCTION.trim()),
        ],
    )?;
    Ok(ARTIFACTS.request(user, max_tokens))
}
