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
use crate::build::{para_of, Minter};
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
}

impl StructureEdits {
    pub fn is_empty(&self) -> bool {
        self.headings.is_empty()
            && self.zones.is_empty()
            && self.indented.is_empty()
            && self.metadata.is_none()
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
    let (headings, epigraph_blocks) =
        crate::headings::levels::apply_heading_edits(headings, &edits.headings);
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
    let tables = extract_tables(
        &input.vectors,
        blocks,
        &owned,
        u32::try_from(input.images.len()).unwrap_or_default(),
        t,
    );
    owned.extend(tables.consumed.iter().copied());
    let (figures, _captions, caption_warnings, bound_captions) =
        associate_captions(&input.images, blocks, &owned, body_size, &input.lang, t);
    // Only a caption that was actually bound owns its block — by identity, as recorded where
    // it was bound. An unbound one stays in the flow and may still be anything else.
    owned.extend(bound_captions.keys().copied());
    let list_skip: Vec<BlockId> = owned.iter().copied().collect();
    let lists = detect_lists(blocks, &list_skip, &noteref_runs, t);
    let (indented, escalations) = classify_indented(blocks, body_size, t);
    let images = drop_ornaments(&input.images, &input.image_hashes, input.page_count, t);
    let (metadata, meta_confidence) = metadata(&input.meta, blocks, body_size, t);
    // The metadata task's answer, when one was admitted, is the book's metadata.
    let metadata = edits.metadata.clone().unwrap_or(metadata);

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
            let mut segment: Vec<&crate::view::LineView> = Vec::new();
            for (position, line) in block.lines.iter().enumerate() {
                let Some(list_id) = taken_at.get(&position) else {
                    segment.push(line);
                    continue;
                };
                let first_time = list_by_id
                    .get(list_id)
                    .is_some_and(|list| !emitted_lists.contains(list.id.as_str()));
                if first_time {
                    if let Some(para) = segment_para(
                        &mut minter,
                        block,
                        &segment,
                        &noteref_runs,
                        &mut pending_cap,
                        t,
                    ) {
                        flow.push(FlowItem {
                            page: block.page,
                            content: Content::Paragraph(para),
                        });
                    }
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
            if let Some(para) = segment_para(
                &mut minter,
                block,
                &segment,
                &noteref_runs,
                &mut pending_cap,
                t,
            ) {
                flow.push(FlowItem {
                    page: block.page,
                    content: Content::Paragraph(para),
                });
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
                Content::BlockQuote(vec![Content::Paragraph(para)])
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
        let content = if epigraph_blocks.contains(&block.id) {
            Content::Epigraph(vec![content])
        } else {
            content
        };
        flow.push(FlowItem {
            page: block.page,
            content,
        });
    }

    let (sections, book_warnings, book_confidence) =
        crate::book::book_structure_with(&flow, &input.labels, &input.lang, t, &edits.zones);

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

/// A paragraph from the lines of a block that a list did not take, if any are left.
///
/// A pending drop cap is joined to the first such paragraph, exactly as the whole-block path
/// joins it, because a cap belongs to the paragraph it opens wherever the block is cut.
fn segment_para(
    minter: &mut Minter,
    block: &BlockView,
    lines: &[&crate::view::LineView],
    noterefs: &crate::build::NoteRefRuns,
    pending_cap: &mut Option<(BlockId, String)>,
    t: &Thresholds,
) -> Option<oc_model::layout::Para> {
    if lines.iter().all(|line| line.text.trim().is_empty()) {
        return None;
    }
    let mut para = para_of(minter, block.page, &[block.id], lines, noterefs, t);
    if let Some((cap_block, cap)) = pending_cap.take() {
        para.spans
            .insert(0, oc_model::doc::Span::plain(cap.clone()));
        para.text = format!("{cap}{}", para.text);
        para.blocks.insert(0, cap_block);
        para.drop_cap = true;
    }
    Some(para)
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
