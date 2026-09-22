//! Task 1, `metadata`, version 1 (ARCHITECTURE §9.6, R10 §6.16).
//!
//! The input is the text of pages 1–3, one line per printed line, each tagged with what its
//! typography says — `[LARGE]`, `[MEDIUM]`, `[SMALL]`, `[CENTERED]` — and never with a number.
//! D13.6 is emphatic that geometry reaches a model pre-digested into categorical words: a 7B model
//! reading coordinates as text scores 34.3 % (LayTextLLM). A body-size line carries no size tag.

use crate::prompt::render::{self, RenderError};
use crate::prompt::Artifacts;
use crate::provider::{LlmRequest, Purpose};

pub const ARTIFACTS: Artifacts = Artifacts {
    purpose: Purpose::Metadata,
    system: include_str!("../../../prompts/metadata/v1/system.md"),
    user_template: include_str!("../../../prompts/metadata/v1/user.tmpl"),
    grammar: include_str!("../../../prompts/metadata/v1/grammar.gbnf"),
    schema: include_str!("../../../prompts/metadata/v1/schema.json"),
};

/// A line's size, relative to the body text: the category, not the points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    Large,
    Medium,
    Small,
}

impl Size {
    fn tag(self) -> &'static str {
        match self {
            Size::Large => "[LARGE]",
            Size::Medium => "[MEDIUM]",
            Size::Small => "[SMALL]",
        }
    }
}

/// One printed line of pages 1–3.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyledLine {
    /// `None` for a line at body size.
    pub size: Option<Size>,
    pub centered: bool,
    pub text: String,
}

impl StyledLine {
    /// The line as the model reads it: its tags, a space, its text.
    ///
    /// A line is one line, so a line break inside its text becomes a space — otherwise the text
    /// after it would read as a new, untagged line. The verbatim check the answer faces is
    /// whitespace-normalised (ARCHITECTURE §9.6), so nothing a model copies is lost to this.
    fn render(&self) -> String {
        let mut out = String::new();
        if let Some(size) = self.size {
            out.push_str(size.tag());
        }
        if self.centered {
            out.push_str("[CENTERED]");
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.extend(
            self.text
                .chars()
                .map(|c| if c == '\n' || c == '\r' { ' ' } else { c }),
        );
        out
    }
}

/// Everything the metadata question is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataInput {
    pub lines: Vec<StyledLine>,
}

/// The metadata request for these pages.
pub fn request(input: &MetadataInput, max_tokens: u32) -> Result<LlmRequest, RenderError> {
    let pages = input
        .lines
        .iter()
        .map(StyledLine::render)
        .collect::<Vec<_>>()
        .join("\n");
    let user = render::fill(ARTIFACTS.user_template, &[("pages", &pages)])?;
    Ok(ARTIFACTS.request(user, max_tokens))
}
