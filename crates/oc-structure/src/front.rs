//! The pages before the first heading: which of them is the title page, the copyright page, the
//! dedication, the epigraph, the printed contents (PIPELINE §8, the front zone).
//!
//! Read from what a page *is*, not from what it is called, so the same rules hold in every
//! language and script: a copyright page carries `©` or an ISBN, a title page prints the book's
//! title and little else, a dedication is a few words alone on a page, an epigraph ends in its
//! attribution, a contents page is lines that link to headings. A page none of these describe
//! stays plain front matter. Pages of one kind that follow each other are one section — a
//! copyright notice that runs over two pages is one copyright page.
//!
//! Nothing here changes a character: the content of a page is moved into its section as it
//! was, except that a title page's or a dedication's lines are unwrapped from the block
//! quotation their centring made of them, which changes the markup and not the text.

use oc_core::thresholds::Thresholds;
use oc_model::confidence::{Confidence, Signal};
use oc_model::doc::{Content, FrontMatterKind, Section, SectionRole};
use oc_model::ids::BlockId;
use oc_model::lang::LangTag;
use oc_text::fold::fold_key;

/// One page of the preamble: its index and what the flow put on it.
#[derive(Clone, Debug, PartialEq)]
pub struct FrontPage {
    pub page: u32,
    pub content: Vec<Content>,
}

/// What the rules can say about one page, before the pages are grouped.
fn kind_of(
    page: &FrontPage,
    title_key: Option<&str>,
    lang: &LangTag,
    t: &Thresholds,
) -> FrontMatterKind {
    let text = page_text(&page.content);
    let words = text.split_whitespace().count();
    if words == 0 {
        return FrontMatterKind::Other;
    }
    if page.content.iter().any(links_somewhere) {
        return FrontMatterKind::TableOfContents;
    }
    if text.contains('\u{00A9}') || has_isbn(&text) {
        return FrontMatterKind::Copyright;
    }
    let max = |value: i64| usize::try_from(value.max(0)).unwrap_or(usize::MAX);
    if let Some(title) = title_key {
        if words <= max(t.front.title_page_max_words) && squash_key(&text, lang).contains(title) {
            return FrontMatterKind::TitlePage;
        }
    }
    if words <= max(t.front.epigraph_max_words) && ends_in_attribution(&page.content) {
        return FrontMatterKind::Epigraph;
    }
    if words <= max(t.front.dedication_max_words) && !text.chars().any(|ch| ch.is_numeric()) {
        return FrontMatterKind::Dedication;
    }
    FrontMatterKind::Other
}

/// The preamble's pages as front-matter sections, one per run of pages of one kind.
///
/// `title` is the book's title as the metadata has it. Of two pages that print it, the first
/// and shorter is the half-title.
pub fn front_sections(
    pages: Vec<FrontPage>,
    title: Option<&str>,
    told: &std::collections::BTreeMap<u32, FrontMatterKind>,
    lang: &LangTag,
    t: &Thresholds,
) -> Vec<Section> {
    let title_key = title
        .map(|title| squash_key(title, lang))
        .filter(|key| key.chars().count() >= MIN_TITLE_KEY_CHARS);
    let mut kinds: Vec<FrontMatterKind> = pages
        .iter()
        .map(|page| kind_of(page, title_key.as_deref(), lang, t))
        .collect();
    let title_pages: Vec<usize> = kinds
        .iter()
        .enumerate()
        .filter(|(_, kind)| **kind == FrontMatterKind::TitlePage)
        .map(|(index, _)| index)
        .collect();
    if let [first, second, ..] = title_pages[..] {
        let words = |index: usize| page_text(&pages[index].content).split_whitespace().count();
        if words(first) <= words(second) {
            kinds[first] = FrontMatterKind::HalfTitle;
        }
    }
    // What the front_page task said, for the pages the rules could not type or typed on the
    // weakest evidence: a few words alone on a page are as often a motto or a series name as a
    // dedication. A copyright sign, an ISBN, the title or a contents link outweighs a model.
    for (page, kind) in pages.iter().zip(kinds.iter_mut()) {
        if matches!(kind, FrontMatterKind::Other | FrontMatterKind::Dedication) {
            if let Some(told) = told.get(&page.page) {
                *kind = *told;
            }
        }
    }

    let mut sections: Vec<Section> = Vec::new();
    for (page, kind) in pages.into_iter().zip(kinds) {
        let content: Vec<Content> = if unwraps_quotes(kind) {
            page.content.into_iter().flat_map(unwrap_quote).collect()
        } else {
            page.content
        };
        match sections.last_mut() {
            Some(open) if open.role == SectionRole::FrontMatter(kind) => {
                open.content.extend(content);
                open.source_pages.1 = open.source_pages.1.max(page.page);
            }
            _ => sections.push(Section {
                id: BlockId::derive(
                    page.page,
                    oc_model::geom::Rect {
                        x0: 0.0,
                        y0: 0.0,
                        x1: 0.0,
                        y1: 0.0,
                    },
                    "front matter",
                ),
                role: SectionRole::FrontMatter(kind),
                level: 1,
                heading: None,
                content,
                children: Vec::new(),
                source_pages: (page.page, page.page),
                confidence: Confidence::deterministic(vec![Signal::new(
                    "front_kind",
                    f32::from(u8::from(kind != FrontMatterKind::Other)),
                )]),
            }),
        }
    }
    sections
}

/// A title shorter than this, squashed, would be found inside too many pages by chance.
const MIN_TITLE_KEY_CHARS: usize = 3;

/// Title pages, half-titles and dedications are centred lines, which the indent test reads as
/// a quotation. Their text is the page itself, not a quotation within it.
fn unwraps_quotes(kind: FrontMatterKind) -> bool {
    matches!(
        kind,
        FrontMatterKind::TitlePage | FrontMatterKind::HalfTitle | FrontMatterKind::Dedication
    )
}

fn unwrap_quote(content: Content) -> Vec<Content> {
    match content {
        Content::BlockQuote(inner) => inner,
        other => vec![other],
    }
}

/// The page's words, in order, from every kind of content that carries text.
fn page_text(content: &[Content]) -> String {
    let mut out = String::new();
    for item in content {
        let text = match item {
            Content::Paragraph(para) => para.text.clone(),
            Content::Heading(heading) => heading.text(),
            Content::BlockQuote(inner) | Content::Epigraph(inner) => page_text(inner),
            Content::Verse(verse) => verse
                .stanzas
                .iter()
                .flatten()
                .flatten()
                .map(|span| span.text.as_str())
                .collect::<Vec<_>>()
                .join(" "),
            Content::Preformatted(pre) => pre.lines.join(" "),
            Content::List(list) => list
                .items
                .iter()
                .map(|item| page_text(&item.content))
                .collect::<Vec<_>>()
                .join(" "),
            _ => String::new(),
        };
        if !text.trim().is_empty() {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(text.trim());
        }
    }
    out
}

/// A contents entry: a paragraph with a link in it.
fn links_somewhere(content: &Content) -> bool {
    match content {
        Content::Paragraph(para) => para.spans.iter().any(|span| span.link.is_some()),
        Content::BlockQuote(inner) => inner.iter().any(links_somewhere),
        _ => false,
    }
}

/// An ISBN: a run of digits and hyphens that is ten or thirteen digits long, the thirteen
/// beginning 978 or 979. The word `ISBN` itself is the same in every language that prints one.
fn has_isbn(text: &str) -> bool {
    if text.to_uppercase().contains("ISBN") {
        return true;
    }
    text.split(|ch: char| !(ch.is_ascii_digit() || ch == '-' || ch == 'X'))
        .map(|run| run.chars().filter(char::is_ascii_digit).collect::<String>())
        .any(|digits| {
            digits.len() == ISBN_13 && (digits.starts_with("978") || digits.starts_with("979"))
        })
}

/// An ISBN-13's digit count: a definition, not a threshold.
const ISBN_13: usize = 13;

/// The page ends in an attribution: its last paragraph opens with a dash and is a few words.
fn ends_in_attribution(content: &[Content]) -> bool {
    let last = content.iter().rev().find_map(|item| match item {
        Content::Paragraph(para) if !para.text.trim().is_empty() => Some(para.text.trim()),
        Content::BlockQuote(inner) | Content::Epigraph(inner) => {
            inner.iter().rev().find_map(|item| match item {
                Content::Paragraph(para) if !para.text.trim().is_empty() => Some(para.text.trim()),
                _ => None,
            })
        }
        _ => None,
    });
    last.is_some_and(|text| {
        text.starts_with(['\u{2014}', '\u{2013}', '\u{2015}'])
            && text.split_whitespace().count() <= ATTRIBUTION_MAX_WORDS
    })
}

/// An attribution is a name and perhaps a work: `— Seneca, Letters`.
const ATTRIBUTION_MAX_WORDS: usize = 8;

/// A lookup key: folded in the book's own locale, spaces and punctuation gone.
fn squash_key(text: &str, lang: &LangTag) -> String {
    fold_key(text, lang.clone())
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use oc_core::thresholds::T;
    use oc_model::doc::Span;

    fn para(text: &str) -> Content {
        Content::Paragraph(oc_model::layout::Para {
            id: BlockId::derive(
                0,
                oc_model::geom::Rect {
                    x0: 0.0,
                    y0: 0.0,
                    x1: 1.0,
                    y1: 1.0,
                },
                text,
            ),
            blocks: Vec::new(),
            lines: Vec::new(),
            text: text.to_owned(),
            first_line_indent: false,
            pages: (0, 0),
            spans: vec![Span::plain(text)],
            drop_cap: false,
            align: oc_model::doc::Align::default(),
            lang: None,
            confidence: None,
        })
    }

    fn page(index: u32, texts: &[&str]) -> FrontPage {
        FrontPage {
            page: index,
            content: texts.iter().map(|text| para(text)).collect(),
        }
    }

    fn kinds(sections: &[Section]) -> Vec<FrontMatterKind> {
        sections
            .iter()
            .map(|section| match section.role {
                SectionRole::FrontMatter(kind) => kind,
                other => panic!("{other:?}"),
            })
            .collect()
    }

    /// The front pages of a translated novel, typed by what each page is — in a language no
    /// keyword list knows.
    #[test]
    fn a_novels_front_pages_are_typed_by_what_they_are() {
        let pages = vec![
            page(1, &["KAYIP ZAMAN"]),
            page(2, &["KAYIP ZAMAN", "Ada Yazar", "Roman"]),
            page(
                3,
                &[
                    "Özgün adı: Lost Time \u{00A9} 2015 Ada Yazar",
                    "Baskı: 2016 ISBN 978-605-0000-00-0",
                ],
            ),
            page(4, &["ANNEME"]),
            page(
                5,
                &[
                    "Kim olduğunu bilen, nereye gittiğini de bilir.",
                    "\u{2014} Eski söz",
                ],
            ),
        ];
        let sections = front_sections(
            pages,
            Some("Kayıp Zaman"),
            &Default::default(),
            &LangTag::TR,
            &T,
        );
        assert_eq!(
            kinds(&sections),
            vec![
                FrontMatterKind::HalfTitle,
                FrontMatterKind::TitlePage,
                FrontMatterKind::Copyright,
                FrontMatterKind::Dedication,
                FrontMatterKind::Epigraph,
            ]
        );
    }

    /// A copyright notice over two pages is one copyright page, and a page of running text is
    /// plain front matter.
    #[test]
    fn pages_of_one_kind_are_one_section() {
        let long = "word ".repeat(200);
        let pages = vec![
            page(3, &["\u{00A9} 2020 Someone"]),
            page(4, &["Printed by a printer. ISBN 0-306-40615-2"]),
            page(5, &[long.as_str()]),
        ];
        let sections = front_sections(pages, None, &Default::default(), &LangTag::EN, &T);
        assert_eq!(
            kinds(&sections),
            vec![FrontMatterKind::Copyright, FrontMatterKind::Other]
        );
        assert_eq!(sections[0].source_pages, (3, 4));
    }

    /// A title page's centred lines lose the quotation their indent made of them; the text is
    /// the same.
    #[test]
    fn a_title_page_is_not_a_quotation() {
        let pages = vec![FrontPage {
            page: 1,
            content: vec![Content::BlockQuote(vec![
                para("THE BOOK"),
                para("A. Writer"),
            ])],
        }];
        let sections = front_sections(
            pages,
            Some("The Book"),
            &Default::default(),
            &LangTag::EN,
            &T,
        );
        assert_eq!(kinds(&sections), vec![FrontMatterKind::TitlePage]);
        assert!(sections[0]
            .content
            .iter()
            .all(|item| matches!(item, Content::Paragraph(_))));
        assert_eq!(page_text(&sections[0].content), "THE BOOK A. Writer");
    }

    /// The model's kind is taken for a page the rules left untyped or called a dedication, and
    /// never over a copyright sign.
    #[test]
    fn the_front_page_task_types_only_what_the_rules_could_not() {
        let long = "word ".repeat(200);
        let pages = vec![
            page(3, &["\u{00A9} 2020 Someone"]),
            page(4, &["For the ones who stayed"]),
            page(5, &[long.as_str()]),
        ];
        let told: std::collections::BTreeMap<u32, FrontMatterKind> = [
            (3, FrontMatterKind::Preface),
            (4, FrontMatterKind::Epigraph),
            (5, FrontMatterKind::Foreword),
        ]
        .into_iter()
        .collect();
        let sections = front_sections(pages, None, &told, &LangTag::EN, &T);
        assert_eq!(
            kinds(&sections),
            vec![
                FrontMatterKind::Copyright,
                FrontMatterKind::Epigraph,
                FrontMatterKind::Foreword,
            ]
        );
    }

    #[test]
    fn an_isbn_is_found_with_or_without_its_name() {
        assert!(has_isbn("ISBN 1"));
        assert!(has_isbn("978-3-16-148410-0"));
        assert!(!has_isbn("Tel: (0212) 544 32 02"));
    }
}
