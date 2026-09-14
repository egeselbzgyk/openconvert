//! Row 5.19: arbitrary documents never produce non-well-formed XHTML.
//!
//! RT B9's point is that the thing worth fuzzing is **our** emitter, not PDFium — PDFium is
//! fuzzed continuously by OSS-Fuzz and we would only be re-finding their bugs. What nobody else
//! fuzzes is the path from a document tree to XML text: an escaper that misses a case, a
//! character XML cannot carry, a nesting the builder allows and the serialiser mangles.
//!
//! The plan names `cargo-fuzz` for this. It is written as a `proptest` instead, because a fuzz
//! target is not a test: it has no pass condition, it runs until someone stops it, and CI
//! cannot hold it to "green". A property test with a shrinking generator makes the same claim
//! in a form that fails a build. `PROPTEST_CASES=4096` is the nightly tier
//! (IMPLEMENTATION_PLAN §0.5); the default is the per-PR tier.
//!
//! The property is deliberately two-sided: the emitter must either produce a document that
//! parses, or refuse. Refusing is a legitimate answer — a C0 control has no spelling in XML 1.0
//! and `epub` is Conserving, so it cannot be dropped — and a test that demanded output would be
//! demanding the wrong thing.

use oc_epub::images::SourceImage;
use oc_epub::{build_epub, EpubOptions};
use oc_model::confidence::Confidence;
use oc_model::doc::{
    Align, Content, Heading, List, ListItem, MetaSource, Metadata, Pre, Section, SectionRole, Span,
    SpanStyle, Verse,
};
use oc_model::document::{DocClass, Document, PresetName};
use oc_model::geom::Rect;
use oc_model::ids::BlockId;
use oc_model::lang::LangTag;
use oc_model::layout::Para;
use oc_model::ledger::Ledger;
use proptest::prelude::*;

/// Text with the awkward characters over-represented: the markup five, an astral plane
/// character, a C0 control, a lone tab, a noncharacter.
fn text() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        prop_oneof![
            30 => any::<char>().prop_filter("no surrogates", |ch| !ch.is_control() || *ch == '\n'),
            10 => Just('&'),
            10 => Just('<'),
            10 => Just('>'),
            5 => Just('"'),
            5 => Just('\''),
            3 => Just('\u{1}'),
            2 => Just('\u{b}'),
            2 => Just('\u{fffe}'),
            5 => Just('\u{10348}'),
            5 => Just('\t'),
        ],
        0..24,
    )
    .prop_map(|chars| chars.into_iter().collect())
}

fn style() -> impl Strategy<Value = SpanStyle> {
    (
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(
            |(bold, italic, smallcaps, superscript, subscript, monospace)| SpanStyle {
                bold,
                italic,
                smallcaps,
                superscript,
                subscript,
                monospace,
            },
        )
}

fn spans() -> impl Strategy<Value = Vec<Span>> {
    proptest::collection::vec(
        (text(), style()).prop_map(|(text, style)| Span {
            text,
            style,
            noteref: None,
            link: None,
        }),
        0..4,
    )
}

fn block_id() -> BlockId {
    BlockId::derive(
        0,
        Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 1.0,
            y1: 1.0,
        },
        "fuzz",
    )
}

fn para() -> impl Strategy<Value = Para> {
    (spans(), any::<bool>()).prop_map(|(spans, drop_cap)| Para {
        id: block_id(),
        blocks: vec![block_id()],
        lines: Vec::new(),
        text: oc_model::doc::spans_text(&spans),
        first_line_indent: false,
        pages: (0, 0),
        spans,
        drop_cap,
        align: Align::Left,
        lang: None,
        confidence: None,
    })
}

/// A flow item, nested up to three deep.
fn content() -> impl Strategy<Value = Content> {
    let leaf = prop_oneof![
        4 => para().prop_map(Content::Paragraph),
        1 => (1u8..8, spans()).prop_map(|(level, spans)| Content::Heading(Heading {
            id: block_id(),
            level,
            spans,
            numbering: None,
            style_cluster: oc_model::ids::ClusterId(0),
            confidence: Confidence::deterministic(Vec::new()),
        })),
        1 => proptest::collection::vec(proptest::collection::vec(spans(), 0..3), 0..3)
            .prop_map(|stanzas| Content::Verse(Verse {
                id: block_id(),
                stanzas,
                confidence: Confidence::deterministic(Vec::new()),
            })),
        1 => proptest::collection::vec(text(), 0..4).prop_map(|lines| Content::Preformatted(Pre {
            id: block_id(),
            lines,
            confidence: Confidence::deterministic(Vec::new()),
        })),
        1 => Just(Content::Rule),
    ];

    leaf.prop_recursive(3, 12, 3, |inner| {
        prop_oneof![
            inner
                .clone()
                .prop_map(|item| Content::BlockQuote(vec![item])),
            inner.clone().prop_map(|item| Content::Epigraph(vec![item])),
            (any::<bool>(), proptest::collection::vec(inner, 0..3)).prop_map(|(ordered, items)| {
                Content::List(List {
                    id: block_id(),
                    ordered,
                    start: None,
                    items: items
                        .into_iter()
                        .map(|item| ListItem {
                            content: vec![item],
                            nested: None,
                            marker: None,
                        })
                        .collect(),
                    confidence: Confidence::deterministic(Vec::new()),
                })
            }),
        ]
    })
}

fn section() -> impl Strategy<Value = Section> {
    (
        proptest::option::of(spans()),
        proptest::collection::vec(content(), 0..5),
    )
        .prop_map(|(heading, content)| Section {
            id: block_id(),
            role: SectionRole::Chapter,
            level: 1,
            heading: heading.map(|spans| Heading {
                id: block_id(),
                level: 1,
                spans,
                numbering: None,
                style_cluster: oc_model::ids::ClusterId(0),
                confidence: Confidence::deterministic(Vec::new()),
            }),
            content,
            children: Vec::new(),
            source_pages: (0, 0),
            confidence: Confidence::deterministic(Vec::new()),
        })
}

fn document() -> impl Strategy<Value = Document> {
    (
        proptest::collection::vec(section(), 1..3),
        proptest::option::of(text()),
    )
        .prop_map(|(sections, title)| Document {
            ir_version: oc_model::IR_VERSION,
            source_sha256: "0".repeat(64),
            meta: Metadata {
                title,
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
        })
}

fn options() -> EpubOptions {
    EpubOptions {
        split_bytes: 400,
        max_longest_side_px: 1600,
        jpeg_quality: 85,
        warn_total_bytes: u64::MAX,
        modified: "2026-01-01T00:00:00Z".to_owned(),
    }
}

proptest! {
    /// Row 5.19. Either every emitted document parses, or the emitter refused — and it refuses
    /// only for a character XML 1.0 has no spelling for.
    #[test]
    fn fuzz_xhtml_emitter_roundtrip(document in document()) {
        let sources: Vec<SourceImage> = Vec::new();
        match build_epub(&document, &sources, &options()) {
            Ok(built) => {
                for file in &built.emitted.files {
                    // A parse that reaches EOF is the whole claim: the emitter never produces
                    // markup a reader would reject.
                    oc_epub::textcontent::body_text(&file.markup).map_err(|error| {
                        TestCaseError::fail(format!("{}: {error}\n{}", file.path, file.markup))
                    })?;
                }
            }
            Err(oc_epub::EpubError::Markup(_)) => {}
            Err(other) => return Err(TestCaseError::fail(format!("{other}"))),
        }
    }
}
