//! The `text`, `furniture` and `layout` stages, driven end to end under the conservation law.
//!
//! Each stage is run and then immediately checked: `C(D_before) ⊎ Added == C(D_after) ⊎
//! Removed`, the reasons it cited are ones it declared, and its cumulative removals are
//! inside their budgets (ARCHITECTURE §5.4). The check is not an assertion in a test — it
//! runs on every conversion, and a violation is an error rather than a warning, because a
//! conservation law that can be disabled is a preference (D13.4).

use oc_core::ledger_check::{c_of, check_invariants, ConservationError, ReasonTotals};
use oc_core::stages;
use oc_core::thresholds::Thresholds;
use oc_layout::blocks::{segment_blocks, LayoutLine, LayoutPage, Segment, SegmentationAgreement};
use oc_layout::columns::{
    assign_columns, detect_columns, median_height, split_lines_at_gutters, ColumnLayout,
};
use oc_layout::continuity::{continuity, Continuity};
use oc_layout::furniture::{apply_furniture, detect_furniture, PageLines};
use oc_layout::paragraphs::{infer_convention, reconstruct_paragraphs};
use oc_layout::reading_order::reading_order;
use oc_model::extract::{CharHistogram, Glyph, PageRef};
use oc_model::geom::Rect;
use oc_model::lang::LangTag;
use oc_model::layout::{Block, Para, ParagraphConvention};
use oc_model::ledger::{LedgerDelta, StageCheck};
use oc_model::text::{Line, Run};
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
}

/// One page as `text` leaves it.
#[derive(Clone, Debug, PartialEq)]
pub struct TextPage {
    pub page: PageRef,
    pub width_pt: f32,
    pub height_pt: f32,
    pub runs: Vec<Run>,
    pub lines: Vec<Line>,
}

/// What `text` produced, and the check that says it did not lose anything.
#[derive(Clone, Debug)]
pub struct TextStage {
    pub pages: Vec<TextPage>,
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
    /// How this book marks a paragraph start, decided once over the whole of it.
    pub convention: ParagraphConvention,
    /// The document's paragraphs, in reading order, across columns and pages.
    pub paragraphs: Vec<Para>,
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
    for page in input {
        let mut assembly = assemble_runs(&page.glyphs, page.page.clone(), t);
        let mut offset: u32 = 0;
        for run in &mut assembly.runs {
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
        delta,
        c_0: after,
        check,
    })
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

    // Paragraphs are `paragraphs`' stage, not `layout`'s, and the split matters: `layout` is
    // Conserving and this is the last point at which that is still true of everything here.
    // Reconstruction itself adds and removes nothing — it is the dehyphenation inside it that
    // is Budgeted, and that has its own stage and its own ledger.
    let convention = infer_convention(&best.pages, &best.blocks, t);
    let paragraphs = reconstruct_paragraphs(&best.pages, &best.blocks, convention, t);

    let after = block_chars(&best.blocks, &best.pages);
    let delta = LedgerDelta::default();
    let check = check_invariants(&before, &after, &delta, stages::LAYOUT, totals)?;

    Ok(LayoutStage {
        pages: best.pages,
        blocks: best.blocks,
        columns: best.columns,
        agreement: best.agreement,
        continuity: best.continuity,
        column_retries: retries,
        convention,
        paragraphs,
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
        let (mut page_blocks, page_agreement) = segment_blocks(&page, t);
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
    let mut histogram = CharHistogram::new();
    for (page_blocks, page) in blocks.iter().zip(pages) {
        for block in page_blocks {
            for line in &block.lines {
                histogram = histogram.union(&c_of(text_of(page, line)));
            }
        }
    }
    histogram
}

/// `C` of the glyph stream: the document as extraction left it.
pub fn glyph_chars(input: &[PageInput]) -> CharHistogram {
    let mut histogram = CharHistogram::new();
    for page in input {
        for glyph in &page.glyphs {
            if !glyph.ch.is_whitespace() {
                histogram.add(glyph.ch);
            }
        }
    }
    histogram
}

/// `C` of the assembled runs.
pub fn run_chars(pages: &[TextPage]) -> CharHistogram {
    let mut histogram = CharHistogram::new();
    for page in pages {
        for run in &page.runs {
            histogram = histogram.union(&c_of(&run.text));
        }
    }
    histogram
}

/// `C` of the surviving body flow.
pub fn line_chars(pages: &[PageLines]) -> CharHistogram {
    let mut histogram = CharHistogram::new();
    for page in pages {
        for line in &page.lines {
            histogram = histogram.union(&c_of(&line.text));
        }
    }
    histogram
}
