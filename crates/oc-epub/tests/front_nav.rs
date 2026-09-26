//! The pages before the first chapter in the book's contents: one entry, not one per page.

use oc_epub::{build_epub, EpubOptions};
use oc_model::confidence::Confidence;
use oc_model::doc::{
    Align, Content, FrontMatterKind, Heading, MetaSource, Metadata, Section, SectionRole, Span,
};
use oc_model::document::{DocClass, Document, PresetName};
use oc_model::geom::Rect;
use oc_model::ids::BlockId;
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

fn para(text: &str) -> Content {
    Content::Paragraph(Para {
        id: id(text),
        blocks: vec![id(text)],
        lines: Vec::new(),
        text: text.to_owned(),
        first_line_indent: false,
        pages: (0, 0),
        spans: vec![Span::plain(text)],
        drop_cap: false,
        align: Align::Left,
        lang: None,
        confidence: None,
    })
}

fn front(kind: FrontMatterKind, text: &str) -> Section {
    Section {
        id: id(text),
        role: SectionRole::FrontMatter(kind),
        level: 1,
        heading: None,
        content: vec![para(text)],
        children: Vec::new(),
        source_pages: (0, 0),
        confidence: Confidence::deterministic(Vec::new()),
    }
}

fn chapter(title: &str) -> Section {
    Section {
        id: id(title),
        role: SectionRole::Chapter,
        level: 1,
        heading: Some(Heading {
            id: id(title),
            level: 1,
            spans: vec![Span::plain(title)],
            numbering: None,
            style_cluster: oc_model::ids::ClusterId(0),
            confidence: Confidence::deterministic(Vec::new()),
        }),
        content: vec![para("It began.")],
        children: Vec::new(),
        source_pages: (1, 1),
        confidence: Confidence::deterministic(Vec::new()),
    }
}

/// A title page, a copyright page and a dedication are one entry under the book's title, then
/// the chapters; all three pages are still in the book.
#[test]
fn the_pages_before_the_first_chapter_are_one_contents_entry() {
    let document = Document {
        ir_version: oc_model::IR_VERSION,
        source_sha256: "0".repeat(64),
        meta: Metadata {
            title: Some("A Book".to_owned()),
            subtitle: None,
            authors: Vec::new(),
            translator: None,
            publisher: None,
            date: None,
            identifier: "urn:uuid:00000000-0000-0000-0000-000000000000".to_owned(),
            language: LangTag::EN,
            source: MetaSource::Heuristic,
        },
        language: LangTag::EN,
        sections: vec![
            front(FrontMatterKind::TitlePage, "A Book"),
            front(FrontMatterKind::Copyright, "Copyright 2020"),
            front(FrontMatterKind::Dedication, "For my mother"),
            chapter("One"),
            chapter("Two"),
        ],
        notes: Vec::new(),
        figures: Vec::new(),
        tables: Vec::new(),
        page_breaks: Vec::new(),
        ledger: Ledger::default(),
        decisions: Vec::new(),
        warnings: Vec::new(),
        classification: DocClass::BookProse,
        presets: PresetName::Novel,
        cover: None,
    };
    let options = EpubOptions {
        split_bytes: 100_000,
        max_longest_side_px: 1600,
        jpeg_quality: 85,
        warn_total_bytes: u64::MAX,
        modified: "2026-01-01T00:00:00Z".to_owned(),
    };
    let built = build_epub(&document, &[], &options).expect("builds");
    let titles: Vec<&str> = built
        .emitted
        .toc
        .iter()
        .map(|point| point.title.as_str())
        .collect();
    assert_eq!(titles, vec!["A Book", "One", "Two"]);

    let text: String = built
        .emitted
        .files
        .iter()
        .map(|file| file.markup.clone())
        .collect();
    for page in ["Copyright 2020", "For my mother"] {
        assert!(text.contains(page), "{page} is still in the book");
    }
    assert!(text.contains("epub:type=\"copyright-page\""));
    assert!(text.contains("epub:type=\"dedication\""));
}
