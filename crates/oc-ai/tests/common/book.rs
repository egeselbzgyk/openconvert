//! Small `Document`s, built by hand, for the gates that compare a book before and after an edit.
//!
//! Only the fields the gates read are meaningful: the text every `Content` holds, heading levels,
//! section roles and pages. Everything else is the empty value a document has before the stages
//! that fill it have run.

use oc_model::confidence::Confidence;
use oc_model::doc::{
    Align, Content, Heading, MetaSource, Metadata, Section, SectionRole, Span, Verse,
};
use oc_model::document::{DocClass, Document, PresetName};
use oc_model::geom::Rect;
use oc_model::ids::{BlockId, ClusterId};
use oc_model::lang::LangTag;
use oc_model::layout::Para;
use oc_model::ledger::Ledger;

fn id(text: &str) -> BlockId {
    BlockId::derive(
        0,
        Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 1.0,
            y1: 1.0,
        },
        text,
    )
}

/// A paragraph with its text in one plain span, as `structure` leaves it.
pub fn para(text: &str) -> Content {
    Content::Paragraph(Para {
        id: id(text),
        blocks: vec![id(text)],
        lines: Vec::new(),
        text: text.to_owned(),
        first_line_indent: false,
        pages: (0, 0),
        spans: vec![Span::plain(text)],
        drop_cap: false,
        align: Align::Justify,
        lang: None,
        confidence: None,
    })
}

fn heading_of(level: u8, text: &str) -> Heading {
    Heading {
        id: id(text),
        level,
        spans: vec![Span::plain(text)],
        numbering: None,
        style_cluster: ClusterId(0),
        confidence: Confidence::deterministic(Vec::new()),
    }
}

/// A heading in a section's flow.
pub fn heading(level: u8, text: &str) -> Content {
    Content::Heading(heading_of(level, text))
}

/// Verse, one stanza, one plain span per line.
pub fn verse(lines: &[&str]) -> Content {
    Content::Verse(Verse {
        id: id(lines.first().copied().unwrap_or_default()),
        stanzas: vec![lines.iter().map(|line| vec![Span::plain(*line)]).collect()],
        confidence: Confidence::deterministic(Vec::new()),
    })
}

/// A block quotation around other content.
pub fn quote(inner: Vec<Content>) -> Content {
    Content::BlockQuote(inner)
}

/// A chapter: a section opened by a heading at `level`, on `page`.
pub fn chapter(level: u8, title: &str, page: u32, content: Vec<Content>) -> Section {
    Section {
        id: id(title),
        role: SectionRole::Chapter,
        level,
        heading: Some(heading_of(level, title)),
        content,
        children: Vec::new(),
        source_pages: (page, page),
        confidence: Confidence::deterministic(Vec::new()),
    }
}

/// A book made of these sections.
pub fn document(sections: Vec<Section>) -> Document {
    Document {
        ir_version: oc_model::IR_VERSION,
        source_sha256: "0".repeat(64),
        meta: Metadata {
            title: None,
            subtitle: None,
            authors: Vec::new(),
            translator: None,
            publisher: None,
            date: None,
            identifier: "urn:uuid:00000000-0000-0000-0000-000000000000".to_owned(),
            language: LangTag::UND,
            source: MetaSource::Heuristic,
        },
        language: LangTag::UND,
        sections,
        notes: Vec::new(),
        figures: Vec::new(),
        tables: Vec::new(),
        page_breaks: Vec::new(),
        ledger: Ledger::default(),
        decisions: Vec::new(),
        warnings: Vec::new(),
        classification: DocClass::BookProse,
        presets: PresetName::Novel,
    }
}

/// Every string the conservation law counts, mutably, in the order `Document::text_pieces`
/// reads them — which is what an edit of the book's text has to reach.
pub fn text_slots(document: &mut Document) -> Vec<&mut String> {
    let mut out = Vec::new();
    for section in &mut document.sections {
        section_slots(section, &mut out);
    }
    out
}

fn section_slots<'a>(section: &'a mut Section, out: &mut Vec<&'a mut String>) {
    if let Some(heading) = &mut section.heading {
        out.extend(heading.spans.iter_mut().map(|span| &mut span.text));
    }
    content_slots(&mut section.content, out);
    for child in &mut section.children {
        section_slots(child, out);
    }
}

fn content_slots<'a>(content: &'a mut [Content], out: &mut Vec<&'a mut String>) {
    for item in content {
        match item {
            Content::Paragraph(para) => out.push(&mut para.text),
            Content::Heading(heading) => {
                out.extend(heading.spans.iter_mut().map(|span| &mut span.text));
            }
            Content::Verse(verse) => {
                for line in verse.stanzas.iter_mut().flatten() {
                    out.extend(line.iter_mut().map(|span| &mut span.text));
                }
            }
            Content::BlockQuote(inner) | Content::Epigraph(inner) => content_slots(inner, out),
            _ => {}
        }
    }
}
