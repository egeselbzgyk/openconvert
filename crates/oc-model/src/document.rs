//! The finished document tree: what `document` assembles and what `epub` serialises
//! (IR_SKETCH, PIPELINE §9).
//!
//! [`Document`] is the last IR state. Everything above it is a view of one PDF; everything
//! below it is bytes in a container. It is therefore the point at which every cross-reference
//! has to close: a `Content::Figure(id)` in some section's flow must name a figure this
//! document carries, or the EPUB will contain an `<img>` pointing at a file that is not in
//! the manifest — EPUBCheck's `RSC-007` class, and a broken book.
//!
//! [`Document::dangling_references`] is that check, stated once here rather than separately
//! in `document`, in the Tier-1 validator and in the emitter.

use serde::Serialize;

use crate::decision::Decision;
use crate::doc::{Content, Figure, Metadata, Note, PageBreak, Section, Table, Warning};
use crate::ids::{FigureId, NoteId, PageBreakId, TableId};
use crate::lang::LangTag;
use crate::ledger::Ledger;

/// What kind of document this is, decided once from what the pipeline measured (D13.10).
///
/// The classification is recorded in the report and is what `auto` resolves a preset from,
/// so it is a fact about the book rather than an internal switch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DocClass {
    /// Running prose in one column: a novel, an essay collection, a biography.
    #[default]
    BookProse,
    /// Two or more columns of body text: a journal article, a proceedings volume.
    AcademicMulticolumn,
    /// Pages that carry no extractable text of their own.
    Scanned,
    /// Landscape pages, little text, large type: slides, and everything else.
    SlidesOther,
}

impl DocClass {
    /// The name as the report and the `--json` output print it.
    pub fn as_str(self) -> &'static str {
        match self {
            DocClass::BookProse => "book-prose",
            DocClass::AcademicMulticolumn => "academic-multicolumn",
            DocClass::Scanned => "scanned",
            DocClass::SlidesOther => "slides/other",
        }
    }
}

/// A document preset: a named partial override map over `thresholds.toml` keys (D13.11).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PresetName {
    /// Resolved from [`DocClass`] at the `document` stage. Never the value a finished
    /// `Document` carries: [`PresetName::resolve`] has always run by then, so the report
    /// names the preset that was applied rather than the instruction to pick one.
    #[default]
    Auto,
    Novel,
    Academic,
    Textbook,
    Poetry,
    Scanned,
}

impl PresetName {
    /// The name as the report and the CLI print it.
    pub fn as_str(self) -> &'static str {
        match self {
            PresetName::Auto => "auto",
            PresetName::Novel => "novel",
            PresetName::Academic => "academic",
            PresetName::Textbook => "textbook",
            PresetName::Poetry => "poetry",
            PresetName::Scanned => "scanned",
        }
    }

    /// Turn `auto` into the preset the document's class implies; leave an explicit choice
    /// alone, because a user who named a preset has overruled the classifier (D13.11's
    /// precedence chain).
    ///
    /// `textbook` and `poetry` are not reachable from a class, and that is deliberate.
    /// Neither is a *classification* — they are shapes of book the classifier cannot see from
    /// column counts and page classes — and picking one on a guess would silently retune a
    /// dozen thresholds. They stay user-selectable and unreachable by `auto`.
    pub fn resolve(self, class: DocClass) -> Self {
        match self {
            PresetName::Auto => match class {
                DocClass::BookProse | DocClass::SlidesOther => PresetName::Novel,
                DocClass::AcademicMulticolumn => PresetName::Academic,
                DocClass::Scanned => PresetName::Scanned,
            },
            explicit => explicit,
        }
    }
}

/// The whole book, as the pipeline finished understanding it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Document {
    /// First key of the serialised form (ARCHITECTURE §4.5).
    pub ir_version: u32,
    /// The digest of the input bytes. What `dc:identifier` is minted from, so that a
    /// re-conversion of the same file keeps the same book identity (R5 §A2).
    pub source_sha256: String,
    pub meta: Metadata,
    pub language: LangTag,
    pub sections: Vec<Section>,
    pub notes: Vec<Note>,
    pub figures: Vec<Figure>,
    pub tables: Vec<Table>,
    pub page_breaks: Vec<PageBreak>,
    pub ledger: Ledger,
    pub decisions: Vec<Decision>,
    pub warnings: Vec<Warning>,
    pub classification: DocClass,
    /// The preset that was applied, already resolved: never [`PresetName::Auto`].
    pub presets: PresetName,
}

impl Document {
    /// Every section, depth first, in document order.
    pub fn walk(&self) -> Vec<&Section> {
        self.sections.iter().flat_map(Section::walk).collect()
    }

    pub fn figure(&self, id: FigureId) -> Option<&Figure> {
        self.figures.iter().find(|figure| figure.id == id)
    }

    pub fn table(&self, id: TableId) -> Option<&Table> {
        self.tables.iter().find(|table| table.id == id)
    }

    pub fn note(&self, id: NoteId) -> Option<&Note> {
        self.notes.iter().find(|note| note.id == id)
    }

    pub fn page_break(&self, id: PageBreakId) -> Option<&PageBreak> {
        self.page_breaks
            .iter()
            .find(|page_break| page_break.id == id)
    }

    /// Every reference in the flow that names something this document does not carry.
    ///
    /// Returned as readable strings — `figure 3` — in document order, so a failure names what
    /// is missing rather than that something is. An empty vector is what the `document` stage
    /// and the Tier-1 validator both require.
    pub fn dangling_references(&self) -> Vec<String> {
        let mut missing = Vec::new();
        for section in self.walk() {
            self.dangling_in(&section.content, &mut missing);
        }
        for note in &self.notes {
            self.dangling_in(&note.body, &mut missing);
        }
        missing
    }

    /// Every piece of text the document holds, for the conservation check.
    ///
    /// Four places, and a block's text is in exactly one of them — the same four
    /// `oc_structure::stage::StructureOutput::emitted_text` counts, because `document` adds no
    /// text and moves none between them. Metadata, `alt` and page-list labels are outside `C`
    /// by definition (ARCHITECTURE §5.2) and are deliberately absent.
    pub fn text_pieces(&self) -> Vec<String> {
        let mut out = Vec::new();
        for section in self.walk() {
            if let Some(heading) = &section.heading {
                out.push(heading.text());
            }
            collect_text(&section.content, &mut out);
        }
        for note in &self.notes {
            collect_text(&note.body, &mut out);
        }
        for table in &self.tables {
            out.extend(table.cell_texts());
        }
        for figure in &self.figures {
            if let Some(caption) = &figure.caption {
                out.push(crate::doc::spans_text(caption));
            }
        }
        out
    }

    fn dangling_in(&self, content: &[Content], missing: &mut Vec<String>) {
        for item in walk_content(content) {
            match item {
                Content::Figure(id) if self.figure(*id).is_none() => {
                    missing.push(format!("figure {}", id.0));
                }
                Content::Table(id) if self.table(*id).is_none() => {
                    missing.push(format!("table {}", id.0));
                }
                Content::NoteRefAnchor(id) if self.note(*id).is_none() => {
                    missing.push(format!("note {}", id.0));
                }
                Content::PageBreak(id) if self.page_break(*id).is_none() => {
                    missing.push(format!("page break {}", id.0));
                }
                _ => {}
            }
        }
    }
}

/// The first block a content item came from, when it came from one.
///
/// A figure, a table, a note anchor, a page break and a rule return `None`: none of them is a
/// block of the page in its own right — a figure is anchored *before* one, a table is a region
/// several blocks wide — so asking which block they start at has no answer. The callers that
/// need to place something in the flow by page therefore attach it to the next item that is
/// text, which is where a reader would see the boundary anyway.
pub fn first_block(content: &Content) -> Option<crate::ids::BlockId> {
    match content {
        Content::Paragraph(para) => para.blocks.first().copied(),
        Content::Heading(heading) => Some(heading.id),
        Content::Verse(verse) => Some(verse.id),
        Content::Preformatted(pre) => Some(pre.id),
        Content::BlockQuote(inner) | Content::Epigraph(inner) => inner.iter().find_map(first_block),
        Content::List(list) => list
            .items
            .iter()
            .find_map(|item| item.content.iter().find_map(first_block)),
        Content::Figure(_)
        | Content::Table(_)
        | Content::NoteRefAnchor(_)
        | Content::PageBreak(_)
        | Content::Rule => None,
    }
}

/// Every content item in a flow, including the ones nested inside a block quote, an epigraph
/// or a list.
///
/// Flat, because every caller here is asking "does this appear anywhere in the book", and a
/// reference two levels down in a quoted list is exactly as dangling as one at the top.
pub fn walk_content(content: &[Content]) -> Vec<&Content> {
    let mut out = Vec::new();
    for item in content {
        out.push(item);
        match item {
            Content::BlockQuote(inner) | Content::Epigraph(inner) => {
                out.extend(walk_content(inner));
            }
            Content::List(list) => out.extend(walk_list(list)),
            _ => {}
        }
    }
    out
}

/// Walk a flow, collecting the text every item holds in its own right.
///
/// A figure, a table and a note are referenced from the flow and hold their text where they
/// live, so counting them here would count it twice.
fn collect_text(content: &[Content], out: &mut Vec<String>) {
    for item in content {
        match item {
            Content::Paragraph(para) => out.push(para.text.clone()),
            Content::Heading(heading) => out.push(heading.text()),
            Content::List(list) => collect_list_text(list, out),
            Content::BlockQuote(inner) | Content::Epigraph(inner) => collect_text(inner, out),
            Content::Verse(verse) => {
                for stanza in &verse.stanzas {
                    for line in stanza {
                        out.push(crate::doc::spans_text(line));
                    }
                }
            }
            Content::Preformatted(pre) => out.extend(pre.lines.iter().cloned()),
            Content::Figure(_)
            | Content::Table(_)
            | Content::NoteRefAnchor(_)
            | Content::PageBreak(_)
            | Content::Rule => {}
        }
    }
}

fn collect_list_text(list: &crate::doc::List, out: &mut Vec<String>) {
    for item in &list.items {
        collect_text(&item.content, out);
        if let Some(nested) = &item.nested {
            collect_list_text(nested, out);
        }
    }
}

fn walk_list(list: &crate::doc::List) -> Vec<&Content> {
    let mut out = Vec::new();
    for item in &list.items {
        out.extend(walk_content(&item.content));
        if let Some(nested) = &item.nested {
            out.extend(walk_list(nested));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Not rows of the Phase 5 table: that
// table starts at the typed builder, and these are the two properties of `Document` the
// emitter is entitled to assume before it writes one byte.
// ---------------------------------------------------------------------------

#[cfg(test)]
fn sample(content: Vec<Content>) -> Document {
    use crate::confidence::Confidence;
    use crate::doc::{MetaSource, SectionRole};
    use crate::geom::Rect;
    use crate::ids::BlockId;

    let id = BlockId::derive(
        0,
        Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 1.0,
            y1: 1.0,
        },
        "x",
    );
    Document {
        ir_version: crate::IR_VERSION,
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
        sections: vec![Section {
            id,
            role: SectionRole::Chapter,
            level: 1,
            heading: None,
            content,
            children: Vec::new(),
            source_pages: (0, 0),
            confidence: Confidence::deterministic(Vec::new()),
        }],
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

/// A reference the document cannot resolve is an `<img src>` or an `<a href>` pointing at
/// nothing, and it has to be caught here rather than by EPUBCheck. Nesting is the case that
/// matters: a figure quoted inside a block quote is as dangling as one in the top flow, and a
/// check that walked only the top level would pass a broken book.
#[test]
fn a_dangling_reference_is_found_at_every_nesting_depth() {
    let document = sample(vec![Content::Figure(FigureId(3))]);
    assert_eq!(document.dangling_references(), vec!["figure 3"]);

    let nested = sample(vec![Content::BlockQuote(vec![Content::Epigraph(vec![
        Content::Table(TableId(7)),
    ])])]);
    assert_eq!(nested.dangling_references(), vec!["table 7"]);

    let empty = sample(Vec::new());
    assert!(empty.dangling_references().is_empty());
}

/// `auto` is an instruction, not a preset. A `Document` that still carried it would make the
/// report say the book was converted with "pick one", and would leave `epub` to resolve a
/// classification question at serialisation time.
#[test]
fn auto_resolves_to_a_concrete_preset_and_an_explicit_choice_survives() {
    assert_eq!(
        PresetName::Auto.resolve(DocClass::BookProse),
        PresetName::Novel
    );
    assert_eq!(
        PresetName::Auto.resolve(DocClass::AcademicMulticolumn),
        PresetName::Academic
    );
    assert_eq!(
        PresetName::Auto.resolve(DocClass::Scanned),
        PresetName::Scanned
    );
    assert_ne!(
        PresetName::Auto.resolve(DocClass::SlidesOther),
        PresetName::Auto
    );

    // A user who named a preset has overruled the classifier (D13.11).
    assert_eq!(
        PresetName::Poetry.resolve(DocClass::AcademicMulticolumn),
        PresetName::Poetry
    );
}
