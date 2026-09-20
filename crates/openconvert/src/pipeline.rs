//! The `text`, `furniture` and `layout` stages, driven end to end under the conservation law.
//!
//! Each stage is run and then immediately checked: `C(D_before) ⊎ Added == C(D_after) ⊎
//! Removed`, the reasons it cited are ones it declared, and its cumulative removals are
//! inside their budgets (ARCHITECTURE §5.4). The check is not an assertion in a test — it
//! runs on every conversion, and a violation is an error rather than a warning, because a
//! conservation law that can be disabled is a preference (D13.4).

use oc_core::ledger_check::{c_of, c_of_parts, check_invariants, ConservationError, ReasonTotals};
use oc_core::stages;
use oc_core::thresholds::Thresholds;
use oc_layout::anchor::{anchor_images, drop_caps, Anchor, DropCap};
use oc_layout::blocks::{segment_blocks, LayoutLine, LayoutPage, Segment, SegmentationAgreement};
use oc_layout::columns::{
    assign_columns, detect_columns, median_height, split_lines_at_gutters, ColumnLayout,
};
use oc_layout::continuity::{continuity, Continuity};
use oc_layout::furniture::{apply_furniture, detect_furniture, PageLines};
use oc_layout::paragraphs::{dehyphenate_paragraphs, infer_convention, reconstruct_paragraphs};
use oc_layout::reading_order::reading_order;
use oc_model::extract::{CharHistogram, FontId, FontInfo, Glyph, ImageRef, PageRef};
use oc_model::geom::Rect;
use oc_model::lang::LangTag;
use oc_model::layout::{Block, Para, ParagraphConvention};
use oc_model::ledger::{LedgerDelta, StageCheck};
use oc_model::text::{Line, Run};
use oc_text::dehyphen::DocLexicon;
use oc_text::lines::assemble_lines;
use oc_text::normalize::{normalize, LedgerSite};
use oc_text::words::assemble_runs;

/// One page as `ingest` leaves it.
#[derive(Clone, Debug, PartialEq)]
pub struct PageInput {
    pub page: PageRef,
    pub width_pt: f32,
    pub height_pt: f32,
    pub glyphs: Vec<Glyph>,
    /// The fonts this page's glyphs point at, indexed by the page's own [`FontId`]s.
    /// Interning is per page in the backend, so two pages may number the same face
    /// differently; `text` merges them into one document table and remaps the runs.
    pub fonts: Vec<FontInfo>,
    /// The images `ingest` found on the page. `layout` anchors them into the flow and drops
    /// none of them (PIPELINE §6 step 5).
    pub images: Vec<ImageRef>,
}

/// One page as `text` leaves it.
#[derive(Clone, Debug, PartialEq)]
pub struct TextPage {
    pub page: PageRef,
    pub width_pt: f32,
    pub height_pt: f32,
    pub images: Vec<ImageRef>,
    pub runs: Vec<Run>,
    pub lines: Vec<Line>,
}

/// What `text` produced, and the check that says it did not lose anything.
#[derive(Clone, Debug)]
pub struct TextStage {
    pub pages: Vec<TextPage>,
    /// The document's fonts, merged from the per-page tables. Every `Run.font` in
    /// `pages` indexes into this, not into the page it came from.
    pub fonts: Vec<FontInfo>,
    pub delta: LedgerDelta,
    /// `C_0`, the retention denominator: the multiset after `N` (ARCHITECTURE §5.2). Overdraw
    /// and OCR-layer dedup, the other two things folded into `C_0`, happen in `ingest` and in
    /// Phase 13; when they arrive they subtract from this same quantity.
    pub c_0: CharHistogram,
    pub check: StageCheck,
}

/// What `furniture` produced.
#[derive(Clone, Debug)]
pub struct FurnitureStage {
    pub pages: Vec<PageLines>,
    pub delta: LedgerDelta,
    /// The printed page number recovered per page, bound for `PageRef.label` and `page-list`.
    pub labels: Vec<Option<String>>,
    /// Per page, which of `text`'s lines survived, by index. `furniture` reduces a line to
    /// its text, and `layout` needs the geometry back — the indent, the right gap and the
    /// run ids — so the way back to the `Line` has to be carried rather than re-derived.
    pub kept: Vec<Vec<usize>>,
    pub check: StageCheck,
}

/// What `layout` produced: blocks, cross-checked, with an empty ledger.
#[derive(Clone, Debug)]
pub struct LayoutStage {
    pub pages: Vec<LayoutPage>,
    pub blocks: Vec<Vec<Block>>,
    pub columns: Vec<ColumnLayout>,
    pub agreement: Vec<SegmentationAgreement>,
    /// How well the document read across its own page boundaries under the hypothesis that
    /// was kept.
    pub continuity: Continuity,
    /// How many times the column count had to be narrowed before it read that way.
    pub column_retries: u32,
    /// Per page, where each image sits in the flow.
    pub anchors: Vec<Vec<Anchor>>,
    /// Per page, the drop caps found on it.
    pub drop_caps: Vec<Vec<DropCap>>,
    pub delta: LedgerDelta,
    pub check: StageCheck,
}

/// Run `text`: assemble glyphs into runs and lines, then apply `N` exactly once.
///
/// The order is the one PIPELINE §4 insists on. Superscript and subscript are read from
/// geometry during assembly, *before* normalisation, because a normalisation that folded `¹`
/// to `1` would destroy the signal in the same pass — which is why NFKC is banned and why
/// this ordering is a property of the stage rather than an implementation detail.
pub fn text_stage(
    input: &[PageInput],
    totals: &mut ReasonTotals,
    t: &Thresholds,
) -> Result<TextStage, ConservationError> {
    let before = glyph_chars(input);

    let mut delta = LedgerDelta::default();
    let mut pages = Vec::with_capacity(input.len());
    let mut fonts: Vec<FontInfo> = Vec::new();
    for page in input {
        let remap = merge_fonts(&mut fonts, &page.fonts);
        let mut assembly = assemble_runs(&page.glyphs, page.page.clone(), t);
        let mut offset: u32 = 0;
        for run in &mut assembly.runs {
            run.font = remap
                .get(usize::from(run.font.0))
                .copied()
                .unwrap_or(run.font);
            let width = u32::try_from(run.text.chars().count()).unwrap_or(u32::MAX);
            let site = LedgerSite {
                stage: stages::TEXT.name,
                page: page.page.index,
                char_offset: offset,
            };
            run.text = normalize(&run.text, &mut delta, site).into_string();
            offset = offset.saturating_add(width);
        }
        let lines = assemble_lines(&assembly.runs, t);
        pages.push(TextPage {
            page: page.page.clone(),
            width_pt: page.width_pt,
            height_pt: page.height_pt,
            images: page.images.clone(),
            runs: assembly.runs,
            lines,
        });
    }

    let after = run_chars(&pages);
    // `C_0` is defined *after* `N`, so the account the budgets are fractions of is opened
    // here rather than at extraction (ARCHITECTURE §5.2).
    *totals = ReasonTotals::new(&after);
    let check = check_invariants(&before, &after, &delta, stages::TEXT, totals)?;

    Ok(TextStage {
        pages,
        fonts,
        delta,
        c_0: after,
        check,
    })
}

/// Merge one page's font table into the document's, returning the page's id-to-document-id
/// map.
///
/// Keyed on the declared name, which is what the backend's own per-page interning keys on —
/// so two pages that draw in one face agree, and two faces that differ only in their subset
/// prefix stay apart until `family_key` folds them, which is a clustering decision and not
/// this function's.
fn merge_fonts(document: &mut Vec<FontInfo>, page: &[FontInfo]) -> Vec<FontId> {
    page.iter()
        .map(|font| {
            let position = match document.iter().position(|known| known.name == font.name) {
                Some(position) => position,
                None => {
                    document.push(font.clone());
                    document.len() - 1
                }
            };
            let id = FontId(u16::try_from(position).unwrap_or(u16::MAX));
            if let Some(entry) = document.get_mut(position) {
                entry.id = id;
            }
            id
        })
        .collect()
}

/// Run `furniture`: find the running heads, feet and page numbers, and remove them.
pub fn furniture_stage(
    text: &TextStage,
    lang: LangTag,
    totals: &mut ReasonTotals,
    t: &Thresholds,
) -> Result<FurnitureStage, ConservationError> {
    let before = run_chars(&text.pages);

    let pages: Vec<PageLines> = text
        .pages
        .iter()
        .map(|page| {
            PageLines::from_runs(page.page.clone(), page.height_pt, &page.runs, &page.lines)
        })
        .collect();
    let verdicts = detect_furniture(&pages, lang, t);
    let outcome = apply_furniture(&pages, &verdicts);

    // The verdicts are page-major and line-aligned with the input, so survival is readable
    // straight off them; a line that stayed is a line `layout` still has the geometry of.
    let mut kept = Vec::with_capacity(pages.len());
    let mut offset = 0usize;
    for page in &pages {
        kept.push(
            (0..page.lines.len())
                .filter(|line| {
                    verdicts
                        .get(offset + line)
                        .is_none_or(|verdict| verdict.kind.is_none())
                })
                .collect(),
        );
        offset += page.lines.len();
    }

    let after = line_chars(&outcome.pages);
    let check = check_invariants(&before, &after, &outcome.delta, stages::FURNITURE, totals)?;

    Ok(FurnitureStage {
        pages: outcome.pages,
        delta: outcome.delta,
        labels: outcome.labels,
        kept,
        check,
    })
}

/// Run `layout`: group the surviving lines into blocks and cross-check the segmentation.
///
/// Conserving, and checked as such: the stage rearranges text into blocks and may not change
/// one character of it, so its ledger is empty and I-3 reduces to plain multiset equality
/// (PIPELINE §6).
pub fn layout_stage(
    text: &TextStage,
    furniture: &FurnitureStage,
    totals: &mut ReasonTotals,
    t: &Thresholds,
) -> Result<LayoutStage, ConservationError> {
    let before = line_chars(&furniture.pages);

    let source: Vec<LayoutPage> = text
        .pages
        .iter()
        .enumerate()
        .map(|(index, page)| LayoutPage {
            page: page.page.clone(),
            width_pt: page.width_pt,
            height_pt: page.height_pt,
            lines: furniture
                .kept
                .get(index)
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .filter_map(|line| page.lines.get(*line))
                .map(|line| LayoutLine {
                    line: line.clone(),
                    text: line_text(&page.runs, line),
                    segments: line
                        .runs
                        .iter()
                        .filter_map(|id| page.runs.get(id.0 as usize))
                        .map(|run| Segment {
                            run: run.id,
                            bbox: run.bbox,
                            text: run.text.trim().to_owned(),
                            size_pt: run.size_pt,
                        })
                        .filter(|segment| !segment.text.is_empty())
                        .collect(),
                })
                .collect(),
        })
        .collect();

    // The column hypothesis, checked by its consequences (PIPELINE §6 step 4). A wrong one
    // reorders the page, and a reordered page stops flowing into the next; so the document is
    // laid out, its cross-page continuity measured, and — only if a narrower hypothesis
    // actually reads better — laid out again with one column fewer.
    let mut best = lay_out(&source, usize::MAX, t);
    let mut retries = 0u32;
    let max_break = t.layout.columns.continuity_break_max as f32;
    let max_retries = u32::try_from(t.layout.columns.max_column_retries.max(0)).unwrap_or(0);
    while best.continuity.break_rate() > max_break && retries < max_retries {
        let narrower = best
            .columns
            .iter()
            .map(oc_layout::columns::ColumnLayout::count)
            .max()
            .unwrap_or(1)
            .saturating_sub(1);
        if narrower < 1 {
            break;
        }
        let candidate = lay_out(&source, narrower, t);
        // A re-run that does not read better is not evidence. Without this comparison a
        // genuine two-column document whose one page boundary happens to fall at the end of a
        // sentence would be downgraded on a single sample.
        if candidate.continuity.break_rate() >= best.continuity.break_rate() {
            break;
        }
        best = candidate;
        retries += 1;
    }

    let after = block_chars(&best.blocks, &best.pages);
    let delta = LedgerDelta::default();
    let check = check_invariants(&before, &after, &delta, stages::LAYOUT, totals)?;

    let anchors: Vec<Vec<Anchor>> = text
        .pages
        .iter()
        .zip(&best.blocks)
        .map(|(page, blocks)| anchor_images(&page.images, blocks))
        .collect();
    let drop_caps: Vec<Vec<DropCap>> = best
        .pages
        .iter()
        .zip(&best.blocks)
        .map(|(page, blocks)| drop_caps(page, blocks, t))
        .collect();

    Ok(LayoutStage {
        pages: best.pages,
        blocks: best.blocks,
        columns: best.columns,
        agreement: best.agreement,
        continuity: best.continuity,
        column_retries: retries,
        anchors,
        drop_caps,
        delta,
        check,
    })
}

/// One pass of the layout stage at a given column cap: columns, the line splits they imply,
/// blocks, reading order, and the continuity that results.
fn lay_out(source: &[LayoutPage], limit: usize, t: &Thresholds) -> LayoutPass {
    let mut pages = Vec::with_capacity(source.len());
    let mut blocks = Vec::with_capacity(source.len());
    let mut columns = Vec::with_capacity(source.len());
    let mut agreement = Vec::with_capacity(source.len());

    for page in source {
        let ink: Vec<Rect> = page
            .lines
            .iter()
            .flat_map(|line| line.segments.iter().map(|segment| segment.bbox))
            .collect();
        let em = median_height(&page.lines.iter().map(LayoutLine::bbox).collect::<Vec<_>>());
        let layout = detect_columns(&ink, em, limit, t);

        let page = LayoutPage {
            lines: split_lines_at_gutters(&page.lines, &layout),
            ..page.clone()
        };
        let (mut page_blocks, page_agreement) = segment_blocks(&page, &layout, t);
        assign_columns(&mut page_blocks, &layout);
        // No masks yet: `ingest` carries images and rules, and anchoring them is this phase's
        // last item. Blocks that span columns are pre-masked either way.
        reading_order(&mut page_blocks, &layout, &[], t);
        let (page_blocks, page_agreement) = into_reading_order(page_blocks, page_agreement);

        pages.push(page);
        blocks.push(page_blocks);
        columns.push(layout);
        agreement.push(page_agreement);
    }

    let reading: Vec<Vec<String>> = blocks
        .iter()
        .zip(&pages)
        .map(|(page_blocks, page)| {
            page_blocks
                .iter()
                .map(|block| block_text(block, page))
                .collect()
        })
        .collect();

    LayoutPass {
        continuity: continuity(&reading),
        pages,
        blocks,
        columns,
        agreement,
    }
}

/// One attempt at laying out the document, kept whole so two can be compared.
struct LayoutPass {
    pages: Vec<LayoutPage>,
    blocks: Vec<Vec<Block>>,
    columns: Vec<ColumnLayout>,
    agreement: Vec<SegmentationAgreement>,
    continuity: Continuity,
}

/// What `paragraphs` produced.
#[derive(Clone, Debug)]
pub struct ParagraphStage {
    /// How this book marks a paragraph start, decided once over the whole of it.
    pub convention: ParagraphConvention,
    /// The document's paragraphs, in reading order, across columns and pages.
    pub paragraphs: Vec<Para>,
    /// The document's own vocabulary, which is what decided most of the hyphens.
    pub lexicon: DocLexicon,
    pub delta: LedgerDelta,
    pub check: StageCheck,
}

/// Run `paragraphs`: lines into paragraphs, then dehyphenation (PIPELINE §7).
///
/// Budgeted over `Dehyphenate` and nothing else. Reconstruction changes no text — it decides
/// where one paragraph ends and the next begins — so every character this stage removes is a
/// hyphen at a line break, and I-5 says so entry by entry.
///
/// The order inside the stage is not incidental. The in-document lexicon is built from the
/// paragraphs *before* any hyphen is resolved, because it is evidence about the book and a
/// lexicon built from already-joined text would be evidence about this function's own
/// earlier decisions.
pub fn paragraphs_stage(
    layout: &LayoutStage,
    lang: LangTag,
    totals: &mut ReasonTotals,
    t: &Thresholds,
) -> Result<ParagraphStage, ConservationError> {
    let before = block_chars(&layout.blocks, &layout.pages);

    let convention = infer_convention(&layout.pages, &layout.blocks, t);
    let mut built = reconstruct_paragraphs(&layout.pages, &layout.blocks, convention, t);
    let lexicon = DocLexicon::build(
        built.paragraphs.iter().map(|para| para.text.as_str()),
        &lang,
    );
    let delta = dehyphenate_paragraphs(&mut built, &lexicon, &lang, stages::PARAGRAPHS.name, t);

    let after = paragraph_chars(&built.paragraphs);
    let check = check_invariants(&before, &after, &delta, stages::PARAGRAPHS, totals)?;

    Ok(ParagraphStage {
        convention,
        paragraphs: built.paragraphs,
        lexicon,
        delta,
        check,
    })
}

/// What `structure` produced, and the check that says it labelled rather than edited.
pub struct StructureStage {
    pub output: oc_structure::stage::StructureOutput,
    pub delta: LedgerDelta,
    pub check: StageCheck,
}

/// Run `structure`: roles, the section tree, notes, figures, tables and metadata (PIPELINE §8).
///
/// Conserving, and checked as such against the blocks `layout` produced. The check is the
/// reason the stage is written the way it is: a block's text lands in exactly one of a
/// section's content, a note's body, a table's cells or a figure's caption, and anything
/// counted twice or dropped fails plain multiset equality here rather than silently in an
/// EPUB somebody reads.
pub fn structure_stage(
    layout: &LayoutStage,
    input: &oc_structure::stage::StructureInput,
    totals: &mut ReasonTotals,
    t: &Thresholds,
) -> Result<StructureStage, ConservationError> {
    let before = block_chars(&layout.blocks, &layout.pages);
    let output = oc_structure::stage::structure(input, t);

    let emitted = output.emitted_text();
    let after = c_of_parts(emitted.iter().map(String::as_str));
    let delta = LedgerDelta::default();
    let check = check_invariants(&before, &after, &delta, stages::STRUCTURE, totals)?;

    Ok(StructureStage {
        output,
        delta,
        check,
    })
}

/// What `document` produced: the book, and the check that says assembling it changed nothing.
pub struct DocumentStage {
    pub document: oc_model::document::Document,
    pub delta: LedgerDelta,
    pub check: StageCheck,
}

/// Run `document`: page breaks, classification, the preset, and closure (PIPELINE §9).
///
/// Conserving, and checked as such against what `structure` emitted. The stage moves no text
/// between the four places it can live and adds none: a `PageBreak` carries the printed page
/// label, and a label is outside `C` (ARCHITECTURE §5.2). So the multiset before and the
/// multiset after are the same multiset, and the check says so rather than the comment.
///
/// The stage's own postcondition is closure: every reference in the flow names something the
/// document carries. A dangling one here becomes an `<img src>` or an `<a href>` pointing at
/// a file the manifest does not list, and it is cheaper to fail now than to have EPUBCheck
/// find it (PIPELINE §9, "every nav target resolves to a heading id").
pub fn document_stage(
    structure: &StructureStage,
    input: crate::document::DocumentInput<'_>,
    totals: &mut ReasonTotals,
    t: &Thresholds,
) -> Result<DocumentStage, DocumentError> {
    let emitted = structure.output.emitted_text();
    let before = c_of_parts(emitted.iter().map(String::as_str));

    let document = crate::document::assemble(input, t);

    let dangling = document.dangling_references();
    if !dangling.is_empty() {
        return Err(DocumentError::Dangling(dangling));
    }

    let pieces = document.text_pieces();
    let after = c_of_parts(pieces.iter().map(String::as_str));
    let delta = LedgerDelta::default();
    let check = check_invariants(&before, &after, &delta, stages::DOCUMENT, totals)?;

    Ok(DocumentStage {
        document,
        delta,
        check,
    })
}

/// Why assembling the document failed.
#[derive(Debug, thiserror::Error)]
pub enum DocumentError {
    #[error(transparent)]
    Conservation(#[from] ConservationError),
    /// The flow names something the document does not carry. Listed rather than counted: a
    /// failure that says *what* is missing is a bug report, and one that says how many is not.
    #[error("the flow refers to {} things this document does not carry: {}", .0.len(), .0.join(", "))]
    Dangling(Vec<String>),
}

/// What `epub` produced: the container, and the check that says the book came through it.
pub struct EpubStage {
    pub built: oc_epub::BuiltEpub,
    pub delta: LedgerDelta,
    pub check: StageCheck,
}

/// Run `epub`: serialise the container (PIPELINE §10).
///
/// Conserving, and checked by **reading the output back**. The check parses the `<body>` of
/// every content document it emitted and compares that multiset against the document's own —
/// not against what the emitter believes it wrote, because an emitter checked against its own
/// intentions is checked against nothing (D6). Metadata, `alt` and page-list labels are
/// outside `C` by definition (ARCHITECTURE §5.2) and the parse leaves them out; `nav.xhtml` is
/// nav text and is left out for the same reason.
pub fn epub_stage(
    document: &oc_model::document::Document,
    images: &[oc_epub::images::SourceImage],
    options: &oc_epub::EpubOptions,
    totals: &mut ReasonTotals,
) -> Result<EpubStage, EpubStageError> {
    let built = oc_epub::build_epub(document, images, options)?;
    epub_check(document, built, totals)
}

/// The `epub` stage's conservation check, over a container that has already been built.
///
/// Split from the build because the validate→repair loop is the only thing that emits on the real
/// path: the loop may re-emit up to `repair.max_iterations` times, and a `convert` that built the
/// book once for the check and again for the loop would re-encode every image in the book twice for
/// no reason (PIPELINE §12 budgets one regeneration *per iteration*, not two).
pub fn epub_check(
    document: &oc_model::document::Document,
    built: oc_epub::BuiltEpub,
    totals: &mut ReasonTotals,
) -> Result<EpubStage, EpubStageError> {
    let pieces = document.text_pieces();
    let before = c_of_parts(pieces.iter().map(String::as_str));

    let mut bodies: Vec<String> = Vec::with_capacity(built.emitted.files.len());
    for file in &built.emitted.files {
        bodies.push(oc_epub::textcontent::body_text(&file.markup)?);
    }
    let after = c_of_parts(bodies.iter().map(String::as_str));

    let delta = LedgerDelta::default();
    let check = check_invariants(&before, &after, &delta, stages::EPUB, totals)?;

    Ok(EpubStage {
        built,
        delta,
        check,
    })
}

/// What `validate` and `repair` produced (PIPELINE §11, §12).
pub struct ValidateRepairStage {
    /// The container the loop settled on.
    pub built: oc_epub::BuiltEpub,
    /// The document that produced it: the original, unless a repair was accepted.
    pub document: oc_model::document::Document,
    pub outcome: oc_validate::repair::RepairOutcome,
    pub tier1: oc_validate::Tier1Report,
    pub structural: oc_validate::structural::StructuralReport,
    /// `validate` then `repair`, both Conserving, in PIPELINE's order.
    pub checks: Vec<StageCheck>,
}

/// Run `validate` and `repair`: the loop, then the final report over what it settled on.
///
/// The loop owns the emission, because each of its iterations is one `epub` regeneration plus one
/// `validate` pass and the caller cannot know in advance how many there will be. What comes back is
/// the container to write, the document that produced it, and the two stages' conservation checks.
///
/// **`repair`'s check is the one that earns its keep.** It compares `C` of the document the loop was
/// given against `C` of the document it settled on, with an empty ledger — so a repair that changed
/// one character of the book fails I-1 and the conversion stops. That is the whole of PIPELINE §12's
/// "repairs are structural; none of them may change the character content of the book", stated as an
/// invariant rather than as a property of three functions nobody re-reads.
pub fn validate_repair_stage(
    document: &oc_model::document::Document,
    images: &[oc_epub::images::SourceImage],
    options: &oc_epub::EpubOptions,
    expectations: oc_validate::Expectations,
    page_count: u32,
    totals: &mut ReasonTotals,
    t: &Thresholds,
) -> Result<ValidateRepairStage, ValidateRepairError> {
    let mut host = oc_validate::repair::EpubHost::new(images, options, expectations);
    let opts = oc_validate::repair::RepairOpts {
        max_iterations: u32::try_from(t.repair.max_iterations).unwrap_or(1),
        require_strict_decrease: t.repair.require_strict_decrease,
    };
    let outcome = oc_validate::repair::repair_loop(document, &mut host, &opts)?;

    let built = host
        .take_built()
        .ok_or(ValidateRepairError::NothingEmitted)?;

    // The loop's last emission is the one it settled on only when nothing was reverted. A rejected
    // repair leaves `outcome.document` as the original, and the container the host is holding is
    // the candidate's — so the settled document is re-emitted in that case, and only in it.
    let settled = outcome.document.clone();
    let built = if settled == *document {
        match &outcome.status {
            oc_validate::repair::RepairStatus::NoProgress => {
                oc_epub::build_epub(document, images, options)?
            }
            _ => built,
        }
    } else {
        built
    };

    let tier1 = oc_validate::validate_tier1(&built.bytes, &expectations);
    let structural = oc_validate::structural::validate_structural(
        &settled,
        &built.bytes,
        &tier1,
        page_count,
        t,
    )?;

    // `validate` is read-only: `C` on either side is the container's own text.
    let emitted = oc_validate::structural::epub_chars(&built.bytes)?;
    let empty = LedgerDelta::default();
    let validate_check = check_invariants(&emitted, &emitted, &empty, stages::VALIDATE, totals)?;

    // `repair` compares the two documents. Equal by I-1 with an empty ledger, which is the claim.
    let given = document.text_pieces();
    let before = c_of_parts(given.iter().map(String::as_str));
    let kept = settled.text_pieces();
    let after = c_of_parts(kept.iter().map(String::as_str));
    let repair_check = check_invariants(&before, &after, &empty, stages::REPAIR, totals)?;

    Ok(ValidateRepairStage {
        built,
        document: settled,
        outcome,
        tier1,
        structural,
        checks: vec![validate_check, repair_check],
    })
}

/// Why `validate` or `repair` could not finish.
#[derive(Debug, thiserror::Error)]
pub enum ValidateRepairError {
    #[error(transparent)]
    Conservation(#[from] ConservationError),
    #[error(transparent)]
    Host(#[from] oc_validate::repair::HostError),
    #[error(transparent)]
    Emit(#[from] oc_epub::EpubError),
    #[error(transparent)]
    Structural(#[from] oc_validate::StructuralError),
    /// The loop returned without ever emitting, which cannot happen: it emits before it decides
    /// anything. Stated rather than unwrapped, because `main()` is the only place that may panic.
    #[error("the repair loop returned without emitting a container")]
    NothingEmitted,
}

/// Why the container could not be emitted.
#[derive(Debug, thiserror::Error)]
pub enum EpubStageError {
    #[error(transparent)]
    Conservation(#[from] ConservationError),
    #[error(transparent)]
    Emit(#[from] oc_epub::EpubError),
    /// The emitter produced a document it cannot read back. There is no fallback path here:
    /// "an emitter failure is a bug, and the correct response is to fix the emitter"
    /// (PIPELINE §10).
    #[error("the emitted XHTML could not be read back: {0}")]
    Reparse(#[from] oc_epub::textcontent::TextError),
}

/// The runs that survive into the body flow: those of the lines `furniture` kept.
///
/// What `structure`'s style clustering is fed. Clustering before furniture removal would put
/// an 8 pt running head into the inventory as a style of its own, on every page of the book,
/// and the validity gate counts clusters (PIPELINE §8.2).
pub fn body_runs(text: &TextStage, furniture: &FurnitureStage) -> Vec<Run> {
    text.pages
        .iter()
        .enumerate()
        .flat_map(|(index, page)| {
            furniture
                .kept
                .get(index)
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .filter_map(|line| page.lines.get(*line))
                .flat_map(|line| line.runs.iter())
                .filter_map(|id| page.runs.get(usize::try_from(id.0).unwrap_or(usize::MAX)))
                .cloned()
                .collect::<Vec<Run>>()
        })
        .collect()
}

/// `C` of the reconstructed paragraphs.
pub fn paragraph_chars(paragraphs: &[Para]) -> CharHistogram {
    c_of_parts(paragraphs.iter().map(|para| para.text.as_str()))
}

/// One line's text, found on the page it came from.
///
/// Keyed on the run ids, which are the only part of a `Line` that survives being put into a
/// block unchanged: `blocks` re-measures the indent and the right gap against the block, so
/// the line in the block is deliberately not equal to the line on the page.
pub fn text_of<'a>(page: &'a LayoutPage, line: &Line) -> &'a str {
    page.lines
        .iter()
        .find(|candidate| candidate.line.runs == line.runs)
        .map(|candidate| candidate.text.as_str())
        .unwrap_or_default()
}

/// A block's text, its lines joined by spaces, as the continuity proxy reads it.
pub fn block_text(block: &Block, page: &LayoutPage) -> String {
    block
        .lines
        .iter()
        .map(|line| text_of(page, line))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Put a page's blocks into reading order, carrying the agreement vectors with them.
///
/// The two are index-aligned by construction — `segment_blocks` returns them that way — and
/// a permutation applied to one and not the other would report the wrong block as the
/// low-confidence one, which is worse than not reporting it at all.
fn into_reading_order(
    blocks: Vec<Block>,
    agreement: SegmentationAgreement,
) -> (Vec<Block>, SegmentationAgreement) {
    let mut permutation: Vec<usize> = (0..blocks.len()).collect();
    permutation.sort_by_key(|index| {
        blocks
            .get(*index)
            .map(|block| block.reading_index)
            .unwrap_or(u32::MAX)
    });
    let iou = permutation
        .iter()
        .filter_map(|index| agreement.iou.get(*index).copied())
        .collect();
    let low_confidence = permutation
        .iter()
        .filter_map(|index| agreement.low_confidence.get(*index).copied())
        .collect();
    let mut ordered: Vec<Option<Block>> = blocks.into_iter().map(Some).collect();
    let blocks = permutation
        .iter()
        .filter_map(|index| ordered.get_mut(*index).and_then(Option::take))
        .collect();
    (
        blocks,
        SegmentationAgreement {
            iou,
            low_confidence,
            whitespace: agreement.whitespace,
        },
    )
}

/// One line's text, assembled from the page's runs.
///
/// The same flattening `furniture` does, and it has to stay the same: `layout`'s "before"
/// multiset is the one `furniture` left behind, so a different join here would show up as a
/// conservation violation in a stage that changed nothing.
pub fn line_text(runs: &[Run], line: &Line) -> String {
    line.runs
        .iter()
        .filter_map(|id| runs.get(id.0 as usize))
        .map(|run| run.text.as_str())
        .collect::<String>()
        .trim()
        .to_owned()
}

/// `C` of the segmented blocks — every line of every block, through the same flattening.
pub fn block_chars(blocks: &[Vec<Block>], pages: &[LayoutPage]) -> CharHistogram {
    let mut parts: Vec<&str> = Vec::new();
    for (page_blocks, page) in blocks.iter().zip(pages) {
        for block in page_blocks {
            for line in &block.lines {
                parts.push(text_of(page, line));
            }
        }
    }
    c_of_parts(parts)
}

/// `C` of the glyph stream: the document as extraction left it.
pub fn glyph_chars(input: &[PageInput]) -> CharHistogram {
    let text: String = input
        .iter()
        .flat_map(|page| page.glyphs.iter().map(|glyph| glyph.ch))
        .collect();
    c_of(&text)
}

/// `C` of the assembled runs.
pub fn run_chars(pages: &[TextPage]) -> CharHistogram {
    c_of_parts(
        pages
            .iter()
            .flat_map(|page| page.runs.iter().map(|run| run.text.as_str())),
    )
}

/// `C` of the surviving body flow.
pub fn line_chars(pages: &[PageLines]) -> CharHistogram {
    c_of_parts(
        pages
            .iter()
            .flat_map(|page| page.lines.iter().map(|line| line.text.as_str())),
    )
}
