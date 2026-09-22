//! Task 3, `book_structure`, version 1 (ARCHITECTURE §9.6, R10 §6.8, RT A8).
//!
//! The input is the flat heading list; the answer is three kinds of boundary, not one object per
//! heading. A reference work with 1 200 headings would otherwise cost ~10 K output tokens and make
//! an id-bijection failure near-certain (RT A8.4).

use serde::{Deserialize, Serialize};

use crate::gates::schema::Answer;
use crate::gates::GateFailure;
use crate::prompt::render::{self, RenderError};
use crate::prompt::Artifacts;
use crate::provider::{LlmRequest, Purpose};

pub const ARTIFACTS: Artifacts = Artifacts {
    purpose: Purpose::BookStructure,
    system: include_str!("../../../prompts/book_structure/v1/system.md"),
    user_template: include_str!("../../../prompts/book_structure/v1/user.tmpl"),
    grammar: include_str!("../../../prompts/book_structure/v1/grammar.gbnf"),
    schema: include_str!("../../../prompts/book_structure/v1/schema.json"),
};

/// One heading of the book, in reading order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HeadingEntry {
    /// Its position in this list: what the answer's indices count.
    pub idx: u32,
    pub text: String,
    /// The printed page it opens on.
    pub page: u32,
    /// Its style cluster.
    pub c: u32,
}

/// Everything the book-structure question is about.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BookStructureInput {
    /// The book's language, as a BCP-47 primary subtag.
    pub language: String,
    pub headings: Vec<HeadingEntry>,
}

/// The book-structure request for this heading list.
pub fn request(input: &BookStructureInput, max_tokens: u32) -> Result<LlmRequest, RenderError> {
    let payload = render::json(input)?;
    let user = render::fill(ARTIFACTS.user_template, &[("payload", &payload)])?;
    Ok(ARTIFACTS.request(user, max_tokens))
}

/// The answer, as gate S admits it: three kinds of boundary, by heading index.
///
/// Gate S checks the shape and nothing more. That the indices are strictly increasing, in range,
/// and put front matter first and back matter last is the task's own validation, applied when the
/// answer is applied (IMPLEMENTATION_PLAN Phase 10, test 10.9): a grammar cannot compare two
/// numbers, and neither does a shape.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BookStructureAnswer {
    pub frontmatter_end_idx: u32,
    pub part_boundaries: Vec<u32>,
    pub backmatter_start_idx: u32,
}

impl Answer for BookStructureAnswer {
    type Context = BookStructureInput;
    type Wire = BookStructureAnswer;

    fn check(wire: BookStructureAnswer, _input: &BookStructureInput) -> Result<Self, GateFailure> {
        Ok(wire)
    }
}
