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

use crate::book::FlowItem;
use crate::build::{continue_para, para_of, Minter, NoteRefRuns};
use crate::claims::{Claim, ClaimKind, Claimant, Claims};
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
    /// Per image, its perceptual hash — `None` for an image nothing will compare, which is
    /// every image too large to be an ornament (`images::needs_hash`).
    pub image_hashes: Vec<Option<u64>>,
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
    /// Which structure took each block out of the flow, and what it undertook to emit.
    pub claims: Claims,
    /// The printed contents page, when one was found, with the heading each entry names.
    pub contents: Option<crate::contents::Contents>,
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

    /// Every piece of text the *book* contains — as distinct from every piece the stage is
    /// holding somewhere (PHASE 7.5).
    ///
    /// [`Self::emitted_text`] iterates `self.notes`, `self.tables` and `self.figures`: every
    /// container the stage built, whether or not anything in the flow points at it. That is
    /// the wrong denominator for a conservation check. A table whose region was detected but
    /// whose first block could not be located is never pushed into the flow; a figure whose
    /// image was dropped as an ornament takes its bound caption with it. In both cases the
    /// blocks are claimed, so they leave the flow, and `emitted_text` counts the orphaned
    /// container's text as though a reader would see it. The stage balances and the book is
    /// short.
    ///
    /// This walks the other way: from the flow outwards, following each reference, so a
    /// container nothing references contributes nothing. The difference between the two is
    /// exactly the text that has been lost without anyone being told.
    pub fn reachable_text(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut figures = std::collections::BTreeSet::new();
        let mut tables = std::collections::BTreeSet::new();
        let mut notes = std::collections::BTreeSet::new();

        for section in &self.sections {
            for section in section.walk() {
                if let Some(heading) = &section.heading {
                    out.push(heading.text());
                }
                reach(
                    &section.content,
                    &mut out,
                    &mut figures,
                    &mut tables,
                    &mut notes,
                );
            }
        }

        // **Every** note reaches the book, referenced or not. `oc_epub::content` emits a note
        // nothing refers to as a plain `<aside>` at the end of the spine document covering its
        // page, precisely so that its text is not lost. An earlier version of this function
        // counted only notes a marker pointed at, which was stricter than the book and made
        // unreferenced notes look like losses — a diagnostic reporting defects the output
        // does not have (PHASE 7.5, `docs/DECISIONS_LOG.md` 2026-09-22).
        //
        // Figures and tables have no such path: one the flow does not reference is not
        // emitted, so for them reachability really is the flow.
        for note in &self.notes {
            collect(&note.body, &mut out);
        }
        for table in self
            .tables
            .iter()
            .filter(|table| tables.contains(&table.id))
        {
            out.extend(table.cell_texts());
        }
        for figure in self
            .figures
            .iter()
            .filter(|figure| figures.contains(&figure.id))
        {
            if let Some(caption) = &figure.caption {
                out.push(oc_model::doc::spans_text(caption));
            }
        }
        out
    }

    /// Every claimant the reader can reach, as `(kind, id)` — the same names `Claimant` uses.
    ///
    /// A list is reached when the flow holds it; a table or a figure when the flow holds a
    /// reference to it; a note when a marker in the text refers to it.
    pub fn reachable_claimants(&self) -> std::collections::BTreeSet<(ClaimKind, String)> {
        let mut out = std::collections::BTreeSet::new();
        for section in &self.sections {
            for section in section.walk() {
                reach_claimants(&section.content, &mut out);
            }
        }
        // Every note, for the reason given in `reachable_text`: `epub` emits the unreferenced
        // ones as asides, so a note is never an orphan in the book.
        for note in &self.notes {
            out.insert((ClaimKind::Note, format!("n{}", note.id.0)));
        }
        out
    }

    /// Claims whose claimant is **not in the book** (PHASE 7.5).
    ///
    /// A block leaves the flow because a structure undertook to carry its text. If that
    /// structure is then never placed, the undertaking is void and the text is gone — and
    /// nothing downstream can see it, because nothing downstream ever meets the structure.
    /// An unnamed claimant is orphaned by definition: there is no structure to reach.
    pub fn orphaned_claims(&self) -> Vec<&crate::claims::Claim> {
        let reachable = self.reachable_claimants();
        self.claims
            .iter()
            .filter(|claim| match &claim.by.id {
                Some(id) => !reachable.contains(&(claim.by.kind, id.clone())),
                None => true,
            })
            .collect()
    }
}

/// Which claimants a content list reaches, recursively.
fn reach_claimants(content: &[Content], out: &mut std::collections::BTreeSet<(ClaimKind, String)>) {
    for item in content {
        match item {
            Content::List(list) => {
                out.insert((ClaimKind::List, list.id.as_str().to_owned()));
            }
            Content::Table(id) => {
                out.insert((ClaimKind::Table, format!("t{}", id.0)));
            }
            Content::Figure(id) => {
                out.insert((ClaimKind::Caption, format!("f{}", id.0)));
            }
            Content::NoteRefAnchor(id) => {
                out.insert((ClaimKind::Note, format!("n{}", id.0)));
            }
            Content::BlockQuote(inner) | Content::Epigraph(inner) => reach_claimants(inner, out),
            _ => {}
        }
    }
}

/// Walk a content list the way a reader does, recording which containers it reaches.
fn reach(
    content: &[Content],
    out: &mut Vec<String>,
    figures: &mut std::collections::BTreeSet<oc_model::ids::FigureId>,
    tables: &mut std::collections::BTreeSet<oc_model::ids::TableId>,
    notes: &mut std::collections::BTreeSet<oc_model::ids::NoteId>,
) {
    for item in content {
        match item {
            Content::Paragraph(para) => out.push(para.text.clone()),
            Content::Heading(heading) => out.push(heading.text()),
            Content::List(list) => reach_list(list, out, figures, tables, notes),
            Content::BlockQuote(inner) | Content::Epigraph(inner) => {
                reach(inner, out, figures, tables, notes)
            }
            Content::Verse(verse) => {
                for stanza in &verse.stanzas {
                    for line in stanza {
                        out.push(oc_model::doc::spans_text(line));
                    }
                }
            }
            Content::Preformatted(pre) => out.extend(pre.lines.iter().cloned()),
            Content::Figure(id) => {
                figures.insert(*id);
            }
            Content::Table(id) => {
                tables.insert(*id);
            }
            Content::NoteRefAnchor(id) => {
                notes.insert(*id);
            }
            Content::PageBreak(_) | Content::Rule => {}
        }
    }
}

fn reach_list(
    list: &List,
    out: &mut Vec<String>,
    figures: &mut std::collections::BTreeSet<oc_model::ids::FigureId>,
    tables: &mut std::collections::BTreeSet<oc_model::ids::TableId>,
    notes: &mut std::collections::BTreeSet<oc_model::ids::NoteId>,
) {
    for item in &list.items {
        reach(&item.content, out, figures, tables, notes);
        if let Some(nested) = &item.nested {
            reach_list(nested, out, figures, tables, notes);
        }
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

/// What the AI step may change about a `structure` run (PHASE 10): labels, levels, zones and
/// wrappers — never text.
///
/// An admitted answer does not patch the output; the stage is **run again** with the edit, so the
/// model's answer reaches the book through exactly the code the deterministic answer took, and
/// the stage's conservation check runs over the result like any other. The default is no edit,
/// and [`structure`] is this stage with the default.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StructureEdits {
    /// Task 2: heading levels and demotions by style cluster.
    pub headings: crate::headings::levels::HeadingEdits,
    /// Task 3: each heading's zone, by its index among the flow's headings.
    pub zones: crate::book::ZoneEdits,
    /// Task 4: what an ambiguous indented block is emitted as, by block. Read only for the blocks
    /// `quotes` classified — a label for any other block has nothing to replace.
    pub indented: std::collections::BTreeMap<BlockId, crate::quotes::IndentedKind>,
    /// Task 1: the metadata, when a model's answer was admitted. Outside `C` (ARCHITECTURE §5.2).
    pub metadata: Option<Metadata>,
    /// Task `front_page`: a kind for a page before the first chapter, by page index. Read only
    /// for pages the rules left untyped or called a dedication (`front::front_sections`).
    pub front_pages: std::collections::BTreeMap<u32, oc_model::doc::FrontMatterKind>,
}

impl StructureEdits {
    pub fn is_empty(&self) -> bool {
        self.headings.is_empty()
            && self.zones.is_empty()
            && self.indented.is_empty()
            && self.metadata.is_none()
            && self.front_pages.is_empty()
    }
}

/// Run the stage.
pub fn structure(input: &StructureInput, t: &Thresholds) -> StructureOutput {
    structure_with(input, t, &StructureEdits::default())
}

/// Run the stage with the AI step's edits applied.
pub fn structure_with(
    input: &StructureInput,
    t: &Thresholds,
    edits: &StructureEdits,
) -> StructureOutput {
    let blocks = &input.blocks;
    let inventory = cluster_styles(&input.runs, &input.fonts, t);
    let body_size = inventory.body_size_pt();

    // The printed contents, by its shape; read before the headings, because the entries it
    // names are evidence about them.
    let mut contents = crate::contents::find_contents(blocks, input.page_count, t);
    let toc = parse_toc_page(blocks, t).or_else(|| {
        contents
            .as_ref()
            .and_then(crate::contents::Contents::as_toc_page)
    });
    let candidates = heading_candidates(blocks, &inventory, &input.fonts, t);
    let (headings, heading_confidence) = assign_levels(
        &inventory,
        &candidates,
        &input.outline,
        toc.as_ref(),
        &input.lang,
        t,
    );
    let (headings, epigraph_blocks) =
        crate::headings::levels::apply_heading_edits(headings, &edits.headings);
    // The chapters a book numbers at the body size, found by their place and their sequence.
    let openers = crate::headings::openers::chapter_openers(blocks, body_size, t);
    let body_cluster = inventory
        .body_cluster()
        .map_or(oc_model::ids::ClusterId(0), |cluster| cluster.id);
    let cluster_of = |id: BlockId| {
        blocks
            .iter()
            .find(|block| block.id == id)
            .and_then(|block| {
                crate::headings::candidate::dominant_cluster(block, &inventory, &input.fonts, t)
            })
            .unwrap_or(body_cluster)
    };
    let mut headings = crate::headings::levels::with_openers(headings, &openers, cluster_of);
    // A chapter number found by its sequence is often set over a title at the body size, which
    // size rank does not see: the short block right under it, on its page, that does not read as
    // the start of prose, is that title.
    let titles: Vec<HeadingAssignment> = headings
        .iter()
        .filter(|heading| heading.source == crate::headings::levels::LevelSource::Sequence)
        .filter_map(|heading| {
            let label = blocks.iter().find(|block| block.id == heading.block)?;
            let next = blocks.iter().find(|block| block.order == label.order + 1)?;
            let size = label.size_pt().max(next.size_pt()).max(body_size);
            let gap = next.bbox.y0 - label.bbox.y1;
            let text = next.text.trim();
            let first = text.chars().next()?;
            let last = text.chars().next_back()?;
            let titled = next.page == label.page
                && next.lines.len()
                    <= usize::try_from(t.headings.opener_max_lines.max(1)).unwrap_or(1)
                && f64::from(next.width_ratio()) < t.headings.short_line_max_width_ratio
                && gap >= -1.0
                && gap <= t.headings.label_title_gap_em as f32 * size
                && first.is_alphanumeric()
                && !matches!(last, '.' | ',' | ';' | ':')
                && crate::headings::candidate::legible(text, t)
                && !headings.iter().any(|other| other.block == next.id);
            titled.then(|| HeadingAssignment {
                block: next.id,
                order: next.order,
                page: next.page,
                text: text.to_owned(),
                level: heading.level,
                cluster: heading.cluster,
                numbering: None,
                source: crate::headings::levels::LevelSource::Sequence,
            })
        })
        .collect();
    headings.extend(titles);
    headings.sort_by_key(|heading| heading.order);
    let contents_blocks: std::collections::BTreeSet<BlockId> = contents
        .as_ref()
        .map(crate::contents::Contents::blocks)
        .unwrap_or_default();
    // A contents entry is a line naming a heading, never the heading itself.
    headings.retain(|heading| !contents_blocks.contains(&heading.block));
    if let Some(contents) = contents.as_mut() {
        // Titles set over several blocks are matched as the whole title.
        let pre_joins = join_multiline_headings(&headings, blocks, t);
        let members: std::collections::BTreeSet<BlockId> =
            pre_joins.values().flatten().copied().collect();
        let joined: std::collections::BTreeMap<BlockId, String> = pre_joins
            .iter()
            .filter_map(|(head, rest)| {
                let first = headings.iter().find(|heading| heading.block == *head)?;
                let mut text = first.text.clone();
                for block in rest {
                    if let Some(more) = headings.iter().find(|heading| heading.block == *block) {
                        text.push(' ');
                        text.push_str(&more.text);
                    }
                }
                Some((*head, text))
            })
            .collect();
        crate::contents::link_contents(
            contents,
            &mut headings,
            blocks,
            &input.labels,
            &input.lang,
            cluster_of,
            &joined,
            &members,
            t,
        );
    }
    let headings = headings;
    // A title printed over several lines that `layout` set as several blocks is one heading:
    // blocks that follow each other in reading order on one page, set at one size within the
    // level tolerance, with no more than a line of air between them.
    let heading_joins = join_multiline_headings(&headings, blocks, t);
    // A contents entry that named the second line of such a title names the title.
    if let Some(contents) = contents.as_mut() {
        for entry in contents.entries.iter_mut() {
            if let Some(target) = entry.target {
                if let Some((head, _)) = heading_joins
                    .iter()
                    .find(|(_, members)| members.contains(&target))
                {
                    entry.target = Some(*head);
                }
            }
        }
    }
    let run_ins = run_in_candidates(blocks, t);

    let (notes, note_refs, note_stats) = link_notes(blocks, &input.vectors, body_size, t);
    // Which run carries which marker. `NoteRef` names the run rather than the block precisely
    // so that exactly that run becomes a `Span` with a `noteref` on it and `epub` can turn it
    // into `<a epub:type="noteref">` — which is what the bijection check in the output reads.
    let noteref_runs: std::collections::BTreeMap<
        (u32, oc_model::text::RunId),
        oc_model::ids::NoteId,
    > = note_refs
        .iter()
        .map(|entry| ((entry.page, entry.run), entry.note))
        .collect();
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
    // **One owner per block, decided by precedence.** The four detectors used to run over
    // every block independently, so a block could be a note body *and* a table row *and* a
    // list item, and each emitted it. 44 of 95 corpus documents had such blocks, in all six
    // strata (PHASE 7.5, `structure/appeared/contested-claim`). So each detector is built from
    // what the ones before it left — notes, then tables, then captions, then lists — and the
    // text of a block is in exactly one structure by construction rather than by luck.
    //
    // The order is the strength of the evidence each one has. A note is decided by the zone
    // at the foot of the page and its font size; a table by ruling lines; a caption by an
    // image beside it; a list only by a line that opens with a marker, which is the weakest
    // and the most promiscuous signal of the four — it once took 451 blocks of one paper.
    let mut owned: std::collections::BTreeSet<BlockId> = note_blocks.iter().copied().collect();
    // The contents page first of all: its two columns of titles and numbers are a table to
    // the table detector, and a list to the list detector.
    owned.extend(contents_blocks.iter().copied());
    let tables = extract_tables(
        &input.vectors,
        input.page_count,
        blocks,
        &owned,
        u32::try_from(input.images.len()).unwrap_or_default(),
        t,
    );
    owned.extend(tables.consumed.iter().copied());
    // Ornaments first: a caption bound to an image that is then dropped as an ornament took a
    // block out of the flow for a figure the book never shows, and its text went with it —
    // whole paragraphs of one book, beside a chapter-opening flourish (2026-09-26).
    let mut images = drop_ornaments(&input.images, &input.image_hashes, input.page_count, t);
    let chars_on_page = blocks.iter().fold(
        std::collections::BTreeMap::<u32, usize>::new(),
        |mut chars, block| {
            *chars.entry(block.page).or_default() +=
                block.text.chars().filter(|ch| !ch.is_whitespace()).count();
            chars
        },
    );
    crate::images::drop_text_backgrounds(&mut images, &input.images, &chars_on_page, t);
    let kept_images: Vec<oc_model::extract::ImageRef> = input
        .images
        .iter()
        .filter(|image| images.kept.contains(&image.id))
        .cloned()
        .collect();
    let (figures, _captions, caption_warnings, bound_captions) =
        associate_captions(&kept_images, blocks, &owned, body_size, &input.lang, t);
    // Only a caption that was actually bound owns its block — by identity, as recorded where
    // it was bound. An unbound one stays in the flow and may still be anything else.
    owned.extend(bound_captions.keys().copied());
    let list_skip: Vec<BlockId> = owned.iter().copied().collect();
    let lists = detect_lists(blocks, &list_skip, &noteref_runs, t);
    let (indented, escalations) = classify_indented(blocks, body_size, t);
    let (metadata, meta_confidence) = metadata(&input.meta, blocks, body_size, t);
    // The metadata task's answer, when one was admitted, is the book's metadata.
    let metadata = without_controls(edits.metadata.clone().unwrap_or(metadata));

    // Which blocks have had their text taken by something other than the flow, and — the
    // part a bare `BTreeSet<BlockId>` could not say — *which* structure undertook to emit
    // each one. Four independent claimants feed this, and until the claim carried its
    // claimant there was no way to ask any of them whether it kept its word (PHASE 7.5).
    let mut claims = Claims::new();
    // Built once. Looking a block up by scanning `blocks` is linear, and every claim asks
    // twice, so on a book with thousands of blocks and thousands of claims the stage stops
    // returning — measured at over four minutes on a 368-page InDesign volume where `ingest`,
    // `text` and `layout` together take twenty-seven seconds.
    let by_id: std::collections::BTreeMap<BlockId, (u32, &str)> = blocks
        .iter()
        .map(|block| (block.id, (block.page, block.text.as_str())))
        .collect();
    let page_of =
        |id: BlockId| -> u32 { by_id.get(&id).map(|(page, _)| *page).unwrap_or_default() };
    let text_of = |id: BlockId| -> String {
        by_id
            .get(&id)
            .map(|(_, text)| (*text).to_owned())
            .unwrap_or_default()
    };

    for note in &notes {
        for content in &note.body {
            if let Content::Paragraph(para) = content {
                if let Some(&block) = para.blocks.first() {
                    claims.push(Claim {
                        block,
                        page: page_of(block),
                        by: Claimant::new(ClaimKind::Note, format!("n{}", note.id.0)),
                        // What *this note* took, not the whole block. One footnote block
                        // routinely holds several notes, split between them; recording the
                        // block's full text against each made a correct split look like the
                        // same text owned four times over.
                        text: para.text.clone(),
                    });
                }
            }
        }
    }
    for &block in &tables.consumed {
        // Which table, as recorded where the text was taken. Re-deriving it here from the
        // region geometry would be the same mistake this class is about: a second predicate
        // that agrees with the first until it does not.
        let named = tables
            .claimed_by
            .get(&block)
            .map(|id| Claimant::new(ClaimKind::Table, format!("t{}", id.0)));
        claims.push(Claim {
            block,
            page: page_of(block),
            by: named.unwrap_or_else(|| Claimant::unnamed(ClaimKind::Table)),
            text: text_of(block),
        });
    }
    // Which list took a block is recorded in `lists.taken`, at the moment the list took it.
    // Re-deriving it by walking every list's items for every consumed block was cubic, and it
    // was also the pattern this phase keeps finding: a second predicate standing in for a
    // decision that has already been recorded.
    for &block in &lists.consumed {
        let named = lists
            .taken
            .get(&block)
            .and_then(|taken| taken.first())
            .map(|(_, list)| Claimant::new(ClaimKind::List, list.as_str().to_owned()));
        claims.push(Claim {
            block,
            page: page_of(block),
            by: named.unwrap_or_else(|| Claimant::unnamed(ClaimKind::List)),
            text: text_of(block),
        });
    }
    // Only the captions that were actually bound — the very blocks, as `associate_captions`
    // recorded them — not every block whose text happens to equal a bound caption's.
    for (&block, figure) in &bound_captions {
        claims.push(Claim {
            block,
            page: page_of(block),
            by: Claimant::new(ClaimKind::Caption, format!("f{}", figure.0)),
            text: text_of(block),
        });
    }

    // Where each table enters the flow: at the first block, in reading order, whose text it took —
    // recorded where the text was taken. The region's geometry used to decide it, and a table
    // whose blocks stood out past its ruled region had its text claimed and was never placed:
    // a cell's words were in the book's text and nowhere in the book (2026-09-26). A region
    // that took no block falls back to the first block inside it.
    let table_anchors: Vec<(oc_model::ids::TableId, BlockId)> = tables
        .regions
        .iter()
        .filter_map(|region| {
            let claimed = blocks
                .iter()
                .filter(|block| tables.claimed_by.get(&block.id) == Some(&region.id))
                .min_by_key(|block| block.order)
                .map(|block| block.id);
            claimed
                .or_else(|| first_block_of(blocks, region))
                .map(|anchor| (region.id, anchor))
        })
        .collect();
    // A title joined across blocks is emitted by its first block, and the rest are skipped. That
    // holds only for blocks that reach the heading arm below: a block a note, a list, a table or
    // the contents took is emitted by its owner first, so joining it would put its words in the
    // book twice — or, when the owner is the head, lose the rest (2026-09-26). A title's chain is
    // cut at the first block that is owned elsewhere.
    let reaches_heading_arm = |id: &BlockId| {
        !claims.contains(*id)
            && !lists.taken.contains_key(id)
            && !contents_blocks.contains(id)
            && !tables.claimed_by.contains_key(id)
    };
    let heading_joins = joins_within(heading_joins, reaches_heading_arm);
    let joined_away: std::collections::BTreeSet<BlockId> =
        heading_joins.values().flatten().copied().collect();
    let mut minter = Minter::new();
    let mut flow: Vec<FlowItem> = Vec::new();
    let mut emitted_lists: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let list_by_id: std::collections::BTreeMap<BlockId, &List> =
        lists.lists.iter().map(|list| (list.id, list)).collect();
    // A drop cap that `layout` gave a block of its own, waiting to be joined to the paragraph
    // it opens. Failing to do this is the classic stray one-character paragraph — `<p>W</p>`
    // followed by a paragraph beginning "hen the survey" — which is a very visible EPUB
    // defect (PIPELINE §6 step 6).
    let mut pending_cap: Option<(BlockId, String)> = None;
    // Per block, the flow item its last paragraph of running text ended up in, and whether that
    // paragraph ends on a word `paragraphs` joined across the break — where a block `paragraphs`
    // said carries it on is joined to it.
    let mut tails: std::collections::BTreeMap<BlockId, (usize, bool)> =
        std::collections::BTreeMap::new();

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
        // Every table whose anchor this block is, in the order the tables were found.
        for id in table_anchors
            .iter()
            .filter(|(_, anchor)| *anchor == block.id)
            .map(|(id, _)| *id)
        {
            flow.push(FlowItem {
                page: block.page,
                content: Content::Table(id),
            });
        }
        // A block of the printed contents: its entries, their titles linked to the headings
        // they name.
        // Unless a note took the block first: the note is its owner then, and emitting it here
        // as well would put its text in the book twice.
        if contents_blocks.contains(&block.id) && !claims.contains(block.id) {
            if let Some(contents) = contents.as_ref() {
                emit_contents(&mut flow, &mut minter, block, contents, &noteref_runs, t);
                continue;
            }
        }

        // The lists that took lines of this block, in line order. A list is emitted the first
        // time the loop meets **any** line it took — the emission point is derived from its
        // claims, so a list with even one taken line cannot fail to reach the book. The old
        // trigger fired only when the list's first item's block was itself claimed, and 13 of
        // 13 measured orphans were lists whose first item shared a block with the sentence
        // introducing it (PHASE 7.5, `structure/lost/orphaned-claimant`).
        let taken_here: &[(usize, BlockId)] =
            lists.taken.get(&block.id).map(Vec::as_slice).unwrap_or(&[]);

        if claims.contains(block.id) {
            for (_, list_id) in taken_here {
                if let Some(list) = list_by_id.get(list_id) {
                    if emitted_lists.insert(list.id.as_str().to_owned()) {
                        flow.push(FlowItem {
                            page: block.page,
                            content: Content::List((*list).clone()),
                        });
                    }
                }
            }
            continue;
        }

        if !taken_here.is_empty() {
            // Taken in part. The block is split at exactly the lines the list took: what comes
            // before stays a paragraph, the list follows, and what comes after follows it.
            // Every line is in one place — the list, or a paragraph of this block — which is
            // what the two rejected fixes could not both say.
            let taken_at: std::collections::BTreeMap<usize, BlockId> =
                taken_here.iter().copied().collect();
            let mut segment: Vec<usize> = Vec::new();
            for position in 0..block.lines.len() {
                let Some(list_id) = taken_at.get(&position) else {
                    segment.push(position);
                    continue;
                };
                let first_time = list_by_id
                    .get(list_id)
                    .is_some_and(|list| !emitted_lists.contains(list.id.as_str()));
                if first_time {
                    emit_running_text(
                        &mut Emit {
                            flow: &mut flow,
                            tails: &mut tails,
                            minter: &mut minter,
                            pending_cap: &mut pending_cap,
                        },
                        block,
                        &segment,
                        &noteref_runs,
                        &[],
                        t,
                    );
                    segment.clear();
                    if let Some(list) = list_by_id.get(list_id) {
                        emitted_lists.insert(list.id.as_str().to_owned());
                        flow.push(FlowItem {
                            page: block.page,
                            content: Content::List((*list).clone()),
                        });
                    }
                }
            }
            emit_running_text(
                &mut Emit {
                    flow: &mut flow,
                    tails: &mut tails,
                    minter: &mut minter,
                    pending_cap: &mut pending_cap,
                },
                block,
                &segment,
                &noteref_runs,
                &[],
                t,
            );
            continue;
        }

        if joined_away.contains(&block.id) {
            // Emitted with the heading it continues.
            continue;
        }
        if let Some(heading) = headings.iter().find(|heading| heading.block == block.id) {
            let mut text = heading.text.clone();
            for rest in heading_joins.get(&block.id).into_iter().flatten() {
                if let Some(more) = headings.iter().find(|heading| heading.block == *rest) {
                    text.push(' ');
                    text.push_str(&more.text);
                }
            }
            flow.push(FlowItem {
                page: block.page,
                content: Content::Heading(oc_model::doc::Heading {
                    id: block.id,
                    level: oc_model::doc::Heading::clamp_level(heading.level),
                    spans: vec![oc_model::doc::Span::plain(text)],
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

        // The verse-or-quote task's label, where one was admitted, replaces the default for a
        // block `quotes` classified: the same block, emitted through the same arms below.
        let quoted = indented
            .iter()
            .find(|entry| entry.block == block.id)
            .map(|entry| {
                edits
                    .indented
                    .get(&block.id)
                    .copied()
                    .unwrap_or(entry.resolved)
            });
        let epigraph = epigraph_blocks.contains(&block.id);

        // Running text: the block cut into the paragraphs `paragraphs` found in it, and its
        // first joined to the paragraph it carries on. The common case by a wide margin.
        if quoted.is_none() && !epigraph {
            let positions: Vec<usize> = (0..block.lines.len()).collect();
            emit_running_text(
                &mut Emit {
                    flow: &mut flow,
                    tails: &mut tails,
                    minter: &mut minter,
                    pending_cap: &mut pending_cap,
                },
                block,
                &positions,
                &noteref_runs,
                &input.drop_caps,
                t,
            );
            continue;
        }

        let mut para = para_of(
            &mut minter,
            block.page,
            &[block.id],
            &lines,
            &noteref_runs,
            t,
        );
        if let Some((cap_block, cap)) = pending_cap.take() {
            // No space: the cap is the paragraph's first *character*, not its first word.
            // Prepended as a span of its own rather than folded into the text: rebuilding
            // `spans` from the joined string would throw away every style and every note
            // reference the paragraph's runs carried, which is the whole of what `epub` reads.
            para.spans
                .insert(0, oc_model::doc::Span::plain(cap.clone()));
            para.text = format!("{cap}{}", para.text);
            para.blocks.insert(0, cap_block);
            para.drop_cap = true;
        } else if input.drop_caps.iter().any(|cap| opens_with(cap, block)) {
            // `layout` kept the cap and its paragraph in one block, so there is nothing to
            // join — only something to record.
            para.drop_cap = true;
        }
        para.confidence = Some(Confidence::deterministic(vec![Signal::new(
            "block_lines",
            block.lines.len() as f32,
        )]));

        // Verse and preformatted text are emitted as themselves rather than as paragraphs,
        // because the one thing a reflowable format must not do to either of them is re-wrap
        // their lines. The characters are the same either way — this is where the *line
        // breaks* survive.
        let content = match quoted {
            Some(crate::quotes::IndentedKind::BlockQuote) => {
                // A quotation of several paragraphs is several paragraphs inside one quote.
                let positions: Vec<usize> = (0..block.lines.len()).collect();
                let pieces = pieces_of(block, &positions);
                if pieces.len() > 1 && !para.drop_cap {
                    Content::BlockQuote(
                        pieces
                            .iter()
                            .map(|piece| {
                                let lines: Vec<&crate::view::LineView> = piece
                                    .iter()
                                    .filter_map(|&position| block.lines.get(position))
                                    .collect();
                                Content::Paragraph(para_of(
                                    &mut minter,
                                    block.page,
                                    &[block.id],
                                    &lines,
                                    &noteref_runs,
                                    t,
                                ))
                            })
                            .collect(),
                    )
                } else {
                    Content::BlockQuote(vec![Content::Paragraph(para)])
                }
            }
            Some(crate::quotes::IndentedKind::Verse) => Content::Verse(oc_model::doc::Verse {
                id: block.id,
                stanzas: vec![block
                    .lines
                    .iter()
                    .map(|line| vec![oc_model::doc::Span::plain(line.text.trim().to_owned())])
                    .collect()],
                confidence: Confidence::deterministic(vec![Signal::new(
                    "lines",
                    block.lines.len() as f32,
                )]),
            }),
            Some(crate::quotes::IndentedKind::Pre) => Content::Preformatted(oc_model::doc::Pre {
                id: block.id,
                lines: block
                    .lines
                    .iter()
                    .map(|line| line.text.trim().to_owned())
                    .collect(),
                confidence: Confidence::deterministic(vec![Signal::new("monospace", 1.0)]),
            }),
            _ => Content::Paragraph(para),
        };
        // A heading the heading-roles task demoted to an epigraph: the same content, wrapped.
        let content = if epigraph {
            Content::Epigraph(vec![content])
        } else {
            content
        };
        flow.push(FlowItem {
            page: block.page,
            content,
        });
    }

    let (sections, book_warnings, book_confidence) = crate::book::book_structure_with(
        &flow,
        &input.labels,
        &input.lang,
        metadata.title.as_deref(),
        &edits.front_pages,
        t,
        &edits.zones,
    );

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
        claims,
        contents,
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

/// The metadata with every control character taken out. A PDF's Info dictionary is written by
/// whatever made the file, and `Bernard Lew\0s` is a title one of them wrote; metadata is outside
/// `C`, so cleaning it costs the book nothing, and XML 1.0 cannot carry it otherwise.
fn without_controls(mut metadata: Metadata) -> Metadata {
    let clean = |text: &str| -> String {
        text.chars()
            .map(|ch| if ch.is_control() { ' ' } else { ch })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let clean_opt = |text: Option<String>| -> Option<String> {
        text.map(|text| clean(&text))
            .filter(|text| !text.is_empty())
    };
    metadata.title = clean_opt(metadata.title);
    metadata.subtitle = clean_opt(metadata.subtitle);
    metadata.authors = metadata
        .authors
        .iter()
        .map(|author| clean(author))
        .filter(|author| !author.is_empty())
        .collect();
    metadata.translator = clean_opt(metadata.translator);
    metadata.publisher = clean_opt(metadata.publisher);
    metadata.date = clean_opt(metadata.date);
    metadata
}

/// Which headings continue the heading before them: per first block, the blocks joined to it.
/// `joins` kept to the blocks `reaches` admits: a title whose first block is not admitted is not
/// joined at all, and a chain is cut before its first block that is not.
fn joins_within(
    joins: std::collections::BTreeMap<BlockId, Vec<BlockId>>,
    reaches: impl Fn(&BlockId) -> bool,
) -> std::collections::BTreeMap<BlockId, Vec<BlockId>> {
    joins
        .into_iter()
        .filter(|(head, _)| reaches(head))
        .map(|(head, rest)| {
            let rest: Vec<BlockId> = rest.into_iter().take_while(|id| reaches(id)).collect();
            (head, rest)
        })
        .filter(|(_, rest)| !rest.is_empty())
        .collect()
}

fn join_multiline_headings(
    headings: &[HeadingAssignment],
    blocks: &[BlockView],
    t: &Thresholds,
) -> std::collections::BTreeMap<BlockId, Vec<BlockId>> {
    let by_id: std::collections::BTreeMap<BlockId, &BlockView> =
        blocks.iter().map(|block| (block.id, block)).collect();
    let tolerance = t.headings.level_size_tolerance as f32;
    let gap_em = t.headings.join_gap_em as f32;
    let label_gap_em = t.headings.label_title_gap_em as f32;
    let mut joins: std::collections::BTreeMap<BlockId, Vec<BlockId>> =
        std::collections::BTreeMap::new();
    let mut head: Option<BlockId> = None;
    for pair in headings.windows(2) {
        let (Some(a), Some(b)) = (by_id.get(&pair[0].block), by_id.get(&pair[1].block)) else {
            head = None;
            continue;
        };
        let (size_a, size_b) = (a.size_pt(), b.size_pt());
        let largest = size_a.max(size_b);
        let same_size = largest > 0.0 && (size_a - size_b).abs() / largest <= tolerance;
        let gap = b.bbox.y0 - a.bbox.y1;
        let close = gap >= -1.0 && gap <= gap_em * largest;
        // A chapter's label and its title — `3` over `The Road`, `BÖLÜM 3` over `Başlangıç` —
        // are one heading whatever sizes they are set in, with the air a designer puts between
        // them.
        let label_then_title = crate::headings::openers::is_number_label(&pair[0].text, t)
            && !crate::headings::openers::is_number_label(&pair[1].text, t)
            && gap >= -1.0
            && gap <= label_gap_em * largest;
        let joined = b.order == a.order + 1
            && b.page == a.page
            && ((same_size && close) || label_then_title);
        if joined {
            let first = *head.get_or_insert(a.id);
            joins.entry(first).or_default().push(b.id);
        } else {
            head = None;
        }
    }
    joins
}

/// Emit one block of the printed contents: each entry one paragraph, its title a link to the
/// heading it names when one was found. Every line of the block is in exactly one entry, so the
/// block's text is emitted whole.
fn emit_contents(
    flow: &mut Vec<FlowItem>,
    minter: &mut Minter,
    block: &BlockView,
    contents: &crate::contents::Contents,
    noterefs: &NoteRefRuns,
    t: &Thresholds,
) {
    let entries: Vec<&crate::contents::ContentsEntry> = contents
        .entries
        .iter()
        .filter(|entry| entry.block == block.id)
        .collect();
    let covered: std::collections::BTreeSet<usize> = entries
        .iter()
        .flat_map(|entry| entry.positions.iter().copied())
        .collect();
    for position in 0..block.lines.len() {
        let entry = entries
            .iter()
            .find(|entry| entry.positions.first() == Some(&position));
        let positions: Vec<usize> = match entry {
            Some(entry) => entry.positions.clone(),
            None if covered.contains(&position) => continue,
            None => vec![position],
        };
        let lines: Vec<&crate::view::LineView> = positions
            .iter()
            .filter_map(|&at| block.lines.get(at))
            .collect();
        if lines.iter().all(|line| line.text.trim().is_empty()) {
            continue;
        }
        let mut para = para_of(minter, block.page, &[block.id], &lines, noterefs, t);
        if let Some(target) = entry.and_then(|entry| entry.target) {
            if let Some(title) = entry.map(|entry| entry.title.as_str()) {
                if para.text.starts_with(title) && !title.is_empty() {
                    crate::build::link_prefix(&mut para, title.len(), target);
                }
            }
        }
        flow.push(FlowItem {
            page: block.page,
            content: Content::Paragraph(para),
        });
    }
}

/// What emitting running text writes to, borrowed together.
struct Emit<'a> {
    flow: &'a mut Vec<FlowItem>,
    tails: &'a mut std::collections::BTreeMap<BlockId, (usize, bool)>,
    minter: &'a mut Minter,
    pending_cap: &'a mut Option<(BlockId, String)>,
}

/// Some of a block's lines, cut into the paragraphs `paragraphs` found in them.
fn pieces_of(block: &BlockView, positions: &[usize]) -> Vec<Vec<usize>> {
    let mut pieces: Vec<Vec<usize>> = Vec::new();
    for &position in positions {
        if pieces.is_empty() || block.para_starts.binary_search(&position).is_ok() {
            pieces.push(Vec::new());
        }
        if let Some(piece) = pieces.last_mut() {
            piece.push(position);
        }
    }
    pieces
}

/// Emit some of a block's lines as running text: one paragraph per piece `paragraphs` cut,
/// the first joined to the paragraph it carries on when `paragraphs` said it does.
///
/// A carry-over is joined only to a paragraph that is still the last thing in the flow but for
/// figures — a figure set at the top of the next page sits after the whole paragraph rather
/// than inside it. A pending drop cap is joined to the first paragraph emitted, because a cap
/// belongs to the paragraph it opens wherever the block is cut.
fn emit_running_text(
    out: &mut Emit<'_>,
    block: &BlockView,
    positions: &[usize],
    noterefs: &NoteRefRuns,
    drop_caps: &[oc_layout::anchor::DropCap],
    t: &Thresholds,
) {
    let last_position = block.lines.len().saturating_sub(1);
    for piece in pieces_of(block, positions) {
        let lines: Vec<&crate::view::LineView> = piece
            .iter()
            .filter_map(|&position| block.lines.get(position))
            .collect();
        if lines.iter().all(|line| line.text.trim().is_empty()) {
            continue;
        }
        let opens_block = piece.first() == Some(&0);
        let ends_block = piece.last() == Some(&last_position);
        let glued_end = lines.last().is_some_and(|line| line.glue);

        let mut para = para_of(out.minter, block.page, &[block.id], &lines, noterefs, t);
        if let Some((cap_block, cap)) = out.pending_cap.take() {
            para.spans
                .insert(0, oc_model::doc::Span::plain(cap.clone()));
            para.text = format!("{cap}{}", para.text);
            para.blocks.insert(0, cap_block);
            para.drop_cap = true;
        } else if opens_block && drop_caps.iter().any(|cap| opens_with(cap, block)) {
            para.drop_cap = true;
        }
        para.confidence = Some(Confidence::deterministic(vec![Signal::new(
            "block_lines",
            block.lines.len() as f32,
        )]));

        let carried = (opens_block && !para.drop_cap)
            .then_some(block.continues)
            .flatten()
            .and_then(|first| out.tails.get(&first).copied())
            .filter(|(at, _)| {
                matches!(
                    out.flow.get(*at).map(|item| &item.content),
                    Some(Content::Paragraph(_))
                ) && out.flow[at + 1..]
                    .iter()
                    .all(|item| matches!(item.content, Content::Figure(_)))
            });
        if let Some((at, glued)) = carried {
            if let Some(FlowItem {
                content: Content::Paragraph(open),
                ..
            }) = out.flow.get_mut(at)
            {
                continue_para(open, para, glued);
                if ends_block {
                    out.tails.insert(block.id, (at, glued_end));
                }
                continue;
            }
        }

        out.flow.push(FlowItem {
            page: block.page,
            content: Content::Paragraph(para),
        });
        if ends_block {
            out.tails.insert(block.id, (out.flow.len() - 1, glued_end));
        }
    }
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

#[cfg(test)]
mod join_tests {
    use super::*;

    /// A title's blocks are joined only while each one is emitted as a heading: a block a list
    /// or a note owns ends the chain, and an owned head joins nothing — else its words reach the
    /// book twice, or not at all (I-1 on six technical books, 2026-09-26).
    #[test]
    fn a_title_is_joined_only_over_blocks_the_heading_arm_emits() {
        let id = |n: u32| {
            let at = oc_model::geom::Rect {
                x0: 0.0,
                y0: 0.0,
                x1: 1.0,
                y1: 1.0,
            };
            BlockId::derive(n, at, "block")
        };
        let joins: std::collections::BTreeMap<BlockId, Vec<BlockId>> =
            [(id(1), vec![id(2), id(3), id(4)]), (id(10), vec![id(11)])]
                .into_iter()
                .collect();
        let owned = [id(3), id(10)];
        let kept = joins_within(joins, |block| !owned.contains(block));
        assert_eq!(
            kept.get(&id(1)),
            Some(&vec![id(2)]),
            "cut before the owned block"
        );
        assert!(!kept.contains_key(&id(10)), "an owned head joins nothing");
    }
}
