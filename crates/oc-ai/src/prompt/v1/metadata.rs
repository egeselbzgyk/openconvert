//! Task 1, `metadata`, version 1 (ARCHITECTURE §9.6, R10 §6.16).
//!
//! The input is the text of pages 1–3, one line per printed line, each tagged with what its
//! typography says — `[LARGE]`, `[MEDIUM]`, `[SMALL]`, `[CENTERED]` — and never with a number.
//! D13.6 is emphatic that geometry reaches a model pre-digested into categorical words: a 7B model
//! reading coordinates as text scores 34.3 % (LayTextLLM). A body-size line carries no size tag.

use std::fmt;

use serde::de::{self, Deserializer, Visitor};
use serde::Deserialize;

use crate::gates::schema::Answer;
use crate::gates::GateFailure;
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

/// The answer, as gate S admits it: every field named, each one text or absent.
///
/// Whether the text is *true* — a verbatim substring of pages 1–3 — is the task's own validation,
/// applied after this gate (ARCHITECTURE §9.6). An empty string is refused here because it would
/// pass that check vacuously: "" is a substring of every page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataAnswer {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub authors: Vec<String>,
    pub translator: Option<String>,
    pub publisher: Option<String>,
    pub date: Option<String>,
}

/// The answer as JSON spells it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataWire {
    title: Field,
    subtitle: Field,
    authors: Vec<String>,
    translator: Field,
    publisher: Field,
    date: Field,
}

impl Answer for MetadataAnswer {
    type Context = MetadataInput;
    type Wire = MetadataWire;

    fn check(wire: MetadataWire, _pages: &MetadataInput) -> Result<Self, GateFailure> {
        let fields = [
            &wire.title.0,
            &wire.subtitle.0,
            &wire.translator.0,
            &wire.publisher.0,
            &wire.date.0,
        ];
        if fields.iter().any(|field| field.as_deref() == Some(""))
            || wire.authors.iter().any(String::is_empty)
        {
            return Err(GateFailure::WrongShape(
                "an empty string where an absent field is null".to_owned(),
            ));
        }
        Ok(MetadataAnswer {
            title: wire.title.0,
            subtitle: wire.subtitle.0,
            authors: wire.authors,
            translator: wire.translator.0,
            publisher: wire.publisher.0,
            date: wire.date.0,
        })
    }
}

/// A field that must be present, as a string or as `null`.
///
/// Not `Option<String>`: serde reads a *missing* `Option` field as `None` without saying so, and
/// the grammar requires all six. Deserialising through `deserialize_any` is what makes a missing
/// field an error — serde's missing-field deserializer answers `deserialize_option` with `None` and
/// `deserialize_any` with the error.
struct Field(Option<String>);

impl<'de> Deserialize<'de> for Field {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(FieldVisitor)
    }
}

struct FieldVisitor;

impl<'de> Visitor<'de> for FieldVisitor {
    type Value = Field;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a string or null")
    }

    fn visit_unit<E: de::Error>(self) -> Result<Field, E> {
        Ok(Field(None))
    }

    fn visit_none<E: de::Error>(self) -> Result<Field, E> {
        Ok(Field(None))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Field, E> {
        Ok(Field(Some(value.to_owned())))
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Field, E> {
        Ok(Field(Some(value)))
    }
}
