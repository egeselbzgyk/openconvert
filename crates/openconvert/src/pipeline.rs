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
use oc_layout::blocks::{segment_blocks, LayoutLine, LayoutPage, SegmentationAgreement};
use oc_layout::columns::{assign_columns, detect_columns, ColumnLayout};
use oc_layout::furniture::{apply_furniture, detect_furniture, PageLines};
use oc_layout::reading_order::reading_order;
use oc_model::extract::{CharHistogram, Glyph, PageRef};
use oc_model::lang::LangTag;
use oc_model::layout::Block;
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

    let pages: Vec<LayoutPage> = text
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
                })
                .collect(),
        })
        .collect();

    let mut blocks = Vec::with_capacity(pages.len());
    let mut columns = Vec::with_capacity(pages.len());
    let mut agreement = Vec::with_capacity(pages.len());
    for page in &pages {
        let (mut page_blocks, page_agreement) = segment_blocks(page, t);
        let layout = detect_columns(&page_blocks, t);
        assign_columns(&mut page_blocks, &layout);
        // No masks yet: `ingest` carries images and rules, and anchoring them is this
        // phase's last item. Blocks that span columns are pre-masked either way.
        reading_order(&mut page_blocks, &layout, &[], t);
        let (page_blocks, page_agreement) = into_reading_order(page_blocks, page_agreement);
        blocks.push(page_blocks);
        columns.push(layout);
        agreement.push(page_agreement);
    }

    let after = block_chars(&blocks, &pages);
    let delta = LedgerDelta::default();
    let check = check_invariants(&before, &after, &delta, stages::LAYOUT, totals)?;

    Ok(LayoutStage {
        pages,
        blocks,
        columns,
        agreement,
        delta,
        check,
    })
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
                let text = page
                    .lines
                    .iter()
                    .find(|candidate| candidate.line == *line)
                    .map(|candidate| candidate.text.clone())
                    .unwrap_or_default();
                histogram = histogram.union(&c_of(&text));
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
