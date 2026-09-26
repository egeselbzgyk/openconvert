//! The three fuzz properties (PHASE 14 detail 8, D11, SECURITY §12): OpenConvert's own parsers,
//! not PDFium's, which OSS-Fuzz already fuzzes.
//!
//! Each property is a function of arbitrary bytes that panics when the property fails. The
//! `cargo-fuzz` targets under `fuzz/` are one line each — they call these — and the ordinary test
//! suite calls them too, over the committed seed corpora and a few hundred generated inputs, so a
//! property that stops compiling or starts failing is caught by the gates and not only by the
//! nightly fuzz job.
//!
//! - [`ir_deserialize`]: bytes → the IR's semantic layer (what the engine reads back from a saved
//!   run: sections, notes, figures, tables, metadata, page breaks) → canonical JSON. No panic, and
//!   canonical JSON is a fixed point: what it produces reads back and re-serialises byte for byte
//!   (ordering and rounding bugs show up here as well as crashes).
//! - [`job_spec`]: bytes → `oc_core::jobspec::parse`. No panic, and every accepted spec names every
//!   file by an absolute path with no `..` in it (RT B15: the one argument the GUI passes is only
//!   as safe as this validator).
//! - [`xhtml_opf_roundtrip`]: bytes → a document tree → the typed builder → XHTML, OPF, nav → read
//!   back. Every emitted file parses, and Tier 1 finds nothing a repair would fire on — or the
//!   emitter refused, which it may do only for a character XML 1.0 cannot carry.

use oc_model::confidence::Confidence;
use oc_model::doc::{
    Align, Content, Figure, Heading, List, ListItem, MetaSource, Metadata, Note, PageBreak, Pre,
    Section, SectionRole, Span, SpanStyle, Table, Verse,
};
use oc_model::document::{DocClass, Document, PresetName};
use oc_model::geom::Rect;
use oc_model::ids::{BlockId, ClusterId};
use oc_model::lang::LangTag;
use oc_model::layout::Para;
use oc_model::ledger::Ledger;
use serde::{Deserialize, Serialize};

/// The part of the IR the engine deserialises: a saved run's semantic layer (`openconvert::cache`).
/// The whole `Document` carries `&'static str` registries (warning codes, ledger stages) and is
/// never read back; these are, from a file a user's cache directory holds.
#[derive(Debug, Serialize, Deserialize)]
pub struct SemanticIr {
    pub meta: Metadata,
    pub language: LangTag,
    pub sections: Vec<Section>,
    pub notes: Vec<Note>,
    pub figures: Vec<Figure>,
    pub tables: Vec<Table>,
    pub page_breaks: Vec<PageBreak>,
}

/// Property 1 (row 14.13).
pub fn ir_deserialize(data: &[u8]) {
    let Ok(ir) = serde_json::from_slice::<SemanticIr>(data) else {
        return;
    };
    let Ok(first) = oc_model::canonical::to_canonical_json(&ir) else {
        // Non-finite floats cannot come out of JSON, and nothing else refuses; either way a
        // refusal is an answer, not a crash.
        return;
    };
    let again: SemanticIr = serde_json::from_str(&first)
        .unwrap_or_else(|error| panic!("canonical JSON does not read back: {error}\n{first}"));
    let second = oc_model::canonical::to_canonical_json(&again)
        .unwrap_or_else(|error| panic!("canonical JSON does not re-serialise: {error}"));
    assert_eq!(first, second, "canonical JSON is not a fixed point");
}

/// Property 2 (row 14.14).
pub fn job_spec(data: &[u8]) {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(spec) = oc_core::jobspec::parse(text) else {
        return;
    };
    let ai = spec.ai.as_ref();
    let paths = [
        Some(&spec.input.path),
        spec.input.password_file.as_ref(),
        Some(&spec.output.path),
        spec.output.report_path.as_ref(),
        spec.overrides_path.as_ref(),
        ai.and_then(|ai| ai.api_key_file.as_ref()),
        ai.and_then(|ai| ai.model_path.as_ref()),
    ];
    for path in paths.into_iter().flatten() {
        assert!(
            path.is_absolute(),
            "an accepted spec names {}",
            path.display()
        );
        assert!(
            !path
                .components()
                .any(|component| component == std::path::Component::ParentDir),
            "an accepted spec climbs: {}",
            path.display()
        );
    }
}

/// Property 3 (row 14.15). Returns whether a container was emitted (the emitter may refuse), so a
/// caller can tell a vacuous run from a real one.
pub fn xhtml_opf_roundtrip(data: &[u8]) -> bool {
    let document = document_from(data);
    let sources: Vec<oc_epub::images::SourceImage> = Vec::new();
    let options = oc_epub::EpubOptions {
        split_bytes: SPLIT_BYTES,
        max_longest_side_px: u32::MAX,
        jpeg_quality: JPEG_QUALITY,
        warn_total_bytes: u64::MAX,
        modified: "2026-01-01T00:00:00Z".to_owned(),
    };
    let built = match oc_epub::build_epub(&document, &sources, &options) {
        Ok(built) => built,
        Err(oc_epub::EpubError::Markup(_)) => return false,
        Err(other) => panic!("the emitter failed for a reason other than markup: {other}"),
    };
    for file in &built.emitted.files {
        if let Err(error) = oc_epub::textcontent::body_text(&file.markup) {
            panic!("{} does not parse: {error}\n{}", file.path, file.markup);
        }
    }
    let report =
        oc_validate::validate_tier1(&built.bytes, &oc_validate::Expectations { images: Some(0) });
    let plan = oc_validate::repair::table::plan_repairs(&report.findings);
    assert!(
        plan.actions.is_empty(),
        "Tier 1 would fire a repair on the emitter's own output: {:?}",
        report.findings
    );
    true
}

/// Small enough that a fuzzed book is split across files, so the nav and the spine are exercised.
const SPLIT_BYTES: usize = 400;
const JPEG_QUALITY: u8 = 85;

/// A document tree drawn from `data`, a byte at a time: every input is some document, a mutation
/// of the input is a nearby document, and exhausted input reads as zeros — so the walk always ends
/// (bounded counts, bounded depth), whatever the fuzzer hands in.
pub fn document_from(data: &[u8]) -> Document {
    let mut bytes = Bytes { data, at: 0 };
    let count = 1 + bytes.below(MAX_SECTIONS);
    let sections = (0..count).map(|_| section(&mut bytes)).collect();
    empty_document(sections)
}

const MAX_SECTIONS: usize = 3;
const MAX_ITEMS: usize = 5;
const MAX_SPANS: usize = 4;
const MAX_CHARS: usize = 24;
const MAX_DEPTH: u32 = 3;
/// How the next item is chosen: paragraphs dominate, as in a book.
const KINDS: usize = 10;

/// The fuzzer's bytes as a stream of small choices.
struct Bytes<'a> {
    data: &'a [u8],
    at: usize,
}

impl Bytes<'_> {
    fn byte(&mut self) -> u8 {
        let byte = self.data.get(self.at).copied().unwrap_or(0);
        self.at += 1;
        byte
    }

    fn below(&mut self, bound: usize) -> usize {
        usize::from(self.byte()) % bound.max(1)
    }

    fn flag(&mut self) -> bool {
        self.byte() & 1 == 1
    }

    fn word(&mut self) -> u32 {
        u32::from_le_bytes([self.byte(), self.byte(), self.byte(), self.byte()])
    }
}

/// Characters with the awkward ones over-represented: the markup five, an astral character, a
/// combining mark, Hebrew, plain letters, anything else a code point can be — and, rarely, the two
/// XML 1.0 cannot carry at all (a C0 control, a noncharacter), which the emitter must refuse.
fn character(bytes: &mut Bytes<'_>) -> char {
    const AWKWARD: [char; 10] = [
        '&',
        '<',
        '>',
        '"',
        '\'',
        '\u{10348}',
        '\u{0301}',
        '\u{05d0}',
        ' ',
        '\u{a0}',
    ];
    const UNREPRESENTABLE_C0: u8 = 255;
    const UNREPRESENTABLE_NONCHAR: u8 = 254;
    const AWKWARD_BELOW: u8 = 100;
    const LETTERS_BELOW: u8 = 200;
    const LETTERS: u8 = 26;
    match bytes.byte() {
        UNREPRESENTABLE_C0 => '\u{1}',
        UNREPRESENTABLE_NONCHAR => '\u{fffe}',
        choice if choice < AWKWARD_BELOW => AWKWARD[usize::from(choice) % AWKWARD.len()],
        choice if choice < LETTERS_BELOW => char::from(b'a' + choice % LETTERS),
        _ => char::from_u32(bytes.word() % (u32::from(char::MAX) + 1)).unwrap_or('?'),
    }
}

fn text(bytes: &mut Bytes<'_>) -> String {
    let length = bytes.below(MAX_CHARS);
    (0..length).map(|_| character(bytes)).collect()
}

fn spans(bytes: &mut Bytes<'_>) -> Vec<Span> {
    let count = bytes.below(MAX_SPANS);
    (0..count)
        .map(|_| {
            let flags = bytes.byte();
            Span {
                text: text(bytes),
                style: SpanStyle {
                    bold: flags & 1 != 0,
                    italic: flags & 2 != 0,
                    smallcaps: flags & 4 != 0,
                    superscript: flags & 8 != 0,
                    subscript: flags & 16 != 0,
                    monospace: flags & 32 != 0,
                },
                noteref: None,
                link: None,
            }
        })
        .collect()
}

/// A block id unique per `seed`: ids are what the emitter writes as XML ids, and two equal ids in
/// one file is a finding the fuzzer should not be manufacturing.
fn block_id(seed: u32) -> BlockId {
    BlockId::derive(
        seed,
        Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 1.0,
            y1: 1.0,
        },
        &format!("fuzz-{seed}"),
    )
}

fn para(bytes: &mut Bytes<'_>) -> Para {
    let spans = spans(bytes);
    let seed = bytes.word();
    Para {
        id: block_id(seed),
        blocks: vec![block_id(seed)],
        lines: Vec::new(),
        text: oc_model::doc::spans_text(&spans),
        first_line_indent: false,
        pages: (0, 0),
        spans,
        drop_cap: bytes.flag(),
        align: Align::Left,
        lang: None,
        confidence: None,
    }
}

fn content(bytes: &mut Bytes<'_>, depth: u32) -> Content {
    let nested = depth < MAX_DEPTH;
    match bytes.below(KINDS) {
        5 => Content::Heading(Heading {
            level: u8::try_from(2 + bytes.below(5)).unwrap_or(2),
            spans: spans(bytes),
            id: block_id(bytes.word()),
            numbering: None,
            style_cluster: ClusterId(0),
            confidence: Confidence::deterministic(Vec::new()),
        }),
        6 => {
            let stanzas = (0..bytes.below(3))
                .map(|_| (0..bytes.below(3)).map(|_| spans(bytes)).collect())
                .collect();
            Content::Verse(Verse {
                id: block_id(bytes.word()),
                stanzas,
                confidence: Confidence::deterministic(Vec::new()),
            })
        }
        7 => Content::Preformatted(Pre {
            lines: (0..bytes.below(4)).map(|_| text(bytes)).collect(),
            id: block_id(bytes.word()),
            confidence: Confidence::deterministic(Vec::new()),
        }),
        8 if nested => match bytes.below(3) {
            0 => Content::BlockQuote(vec![content(bytes, depth + 1)]),
            1 => Content::Epigraph(vec![content(bytes, depth + 1)]),
            _ => Content::List(List {
                ordered: bytes.flag(),
                start: None,
                items: (0..bytes.below(3))
                    .map(|_| ListItem {
                        content: vec![content(bytes, depth + 1)],
                        nested: None,
                        marker: None,
                    })
                    .collect(),
                id: block_id(bytes.word()),
                confidence: Confidence::deterministic(Vec::new()),
            }),
        },
        9 => Content::Rule,
        _ => Content::Paragraph(para(bytes)),
    }
}

fn section(bytes: &mut Bytes<'_>) -> Section {
    let seed = bytes.word();
    let heading = bytes.flag().then(|| Heading {
        id: block_id(seed.wrapping_add(1)),
        level: 1,
        spans: spans(bytes),
        numbering: None,
        style_cluster: ClusterId(0),
        confidence: Confidence::deterministic(Vec::new()),
    });
    let count = bytes.below(MAX_ITEMS);
    Section {
        id: block_id(seed),
        role: SectionRole::Chapter,
        level: 1,
        heading,
        content: (0..count).map(|_| content(bytes, 0)).collect(),
        children: Vec::new(),
        source_pages: (0, 0),
        confidence: Confidence::deterministic(Vec::new()),
    }
}

fn empty_document(sections: Vec<Section>) -> Document {
    Document {
        ir_version: oc_model::IR_VERSION,
        source_sha256: "0".repeat(64),
        meta: Metadata {
            title: Some("Fuzz".to_owned()),
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
        cover: None,
    }
}
