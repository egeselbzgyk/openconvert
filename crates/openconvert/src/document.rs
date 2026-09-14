//! The `document` stage: the assembled book (PIPELINE §9).
//!
//! `structure` produced everything but the document — sections, notes, figures, tables,
//! metadata. Three things are still missing when it finishes, and all three are properties of
//! the *book* rather than of any block, which is why they are a stage of their own:
//!
//! 1. **Page breaks.** `furniture` removed the printed page numbers and recovered them as
//!    labels; nothing has yet put them back as places in the flow. A `PageBreak` is what
//!    becomes `epub:type="pagebreak"` and a `page-list` entry, and it is how a reflowed book
//!    keeps citable print pagination — the differentiator R1 §C.4 #7 names and Calibre does
//!    not have.
//! 2. **The classification and the preset.** Both are once-per-book choices, recorded as
//!    [`Decision`]s so the report can say what was chosen and against what (D13.10, D13.11).
//! 3. **Closure.** Every cross-reference in the flow has to name something the document
//!    carries, because after this stage the next thing that reads them writes bytes.
//!
//! The stage is **Conserving** and adds no text: a page break carries a *label*, and a label
//! is outside `C` (ARCHITECTURE §5.2).

use std::collections::BTreeMap;

use oc_core::stages;
use oc_core::thresholds::Thresholds;
use oc_model::confidence::Confidence;
use oc_model::decision::Decision;
use oc_model::doc::{Content, Figure, PageBreak, Section, SectionRole, Severity, Warning};
use oc_model::document::{first_block, DocClass, Document, PresetName};
use oc_model::extract::{ImageRef, PageRef};
use oc_model::ids::{BlockId, PageBreakId};
use oc_model::lang::LangTag;
use oc_model::ledger::Ledger;
use oc_pdf::classify::PageClass;
use oc_structure::stage::StructureOutput;

/// The decision kinds this stage records, named as the report prints them.
pub const DECISION_DOCUMENT_CLASS: &str = "document_class";
pub const DECISION_PRESET: &str = "preset";

/// A page the book ends on that no page break could be attached to.
///
/// Emitted rather than passed over in silence: a `page-list` missing an entry is a citation
/// that does not resolve, and the cause — a page whose every block was taken by a table, or a
/// page of nothing but an image — is worth naming in the report.
pub const W_PAGE_BREAK_UNPLACED: &str = "W_PAGE_BREAK_UNPLACED";

/// The document yielded no text at all, and its pages are carried as images.
///
/// A scan with no OCR available is the case: PIPELINE §10 says such a page "becomes an image
/// inside a `<figure>` … rather than being dropped silently", and this is the warning that says
/// so in the report. A book with no sections would otherwise reach `epub` as an empty spine,
/// which is not a valid publication and is not a book either.
pub const W_NO_TEXT_EXTRACTED: &str = "W_NO_TEXT_EXTRACTED";

/// Everything the stage reads beyond what `structure` produced.
pub struct DocumentInput<'a> {
    pub source_sha256: &'a str,
    pub structure: &'a StructureOutput,
    /// The printed page label per page, as `furniture` recovered it.
    pub labels: &'a [Option<String>],
    /// Per page, what `inspect` classified it as.
    pub classes: &'a [PageClass],
    /// Per page, whether it is wider than it is tall.
    pub landscape: &'a [bool],
    /// Per page, how many columns `layout` laid it out in.
    pub column_counts: &'a [usize],
    /// Which page each block was printed on.
    pub block_pages: &'a BTreeMap<BlockId, u32>,
    /// Every image in the document, in page order. Read only when the book yielded no text,
    /// and then to carry the pages as figures rather than as nothing.
    pub images: &'a [ImageRef],
    pub language: LangTag,
    /// The preset as configuration asked for it, before [`PresetName::resolve`].
    pub preset: PresetName,
    pub ledger: Ledger,
}

/// Assemble the document.
pub fn assemble(input: DocumentInput<'_>, t: &Thresholds) -> Document {
    let mut sections = input.structure.sections.clone();
    let mut warnings = input.structure.warnings.clone();
    let mut figures = input.structure.figures.clone();

    // A document that yielded no text is not an empty book: its pages are pictures, and a
    // picture of a page is what a reader has (PIPELINE §10). Emitted here rather than in
    // `epub`, because it is a statement about the *document* — a section that exists, holding
    // figures that exist — and `epub` serialises documents rather than inventing them.
    if sections.is_empty() {
        let (fallback, fallback_figures) = pages_as_figures(input.images, &figures);
        if !fallback.content.is_empty() {
            figures.extend(fallback_figures);
            sections.push(fallback);
        }
        warnings.push(Warning::new(W_NO_TEXT_EXTRACTED, Severity::Warn));
    }

    let page_breaks = insert_page_breaks(&mut sections, input.block_pages, input.labels);
    let placed: std::collections::BTreeSet<u32> =
        page_breaks.iter().map(|brk| brk.page.index).collect();
    for page in 0..u32::try_from(input.labels.len()).unwrap_or(0) {
        if !placed.contains(&page) {
            warnings.push(
                Warning::new(W_PAGE_BREAK_UNPLACED, oc_model::doc::Severity::Info)
                    .with_page(page_ref(page, input.labels)),
            );
        }
    }

    let classification = classify(input.classes, input.landscape, input.column_counts, t);
    let presets = input.preset.resolve(classification);
    let decisions = vec![
        Decision::deterministic(
            stages::DOCUMENT.name,
            DECISION_DOCUMENT_CLASS,
            classification.as_str(),
        )
        .against(
            [
                DocClass::BookProse,
                DocClass::AcademicMulticolumn,
                DocClass::Scanned,
                DocClass::SlidesOther,
            ]
            .iter()
            .filter(|candidate| **candidate != classification)
            .map(|candidate| candidate.as_str().to_owned())
            .collect(),
        ),
        Decision::deterministic(stages::DOCUMENT.name, DECISION_PRESET, presets.as_str())
            .against(vec![input.preset.as_str().to_owned()]),
    ];

    Document {
        ir_version: oc_model::IR_VERSION,
        source_sha256: input.source_sha256.to_owned(),
        meta: input.structure.metadata.clone(),
        language: input.language,
        sections,
        notes: input.structure.notes.clone(),
        figures,
        tables: input.structure.tables.clone(),
        page_breaks,
        ledger: input.ledger,
        decisions,
        warnings,
        classification,
        presets,
    }
}

/// One section holding every image the book draws, as figures.
///
/// The alt text is left empty on purpose: `epub` supplies a positional name — `Figure 3` — and
/// inventing a description here would be a claim about a picture nobody has looked at
/// (R1 §A.10). Alt text is an attribute and therefore outside `C`, so saying it there costs the
/// conservation law nothing and saying it here would gain nothing.
fn pages_as_figures(images: &[ImageRef], existing: &[Figure]) -> (Section, Vec<Figure>) {
    let anchor = BlockId::derive(
        0,
        oc_model::geom::Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 0.0,
            y1: 0.0,
        },
        "page images",
    );
    let mut figures = Vec::new();
    let mut content = Vec::new();
    let next = existing
        .iter()
        .map(|figure| figure.id.0.saturating_add(1))
        .max()
        .unwrap_or(0);

    for (offset, image) in images.iter().enumerate() {
        let id = oc_model::ids::FigureId(next.saturating_add(u32::try_from(offset).unwrap_or(0)));
        figures.push(Figure {
            id,
            image: image.id,
            caption: None,
            alt: String::new(),
            anchor,
            confidence: Confidence::deterministic(Vec::new()),
        });
        content.push(Content::Figure(id));
    }

    (
        Section {
            id: anchor,
            role: SectionRole::Chapter,
            level: 1,
            heading: None,
            content,
            children: Vec::new(),
            source_pages: (
                0,
                u32::try_from(images.len().saturating_sub(1)).unwrap_or(0),
            ),
            confidence: Confidence::deterministic(Vec::new()),
        },
        figures,
    )
}

/// What kind of book this is (D13.10).
///
/// Tested in the order the classes exclude one another rather than in enum order. A scan is a
/// scan whatever shape its pages are, because nothing else the pipeline measured is
/// trustworthy on a page it could not read; a deck is decided next, because a landscape page
/// of large type reaches the column detector as one wide column and would otherwise be prose;
/// and only then does the column count get a say.
pub fn classify(
    classes: &[PageClass],
    landscape: &[bool],
    column_counts: &[usize],
    t: &Thresholds,
) -> DocClass {
    let pages = classes.len().max(landscape.len()).max(column_counts.len());
    if pages == 0 {
        return DocClass::BookProse;
    }
    let share = |count: usize| count as f32 / pages as f32;

    let unreadable = classes
        .iter()
        .filter(|class| {
            matches!(
                class,
                PageClass::ImageOnly | PageClass::OcrSandwich | PageClass::BrokenText
            )
        })
        .count();
    if share(unreadable) >= t.docclass.scanned_page_share as f32 {
        return DocClass::Scanned;
    }

    let wide = landscape.iter().filter(|wide| **wide).count();
    if share(wide) >= t.docclass.landscape_page_share as f32 {
        return DocClass::SlidesOther;
    }

    let multi = column_counts.iter().filter(|count| **count >= 2).count();
    if share(multi) >= t.docclass.multicolumn_page_share as f32 {
        return DocClass::AcademicMulticolumn;
    }

    DocClass::BookProse
}

/// Insert one `Content::PageBreak` at every place the flow crosses onto a new source page,
/// and return the breaks in document order.
///
/// The anchor is the first item of the *flow* printed on the page, not the first block of the
/// page: a block a table or a note took is not in the flow, and a page break pointing at one
/// would be a `page-list` target that resolves to nothing. Attaching to the next item that is
/// text puts the marker where a reader would see the boundary anyway.
///
/// A section's heading is the first item of that section's flow even though it is not in its
/// content list, so a chapter that opens a page gets its break before the heading rather than
/// after it — otherwise "go to page 57" lands past the title of the chapter that starts there.
/// The break is recorded at the head of the content list and `epub` hoists a leading run of
/// page breaks above the heading, which is the same rule stated once at the other end.
fn insert_page_breaks(
    sections: &mut [Section],
    block_pages: &BTreeMap<BlockId, u32>,
    labels: &[Option<String>],
) -> Vec<PageBreak> {
    let mut breaks = Vec::new();
    let mut current: Option<u32> = None;
    walk_sections(sections, &mut |section| {
        let mut out: Vec<Content> = Vec::with_capacity(section.content.len() + 1);

        // The heading first, and into the same list: `epub` lifts it back above the heading.
        if let Some(heading) = &section.heading {
            if let Some(id) = open_page(
                block_pages.get(&heading.id).copied(),
                heading.id,
                &mut current,
                labels,
                &mut breaks,
            ) {
                out.push(Content::PageBreak(id));
            }
        }

        for item in section.content.drain(..) {
            let block = first_block(&item);
            if let Some(block) = block {
                if let Some(id) = open_page(
                    block_pages.get(&block).copied(),
                    block,
                    &mut current,
                    labels,
                    &mut breaks,
                ) {
                    out.push(Content::PageBreak(id));
                }
            }
            out.push(item);
        }
        section.content = out;
    });
    breaks
}

/// Record a page break when this block is the first of the flow on a page not yet opened.
fn open_page(
    page: Option<u32>,
    block: BlockId,
    current: &mut Option<u32>,
    labels: &[Option<String>],
    breaks: &mut Vec<PageBreak>,
) -> Option<PageBreakId> {
    let page = page?;
    if *current == Some(page) {
        return None;
    }
    *current = Some(page);
    let id = PageBreakId(u32::try_from(breaks.len()).unwrap_or(u32::MAX));
    breaks.push(PageBreak {
        id,
        page: page_ref(page, labels),
        before_block: block,
    });
    Some(id)
}

/// Apply a function to every section, depth first, in document order.
fn walk_sections(sections: &mut [Section], f: &mut impl FnMut(&mut Section)) {
    for section in sections {
        f(section);
        walk_sections(&mut section.children, f);
    }
}

/// The page, with the printed label `furniture` recovered for it.
fn page_ref(index: u32, labels: &[Option<String>]) -> PageRef {
    PageRef {
        index,
        label: labels
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .cloned()
            .flatten(),
    }
}
