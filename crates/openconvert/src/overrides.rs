//! Applying the user's corrections: the last step of `document` (PIPELINE §9 step 7).
//!
//! v1 corrects what users reliably fix and nothing else (D16): the metadata — title, authors,
//! language — and the table of contents — a heading's wording and its level. Everything else in
//! the book is the pipeline's, and a correction that names a heading this book does not have is
//! reported, not guessed at.
//!
//! **What the conservation law sees.** Metadata is outside `C` (ARCHITECTURE §5.2), so a new
//! title costs the ledger nothing. A heading's text is inside it: a rename removes the printed
//! words and adds the user's, and both are ledgered with reason `UserOverride` under
//! [`oc_core::stages::DOCUMENT_CORRECTED`]. Every applied correction is also a [`Decision`] with
//! `method = User` whose alternative is what the pipeline had chosen, so the report shows what was
//! overruled, by whom, and from what.

use oc_core::stages;
use oc_model::decision::Decision;
use oc_model::doc::{MetaSource, Section, SectionRole, Severity, Span, Warning};
use oc_model::document::Document;
use oc_model::ids::BlockId;
use oc_model::lang::LangTag;
use oc_model::ledger::{LedgerDelta, LedgerEntry, Reason};
use oc_model::overrides::{Overrides, OverridesError};

/// The corrections were made for another IR version and were not applied.
pub const W_OVERRIDES_STALE: &str = "W_OVERRIDES_STALE";
/// The corrections were made for another PDF and were not applied.
pub const W_OVERRIDES_OTHER_SOURCE: &str = "W_OVERRIDES_OTHER_SOURCE";
/// The corrections use block-level changes, which v1 does not apply (D16).
pub const W_OVERRIDES_BLOCKS: &str = "W_OVERRIDES_BLOCKS";
/// The corrections file could not be read.
pub const W_OVERRIDES_UNREADABLE: &str = "W_OVERRIDES_UNREADABLE";
/// Some TOC corrections name a heading this book does not have.
pub const W_OVERRIDES_UNMATCHED: &str = "W_OVERRIDES_UNMATCHED";

/// Decision kinds, as the report prints them.
pub const DECISION_TITLE: &str = "metadata_title";
pub const DECISION_AUTHORS: &str = "metadata_authors";
pub const DECISION_LANGUAGE: &str = "metadata_language";
pub const DECISION_HEADING_TEXT: &str = "toc_heading_text";
pub const DECISION_HEADING_LEVEL: &str = "toc_heading_level";

/// Read the corrections the job named, for the book whose digest is `source_sha256`.
///
/// A file this engine will not apply is not an error: the book is converted as the pipeline sees
/// it and the report says, by name, why the corrections were left out (ARCHITECTURE §4.6, "the
/// engine refuses stale `overrides.json` with a named warning"). Not silently ignored, and not a
/// reason to withhold the book either.
pub fn load(text: Option<&str>, source_sha256: &str) -> (Option<Overrides>, Vec<Warning>) {
    let Some(text) = text else {
        return (None, Vec::new());
    };
    match Overrides::parse(text, source_sha256) {
        Ok(overrides) => (Some(overrides), Vec::new()),
        Err(refused) => (None, vec![refusal(&refused)]),
    }
}

/// The warning that names why a file was refused.
pub fn refusal(refused: &OverridesError) -> Warning {
    match refused {
        OverridesError::StaleIrVersion { file, engine } => {
            Warning::new(W_OVERRIDES_STALE, Severity::Warn)
                .with_arg("file_ir", file.to_string())
                .with_arg("engine_ir", engine.to_string())
        }
        OverridesError::OtherSource { .. } => {
            Warning::new(W_OVERRIDES_OTHER_SOURCE, Severity::Warn)
        }
        OverridesError::BlocksReserved => Warning::new(W_OVERRIDES_BLOCKS, Severity::Warn),
        OverridesError::Unreadable(_) => Warning::new(W_OVERRIDES_UNREADABLE, Severity::Warn),
    }
}

/// What applying the corrections did.
#[derive(Debug, Default)]
pub struct Applied {
    /// The `UserOverride` entries: each renamed heading's printed words out, the user's in.
    pub delta: LedgerDelta,
    pub decisions: Vec<Decision>,
    pub warnings: Vec<Warning>,
}

/// Apply `overrides` to the assembled `document`.
pub fn apply(document: &mut Document, overrides: &Overrides) -> Applied {
    let mut applied = Applied::default();
    if let Some(metadata) = &overrides.metadata {
        apply_metadata(document, metadata, &mut applied);
    }
    if let Some(toc) = &overrides.toc {
        apply_toc(document, toc, &mut applied);
    }
    applied
}

fn apply_metadata(
    document: &mut Document,
    patch: &oc_model::overrides::MetadataPatch,
    applied: &mut Applied,
) {
    let stage = stages::DOCUMENT.name;
    if let Some(title) = patch.title.as_deref().map(str::trim) {
        let before = document.meta.title.clone().unwrap_or_default();
        if !title.is_empty() && title != before {
            applied
                .decisions
                .push(Decision::user(stage, DECISION_TITLE, title).against(vec![before]));
            document.meta.title = Some(title.to_owned());
            // The title's provenance is the one D13.5 insists be kept apart from a guess.
            document.meta.source = MetaSource::User;
        }
    }
    if let Some(authors) = &patch.authors {
        let authors: Vec<String> = authors
            .iter()
            .map(|author| author.trim().to_owned())
            .filter(|author| !author.is_empty())
            .collect();
        if authors != document.meta.authors {
            applied.decisions.push(
                Decision::user(stage, DECISION_AUTHORS, authors.join("; "))
                    .against(vec![document.meta.authors.join("; ")]),
            );
            document.meta.authors = authors;
        }
    }
    if let Some(tag) = patch.language.as_deref().filter(|tag| is_language_tag(tag)) {
        let language = LangTag::new(tag);
        if language != document.language {
            applied.decisions.push(
                Decision::user(stage, DECISION_LANGUAGE, language.as_str())
                    .against(vec![document.language.as_str().to_owned()]),
            );
            document.meta.language = language.clone();
            document.language = language;
        }
    }
}

/// A BCP-47-shaped tag: subtags of ASCII letters and digits joined by `-`, the first letters only.
///
/// Shape only, not a registry lookup: the tag becomes `dc:language` and `xml:lang`, and the thing
/// to prevent is markup, not an unusual language.
fn is_language_tag(tag: &str) -> bool {
    let mut subtags = tag.split('-');
    let first_is_letters = subtags
        .next()
        .is_some_and(|first| !first.is_empty() && first.chars().all(|c| c.is_ascii_alphabetic()));
    first_is_letters
        && subtags
            .all(|subtag| !subtag.is_empty() && subtag.chars().all(|c| c.is_ascii_alphanumeric()))
}

fn apply_toc(
    document: &mut Document,
    patches: &[oc_model::overrides::TocPatch],
    applied: &mut Applied,
) {
    let stage = stages::DOCUMENT.name;
    let mut unmatched = 0_usize;
    let mut releveled: Vec<BlockId> = Vec::new();
    for patch in patches {
        let Some(section) = find_heading(&mut document.sections, patch.heading) else {
            unmatched += 1;
            continue;
        };
        let page = section.source_pages.0;
        let Some(heading) = section.heading.as_mut() else {
            unmatched += 1;
            continue;
        };

        if let Some(title) = patch.title.as_deref().map(str::trim) {
            // A note marker in a heading is the book's, not the heading's wording: it stays, after
            // the new words, and only the words are ledgered as replaced.
            let (markers, words): (Vec<Span>, Vec<Span>) = heading
                .spans
                .drain(..)
                .partition(|span| span.noteref.is_some());
            let printed: String = words.iter().map(|span| span.text.as_str()).collect();
            if title.is_empty() || title == printed {
                heading.spans = words.into_iter().chain(markers).collect();
            } else {
                applied.delta.push(LedgerEntry::removed(
                    stage,
                    Reason::UserOverride,
                    page,
                    (0, char_count(&printed)),
                    printed.clone(),
                ));
                applied.delta.push(LedgerEntry::added(
                    stage,
                    Reason::UserOverride,
                    page,
                    (0, char_count(title)),
                    title.to_owned(),
                ));
                applied.decisions.push(
                    Decision::user(stage, DECISION_HEADING_TEXT, title)
                        .about(patch.heading)
                        .against(vec![printed]),
                );
                heading.spans = std::iter::once(Span::plain(title)).chain(markers).collect();
            }
        }

        if let Some(level) = patch.level.map(oc_model::doc::Heading::clamp_level) {
            if level != heading.level {
                applied.decisions.push(
                    Decision::user(stage, DECISION_HEADING_LEVEL, level.to_string())
                        .about(patch.heading)
                        .against(vec![heading.level.to_string()]),
                );
                heading.level = level;
                section.level = level;
                releveled.push(patch.heading);
            }
        }
    }

    if !releveled.is_empty() {
        let flat = flatten(std::mem::take(&mut document.sections));
        document.sections = nest(flat, &releveled);
    }
    if unmatched > 0 {
        applied.warnings.push(
            Warning::new(W_OVERRIDES_UNMATCHED, Severity::Warn)
                .with_arg("count", unmatched.to_string()),
        );
    }
}

fn char_count(text: &str) -> u32 {
    u32::try_from(text.chars().count()).unwrap_or(u32::MAX)
}

/// The section a heading id opens, anywhere in the tree.
fn find_heading(sections: &mut [Section], id: BlockId) -> Option<&mut Section> {
    for section in sections {
        if section
            .heading
            .as_ref()
            .is_some_and(|heading| heading.id == id)
        {
            return Some(section);
        }
        if let Some(found) = find_heading(&mut section.children, id) {
            return Some(found);
        }
    }
    None
}

/// The tree in document order, each section without its children.
fn flatten(sections: Vec<Section>) -> Vec<Section> {
    let mut out = Vec::new();
    for mut section in sections {
        let children = std::mem::take(&mut section.children);
        out.push(section);
        out.extend(flatten(children));
    }
    out
}

/// Nest sections in document order by level: each goes under the nearest earlier section of a
/// lower level. Reading order is untouched — only which heading a section sits under changes, which
/// is what a level is.
///
/// A section the user moved gets the role its new place implies (IR: `Section` is "anything below
/// the chapter level"): a chapter moved under another chapter becomes a section of it, and a section
/// moved to the top becomes a chapter. Front and back matter keep their roles.
fn nest(flat: Vec<Section>, moved: &[BlockId]) -> Vec<Section> {
    let mut roots: Vec<Section> = Vec::new();
    let mut open: Vec<Section> = Vec::new();
    for section in flat {
        while open.last().is_some_and(|top| top.level >= section.level) {
            if let Some(done) = open.pop() {
                close(done, &mut open, &mut roots, moved);
            }
        }
        open.push(section);
    }
    while let Some(done) = open.pop() {
        close(done, &mut open, &mut roots, moved);
    }
    roots
}

fn close(mut done: Section, open: &mut [Section], roots: &mut Vec<Section>, moved: &[BlockId]) {
    let was_moved = done
        .heading
        .as_ref()
        .is_some_and(|heading| moved.contains(&heading.id));
    match open.last_mut() {
        Some(parent) => {
            let under_chapter = matches!(parent.role, SectionRole::Chapter | SectionRole::Section);
            if was_moved && under_chapter && done.role == SectionRole::Chapter {
                done.role = SectionRole::Section;
            }
            parent.children.push(done);
        }
        None => {
            if was_moved && done.role == SectionRole::Section {
                done.role = SectionRole::Chapter;
            }
            roots.push(done);
        }
    }
}
