//! The `structure` stage: every rule in PIPELINE §8, run in order, over one document.
//!
//! **Conserving, and checked as such.** Every operation in this stage is a label — this block
//! is a heading, those two are a list, that small block at the foot of the page is a note —
//! and a label adds and removes nothing. I-3 therefore reduces to plain multiset equality
//! across the stage, and the check is not an assertion in a test: it runs on every conversion
//! (D13.4).
//!
//! That constraint is what shapes the code. Every block's text lands in **exactly one** place:
//! a section's content, a note's body, a table's cells, or a figure's caption. A block
//! consumed by a table is not also a paragraph; a caption bound to a figure does not also sit
//! in the flow. The list marker stays inside its item's text for the same reason — `Reason`
//! is a closed set of fifteen variants and none of them is "a list marker", so a stage that
//! dropped one would be removing text it cannot account for.

use oc_core::thresholds::Thresholds;
use oc_model::confidence::{Confidence, Signal};
use oc_model::doc::{Content, Figure, List, Metadata, Note, Section, Table, Warning};
use oc_model::extract::{FontInfo, ImageRef, OutlineEntry, VectorRegion};
use oc_model::ids::BlockId;
use oc_model::lang::LangTag;
use oc_model::text::Run;

use crate::book::{book_structure, FlowItem};
use crate::build::{para_of, Minter};
use crate::figures::associate_captions;
use crate::headings::candidate::heading_candidates;
use crate::headings::cluster::{cluster_styles, StyleInventory};
use crate::headings::levels::{assign_levels, HeadingAssignment};
use crate::headings::runin::{run_in_candidates, RunInCandidate};
use crate::headings::toc_page::{parse_toc_page, TocPage};
use crate::images::{drop_ornaments, ImagePolicy};
use crate::lists::detect_lists;
use crate::meta::{metadata, MetaSources};
use crate::notes::{link_notes, NoteLinkStats, NoteRef};
use crate::quotes::{classify_indented, Classified, EscalationCandidate};
use crate::tables::{extract_tables, TableRegion};
use crate::view::BlockView;

/// Everything the stage reads.
pub struct StructureInput {
    pub blocks: Vec<BlockView>,
    /// The runs of the body flow, for the style histogram.
    pub runs: Vec<Run>,
    pub fonts: Vec<FontInfo>,
    pub images: Vec<ImageRef>,
    /// Per image, its perceptual hash. Decoding is the backend's.
    pub image_hashes: Vec<u64>,
    pub vectors: Vec<VectorRegion>,
    pub outline: Vec<OutlineEntry>,
    /// The printed page label per page, as `furniture` recovered it.
    pub labels: Vec<Option<String>>,
    pub drop_caps: Vec<oc_layout::anchor::DropCap>,
    pub page_count: u32,
    pub meta: MetaSources,
    pub lang: LangTag,
}

/// What the stage produced.
pub struct StructureOutput {
    pub inventory: StyleInventory,
    pub toc: Option<TocPage>,
    pub headings: Vec<HeadingAssignment>,
    pub run_ins: Vec<RunInCandidate>,
    pub sections: Vec<Section>,
    pub notes: Vec<Note>,
    pub note_refs: Vec<NoteRef>,
    pub note_stats: NoteLinkStats,
    pub figures: Vec<Figure>,
    pub tables: Vec<Table>,
    pub table_regions: Vec<TableRegion>,
    pub lists: Vec<List>,
    pub indented: Vec<Classified>,
    pub images: ImagePolicy,
    pub metadata: Metadata,
    pub escalations: Vec<EscalationCandidate>,
    pub warnings: Vec<Warning>,
    pub confidence: Confidence,
}

impl StructureOutput {
    /// Every piece of text the stage emitted, for the conservation check.
    ///
    /// Four places, and a block's text is in exactly one of them. The check that follows is
    /// what makes that sentence true rather than intended.
    pub fn emitted_text(&self) -> Vec<String> {
        let mut out = Vec::new();
        for section in &self.sections {
            for section in section.walk() {
                if let Some(heading) = &section.heading {
                    out.push(heading.text());
                }
                collect(&section.content, &mut out);
            }
        }
        for note in &self.notes {
            collect(&note.body, &mut out);
        }
        for table in &self.tables {
            out.extend(table.cell_texts());
        }
        for figure in &self.figures {
            if let Some(caption) = &figure.caption {
                out.push(oc_model::doc::spans_text(caption));
            }
        }
        out
    }
}

/// Walk a content list, collecting every piece of text it holds.
fn collect(content: &[Content], out: &mut Vec<String>) {
    for item in content {
        match item {
            Content::Paragraph(para) => out.push(para.text.clone()),
            Content::Heading(heading) => out.push(heading.text()),
            Content::List(list) => collect_list(list, out),
            Content::BlockQuote(inner) | Content::Epigraph(inner) => collect(inner, out),
            Content::Verse(verse) => {
                for stanza in &verse.stanzas {
                    for line in stanza {
                        out.push(oc_model::doc::spans_text(line));
                    }
                }
            }
            Content::Preformatted(pre) => out.extend(pre.lines.iter().cloned()),
            // A figure, a table and a note are referenced from the flow and hold their text
            // where they live, so counting them here would count it twice.
            Content::Figure(_)
            | Content::Table(_)
            | Content::NoteRefAnchor(_)
            | Content::PageBreak(_)
            | Content::Rule => {}
        }
    }
}

fn collect_list(list: &List, out: &mut Vec<String>) {
    for item in &list.items {
        collect(&item.content, out);
        if let Some(nested) = &item.nested {
            collect_list(nested, out);
        }
    }
}

/// Run the stage.
pub fn structure(input: &StructureInput, t: &Thresholds) -> StructureOutput {
    let blocks = &input.blocks;
    let inventory = cluster_styles(&input.runs, &input.fonts, t);
    let body_size = inventory.body_size_pt();

    let toc = parse_toc_page(blocks, t);
    let candidates = heading_candidates(blocks, &inventory, &input.fonts, t);
    let (headings, heading_confidence) = assign_levels(
        &inventory,
        &candidates,
        &input.outline,
        toc.as_ref(),
        &input.lang,
        t,
    );
    let run_ins = run_in_candidates(blocks, t);

    let (notes, note_refs, note_stats) = link_notes(blocks, &input.vectors, body_size, t);
    // The blocks the notes took. Named here because list detection has to skip them: a note
    // opens with `*` exactly as a bulleted item does.
    let note_blocks: Vec<BlockId> = notes
        .iter()
        .flat_map(|note| {
            note.body.iter().filter_map(|content| match content {
                Content::Paragraph(para) => para.blocks.first().copied(),
                _ => None,
            })
        })
        .collect();
    let tables = extract_tables(
        &input.vectors,
        blocks,
        u32::try_from(input.images.len()).unwrap_or_default(),
        t,
    );
    let (figures, captions, caption_warnings) =
        associate_captions(&input.images, blocks, body_size, &input.lang, t);
    let lists = detect_lists(blocks, &note_blocks, t);
    let (indented, escalations) = classify_indented(blocks, body_size, t);
    let images = drop_ornaments(&input.images, &input.image_hashes, input.page_count, t);
    let (metadata, meta_confidence) = metadata(&input.meta, blocks, body_size, t);

    // Which blocks have already had their text taken by something other than the flow.
    let mut taken: std::collections::BTreeSet<BlockId> = std::collections::BTreeSet::new();
    taken.extend(note_blocks.iter().copied());
    taken.extend(tables.consumed.iter().copied());
    taken.extend(lists.consumed.iter().copied());
    // Only the captions that were actually bound: an unattached caption stays in the flow as
    // a paragraph, which is what "abstain rather than guess" means for the text as well as
    // for the link.
    let bound: std::collections::BTreeSet<String> = figures
        .iter()
        .filter_map(|figure| figure.caption.as_ref())
        .map(|caption| oc_model::doc::spans_text(caption))
        .collect();
    taken.extend(
        captions
            .iter()
            .filter(|caption| bound.contains(caption.text.trim()))
            .map(|caption| caption.block),
    );

    let mut minter = Minter::new();
    let mut flow: Vec<FlowItem> = Vec::new();
    let mut emitted_lists: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    // A drop cap that `layout` gave a block of its own, waiting to be joined to the paragraph
    // it opens. Failing to do this is the classic stray one-character paragraph — `<p>W</p>`
    // followed by a paragraph beginning "hen the survey" — which is a very visible EPUB
    // defect (PIPELINE §6 step 6).
    let mut pending_cap: Option<(BlockId, String)> = None;

    for block in blocks {
        // Figures are anchored before the block they precede, so they enter the flow here.
        for figure in figures
            .iter()
            .filter(|figure| figure.anchor == block.id && images.kept.contains(&figure.image))
        {
            flow.push(FlowItem {
                page: block.page,
                content: Content::Figure(figure.id),
            });
        }
        if let Some(region) = tables
            .regions
            .iter()
            .find(|region| first_block_of(blocks, region) == Some(block.id))
        {
            flow.push(FlowItem {
                page: block.page,
                content: Content::Table(region.id),
            });
        }
        if taken.contains(&block.id) {
            // A list is emitted at the first block it consumed, and only once.
            if let Some(list) = lists
                .lists
                .iter()
                .find(|list| list_starts_at(list, block.id))
            {
                if emitted_lists.insert(list.id.as_str().to_owned()) {
                    flow.push(FlowItem {
                        page: block.page,
                        content: Content::List(list.clone()),
                    });
                }
            }
            continue;
        }

        if let Some(heading) = headings.iter().find(|heading| heading.block == block.id) {
            flow.push(FlowItem {
                page: block.page,
                content: Content::Heading(oc_model::doc::Heading {
                    id: block.id,
                    level: oc_model::doc::Heading::clamp_level(heading.level),
                    spans: vec![oc_model::doc::Span::plain(heading.text.clone())],
                    numbering: heading
                        .numbering
                        .as_ref()
                        .map(|numbering| numbering.value.clone()),
                    style_cluster: heading.cluster,
                    confidence: heading_confidence.clone(),
                }),
            });
            continue;
        }

        let lines: Vec<&crate::view::LineView> = block.lines.iter().collect();
        if lines.is_empty() {
            continue;
        }

        // A block that is *nothing but* a drop cap is held back and joined to the next
        // paragraph rather than emitted. The characters are not changed, only moved, which is
        // what keeps the stage Conserving.
        if let Some(cap) = input
            .drop_caps
            .iter()
            .find(|cap| block.text.trim() == cap.text.trim())
        {
            pending_cap = Some((block.id, cap.text.trim().to_owned()));
            continue;
        }

        let mut para = para_of(&mut minter, block.page, &[block.id], &lines);
        if let Some((cap_block, cap)) = pending_cap.take() {
            // No space: the cap is the paragraph's first *character*, not its first word.
            para.text = format!("{cap}{}", para.text);
            para.spans = vec![oc_model::doc::Span::plain(para.text.clone())];
            para.blocks.insert(0, cap_block);
            para.drop_cap = true;
        } else if input.drop_caps.iter().any(|cap| opens_with(cap, block)) {
            // `layout` kept the cap and its paragraph in one block, so there is nothing to
            // join — only something to record.
            para.drop_cap = true;
        }
        let quoted = indented
            .iter()
            .find(|entry| entry.block == block.id)
            .map(|entry| entry.resolved);
        para.confidence = Some(Confidence::deterministic(vec![Signal::new(
            "block_lines",
            block.lines.len() as f32,
        )]));

        flow.push(FlowItem {
            page: block.page,
            content: match quoted {
                Some(crate::quotes::IndentedKind::BlockQuote) => {
                    Content::BlockQuote(vec![Content::Paragraph(para)])
                }
                _ => Content::Paragraph(para),
            },
        });
    }

    let (sections, book_warnings, book_confidence) =
        book_structure(&flow, &input.labels, &input.lang, t);

    let mut warnings = Vec::new();
    warnings.extend(inventory.warnings.clone());
    warnings.extend(note_stats.warnings.clone());
    warnings.extend(caption_warnings);
    warnings.extend(lists.warnings.clone());
    warnings.extend(tables.warnings.clone());
    warnings.extend(images.warnings.clone());
    warnings.extend(book_warnings);

    let confidence = Confidence::deterministic(vec![
        Signal::new("heading_count", headings.len() as f32),
        Signal::new("note_match_rate", note_stats.match_rate),
        Signal::new("escalations", escalations.len() as f32),
        Signal::new("warnings", warnings.len() as f32),
        Signal::new(
            "sources",
            f32::from(u8::from(heading_confidence.fallback_used))
                + f32::from(u8::from(meta_confidence.fallback_used))
                + f32::from(u8::from(book_confidence.fallback_used)),
        ),
    ]);

    StructureOutput {
        inventory,
        toc,
        headings,
        run_ins,
        sections,
        notes,
        note_refs,
        note_stats,
        figures,
        tables: tables.tables,
        table_regions: tables.regions,
        lists: lists.lists,
        indented,
        images,
        metadata,
        escalations,
        warnings,
        confidence,
    }
}

/// The first block of a table's region, in reading order.
fn first_block_of(blocks: &[BlockView], region: &TableRegion) -> Option<BlockId> {
    blocks
        .iter()
        .find(|block| {
            block.page == region.page
                && block.bbox.y0 >= region.bbox.y0 - 1.0
                && block.bbox.y1 <= region.bbox.y1 + 1.0
        })
        .map(|block| block.id)
}

/// Whether a list's first item came from this block.
fn list_starts_at(list: &List, block: BlockId) -> bool {
    list.items
        .first()
        .and_then(|item| item.content.first())
        .is_some_and(|content| match content {
            Content::Paragraph(para) => para.blocks.first() == Some(&block),
            _ => false,
        })
}

/// Whether a drop cap opens this block.
///
/// By id when `layout` put the cap and the paragraph in one block, and by text and geometry
/// when it did not: a producer that draws the cap clear of the text grid gives it a line, and
/// therefore a block, of its own. That second case is the one the stray one-character
/// paragraph comes from, and the attachment is what stops it (PIPELINE §6 step 6).
fn opens_with(cap: &oc_layout::anchor::DropCap, block: &BlockView) -> bool {
    let character = cap.text.trim();
    !character.is_empty()
        && block
            .lines
            .first()
            .is_some_and(|line| line.text.trim_start().starts_with(character))
        && block.bbox.y1 >= cap.bbox.y0
        && block.bbox.y0 <= cap.bbox.y1
}
