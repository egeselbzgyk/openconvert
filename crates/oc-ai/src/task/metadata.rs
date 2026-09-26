//! Task 1, `metadata`: the verbatim-substring check, and the metadata an admitted answer makes
//! (PHASE 10 detail 2, ARCHITECTURE §9.6, R10 §6.16).
//!
//! **The strongest and cheapest anti-hallucination check in the pipeline.** Every non-null field
//! of the answer must be a substring of the text of pages 1–3, case- and whitespace-normalised: a
//! fabricated author cannot be a substring of a title page it was not on. Any field failing
//! rejects the whole response — a model that invented a publisher has shown it was not copying,
//! and its title stops being evidence too.
//!
//! The pages compared against are the lines' **text**, without the `[LARGE]`/`[CENTERED]` tags the
//! model was shown beside it: a tag is an annotation of ours, not something printed in the book,
//! and a "title" that copied one is not a title.

use oc_model::doc::{MetaSource, Metadata};

use crate::gates::GateFailure;
use crate::prompt::v1::metadata::{MetadataAnswer, MetadataInput};
use crate::task::normalise;

/// The bounds the check reads, from `thresholds.toml`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MetadataLimits {
    /// `metadata.llm_title_max_chars`: ARCHITECTURE §9.6's "title length 1–200 chars".
    pub title_max_chars: usize,
    /// `metadata.llm_author_min_words`: the fewest words a name the model calls an author has.
    pub author_min_words: usize,
}

impl MetadataInput {
    /// The pages' text as printed, one line per line, without the tags: what every field of an
    /// answer must be a substring of.
    pub fn verbatim_text(&self) -> String {
        self.lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Check an answer gate S admitted against the pages it was read from.
pub fn validate_metadata(
    answer: &MetadataAnswer,
    pages13: &str,
    limits: &MetadataLimits,
) -> Result<(), GateFailure> {
    let pages = normalise(pages13);
    let fields: [(&'static str, Option<&str>); 5] = [
        ("title", answer.title.as_deref()),
        ("subtitle", answer.subtitle.as_deref()),
        ("translator", answer.translator.as_deref()),
        ("publisher", answer.publisher.as_deref()),
        ("date", answer.date.as_deref()),
    ];
    let authors = answer
        .authors
        .iter()
        .map(|author| ("authors", Some(author.as_str())));

    for (field, value) in fields.into_iter().chain(authors) {
        let Some(value) = value else {
            continue;
        };
        let wanted = normalise(value);
        // An empty field after normalisation is whitespace, and "" is a substring of every page.
        if wanted.is_empty() || !pages.contains(&wanted) {
            return Err(GateFailure::NotVerbatim { field });
        }
    }

    if let Some(title) = &answer.title {
        let length = title.trim().chars().count();
        if length == 0 || length > limits.title_max_chars {
            return Err(GateFailure::OutOfRange {
                field: "title",
                detail: format!("{length} characters, of at most {}", limits.title_max_chars),
            });
        }
    }
    Ok(())
}

/// The fields of an admitted answer that are also plausible: a title the pages print larger
/// than their body text, authors that are names of `author_min_words` words or more with no
/// digits in them. The rest is dropped, not refused — the deterministic value stands for it.
///
/// Verbatim is not enough (2026-09-26, a smoke test against the bundled model): a line of a
/// contents page copied exactly is still no title, and a series editor's surname copied exactly
/// is still no author.
pub fn plausible(
    mut answer: MetadataAnswer,
    input: &MetadataInput,
    limits: &MetadataLimits,
) -> MetadataAnswer {
    if let Some(title) = &answer.title {
        let wanted = normalise(title);
        let printed_large = input.lines.iter().any(|line| {
            matches!(
                line.size,
                Some(crate::prompt::v1::metadata::Size::Large)
                    | Some(crate::prompt::v1::metadata::Size::Medium)
            ) && {
                // The title printed on this line, or this line one of a title set over several.
                let printed = normalise(&line.text);
                printed.contains(&wanted)
                    || (printed.chars().count() >= TITLE_PART_MIN_CHARS
                        && wanted.contains(&printed))
            }
        });
        if !printed_large {
            answer.title = None;
            answer.subtitle = None;
        }
    }
    answer.authors.retain(|author| {
        author.split_whitespace().count() >= limits.author_min_words
            && !author.chars().any(char::is_numeric)
    });
    answer
}

/// A large line shorter than this is a numeral or an initial, found inside any title by chance.
const TITLE_PART_MIN_CHARS: usize = 3;

/// The metadata an admitted answer makes of the deterministic metadata.
///
/// A field the model returned replaces the deterministic one; a field it left null keeps it, and
/// an empty author list keeps the deterministic authors. The identifier and the language are never
/// the model's. `source` becomes `Llm` when anything was taken from the answer, so a title a model
/// read and a title the file declared are never indistinguishable (D13.5).
pub fn apply_metadata(answer: &MetadataAnswer, deterministic: &Metadata) -> Metadata {
    let mut out = deterministic.clone();
    let mut taken = false;
    for (slot, value) in [
        (&mut out.title, &answer.title),
        (&mut out.subtitle, &answer.subtitle),
        (&mut out.translator, &answer.translator),
        (&mut out.publisher, &answer.publisher),
        (&mut out.date, &answer.date),
    ] {
        if let Some(value) = value {
            *slot = Some(value.trim().to_owned());
            taken = true;
        }
    }
    if !answer.authors.is_empty() {
        out.authors = answer
            .authors
            .iter()
            .map(|author| author.trim().to_owned())
            .collect();
        taken = true;
    }
    if taken {
        out.source = MetaSource::Llm;
    }
    out
}
