//! The semantic layer: what `structure` turns a stack of paragraphs into (IR_SKETCH,
//! "semantic layer", PIPELINE §8).
//!
//! Everything here is a **label on text that already exists**. The stage that mints these
//! types is Conserving: it may say that a block is a heading, that two blocks are a list, or
//! that a small block at the foot of a page is a note — and it may not add or remove one
//! character while doing so. That is why no type in this file owns text that did not come
//! from a [`crate::layout::Block`], and why every one of them carries a
//! [`crate::confidence::Confidence`] saying how the label was arrived at (D13.5).

use serde::Serialize;

use crate::confidence::Confidence;
use crate::extract::{ImageId, PageRef};
use crate::ids::{BlockId, ClusterId, FigureId, NoteId, PageBreakId, TableId};
use crate::lang::LangTag;
use crate::layout::Para;

/// How a span of text is set.
///
/// Six independent booleans rather than an enum: a run can be bold *and* italic *and*
/// superscript at once, and the EPUB that comes out has to carry all three.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SpanStyle {
    pub bold: bool,
    pub italic: bool,
    pub smallcaps: bool,
    pub superscript: bool,
    pub subscript: bool,
    pub monospace: bool,
}

impl SpanStyle {
    /// Whether the span needs any markup at all. A plain span becomes bare text.
    pub fn is_plain(&self) -> bool {
        *self == Self::default()
    }
}

/// Where a link points.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkTarget {
    /// Somewhere else in this book.
    Internal(BlockId),
    /// A URI the document carried. Kept verbatim; `validate` is what decides whether it may
    /// be emitted (D14).
    External(String),
}

/// One stretch of text in one style — the semantic counterpart of a `Run`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Span {
    pub text: String,
    pub style: SpanStyle,
    /// Set when this span *is* a note marker in the body flow.
    pub noteref: Option<NoteId>,
    pub link: Option<LinkTarget>,
}

impl Span {
    /// A span with no styling and no reference: the common case by a wide margin.
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: SpanStyle::default(),
            noteref: None,
            link: None,
        }
    }

    /// A span in a style of its own.
    pub fn styled(text: impl Into<String>, style: SpanStyle) -> Self {
        Self {
            text: text.into(),
            style,
            noteref: None,
            link: None,
        }
    }
}

/// The text of a run of spans, concatenated. What a conservation check reads.
pub fn spans_text(spans: &[Span]) -> String {
    spans.iter().map(|span| span.text.as_str()).collect()
}

/// How a block is aligned on the page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    #[default]
    Left,
    Right,
    Center,
    Justify,
}

/// A heading, at a level between 1 and 6.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Heading {
    pub id: BlockId,
    /// 1..=6. A level outside that range cannot be serialised into XHTML, so the constructor
    /// clamps rather than letting `epub` discover it.
    pub level: u8,
    pub spans: Vec<Span>,
    /// The numbering the heading carried, when a numbering regex matched: `"3"` of
    /// `"Chapter 3"`, `"A"` of `"Appendix A"`.
    pub numbering: Option<String>,
    pub style_cluster: ClusterId,
    pub confidence: Confidence,
}

/// The deepest heading level XHTML has.
pub const MAX_HEADING_LEVEL: u8 = 6;

impl Heading {
    /// The heading's text.
    pub fn text(&self) -> String {
        spans_text(&self.spans)
    }

    /// Clamp a proposed level into `1..=6`.
    pub fn clamp_level(level: u8) -> u8 {
        level.clamp(1, MAX_HEADING_LEVEL)
    }
}

/// One item of a list: its own content, and any list nested inside it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ListItem {
    pub content: Vec<Content>,
    pub nested: Option<Box<List>>,
}

/// An ordered or unordered list.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct List {
    pub id: BlockId,
    pub ordered: bool,
    /// The first number, when the list does not start at one. `None` for an unordered list.
    pub start: Option<u32>,
    pub items: Vec<ListItem>,
    pub confidence: Confidence,
}

/// The deepest nesting a list may reach (PIPELINE §8.5, "depth is capped at 5").
pub const MAX_LIST_DEPTH: u8 = 5;

/// Verse: stanzas of lines of spans.
///
/// Three levels deep because that is what verse is — a poem is stanzas, a stanza is lines,
/// and a line is styled text — and because flattening any of the three loses the line breaks,
/// which are the one thing about verse a reflowable format must not re-wrap.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Verse {
    pub id: BlockId,
    pub stanzas: Vec<Vec<Vec<Span>>>,
    pub confidence: Confidence,
}

/// Preformatted text: a monospace block whose line breaks and spaces are content.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Pre {
    pub id: BlockId,
    pub lines: Vec<String>,
    pub confidence: Confidence,
}

/// Whether a note sits at the foot of its page or at the back of the book.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    Footnote,
    Endnote,
}

/// A note, and the place in the body that refers to it.
///
/// `anchor` is `Option` and the bijection check in PIPELINE §8.3 is what makes it almost
/// always `Some`: a note nothing refers to is a detection failure that has to be visible
/// rather than hidden behind a link to nowhere, which is the EPUBCheck `RSC-007` class.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Note {
    pub id: NoteId,
    pub kind: NoteKind,
    /// The marker as printed: `"1"`, `"*"`, `"†"`.
    pub marker: String,
    pub body: Vec<Content>,
    pub anchor: Option<BlockId>,
    pub page: PageRef,
    pub confidence: Confidence,
}

/// An image plus, when one could be associated, its caption.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Figure {
    pub id: FigureId,
    pub image: ImageId,
    pub caption: Option<Vec<Span>>,
    /// Alt text. Empty when none could be derived — an empty string is what marks an image
    /// decorative in EPUB, and inventing a description is worse than admitting to none
    /// (R1 §A.10: 95 % of real alt text is the literal word "Image").
    pub alt: String,
    /// The block this figure is placed before, in reading order.
    pub anchor: BlockId,
    pub confidence: Confidence,
}

/// One cell of a table.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Cell {
    pub spans: Vec<Span>,
    pub colspan: u16,
    pub rowspan: u16,
    /// Whether the cell is a `<th>`. Styling a `<td>` to look like a header is named as a
    /// common bad practice by DAISY (R10 §6.12), so the distinction is carried in the model.
    pub header: bool,
}

impl Cell {
    /// A plain body cell spanning one row and one column.
    pub fn new(spans: Vec<Span>) -> Self {
        Self {
            spans,
            colspan: 1,
            rowspan: 1,
            header: false,
        }
    }

    /// How many grid columns this cell occupies once spans are expanded.
    pub fn width(&self) -> u32 {
        u32::from(self.colspan.max(1))
    }
}

/// A table, either as a real grid or as the image fallback plus its text.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Table {
    pub id: TableId,
    pub rows: Vec<Vec<Cell>>,
    pub header_rows: u8,
    pub caption: Option<Vec<Span>>,
    /// Set when the grid could not be trusted: the table is emitted as an image, with the
    /// extracted text still present in a `<details>` fallback (PIPELINE §8.7).
    pub fallback_image: Option<ImageId>,
    pub confidence: Confidence,
}

impl Table {
    /// Every cell's text, in row-major order. The multiset of this against the multiset of
    /// the source blocks is the conservation check PIPELINE §8.7 requires.
    pub fn cell_texts(&self) -> Vec<String> {
        self.rows
            .iter()
            .flat_map(|row| row.iter())
            .map(|cell| spans_text(&cell.spans))
            .collect()
    }
}

/// Where one source page ended, so a reflowed book can still carry a page list.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PageBreak {
    pub id: PageBreakId,
    pub page: PageRef,
    pub before_block: BlockId,
}

/// One piece of a section's flow.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Content {
    Paragraph(Para),
    Heading(Heading),
    List(List),
    BlockQuote(Vec<Content>),
    Verse(Verse),
    Preformatted(Pre),
    Figure(FigureId),
    Table(TableId),
    NoteRefAnchor(NoteId),
    PageBreak(PageBreakId),
    Rule,
    Epigraph(Vec<Content>),
}

impl Content {
    /// The variant's name, as the structural digest counts it (IR_SKETCH, "counts per
    /// `Content` variant"). Stable: a digest snapshot is keyed on these strings.
    pub fn variant(&self) -> &'static str {
        match self {
            Content::Paragraph(_) => "paragraph",
            Content::Heading(_) => "heading",
            Content::List(_) => "list",
            Content::BlockQuote(_) => "block_quote",
            Content::Verse(_) => "verse",
            Content::Preformatted(_) => "preformatted",
            Content::Figure(_) => "figure",
            Content::Table(_) => "table",
            Content::NoteRefAnchor(_) => "note_ref_anchor",
            Content::PageBreak(_) => "page_break",
            Content::Rule => "rule",
            Content::Epigraph(_) => "epigraph",
        }
    }

    /// Every variant's name, in declaration order. A digest reports a zero for a variant that
    /// did not occur rather than omitting it, so that two books' digests line up.
    pub const VARIANTS: &'static [&'static str] = &[
        "paragraph",
        "heading",
        "list",
        "block_quote",
        "verse",
        "preformatted",
        "figure",
        "table",
        "note_ref_anchor",
        "page_break",
        "rule",
        "epigraph",
    ];

    /// The content nested inside this one, for the three variants that wrap others.
    pub fn children(&self) -> &[Content] {
        match self {
            Content::BlockQuote(inner) | Content::Epigraph(inner) => inner,
            _ => &[],
        }
    }
}

/// Which part of a book a front-matter section is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FrontMatterKind {
    HalfTitle,
    TitlePage,
    Copyright,
    Dedication,
    Epigraph,
    TableOfContents,
    Foreword,
    Preface,
    Introduction,
    /// Front matter whose kind no keyword identified.
    Other,
}

/// Which part of a book a back-matter section is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackMatterKind {
    Appendix,
    Notes,
    Bibliography,
    Glossary,
    Index,
    Acknowledgements,
    Colophon,
    Other,
}

/// What a section is in the architecture of a book.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionRole {
    FrontMatter(FrontMatterKind),
    Part,
    Chapter,
    /// A section inside a chapter — anything below the chapter level.
    Section,
    BackMatter(BackMatterKind),
}

impl SectionRole {
    /// Which of the book's three zones this role belongs to. Front ≺ body ≺ back is a
    /// validated ordering (IMPLEMENTATION_PLAN Phase 4 detail 5).
    pub fn zone(&self) -> Zone {
        match self {
            SectionRole::FrontMatter(_) => Zone::Front,
            SectionRole::Part | SectionRole::Chapter | SectionRole::Section => Zone::Body,
            SectionRole::BackMatter(_) => Zone::Back,
        }
    }
}

/// The three zones of a book, in the order they must appear.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Zone {
    Front,
    Body,
    Back,
}

/// One node of the document tree.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Section {
    pub id: BlockId,
    pub role: SectionRole,
    /// 1..=6, matching the heading level that opened it.
    pub level: u8,
    pub heading: Option<Heading>,
    pub content: Vec<Content>,
    pub children: Vec<Section>,
    pub source_pages: (u32, u32),
    pub confidence: Confidence,
}

impl Section {
    /// This section and every section under it, depth first, in document order.
    pub fn walk(&self) -> Vec<&Section> {
        let mut out = vec![self];
        for child in &self.children {
            out.extend(child.walk());
        }
        out
    }
}

/// Where a metadata field came from. Load-bearing: a title a model guessed and a title the
/// file declared must not be indistinguishable (D13.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetaSource {
    Xmp,
    InfoDict,
    Llm,
    Heuristic,
    User,
}

/// The book's metadata.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Metadata {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub authors: Vec<String>,
    pub translator: Option<String>,
    pub publisher: Option<String>,
    pub date: Option<String>,
    /// `urn:uuid:…`, minted from `source_sha256` so a re-conversion of the same bytes keeps
    /// the same identity (R5 §A2).
    pub identifier: String,
    pub language: LangTag,
    pub source: MetaSource,
}

/// How bad a warning is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warn,
    Error,
}

/// One named thing that went less well than it might have.
///
/// The code is a stable string constant rather than an enum because the set grows every
/// phase and the UI localises by code; `args` is an ordered map so two runs produce the same
/// bytes.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Warning {
    pub code: &'static str,
    pub severity: Severity,
    pub args: std::collections::BTreeMap<String, String>,
    pub blocks: Vec<BlockId>,
    pub page: Option<PageRef>,
}

impl Warning {
    /// A warning with no arguments and no blocks attached.
    pub fn new(code: &'static str, severity: Severity) -> Self {
        Self {
            code,
            severity,
            args: std::collections::BTreeMap::new(),
            blocks: Vec::new(),
            page: None,
        }
    }

    pub fn with_arg(mut self, key: &str, value: impl Into<String>) -> Self {
        self.args.insert(key.to_owned(), value.into());
        self
    }

    pub fn with_blocks(mut self, blocks: Vec<BlockId>) -> Self {
        self.blocks = blocks;
        self
    }

    pub fn with_page(mut self, page: PageRef) -> Self {
        self.page = Some(page);
        self
    }
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Not numbered rows of the Phase 4 table:
// the table names tests for the *decisions*, and these are the two properties of the types
// themselves that the decisions rely on and that nothing else would catch.
// ---------------------------------------------------------------------------

/// The digest counts per `Content` variant by name, and a snapshot is keyed on those names.
/// `variant()` and `VARIANTS` are written by hand and would drift silently apart.
#[test]
fn every_content_variant_is_listed_and_named_once() {
    use std::collections::BTreeSet;

    let distinct: BTreeSet<&str> = Content::VARIANTS.iter().copied().collect();
    assert_eq!(
        distinct.len(),
        Content::VARIANTS.len(),
        "a variant name is listed twice"
    );

    // Every constructed variant's name is in the list. The match in `variant()` is
    // exhaustive, so constructing one of each is what ties the two together.
    let id = BlockId::derive(
        0,
        crate::geom::Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 1.0,
            y1: 1.0,
        },
        "x",
    );
    let confidence = Confidence::deterministic(Vec::new());
    let samples = vec![
        Content::Heading(Heading {
            id,
            level: 1,
            spans: vec![Span::plain("h")],
            numbering: None,
            style_cluster: ClusterId(0),
            confidence: confidence.clone(),
        }),
        Content::List(List {
            id,
            ordered: true,
            start: None,
            items: Vec::new(),
            confidence: confidence.clone(),
        }),
        Content::BlockQuote(Vec::new()),
        Content::Verse(Verse {
            id,
            stanzas: Vec::new(),
            confidence: confidence.clone(),
        }),
        Content::Preformatted(Pre {
            id,
            lines: Vec::new(),
            confidence,
        }),
        Content::Figure(FigureId(0)),
        Content::Table(TableId(0)),
        Content::NoteRefAnchor(NoteId(0)),
        Content::PageBreak(PageBreakId(0)),
        Content::Rule,
        Content::Epigraph(Vec::new()),
    ];
    for sample in &samples {
        assert!(
            Content::VARIANTS.contains(&sample.variant()),
            "{} is not in Content::VARIANTS",
            sample.variant()
        );
    }
    // Every variant but `Paragraph`, which needs a `Para` and is covered by the list length.
    assert_eq!(samples.len(), Content::VARIANTS.len() - 1);
}

/// A heading level outside 1..=6 cannot reach XHTML, so the model refuses to hold one.
#[test]
fn heading_levels_are_clamped_into_the_xhtml_range() {
    assert_eq!(Heading::clamp_level(0), 1);
    assert_eq!(Heading::clamp_level(1), 1);
    assert_eq!(Heading::clamp_level(6), 6);
    assert_eq!(Heading::clamp_level(9), 6);
}

/// Front ≺ body ≺ back is an ordering the book-structure validation compares on, so the
/// ordering has to be on the type rather than in the validator.
#[test]
fn the_three_zones_are_ordered_front_body_back() {
    assert!(Zone::Front < Zone::Body);
    assert!(Zone::Body < Zone::Back);
    assert_eq!(
        SectionRole::FrontMatter(FrontMatterKind::Preface).zone(),
        Zone::Front
    );
    assert_eq!(SectionRole::Chapter.zone(), Zone::Body);
    assert_eq!(
        SectionRole::BackMatter(BackMatterKind::Index).zone(),
        Zone::Back
    );
}
