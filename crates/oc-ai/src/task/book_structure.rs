//! Task 3, `book_structure`: boundaries, not one object per heading; chunks that must agree where
//! they overlap (PHASE 10 detail 4, ARCHITECTURE §9.6, RT A8.4).
//!
//! **The answer is three kinds of boundary**, by index into the flat heading list: the first
//! heading after the front matter, every heading that starts a part, and the first heading of the
//! back matter (the prompt's own definitions, `prompts/book_structure/v1/user.tmpl`). Its size does
//! not grow with the book, which is the point: a reference work with 1 200 headings asked for one
//! `{idx, role}` object each would cost ~10 K output tokens and make a bijection failure
//! near-certain (RT A8.4). Validation is then trivial — **strictly increasing**, front ≺ parts ≺
//! back, and within range — and any violation rejects the whole answer.
//!
//! **Above `llm.book_structure_chunk_headings` headings the list is chunked**, each chunk
//! overlapping the one before by `llm.book_structure_chunk_overlap`. Every chunk is its own
//! question with its own indices from zero, validated in its own range; two chunks that disagree
//! about any heading both saw reject the whole answer, because a model that reads the same heading
//! two ways has shown it was not reading the book.

use std::collections::BTreeMap;
use std::ops::Range;

use crate::gates::GateFailure;
use crate::prompt::v1::book_structure::{BookStructureAnswer, BookStructureInput, HeadingEntry};
use crate::session::{Asker, TaskResult};

/// The numbers the task reads, from `thresholds.toml`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BookStructureLimits {
    /// `llm.book_structure_chunk_headings`.
    pub chunk_headings: usize,
    /// `llm.book_structure_chunk_overlap`.
    pub chunk_overlap: usize,
}

/// Check one answer against a heading list of `n_headings`: every index in range, and the
/// sequence front, parts…, back strictly increasing.
///
/// Strict at both ends. The v1 prompt defines the front boundary as the first heading *after*
/// the front matter, so a book whose body opens with a part has a legitimate answer with
/// `frontmatter_end_idx == part_boundaries[0]` — which this refuses, and the deterministic
/// structure stands. That is the conservative reading of test 10.9 and ARCHITECTURE §9.6, and it
/// is recorded as such (`docs/DECISIONS_LOG.md`, 2026-09-23).
pub fn validate_structure(a: &BookStructureAnswer, n_headings: u32) -> Result<(), GateFailure> {
    let in_range = |field: &'static str, index: u32, bound: u32| {
        if index > bound {
            Err(GateFailure::OutOfRange {
                field,
                detail: format!("{index} is past {bound}"),
            })
        } else {
            Ok(())
        }
    };
    in_range("frontmatter_end_idx", a.frontmatter_end_idx, n_headings)?;
    in_range("backmatter_start_idx", a.backmatter_start_idx, n_headings)?;
    for part in &a.part_boundaries {
        // A part starts at a heading, so it is an index of one: below the count.
        in_range("part_boundaries", *part, n_headings.saturating_sub(1))?;
    }

    let sequence: Vec<u32> = std::iter::once(a.frontmatter_end_idx)
        .chain(a.part_boundaries.iter().copied())
        .chain(std::iter::once(a.backmatter_start_idx))
        .collect();
    if let Some(pair) = sequence.windows(2).find(|pair| pair[0] >= pair[1]) {
        return Err(GateFailure::NotOrdered(format!(
            "{} is not below {}",
            pair[0], pair[1]
        )));
    }
    Ok(())
}

/// Where a heading is in the book, as an answer says.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Zone {
    Front,
    Body,
    Back,
}

/// One heading's place: its zone, and whether it starts a part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZoneLabel {
    pub zone: Zone,
    pub part: bool,
}

/// The edit an admitted answer makes: a place for each heading it covered, by the heading's index
/// in the book's flat heading list. Headings it did not cover keep the deterministic answer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ZoneEdit {
    pub labels: BTreeMap<u32, ZoneLabel>,
}

/// The chunks a heading list of `n` is asked about in: one when it fits, else windows of
/// `chunk_headings` each overlapping the one before by `chunk_overlap`.
pub fn chunks(n: usize, limits: &BookStructureLimits) -> Vec<Range<usize>> {
    let size = limits.chunk_headings.max(1);
    // An overlap as wide as a chunk would never advance.
    let overlap = limits.chunk_overlap.min(size.saturating_sub(1));
    let mut out = Vec::new();
    let mut start = 0;
    loop {
        let end = n.min(start + size);
        out.push(start..end);
        if end >= n {
            return out;
        }
        start = end - overlap;
    }
}

/// The payload for one chunk: its headings, re-indexed from zero, as the prompt's indices count.
pub fn chunk_input(
    language: &str,
    headings: &[HeadingEntry],
    chunk: &Range<usize>,
) -> BookStructureInput {
    BookStructureInput {
        language: language.to_owned(),
        headings: headings
            .get(chunk.clone())
            .unwrap_or_default()
            .iter()
            .enumerate()
            .map(|(local, heading)| HeadingEntry {
                idx: u32::try_from(local).unwrap_or(u32::MAX),
                ..heading.clone()
            })
            .collect(),
    }
}

/// The place an admitted answer gives each heading of its chunk, by global index.
fn labels_of(answer: &BookStructureAnswer, chunk: &Range<usize>) -> BTreeMap<u32, ZoneLabel> {
    chunk
        .clone()
        .enumerate()
        .map(|(local, global)| {
            let local = u32::try_from(local).unwrap_or(u32::MAX);
            let zone = if local < answer.frontmatter_end_idx {
                Zone::Front
            } else if local >= answer.backmatter_start_idx {
                Zone::Back
            } else {
                Zone::Body
            };
            (
                u32::try_from(global).unwrap_or(u32::MAX),
                ZoneLabel {
                    zone,
                    part: answer.part_boundaries.contains(&local),
                },
            )
        })
        .collect()
}

/// Stitch chunk answers into one edit: every heading two chunks both saw must be placed alike,
/// and the stitched zones must still run front, body, back.
pub fn stitch(answers: &[(Range<usize>, BookStructureAnswer)]) -> Result<ZoneEdit, GateFailure> {
    let mut labels: BTreeMap<u32, ZoneLabel> = BTreeMap::new();
    for (chunk, answer) in answers {
        for (index, label) in labels_of(answer, chunk) {
            match labels.get(&index) {
                Some(seen) if *seen != label => {
                    return Err(GateFailure::OverlapDisagrees { index });
                }
                Some(_) => {}
                None => {
                    labels.insert(index, label);
                }
            }
        }
    }
    let zones: Vec<Zone> = labels.values().map(|label| label.zone).collect();
    if zones.windows(2).any(|pair| pair[1] < pair[0]) {
        return Err(GateFailure::NotOrdered(
            "the stitched zones do not run front, body, back".to_owned(),
        ));
    }
    Ok(ZoneEdit { labels })
}

/// Ask about the first `granted` chunks of the heading list, validate each in its own range,
/// and stitch what came back.
///
/// `granted` is what the call budget's degradation order left the task (ratified note N-4):
/// chunks beyond it are not asked, and the headings only they would have covered keep the
/// deterministic answer.
pub fn run(
    asker: &mut dyn Asker,
    language: &str,
    headings: &[HeadingEntry],
    granted: usize,
    limits: &BookStructureLimits,
    max_tokens: u32,
) -> TaskResult<ZoneEdit> {
    let mut answers = Vec::new();
    let mut trace = None;
    for chunk in chunks(headings.len(), limits).into_iter().take(granted) {
        let input = chunk_input(language, headings, &chunk);
        let request = match crate::prompt::v1::book_structure::request(&input, max_tokens) {
            Ok(request) => request,
            Err(error) => {
                return TaskResult::Unasked(crate::session::Unasked::Unavailable(
                    crate::provider::LlmError::Protocol(error.to_string()),
                ))
            }
        };
        let asked = match asker.ask(&request) {
            Ok(asked) => asked,
            Err(unasked) => return TaskResult::Unasked(unasked),
        };
        let n = u32::try_from(input.headings.len()).unwrap_or(u32::MAX);
        let verdict =
            crate::gates::schema::gate_response::<BookStructureAnswer>(&asked.response, &input)
                .and_then(|answer| validate_structure(&answer, n).map(|()| answer));
        match verdict {
            Ok(answer) => answers.push((chunk, answer)),
            Err(failure) => {
                return TaskResult::Rejected {
                    trace: asked.trace,
                    failure,
                }
            }
        }
        // The first chunk's trace stands for the task: it is the call every book makes.
        trace.get_or_insert(asked.trace);
    }
    let Some(trace) = trace else {
        return TaskResult::Unasked(crate::session::Unasked::Budget(
            oc_model::doc::Warning::new(
                crate::budget::W_LLM_BUDGET_EXHAUSTED,
                oc_model::doc::Severity::Warn,
            )
            .with_arg("task", crate::provider::Purpose::BookStructure.as_str())
            .with_arg("calls", "0"),
        ));
    };
    match stitch(&answers) {
        Ok(edit) => TaskResult::Admitted {
            trace,
            answer: edit,
        },
        Err(failure) => TaskResult::Rejected { trace, failure },
    }
}
