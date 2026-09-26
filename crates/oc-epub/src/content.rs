//! Turning a [`Document`] into content documents (PIPELINE §10, D13.11).
//!
//! One XHTML file per part, chapter or front-/back-matter section, then split at
//! `xhtml.split_bytes` on paragraph boundaries. The spine follows reading order.
//!
//! Three rules shape everything here, and all three come from the conservation law rather than
//! from taste:
//!
//! - **Nothing is invented.** No `<summary>` reading "Table 2 as text", no alt text reading
//!   "Image", no page number written back as text. `epub` is Conserving with an empty ledger,
//!   so every character in the output has to be a character the book contained. What the
//!   emitter needs to *say* goes into an attribute, which is outside `C` (ARCHITECTURE §5.2).
//! - **Nothing is dropped.** A note nothing referred to still carries its text, so it is
//!   emitted — as a plain `<aside>`, not as a footnote, because a footnote with no reference
//!   fails the Tier-1 bijection.
//! - **Splitting happens between pieces, never inside one.** A piece is a paragraph, a list, a
//!   figure, a table or a whole nested section, already serialised. There is therefore no way
//!   for a `<p>` to span two files, because a `<p>` is never half of anything.

use std::collections::{BTreeMap, BTreeSet};

use oc_model::doc::{Content, Note, Section, SectionRole, Span, Table};
use oc_model::document::Document;
use oc_model::extract::ImageId;
use oc_model::ids::{BlockId, NoteId, PageBreakId};

use crate::xhtml::{
    self, flow, frag, sectioning, CssClass, El, EpubType, Flow, FlowContext, FlowFrag, ImgRef,
    ListKind, SectionLabel, SectioningFrag, TableCell,
};

/// The directory content documents live in, inside the container's root.
pub const TEXT_DIR: &str = "text";

/// One emitted content document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XhtmlFile {
    /// Relative to the package document's directory: `text/c0001.xhtml`.
    pub path: String,
    pub title: String,
    pub markup: String,
}

/// One entry of the table of contents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavPoint {
    pub title: String,
    pub href: String,
    pub children: Vec<NavPoint>,
}

/// One entry of the page list: a printed page number and where it landed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageTarget {
    pub label: String,
    pub href: String,
}

/// One entry of the landmarks nav.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Landmark {
    pub kind: &'static str,
    pub title: String,
    pub href: String,
}

/// What the emitter produced.
#[derive(Clone, Debug, Default)]
pub struct Emitted {
    pub files: Vec<XhtmlFile>,
    pub toc: Vec<NavPoint>,
    pub page_list: Vec<PageTarget>,
    pub landmarks: Vec<Landmark>,
    /// The images actually referenced, in the order they were first used. The manifest lists
    /// these and nothing else: an item in the manifest that no document references is
    /// EPUBCheck's `OPF-003` class.
    pub used_images: Vec<ImageId>,
}

/// A file that came out over the size bound because one indivisible piece was already over it.
pub const W_XHTML_OVERSIZE: &str = "W_XHTML_OVERSIZE";

/// How the emitter is configured.
pub struct EmitOptions {
    /// `xhtml.split_bytes` — Calibre's ADE-derived default, the one anchored size number in
    /// the system (R5 §A13).
    pub split_bytes: usize,
}

/// Emit every content document of a book.
pub fn emit(
    document: &Document,
    images: &BTreeMap<ImageId, String>,
    options: &EmitOptions,
) -> Result<Emitted, xhtml::IllegalChar> {
    let mut ctx = Ctx {
        document,
        images,
        used_images: Vec::new(),
        emitted_notes: BTreeSet::new(),
        next_section: 0,
        heading_anchors: heading_anchors(document),
    };

    // Which notes are referenced from the flow, so that the ones that are not can be emitted
    // as ordinary tangential text instead of as footnotes nothing points at.
    let referenced = referenced_notes(document);

    let mut spines: Vec<Spine> = Vec::new();
    for section in &document.sections {
        spines.push(build_spine(section, &referenced, &mut ctx)?);
    }

    // A note nothing referred to still has text, and the text has to reach the book. It goes
    // at the end of the spine document covering the page it was printed on, which is where a
    // reader would have met it — as a plain `<aside>`, because a footnote with no reference
    // fails the Tier-1 bijection.
    let orphan_ids: Vec<NoteId> = document
        .notes
        .iter()
        .map(|note| note.id)
        .filter(|id| !ctx.emitted_notes.contains(id))
        .collect();
    for id in orphan_ids {
        let Some(note) = document.note(id) else {
            continue;
        };
        let home = document
            .sections
            .iter()
            .position(|section| {
                (section.source_pages.0..=section.source_pages.1).contains(&note.page.index)
            })
            .unwrap_or(spines.len().saturating_sub(1));
        let body = note.body.clone();
        let piece = Piece {
            frag: sectioning(|el| el.aside(|inner| emit_flow_list(inner, &body, &mut ctx))),
            anchors: Vec::new(),
        };
        if let Some(spine) = spines.get_mut(home) {
            spine.pieces.push(piece);
        }
    }

    pack(spines, document, options, ctx.used_images)
}

/// Everything the emitter carries from one piece to the next.
struct Ctx<'a> {
    document: &'a Document,
    images: &'a BTreeMap<ImageId, String>,
    used_images: Vec<ImageId>,
    emitted_notes: BTreeSet<NoteId>,
    next_section: u32,
    /// Per heading, the id its element carries — what an internal link points at.
    heading_anchors: BTreeMap<BlockId, String>,
}

impl<'a> Ctx<'a> {
    /// A context for a fragment that cannot reference an image or a note: the body of a note
    /// that nothing referred to. Keeps the recursion total without letting a note's body open
    /// a second note.
    fn stub(document: &'a Document, images: &'a BTreeMap<ImageId, String>) -> Self {
        Self {
            document,
            images,
            used_images: Vec::new(),
            emitted_notes: BTreeSet::new(),
            next_section: u32::MAX,
            heading_anchors: BTreeMap::new(),
        }
    }

    fn take_section_number(&mut self) -> u32 {
        let n = self.next_section;
        self.next_section = self.next_section.saturating_add(1);
        n
    }
}

/// One spine document before it has been split into files.
struct Spine {
    /// The `epub:type` the first fragment carries.
    kind: Option<EpubType>,
    /// `sec3`, the id of the first fragment.
    id: String,
    /// The heading id, when the section has a heading in its own first fragment.
    heading_id: Option<String>,
    title: String,
    pieces: Vec<Piece>,
    nav: NavPlan,
}

/// One indivisible unit of a spine document.
struct Piece {
    frag: SectioningFrag,
    /// The ids this piece defines, so the nav can be told which file to point at.
    anchors: Vec<String>,
}

/// A nav entry before its href is known.
#[derive(Clone, Debug)]
struct NavPlan {
    title: String,
    /// The anchor to point at, or `None` to point at the file itself.
    anchor: Option<String>,
    children: Vec<NavPlan>,
}

/// Build one top-level section into an unsplit spine document.
fn build_spine(
    section: &Section,
    referenced: &BTreeSet<NoteId>,
    ctx: &mut Ctx<'_>,
) -> Result<Spine, xhtml::IllegalChar> {
    let number = ctx.take_section_number();
    let id = format!("sec{number}");
    let heading_id = section.heading.as_ref().map(|_| format!("sec{number}h"));
    let title = section
        .heading
        .as_ref()
        .map(|heading| heading.text())
        .unwrap_or_default();

    let mut pieces = Vec::new();
    let mut nav_children = Vec::new();

    // A leading run of page breaks belongs above the heading: the page starts with the
    // chapter title, so a citation to it must land before the title and not after
    // (`docs/DECISIONS_LOG.md`). `document` put them at the head of the content list for
    // exactly this lift.
    let leading = section
        .content
        .iter()
        .take_while(|item| matches!(item, Content::PageBreak(_)))
        .count();
    for item in section.content.iter().take(leading) {
        pieces.push(flow_piece(item, ctx)?);
    }

    if let (Some(heading), Some(heading_id)) = (&section.heading, &heading_id) {
        let level = heading.level;
        let spans = heading.spans.clone();
        let anchors = vec![heading_id.clone()];
        pieces.push(Piece {
            frag: sectioning(|el| el.heading(level, heading_id, |t| phrasing(t, &spans, ctx))),
            anchors,
        });
    }

    for item in section.content.iter().skip(leading) {
        pieces.push(flow_piece(item, ctx)?);
        for note in notes_of(item, referenced, ctx) {
            pieces.push(note_piece(&note, ctx)?);
        }
    }

    for child in &section.children {
        let (piece, plan) = child_section(child, referenced, ctx)?;
        pieces.push(piece);
        nav_children.push(plan);
    }

    for piece in &pieces {
        if let Some(error) = piece.frag.error_ref() {
            return Err(error.clone());
        }
    }

    Ok(Spine {
        kind: epub_type_of(section.role),
        nav: NavPlan {
            title: if title.is_empty() {
                untitled(section, ctx.document)
            } else {
                title.clone()
            },
            // A section with no heading still has to be reachable, and its own `<section>` id
            // is the anchor: pointing at the first file of the *book* instead — which is what
            // "no anchor" would mean — sends a reader to chapter one from every untitled
            // section after the first.
            anchor: heading_id.clone().or_else(|| Some(id.clone())),
            children: nav_children,
        },
        id,
        heading_id,
        title,
        pieces,
    })
}

/// A nested section, emitted whole as one piece.
///
/// Atomic on purpose: a `<section>` cannot be split across files and still be one section, and
/// a sub-section large enough to matter does not occur in a book whose chapters are the unit of
/// splitting. A file that comes out over the bound because of one says so, in
/// [`W_XHTML_OVERSIZE`].
fn child_section(
    section: &Section,
    referenced: &BTreeSet<NoteId>,
    ctx: &mut Ctx<'_>,
) -> Result<(Piece, NavPlan), xhtml::IllegalChar> {
    let inner = build_spine(section, referenced, ctx)?;
    let mut anchors = vec![inner.id.clone()];
    for piece in &inner.pieces {
        anchors.extend(piece.anchors.iter().cloned());
    }

    let kind = inner.kind;
    let id = inner.id.clone();
    let label = match &inner.heading_id {
        Some(heading) => SectionLabel::By(heading.clone()),
        None => SectionLabel::None,
    };
    let pieces = inner.pieces;
    let frag = sectioning(|el| {
        el.section(kind, &id, &label, |mut section| {
            for piece in &pieces {
                section = section.sectioning_frag(&piece.frag);
            }
            section
        })
    });

    Ok((Piece { frag, anchors }, inner.nav))
}

/// One flow item as its own piece.
fn flow_piece(item: &Content, ctx: &mut Ctx<'_>) -> Result<Piece, xhtml::IllegalChar> {
    let anchors = match item {
        Content::PageBreak(id) => vec![page_anchor(*id)],
        _ => Vec::new(),
    };
    let frag = sectioning(|el| emit_flow(el, item, ctx));
    Ok(Piece { frag, anchors })
}

/// One note body as its own piece, as the footnote half of the pop-up pattern.
fn note_piece(note: &Note, ctx: &mut Ctx<'_>) -> Result<Piece, xhtml::IllegalChar> {
    let id = note_anchor(note.id);
    let body = note.body.clone();
    let anchors = vec![id.clone()];
    let frag = sectioning(|el| el.aside_footnote(&id, |inner| emit_flow_list(inner, &body, ctx)));
    Ok(Piece { frag, anchors })
}

/// The notes a flow item refers to, in the order the item refers to them.
fn notes_of(item: &Content, referenced: &BTreeSet<NoteId>, ctx: &mut Ctx<'_>) -> Vec<Note> {
    let mut out = Vec::new();
    for id in noterefs_in(item) {
        if !referenced.contains(&id) || !ctx.emitted_notes.insert(id) {
            continue;
        }
        if let Some(note) = ctx.document.note(id) {
            out.push(note.clone());
        }
    }
    out
}

/// Every note id referred to from inside one flow item.
fn noterefs_in(item: &Content) -> Vec<NoteId> {
    oc_model::document::walk_content(std::slice::from_ref(item))
        .into_iter()
        .flat_map(|item| match item {
            Content::Paragraph(para) => para.spans.iter().filter_map(|s| s.noteref).collect(),
            Content::Heading(heading) => heading.spans.iter().filter_map(|s| s.noteref).collect(),
            Content::NoteRefAnchor(id) => vec![*id],
            Content::Verse(verse) => verse
                .stanzas
                .iter()
                .flatten()
                .flatten()
                .filter_map(|s| s.noteref)
                .collect(),
            _ => Vec::new(),
        })
        .collect()
}

/// Every note the flow actually points at.
fn referenced_notes(document: &Document) -> BTreeSet<NoteId> {
    let mut out = BTreeSet::new();
    for section in document.walk() {
        for item in &section.content {
            out.extend(noterefs_in(item));
        }
        if let Some(heading) = &section.heading {
            out.extend(heading.spans.iter().filter_map(|span| span.noteref));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Flow
// ---------------------------------------------------------------------------

fn emit_flow_list<C: FlowContext>(mut el: El<C>, items: &[Content], ctx: &mut Ctx<'_>) -> El<C> {
    for item in items {
        el = emit_flow(el, item, ctx);
    }
    el
}

/// One flow item.
///
/// Generic over the content model so that the same code serves a section's own flow, the
/// inside of a block quote and the body of a note. A heading is the one thing it cannot do
/// here — `<h1>` is sectioning content — so a heading that reached a flow context keeps its
/// text and loses its level, which is the honest failure: `book_structure` moves every heading
/// that opens a section into the section, so one arriving here is a heading inside a quotation.
fn emit_flow<C: FlowContext>(el: El<C>, item: &Content, ctx: &mut Ctx<'_>) -> El<C> {
    match item {
        Content::Paragraph(para) => {
            let spans = para.spans.clone();
            let drop_cap = para.drop_cap;
            el.p(|t| phrasing_with_drop_cap(t, &spans, drop_cap, ctx))
        }
        Content::Heading(heading) => {
            let spans = heading.spans.clone();
            el.p(|t| t.strong(|s| phrasing(s, &spans, ctx)))
        }
        Content::List(list) => {
            let items: Vec<FlowFrag> = list
                .items
                .iter()
                .map(|item| {
                    let content = item.content.clone();
                    let nested = item.nested.clone();
                    flow(|f| {
                        let f = emit_flow_list(f, &content, ctx);
                        match nested {
                            Some(nested) => emit_flow(f, &Content::List((*nested).clone()), ctx),
                            None => f,
                        }
                    })
                })
                .collect();
            let kind = if list.ordered {
                ListKind::Ordered
            } else {
                ListKind::Unordered
            };
            // The markers the book printed are still in the item text, because `structure` is
            // Conserving and so is this stage. A reading system that drew its own would number
            // every item twice (`docs/DECISIONS_LOG.md`).
            el.list(kind, list.start, true, items)
        }
        Content::BlockQuote(inner) => {
            let inner = inner.clone();
            el.blockquote(|f| emit_flow_list(f, &inner, ctx))
        }
        Content::Epigraph(inner) => {
            let inner = inner.clone();
            el.div_type(EpubType::Epigraph, |f| emit_flow_list(f, &inner, ctx))
        }
        Content::Verse(verse) => {
            let stanzas = verse.stanzas.clone();
            el.div_class(CssClass::Verse, |mut poem| {
                for stanza in &stanzas {
                    let stanza = stanza.clone();
                    poem = poem.p_class(CssClass::Stanza, |mut line_el| {
                        for (index, line) in stanza.iter().enumerate() {
                            if index > 0 {
                                line_el = line_el.br();
                            }
                            line_el = phrasing(line_el, line, ctx);
                        }
                        line_el
                    });
                }
                poem
            })
        }
        Content::Preformatted(pre) => el.pre(&pre.lines),
        Content::Rule => el.hr(),
        Content::Figure(id) => emit_figure(el, *id, ctx),
        Content::Table(id) => emit_table(el, *id, ctx),
        Content::NoteRefAnchor(id) => {
            let anchor = note_anchor(*id);
            let marker = ctx
                .document
                .note(*id)
                .map(|note| note.marker.clone())
                .unwrap_or_default();
            el.p(|t| t.noteref(&anchor, &marker))
        }
        Content::PageBreak(id) => {
            let label = ctx
                .document
                .page_break(*id)
                .map(|brk| page_label(brk.page.index, brk.page.label.as_deref()))
                .unwrap_or_default();
            el.pagebreak(&page_anchor(*id), &label)
        }
    }
}

/// A figure, with alt text that is never empty.
fn emit_figure<C: FlowContext>(el: El<C>, id: oc_model::ids::FigureId, ctx: &mut Ctx<'_>) -> El<C> {
    let Some(figure) = ctx.document.figure(id) else {
        return el;
    };
    let caption_spans = figure.caption.clone();
    let Some(href) = ctx
        .images
        .get(&figure.image)
        .map(|path| from_text_dir(path))
    else {
        // No file for the image — it was dropped as an ornament, or the backend could not
        // decode it. The caption is still text the book contained, so it stays: dropping it
        // would be the stage losing characters it cannot account for.
        return match caption_spans {
            Some(spans) => el.p_class(CssClass::Caption, |t| plain_phrasing(t, &spans)),
            None => el,
        };
    };
    let caption = figure
        .caption
        .as_ref()
        .map(|caption| oc_model::doc::spans_text(caption));
    let alt = alt_text(&figure.alt, caption.as_deref(), "Figure", id.0);
    let Some(image) = ImgRef::new(href, alt) else {
        return el;
    };
    if !ctx.used_images.contains(&figure.image) {
        ctx.used_images.push(figure.image);
    }
    let caption_frag = figure.caption.as_ref().map(|spans| {
        let spans = spans.clone();
        frag(|t| phrasing(t, &spans, &mut Ctx::stub(ctx.document, ctx.images)))
    });
    el.figure(&image, caption_frag.as_ref())
}

/// A table: a grid when the grid was trusted, and an image plus its text when it was not.
fn emit_table<C: FlowContext>(el: El<C>, id: oc_model::ids::TableId, ctx: &mut Ctx<'_>) -> El<C> {
    let Some(table) = ctx.document.table(id).cloned() else {
        return el;
    };
    let caption = table.caption.as_ref().map(|spans| {
        let spans = spans.clone();
        frag(|t| phrasing(t, &spans, &mut Ctx::stub(ctx.document, ctx.images)))
    });

    match table
        .fallback_image
        .and_then(|image| ctx.images.get(&image).cloned().map(|href| (image, href)))
    {
        Some((image_id, href)) => {
            let label = table
                .caption
                .as_ref()
                .map(|spans| oc_model::doc::spans_text(spans))
                .unwrap_or_default();
            let alt = alt_text(&label, None, "Table", id.0);
            let Some(image) = ImgRef::new(from_text_dir(&href), alt) else {
                return el;
            };
            if !ctx.used_images.contains(&image_id) {
                ctx.used_images.push(image_id);
            }
            // The image *and* the text. An image of a table takes the content away from
            // anyone who cannot see it, which DAISY names as a failure in its own right
            // (R10 §6.12), and the extracted rows are what give it back.
            let fallback_label = format!("Table {}", id.0 + 1);
            el.figure(&image, caption.as_ref()).details(
                &fallback_label,
                CssClass::TableFallback,
                |body| grid(body, &table),
            )
        }
        None => {
            let rows = cells(&table);
            el.table(caption.as_ref(), rows)
        }
    }
}

/// The rows of a table, as a grid.
fn grid(el: El<Flow>, table: &Table) -> El<Flow> {
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| {
                    let spans = cell.spans.clone();
                    TableCell {
                        content: frag(|t| plain_phrasing(t, &spans)),
                        colspan: cell.colspan,
                        rowspan: cell.rowspan,
                        header: cell.header,
                    }
                })
                .collect()
        })
        .collect();
    el.table(None, rows)
}

fn cells(table: &Table) -> Vec<Vec<TableCell>> {
    table
        .rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            row.iter()
                .map(|cell| {
                    let spans = cell.spans.clone();
                    TableCell {
                        content: frag(|t| plain_phrasing(t, &spans)),
                        colspan: cell.colspan,
                        rowspan: cell.rowspan,
                        // A row inside the header band is a header row whether or not the
                        // individual cell said so: styling a `<td>` to look like a header is
                        // named as a common bad practice by DAISY (R10 §6.12).
                        header: cell.header || index < usize::from(table.header_rows),
                    }
                })
                .collect()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Phrasing
// ---------------------------------------------------------------------------

/// A run of spans, with their styles and their note references.
fn phrasing(el: El<xhtml::Phrasing>, spans: &[Span], ctx: &mut Ctx<'_>) -> El<xhtml::Phrasing> {
    phrasing_with_drop_cap(el, spans, false, ctx)
}

fn phrasing_with_drop_cap(
    mut el: El<xhtml::Phrasing>,
    spans: &[Span],
    drop_cap: bool,
    ctx: &mut Ctx<'_>,
) -> El<xhtml::Phrasing> {
    let mut first = drop_cap;
    for span in spans {
        if span.text.is_empty() {
            continue;
        }
        // The initial the book set large. Styled, never positioned: a drop cap is a flourish
        // and on a four-inch screen a positioned one is a liability (R5 §A8).
        let (initial, rest) = if first {
            first = false;
            let mut chars = span.text.chars();
            match chars.next() {
                Some(ch) => (Some(ch.to_string()), chars.as_str().to_owned()),
                None => (None, span.text.clone()),
            }
        } else {
            (None, span.text.clone())
        };
        if let Some(initial) = initial {
            el = el.span_class(CssClass::DropCap, |t| t.text(&initial));
        }
        if rest.is_empty() {
            continue;
        }
        let styled = Span {
            text: rest,
            ..span.clone()
        };
        el = one_span(el, &styled, ctx);
    }
    el
}

/// A run of spans with their styles but no note references: a table cell, a caption.
fn plain_phrasing(mut el: El<xhtml::Phrasing>, spans: &[Span]) -> El<xhtml::Phrasing> {
    for span in spans {
        if span.text.is_empty() {
            continue;
        }
        el = styled(el, span, |t, text| t.text(text));
    }
    el
}

fn one_span(el: El<xhtml::Phrasing>, span: &Span, ctx: &mut Ctx<'_>) -> El<xhtml::Phrasing> {
    match span.noteref.filter(|id| ctx.document.note(*id).is_some()) {
        // The reference half of Apple's pop-up footnote pattern, paired with the
        // `<aside epub:type="footnote">` the flow emits after this paragraph (R5 §A5).
        Some(id) => {
            let anchor = note_anchor(id);
            let marker = span.text.clone();
            el.noteref(&anchor, &marker)
        }
        None => match span.link.as_ref().and_then(|link| match link {
            oc_model::doc::LinkTarget::Internal(block) => ctx.heading_anchors.get(block),
            oc_model::doc::LinkTarget::External(_) => None,
        }) {
            // A link to a heading: written with a placeholder href, because which file the
            // heading lands in is only known once the book is split into files; `pack`
            // resolves it.
            Some(anchor) => {
                let href = format!("{LINK_PLACEHOLDER}{anchor}");
                el.link(&href, |a| styled(a, span, |t, text| t.text(text)))
            }
            None => styled(el, span, |t, text| t.text(text)),
        },
    }
}

/// The href prefix an internal link is written with until `pack` knows its file.
const LINK_PLACEHOLDER: &str = "oc-link:";

/// Per heading, the id `build_spine` gives its element: `sec{n}h`, `n` numbering the sections
/// in the pre-order `build_spine` takes them in, which is `Section::walk`'s.
fn heading_anchors(document: &Document) -> BTreeMap<BlockId, String> {
    document
        .walk()
        .iter()
        .enumerate()
        .filter_map(|(number, section)| {
            let heading = section.heading.as_ref()?;
            Some((heading.id, format!("sec{number}h")))
        })
        .collect()
}

/// Rewrite every placeholder link in the files to the file and anchor it points at. A link to
/// an anchor no file carries is left pointing at the file's own top, which is a dead end but
/// never a broken reference.
fn resolve_links(files: &mut [XhtmlFile], anchors: &BTreeMap<String, String>) {
    let needle = format!("href=\"{LINK_PLACEHOLDER}");
    for file in files.iter_mut() {
        if !file.markup.contains(&needle) {
            continue;
        }
        let mut out = String::with_capacity(file.markup.len());
        let mut rest = file.markup.as_str();
        while let Some(at) = rest.find(&needle) {
            out.push_str(&rest[..at]);
            let after = &rest[at + needle.len()..];
            let end = after.find('"').unwrap_or(after.len());
            let anchor = &after[..end];
            let href = anchors
                .get(anchor)
                .map(|path| {
                    let name = path.rsplit('/').next().unwrap_or(path);
                    format!("{name}#{anchor}")
                })
                .unwrap_or_else(|| format!("#{anchor}"));
            out.push_str("href=\"");
            out.push_str(&href);
            rest = &after[end..];
        }
        out.push_str(rest);
        file.markup = out;
    }
}

/// Wrap one span's text in the elements its style calls for, innermost last.
fn styled<C: xhtml::PhrasingContext>(
    el: El<C>,
    span: &Span,
    write: impl FnOnce(El<C>, &str) -> El<C>,
) -> El<C> {
    let text = span.text.clone();
    let style = span.style;
    let inner = move |t: El<C>| write(t, &text);

    // Nested in a fixed order so that two spans with the same style always serialise to the
    // same bytes — which is half of what makes two builds of one book byte-identical.
    let inner = move |t: El<C>| {
        if style.smallcaps {
            t.span_class(CssClass::SmallCaps, inner)
        } else {
            inner(t)
        }
    };
    let inner = move |t: El<C>| {
        if style.monospace {
            t.code(inner)
        } else {
            inner(t)
        }
    };
    let inner = move |t: El<C>| {
        if style.subscript {
            t.subscript(inner)
        } else {
            inner(t)
        }
    };
    let inner = move |t: El<C>| {
        if style.superscript {
            t.superscript(inner)
        } else {
            inner(t)
        }
    };
    let inner = move |t: El<C>| {
        if style.italic {
            t.em(inner)
        } else {
            inner(t)
        }
    };
    if style.bold {
        el.strong(inner)
    } else {
        inner(el)
    }
}

// ---------------------------------------------------------------------------
// Splitting
// ---------------------------------------------------------------------------

/// Pack each spine document's pieces into files no larger than `split_bytes`.
fn pack(
    spines: Vec<Spine>,
    document: &Document,
    options: &EmitOptions,
    used_images: Vec<ImageId>,
) -> Result<Emitted, xhtml::IllegalChar> {
    let mut files = Vec::new();
    let mut anchors: BTreeMap<String, String> = BTreeMap::new();
    let mut nav_plans = Vec::new();

    for spine in spines {
        let mut batches: Vec<Vec<Piece>> = Vec::new();
        let mut current: Vec<Piece> = Vec::new();
        let mut size = 0usize;

        for piece in spine.pieces {
            let cost = piece.frag.len();
            // The bound is on the *file*, and a file is its pieces plus a wrapper. Flushing
            // when the next piece would cross it — rather than after it has — is what keeps
            // the bound a bound instead of a suggestion.
            if !current.is_empty() && size + cost > options.split_bytes {
                batches.push(std::mem::take(&mut current));
                size = 0;
            }
            size += cost;
            current.push(piece);
        }
        if !current.is_empty() || batches.is_empty() {
            batches.push(current);
        }

        let count = batches.len();
        for (index, batch) in batches.into_iter().enumerate() {
            let path = format!("{TEXT_DIR}/c{:04}.xhtml", files.len() + 1);
            for piece in &batch {
                for anchor in &piece.anchors {
                    anchors.insert(anchor.clone(), path.clone());
                }
            }

            let id = if index == 0 {
                spine.id.clone()
            } else {
                format!("{}-{}", spine.id, index + 1)
            };
            // The `<section>` wrapper's own id is defined in this file, so the nav may point
            // at it. Registered here rather than on a piece because `pack` is what creates it.
            anchors.insert(id.clone(), path.clone());
            // The first fragment carries the semantics and points at its own heading; the rest
            // repeat the heading's text, because `aria-labelledby` may only reference an
            // element in the same document (IMPLEMENTATION_PLAN Phase 5 detail 6).
            let label = match (index, &spine.heading_id) {
                (0, Some(heading)) => SectionLabel::By(heading.clone()),
                (_, Some(_)) if !spine.title.is_empty() => SectionLabel::Text(spine.title.clone()),
                _ => SectionLabel::None,
            };
            let kind = if index == 0 { spine.kind } else { None };

            let title = if spine.title.is_empty() {
                document.meta.title.clone().unwrap_or_default()
            } else {
                spine.title.clone()
            };
            let markup =
                xhtml::content_document(&title, &document.language, "../style.css", |body| {
                    body.section(kind, &id, &label, |mut section| {
                        for piece in &batch {
                            section = section.sectioning_frag(&piece.frag);
                        }
                        section
                    })
                })?;

            files.push(XhtmlFile {
                path,
                title,
                markup,
            });
            let _ = count;
        }

        nav_plans.push(spine.nav);
    }

    // The pages before the first heading are one entry of the contents, named by the book's
    // title: typed as a title page, a copyright page and a dedication, each would otherwise be
    // listed under the book's name, three identical lines at the top of every book's contents.
    // They stay in the spine and in the landmarks.
    let mut front_listed = false;
    let toc = document
        .sections
        .iter()
        .zip(&nav_plans)
        .filter(|(section, _)| {
            let untitled_front =
                section.heading.is_none() && matches!(section.role, SectionRole::FrontMatter(_));
            !untitled_front || !std::mem::replace(&mut front_listed, true)
        })
        .filter_map(|(_, plan)| resolve(plan, &anchors, &files))
        .collect();
    let page_list = page_targets(document, &anchors);
    let landmarks = landmarks(document, &nav_plans, &anchors, &files);
    resolve_links(&mut files, &anchors);

    Ok(Emitted {
        files,
        toc,
        page_list,
        landmarks,
        used_images,
    })
}

/// Turn a nav plan into a nav point, once it is known which file holds its anchor.
fn resolve(
    plan: &NavPlan,
    anchors: &BTreeMap<String, String>,
    files: &[XhtmlFile],
) -> Option<NavPoint> {
    let href = match &plan.anchor {
        Some(anchor) => anchors.get(anchor).map(|file| format!("{file}#{anchor}"))?,
        // A section with no heading still needs somewhere to point, and the first file is
        // where it starts.
        None => files.first().map(|file| file.path.clone())?,
    };
    Some(NavPoint {
        title: plan.title.clone(),
        href,
        children: plan
            .children
            .iter()
            .filter_map(|child| resolve(child, anchors, files))
            .collect(),
    })
}

/// The page list: every page break, with the folio the book printed.
fn page_targets(document: &Document, anchors: &BTreeMap<String, String>) -> Vec<PageTarget> {
    document
        .page_breaks
        .iter()
        .filter_map(|brk| {
            let anchor = page_anchor(brk.id);
            let file = anchors.get(&anchor)?;
            Some(PageTarget {
                label: page_label(brk.page.index, brk.page.label.as_deref()),
                href: format!("{file}#{anchor}"),
            })
        })
        .collect()
}

/// The landmarks nav: where the body starts, and the named parts a reader jumps to.
fn landmarks(
    document: &Document,
    plans: &[NavPlan],
    anchors: &BTreeMap<String, String>,
    files: &[XhtmlFile],
) -> Vec<Landmark> {
    document
        .sections
        .iter()
        .zip(plans)
        .filter_map(|(section, plan)| {
            let kind = match section.role {
                SectionRole::Chapter | SectionRole::Part => "bodymatter",
                SectionRole::FrontMatter(oc_model::doc::FrontMatterKind::TableOfContents) => "toc",
                _ => return None,
            };
            let point = resolve(plan, anchors, files)?;
            Some(Landmark {
                kind,
                title: point.title,
                href: point.href,
            })
        })
        // Only the first bodymatter landmark: `landmarks` names where a thing *starts*, and a
        // list with one entry per chapter is a table of contents wearing the wrong hat.
        .scan(BTreeSet::new(), |seen, landmark| {
            Some(seen.insert(landmark.kind).then_some(landmark))
        })
        .flatten()
        .collect()
}

/// Where the cover page lives inside the container.
pub const COVER_PATH: &str = "text/cover.xhtml";

/// Put the cover page first in the book: its own content document, first in the spine, and
/// the first landmark.
///
/// A picture and nothing else. Its alt text is the book's title, because that is what a cover
/// says; the title is metadata, so the page adds nothing to `C`.
pub fn prepend_cover(
    emitted: &mut Emitted,
    document: &Document,
    title: &str,
    image_path: &str,
) -> Result<(), xhtml::IllegalChar> {
    let alt = if title.trim().is_empty() {
        "Cover".to_owned()
    } else {
        title.to_owned()
    };
    let Some(image) = crate::xhtml::ImgRef::new(from_text_dir(image_path), alt.clone()) else {
        return Ok(());
    };
    let markup = xhtml::content_document(&alt, &document.language, "../style.css", |body| {
        body.section(
            Some(EpubType::Cover),
            "cover",
            &SectionLabel::None,
            |section| section.figure(&image, None),
        )
    })?;
    emitted.files.insert(
        0,
        XhtmlFile {
            path: COVER_PATH.to_owned(),
            title: alt,
            markup,
        },
    );
    emitted.landmarks.insert(
        0,
        Landmark {
            kind: "cover",
            title: "Cover".to_owned(),
            href: COVER_PATH.to_owned(),
        },
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Names and labels
// ---------------------------------------------------------------------------

/// A package-root-relative path, as a content document has to spell it.
///
/// Every content document lives in `text/`, so `images/i0001.jpg` in the manifest is
/// `../images/i0001.jpg` in the markup. Getting this wrong is EPUBCheck's `RSC-007` — the
/// reader looks for `text/images/i0001.jpg`, finds nothing, and shows a broken figure.
fn from_text_dir(path: &str) -> String {
    format!("../{path}")
}

fn note_anchor(id: NoteId) -> String {
    format!("fn{}", id.0)
}

fn page_anchor(id: PageBreakId) -> String {
    format!("page{}", id.0)
}

/// What a page-list entry is called.
///
/// The printed folio when `furniture` recovered one. When it did not, the one-based page index
/// — which is what a reader looking at the PDF would call it, and is at least a number that
/// counts the pages of the book rather than a fabricated folio.
fn page_label(index: u32, printed: Option<&str>) -> String {
    match printed.map(str::trim).filter(|label| !label.is_empty()) {
        Some(label) => label.to_owned(),
        None => (index.saturating_add(1)).to_string(),
    }
}

/// Alt text for an image, which Tier 1 requires to be non-empty (the ACC-001 class, D6).
///
/// The derived text, then the caption, then a positional name. The last is deliberately not a
/// description: R1 §A.10 found that 95 % of real alt text is the literal word "Image", and
/// inventing a description would be worse than admitting to none. `Figure 4` at least tells a
/// reader who cannot see it *where in the book they are*, which is the honest minimum. It is an
/// attribute and therefore outside `C` (ARCHITECTURE §5.2), so saying it costs the conservation
/// law nothing.
fn alt_text(derived: &str, caption: Option<&str>, kind: &str, number: u32) -> String {
    for candidate in [Some(derived), caption] {
        if let Some(text) = candidate.map(str::trim).filter(|text| !text.is_empty()) {
            return text.to_owned();
        }
    }
    format!("{kind} {}", number.saturating_add(1))
}

fn epub_type_of(role: SectionRole) -> Option<EpubType> {
    use oc_model::doc::FrontMatterKind;
    match role {
        SectionRole::FrontMatter(kind) => Some(match kind {
            FrontMatterKind::TitlePage => EpubType::Titlepage,
            FrontMatterKind::HalfTitle => EpubType::Halftitlepage,
            FrontMatterKind::Copyright => EpubType::CopyrightPage,
            FrontMatterKind::Dedication => EpubType::Dedication,
            FrontMatterKind::Epigraph => EpubType::Epigraph,
            FrontMatterKind::Foreword => EpubType::Foreword,
            FrontMatterKind::Preface => EpubType::Preface,
            FrontMatterKind::Introduction => EpubType::Introduction,
            // A printed contents page keeps `frontmatter`: the book's `toc` is the nav.
            FrontMatterKind::TableOfContents | FrontMatterKind::Other => EpubType::Frontmatter,
        }),
        SectionRole::Part => Some(EpubType::Part),
        SectionRole::Chapter => Some(EpubType::Chapter),
        SectionRole::BackMatter(_) => Some(EpubType::Backmatter),
        SectionRole::Section => None,
    }
}

/// What a section with no heading is called in the nav: its own first words, in the book's
/// own language, the way a reading system names an untitled chapter — or, for the pages before
/// the first chapter, the book's title. The role's name is the last resort, for a section
/// with no text at all. Nav text is outside `C`.
fn untitled(section: &Section, document: &Document) -> String {
    const WORDS: usize = 6;
    if matches!(section.role, SectionRole::FrontMatter(_)) {
        if let Some(title) = document
            .meta
            .title
            .as_ref()
            .filter(|title| !title.trim().is_empty())
        {
            return title.trim().to_owned();
        }
    }
    let first = section.content.iter().find_map(|item| match item {
        Content::Paragraph(para) if !para.text.trim().is_empty() => Some(para.text.clone()),
        _ => None,
    });
    if let Some(text) = first {
        let words: Vec<&str> = text.split_whitespace().collect();
        let mut label = words
            .iter()
            .take(WORDS)
            .copied()
            .collect::<Vec<_>>()
            .join(" ");
        if words.len() > WORDS {
            label.push('\u{2026}');
        }
        return label;
    }
    role_title(section.role).to_owned()
}

/// What a section with no heading is called in the nav.
///
/// Its role, because a nav entry has to say something and the role is the only thing known
/// about a section the book did not title. Nav text is outside `C` (ARCHITECTURE §5.2).
fn role_title(role: SectionRole) -> &'static str {
    match role {
        SectionRole::FrontMatter(_) => "Front matter",
        SectionRole::Part => "Part",
        SectionRole::Chapter => "Chapter",
        SectionRole::Section => "Section",
        SectionRole::BackMatter(_) => "Back matter",
    }
}
