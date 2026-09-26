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
    t: &Thresholds,
) -> (Vec<Section>, Vec<Warning>, Confidence) {
    book_structure_with(
        flow,
        labels,
        lang,
        None,
        &std::collections::BTreeMap::new(),
        t,
        &ZoneEdits::default(),
    )
}

/// One heading's place, as the book-structure task (PHASE 10, task 3) placed it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZoneLabel {
    pub zone: Zone,
    /// The heading starts a part.
    pub part: bool,
}

/// The book-structure task's edit: a place for each heading it covered, keyed by the heading's
/// index among the flow's headings. A heading it did not cover is placed as the deterministic
/// rules place it, carrying on from the zone the previous heading was in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ZoneEdits {
    pub labels: std::collections::BTreeMap<u32, ZoneLabel>,
}

impl ZoneEdits {
    pub fn is_empty(&self) -> bool {
        self.labels.is_empty()
    }
}

/// [`book_structure`], with the book-structure task's zones where it gave them.
///
/// A label is a zone and a part flag, and nothing else: the tree is still built by the walk
/// below, from the same headings at the same levels. Where the numbering says `Part`, the
/// heading is a part whatever the label says — the deterministic keyword is counter-evidence a
/// model does not override.
pub fn book_structure_with(
    flow: &[FlowItem],
    labels: &[Option<String>],
    lang: &LangTag,
    // The book's title, by which its title page is known among the pages before the first
    // heading.
    title: Option<&str>,
    // The front_page task's kinds, by page, for the pages the rules could not type.
    front_pages: &std::collections::BTreeMap<u32, FrontMatterKind>,
    t: &Thresholds,
    zones: &ZoneEdits,
) -> (Vec<Section>, Vec<Warning>, Confidence) {
    let body_starts = arabic_reset_page(labels);
    // Back matter is at the back: a keyword heading before `book.back_min_page_share` of the
    // book's pages is an acknowledgement in a preface or a chapter called "Notes", and taking
    // it for the start of the back matter put every chapter after it there (2026-09-26).
    let last_page = flow.iter().map(|item| item.page).max().unwrap_or_default();
    let back_from = f64::from(last_page) * t.book.back_min_page_share;
    let mut heading_index: u32 = 0;

    // Walk the flow once, opening a section at every heading and closing it at the next
    // heading of the same level or shallower.
    let mut roots: Vec<Section> = Vec::new();
    // The open sections, outermost first. `stack[i]` is at level `i + 1`.
    let mut stack: Vec<Section> = Vec::new();
    // Content seen before the first heading — a title page, an epigraph, a dedication — page
    // by page.
    let mut preamble: Vec<crate::front::FrontPage> = Vec::new();
    let mut zone = Zone::Front;

    for item in flow {
        let Content::Heading(heading) = &item.content else {
            match stack.last_mut() {
                Some(open) => {
                    open.content.push(item.content.clone());
                    open.source_pages.1 = open.source_pages.1.max(item.page);
                }
                None => match preamble.last_mut() {
                    Some(page) if page.page == item.page => page.content.push(item.content.clone()),
                    _ => preamble.push(crate::front::FrontPage {
                        page: item.page,
                        content: vec![item.content.clone()],
                    }),
                },
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
        let label = zones.labels.get(&heading_index).copied();
        heading_index = heading_index.saturating_add(1);
        zone = match (label, zone, body_starts) {
            // The book-structure task placed this heading.
            (Some(label), _, _) => label.zone,
            (None, Zone::Front, Some(first)) if item.page >= first => Zone::Body,
            (None, Zone::Front, None) if front.is_none() => Zone::Body,
            (None, Zone::Body, _) if back.is_some() && f64::from(item.page) >= back_from => {
                Zone::Back
            }
            (None, current, _) => current,
        };
        // Once in the back matter a heading without a keyword stays there — `zone` is
        // carried between iterations, and the match above has no transition out of `Back`.
        // A book does not return to its body after its index.

        // Close every open section at or below the new one's level, so that what is left on
        // the stack is the new section's parent.
        while stack.len() >= usize::from(heading.level.max(1)) {
            close(&mut stack, &mut roots);
        }
        // The section right under a part is a chapter, whatever level the part put it at.
        let under_part = stack
            .last()
            .is_some_and(|parent| parent.role == SectionRole::Part);
        let role = match zone {
            Zone::Front => SectionRole::FrontMatter(front.unwrap_or(FrontMatterKind::Other)),
            Zone::Back => SectionRole::BackMatter(back.unwrap_or(BackMatterKind::Other)),
            // The numbering is re-read from the text rather than taken off the `Heading`,
            // which carries it as the string that was printed. `Part One` and `Chapter One`
            // are both `"One"` there, and only the keyword separates them.
            Zone::Body => match crate::headings::numbering::read(&text, lang).map(|n| n.kind) {
                Some(NumberingKind::Part) => SectionRole::Part,
                _ if label.is_some_and(|label| label.part) => SectionRole::Part,
                _ if heading.level == 1 || under_part => SectionRole::Chapter,
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

        // A level that skips is not possible here — `assign_levels` removed the skips — but a
        // tree built from a flow has to be total, so a gap is filled by nesting under the
        // deepest open section rather than by panicking.
        stack.push(opened);
    }
    while !stack.is_empty() {
        close(&mut stack, &mut roots);
    }

    // The preamble becomes front-matter sections of its own, a title page, a copyright page, a
    // dedication, so that nothing printed before the first heading is lost and each page is
    // what it is.
    if !preamble.is_empty() {
        let front = crate::front::front_sections(preamble, title, front_pages, lang, t);
        roots.splice(0..0, front);
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
    use oc_model::ids::BlockId;

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

    fn heading(index: u32, text: &str) -> FlowItem {
        FlowItem {
            page: index,
            content: Content::Heading(oc_model::doc::Heading {
                id: BlockId::derive(
                    index,
                    oc_model::geom::Rect {
                        x0: 0.0,
                        y0: 0.0,
                        x1: 100.0,
                        y1: 10.0,
                    },
                    text,
                ),
                level: 1,
                spans: vec![oc_model::doc::Span::plain(text)],
                numbering: None,
                style_cluster: oc_model::ids::ClusterId(0),
                confidence: Confidence::deterministic(Vec::new()),
            }),
        }
    }

    /// The book-structure task's labels place the headings they cover — a zone and a part flag,
    /// on the same headings at the same levels — and the rules place the ones they do not,
    /// carrying on from the zone the labelled heading left.
    #[test]
    fn zone_labels_place_the_headings_they_cover() {
        use oc_core::thresholds::T;

        // No keyword anywhere: the rules alone would call every heading a chapter.
        let flow = vec![
            heading(0, "A Word First"),
            heading(1, "The Crossing"),
            heading(2, "North"),
            heading(3, "Sources"),
        ];
        let (plain, ..) = book_structure(&flow, &[], &LangTag::EN, &T);
        assert!(plain
            .iter()
            .all(|section| section.role == SectionRole::Chapter));

        let label = |zone, part| ZoneLabel { zone, part };
        let zones = ZoneEdits {
            labels: [
                (0, label(Zone::Front, false)),
                (1, label(Zone::Body, true)),
                (3, label(Zone::Back, false)),
            ]
            .into_iter()
            .collect(),
        };
        let (edited, warnings, _) = book_structure_with(
            &flow,
            &[],
            &LangTag::EN,
            None,
            &std::collections::BTreeMap::new(),
            &T,
            &zones,
        );
        let roles: Vec<SectionRole> = edited.iter().map(|section| section.role).collect();
        assert_eq!(
            roles,
            vec![
                SectionRole::FrontMatter(FrontMatterKind::Other),
                SectionRole::Part,
                // Unlabelled: the rules carry on in the body.
                SectionRole::Chapter,
                SectionRole::BackMatter(BackMatterKind::Other),
            ]
        );
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    /// A back-matter keyword in the first half of the book — the acknowledgements that close a
    /// preface — does not send every chapter after it to the back matter; the same keyword in
    /// the second half starts it.
    #[test]
    fn back_matter_starts_only_in_the_back_of_the_book() {
        use oc_core::thresholds::T;
        let flow = vec![
            heading(0, "Foreword"),
            heading(2, "Acknowledgments"),
            heading(10, "The First Chapter"),
            heading(50, "The Last Chapter"),
            heading(90, "Index"),
        ];
        let (sections, ..) = book_structure(&flow, &[], &LangTag::EN, &T);
        let roles: Vec<SectionRole> = sections.iter().map(|section| section.role).collect();
        assert_eq!(
            roles,
            vec![
                SectionRole::FrontMatter(FrontMatterKind::Foreword),
                SectionRole::Chapter,
                SectionRole::Chapter,
                SectionRole::Chapter,
                SectionRole::BackMatter(BackMatterKind::Index),
            ]
        );
    }

    /// A book in parts: the sections right under a part are its chapters, and the sections
    /// under those are sections.
    #[test]
    fn the_sections_under_a_part_are_chapters() {
        use oc_core::thresholds::T;
        let at = |index: u32, text: &str, level: u8| {
            let mut item = heading(index, text);
            if let Content::Heading(heading) = &mut item.content {
                heading.level = level;
            }
            item
        };
        let flow = vec![
            at(1, "Chapter 1. Introduction", 1),
            at(10, "Part I. Foundations", 1),
            at(11, "Chapter 2. Thinking", 2),
            at(12, "Defining the Terms", 3),
            at(20, "Chapter 3. Modularity", 2),
        ];
        let (sections, ..) = book_structure(&flow, &[], &LangTag::EN, &T);
        let roles: Vec<SectionRole> = sections
            .iter()
            .flat_map(|root| root.walk().into_iter().map(|section| section.role))
            .collect();
        assert_eq!(
            roles,
            vec![
                SectionRole::Chapter,
                SectionRole::Part,
                SectionRole::Chapter,
                SectionRole::Section,
                SectionRole::Chapter,
            ]
        );
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
