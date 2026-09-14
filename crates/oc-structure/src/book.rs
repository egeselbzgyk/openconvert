//! Book structure: front matter, body and back matter, and the tree of parts, chapters and
//! sections inside them (PIPELINE §8, IMPLEMENTATION_PLAN Phase 4 detail 5).
//!
//! Three independent sources are voted (detail 5): the outline, the printed contents page,
//! and the heading clusters with the page breaks. By the time the flow reaches this function
//! the first two have already decided the *levels* — that is `headings::levels`' job — so what
//! is left here is the part none of them settles: which of the book's three **zones** each
//! section is in, and what kind of section it is.
//!
//! Two signals answer that, and they are of very different quality.
//!
//! **Roman folios with an arabic-1 reset is a hard boundary.** A book that restarts its
//! numbering has told the reader where its body begins, and it is the strongest single
//! statement about structure a printed book makes.
//!
//! **Back-matter keywords are a weak one**, and they are matched in all three target
//! languages because a German book's `Anhang` is as much an appendix as an English book's.
//! A keyword *after* the body has begun moves the zone; one before it does not, because
//! `Notes on the Text` is a perfectly ordinary chapter title.
//!
//! The validation is the same list PIPELINE §8 gives: chapters contiguous and
//! non-overlapping, page numbers monotone, and front ≺ body ≺ back.

use oc_core::thresholds::Thresholds;
use oc_model::confidence::{Confidence, Signal};
use oc_model::doc::{
    BackMatterKind, Content, FrontMatterKind, Section, SectionRole, Severity, Warning, Zone,
};
use oc_model::ids::BlockId;
use oc_model::lang::LangTag;
use oc_text::fold::fold_key;

use crate::headings::numbering::{is_roman, roman_value, NumberingKind};

/// The book's zones do not run front, body, back (PIPELINE §8's validation).
pub const W_ZONES_OUT_OF_ORDER: &str = "W_ZONES_OUT_OF_ORDER";
/// Two sections claim overlapping pages, or the pages run backwards.
pub const W_SECTION_PAGES_NOT_MONOTONE: &str = "W_SECTION_PAGES_NOT_MONOTONE";

/// One item of the document's flow, with the page it was printed on.
#[derive(Clone, Debug, PartialEq)]
pub struct FlowItem {
    pub page: u32,
    pub content: Content,
}

/// The front-matter keywords, folded, in EN / DE / TR.
const FRONT_WORDS: [(&str, FrontMatterKind); 16] = [
    ("preface", FrontMatterKind::Preface),
    ("vorwort", FrontMatterKind::Preface),
    ("onsoz", FrontMatterKind::Preface),
    ("önsöz", FrontMatterKind::Preface),
    ("foreword", FrontMatterKind::Foreword),
    ("geleitwort", FrontMatterKind::Foreword),
    ("introduction", FrontMatterKind::Introduction),
    ("einleitung", FrontMatterKind::Introduction),
    ("giris", FrontMatterKind::Introduction),
    ("giriş", FrontMatterKind::Introduction),
    ("contents", FrontMatterKind::TableOfContents),
    ("inhalt", FrontMatterKind::TableOfContents),
    ("icindekiler", FrontMatterKind::TableOfContents),
    ("içindekiler", FrontMatterKind::TableOfContents),
    ("dedication", FrontMatterKind::Dedication),
    ("widmung", FrontMatterKind::Dedication),
];

/// The back-matter keywords, folded, in EN / DE / TR (detail 5's list).
const BACK_WORDS: [(&str, BackMatterKind); 18] = [
    ("appendix", BackMatterKind::Appendix),
    ("anhang", BackMatterKind::Appendix),
    ("ek", BackMatterKind::Appendix),
    ("notes", BackMatterKind::Notes),
    ("anmerkungen", BackMatterKind::Notes),
    ("notlar", BackMatterKind::Notes),
    ("bibliography", BackMatterKind::Bibliography),
    ("literatur", BackMatterKind::Bibliography),
    ("kaynakca", BackMatterKind::Bibliography),
    ("kaynakça", BackMatterKind::Bibliography),
    ("index", BackMatterKind::Index),
    ("register", BackMatterKind::Index),
    ("dizin", BackMatterKind::Index),
    ("glossary", BackMatterKind::Glossary),
    ("glossar", BackMatterKind::Glossary),
    ("acknowledgements", BackMatterKind::Acknowledgements),
    ("acknowledgments", BackMatterKind::Acknowledgements),
    ("danksagung", BackMatterKind::Acknowledgements),
];

/// Build the book's section tree.
///
/// `labels` is the printed page label per page index, as `furniture` recovered it. `None`
/// for a page whose folio was not found, which is most pages of most books.
pub fn book_structure(
    flow: &[FlowItem],
    labels: &[Option<String>],
    lang: &LangTag,
    // Taken and not read, for the same reason `meta::metadata` takes it: the keyword lists
    // and the zone ordering are closed sets rather than tunable numbers.
    _t: &Thresholds,
) -> (Vec<Section>, Vec<Warning>, Confidence) {
    let body_starts = arabic_reset_page(labels);

    // Walk the flow once, opening a section at every heading and closing it at the next
    // heading of the same level or shallower.
    let mut roots: Vec<Section> = Vec::new();
    // The open sections, outermost first. `stack[i]` is at level `i + 1`.
    let mut stack: Vec<Section> = Vec::new();
    // Content seen before the first heading — a title page, an epigraph, a dedication.
    let mut preamble: Vec<Content> = Vec::new();
    let mut zone = Zone::Front;

    for item in flow {
        let Content::Heading(heading) = &item.content else {
            match stack.last_mut() {
                Some(open) => {
                    open.content.push(item.content.clone());
                    open.source_pages.1 = open.source_pages.1.max(item.page);
                }
                None => preamble.push(item.content.clone()),
            }
            continue;
        };

        // Zone transitions. Body begins at the arabic reset if the book has one, and
        // otherwise at the first heading that is not front matter by keyword. Back matter
        // begins at the first back-matter keyword *after* the body has begun: `Notes on the
        // Text` is an ordinary chapter title when it opens a book.
        let text = heading.text();
        let front = front_kind(&text, lang);
        let back = back_kind(&text, lang);
        zone = match (zone, body_starts) {
            (Zone::Front, Some(first)) if item.page >= first => Zone::Body,
            (Zone::Front, None) if front.is_none() => Zone::Body,
            (Zone::Body, _) if back.is_some() => Zone::Back,
            (current, _) => current,
        };
        // Once in the back matter a heading without a keyword stays there — `zone` is
        // carried between iterations, and the match above has no transition out of `Back`.
        // A book does not return to its body after its index.

        let role = match zone {
            Zone::Front => SectionRole::FrontMatter(front.unwrap_or(FrontMatterKind::Other)),
            Zone::Back => SectionRole::BackMatter(back.unwrap_or(BackMatterKind::Other)),
            // The numbering is re-read from the text rather than taken off the `Heading`,
            // which carries it as the string that was printed. `Part One` and `Chapter One`
            // are both `"One"` there, and only the keyword separates them.
            Zone::Body => match crate::headings::numbering::read(&text, lang).map(|n| n.kind) {
                Some(NumberingKind::Part) => SectionRole::Part,
                _ if heading.level == 1 => SectionRole::Chapter,
                _ => SectionRole::Section,
            },
        };

        let opened = Section {
            id: heading.id,
            role,
            level: heading.level,
            heading: Some(heading.clone()),
            content: Vec::new(),
            children: Vec::new(),
            source_pages: (item.page, item.page),
            confidence: Confidence::deterministic(vec![
                Signal::new("zone", zone as u8 as f32),
                Signal::new("level", f32::from(heading.level)),
                Signal::new(
                    "keyword",
                    f32::from(u8::from(front.is_some() || back.is_some())),
                ),
            ]),
        };

        // Close every open section at or below the new one's level.
        while stack.len() >= usize::from(opened.level.max(1)) {
            close(&mut stack, &mut roots);
        }
        // A level that skips is not possible here — `assign_levels` removed the skips — but a
        // tree built from a flow has to be total, so a gap is filled by nesting under the
        // deepest open section rather than by panicking.
        stack.push(opened);
    }
    while !stack.is_empty() {
        close(&mut stack, &mut roots);
    }

    // The preamble becomes a front-matter section of its own, so that nothing printed before
    // the first heading is lost.
    if !preamble.is_empty() {
        let first = flow.first().map(|item| item.page).unwrap_or_default();
        let last = roots
            .first()
            .map(|section| section.source_pages.0)
            .unwrap_or(first);
        roots.insert(
            0,
            Section {
                id: BlockId::derive(
                    first,
                    oc_model::geom::Rect {
                        x0: 0.0,
                        y0: 0.0,
                        x1: 0.0,
                        y1: 0.0,
                    },
                    "front matter",
                ),
                role: SectionRole::FrontMatter(FrontMatterKind::Other),
                level: 1,
                heading: None,
                content: preamble,
                children: Vec::new(),
                source_pages: (first, last),
                confidence: Confidence::fallback(vec![Signal::new("unheaded_preamble", 1.0)]),
            },
        );
    }

    let warnings = validate(&roots);
    let confidence = Confidence::deterministic(vec![
        Signal::new("sections", roots.len() as f32),
        Signal::new("arabic_reset", f32::from(u8::from(body_starts.is_some()))),
        Signal::new("warnings", warnings.len() as f32),
    ]);
    (roots, warnings, confidence)
}

/// Pop the deepest open section into its parent, or into the roots.
fn close(stack: &mut Vec<Section>, roots: &mut Vec<Section>) {
    let Some(done) = stack.pop() else {
        return;
    };
    match stack.last_mut() {
        Some(parent) => {
            parent.source_pages.1 = parent.source_pages.1.max(done.source_pages.1);
            parent.children.push(done);
        }
        None => roots.push(done),
    }
}

/// The first page whose printed folio is arabic `1` after a run of roman folios.
///
/// The hard boundary signal of detail 5. `None` when the book numbers straight through, which
/// is most modern paperbacks.
fn arabic_reset_page(labels: &[Option<String>]) -> Option<u32> {
    let mut seen_roman = false;
    for (index, label) in labels.iter().enumerate() {
        let Some(label) = label.as_deref().map(str::trim) else {
            continue;
        };
        if is_roman(label) && roman_value(label).is_some_and(|value| value > 0) {
            seen_roman = true;
            continue;
        }
        if seen_roman && label == "1" {
            return u32::try_from(index).ok();
        }
    }
    None
}

fn front_kind(text: &str, lang: &LangTag) -> Option<FrontMatterKind> {
    let key = fold_key(first_word(text), lang.clone());
    FRONT_WORDS
        .iter()
        .find(|(word, _)| *word == key.as_str())
        .map(|(_, kind)| *kind)
}

fn back_kind(text: &str, lang: &LangTag) -> Option<BackMatterKind> {
    let key = fold_key(first_word(text), lang.clone());
    BACK_WORDS
        .iter()
        .find(|(word, _)| *word == key.as_str())
        .map(|(_, kind)| *kind)
}

fn first_word(text: &str) -> &str {
    text.split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_end_matches([':', '.', ','])
}

/// PIPELINE §8's structural validations, as warnings rather than failures: nothing in this
/// stage may fail a conversion.
fn validate(roots: &[Section]) -> Vec<Warning> {
    let mut warnings = Vec::new();

    // Front ≺ body ≺ back, over the top-level sections.
    let zones: Vec<Zone> = roots.iter().map(|section| section.role.zone()).collect();
    if zones.windows(2).any(|pair| pair[1] < pair[0]) {
        warnings.push(
            Warning::new(W_ZONES_OUT_OF_ORDER, Severity::Warn)
                .with_arg("zones", format!("{zones:?}")),
        );
    }

    // Pages monotone and non-overlapping, at every level.
    for section in roots {
        let spans: Vec<(u32, u32)> = section
            .walk()
            .iter()
            .map(|section| section.source_pages)
            .collect();
        if spans.iter().any(|(first, last)| last < first) {
            warnings.push(
                Warning::new(W_SECTION_PAGES_NOT_MONOTONE, Severity::Warn)
                    .with_arg("section", section.id.to_string()),
            );
        }
    }
    let starts: Vec<u32> = roots.iter().map(|section| section.source_pages.0).collect();
    if starts.windows(2).any(|pair| pair[1] < pair[0]) {
        warnings.push(
            Warning::new(W_SECTION_PAGES_NOT_MONOTONE, Severity::Warn)
                .with_arg("starts", format!("{starts:?}")),
        );
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_roman_run_followed_by_arabic_one_is_the_body_boundary() {
        let labels: Vec<Option<String>> = ["i", "ii", "iii", "1", "2"]
            .iter()
            .map(|label| Some((*label).to_owned()))
            .collect();
        assert_eq!(arabic_reset_page(&labels), Some(3));

        // A book that numbers straight through has no boundary to find.
        let straight: Vec<Option<String>> = (1..=5).map(|page| Some(page.to_string())).collect();
        assert_eq!(arabic_reset_page(&straight), None);

        // And an arabic 1 with no roman before it is page one, not a reset.
        assert_eq!(arabic_reset_page(&[Some("1".to_owned())]), None);
    }

    #[test]
    fn back_matter_keywords_are_read_in_all_three_languages() {
        for (text, expected) in [
            ("Appendix A", BackMatterKind::Appendix),
            ("Anhang B", BackMatterKind::Appendix),
            ("Ek C", BackMatterKind::Appendix),
            ("Index", BackMatterKind::Index),
            ("Dizin", BackMatterKind::Index),
            ("Kaynakça", BackMatterKind::Bibliography),
            ("Danksagung", BackMatterKind::Acknowledgements),
        ] {
            assert_eq!(
                back_kind(text, &LangTag::EN).or_else(|| back_kind(text, &LangTag::TR)),
                Some(expected),
                "{text}"
            );
        }
        assert_eq!(back_kind("Chapter One", &LangTag::EN), None);
    }

    #[test]
    fn front_matter_keywords_are_read_in_all_three_languages() {
        assert_eq!(
            front_kind("Preface", &LangTag::EN),
            Some(FrontMatterKind::Preface)
        );
        assert_eq!(
            front_kind("Vorwort", &LangTag::DE),
            Some(FrontMatterKind::Preface)
        );
        assert_eq!(
            front_kind("İçindekiler", &LangTag::TR),
            Some(FrontMatterKind::TableOfContents)
        );
        assert_eq!(front_kind("The Weather", &LangTag::EN), None);
    }
}
