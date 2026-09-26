//! The layout layer as canonical JSON: `dump-stage layout` (PIPELINE §6, Phase 3 tests 3.15
//! and 3.16).
//!
//! The third of the stage dumps, and the first one whose subject is a *decision* rather than a
//! measurement. `ingest` says what the backend gave us and `text` says what we made of it;
//! this says how the page was read — which lines the segmenters put together, whether they
//! agreed, where the gutters are, in what order a reader meets the blocks, and how much of
//! that survived the cross-page continuity check.
//!
//! Two things are written that a naive dump would leave out, and both are here because they
//! are what a wrong conversion is diagnosed from:
//!
//! * **the disagreement**, per block, between Docstrum and the whitespace cover — the page's
//!   own opinion of how much to trust its segmentation; and
//! * **the column retries**, because a document whose column count was withdrawn read
//!   differently from the one the geometry first proposed, and a report that does not say so
//!   leaves the reader guessing at a result nothing on the page explains.

use serde::Serialize;

use oc_core::ledger_check::{ConservationError, ReasonTotals};
use oc_core::thresholds::Thresholds;
use oc_layout::anchor::{Anchor, DropCap};
use oc_layout::columns::Gutter;
use oc_model::geom::Rect;
use oc_model::lang::LangTag;
use oc_model::layout::Block;
use oc_model::ledger::StageCheck;

use crate::pipeline::{furniture_stage, layout_stage, text_stage, LayoutStage, PageInput};

/// The stage name this dump belongs to. One of the twelve fixed stage names.
pub const STAGE: &str = "layout";

/// The schema tag every dump carries, so a consumer can refuse a shape it does not know.
const SCHEMA: &str = "openconvert.dump.layout/1";

/// What the document as a whole contributes.
#[derive(Clone, Debug, Serialize)]
pub struct DumpHeader {
    pub schema: &'static str,
    pub stage: &'static str,
    /// First in the output, by D13.3 — a reader has to know the version before the shape.
    pub ir_version: u32,
    pub pages: u32,
    pub blocks: u32,
    /// Page boundaries where the text ran on, out of those examined.
    pub continuity_held: u32,
    pub continuity_boundaries: u32,
    /// How many times the column hypothesis was narrowed before the document read better.
    pub column_retries: u32,
    /// Blocks the two segmenters disagreed about, document-wide.
    pub low_confidence_blocks: u32,
    pub check: StageCheck,
}

/// One page's layout.
#[derive(Clone, Debug, Serialize)]
pub struct DumpPage {
    pub index: u32,
    pub width_pt: f32,
    pub height_pt: f32,
    pub columns: Vec<DumpColumn>,
    pub gutters: Vec<Gutter>,
    pub blocks: Vec<DumpBlock>,
    pub anchors: Vec<Anchor>,
    pub drop_caps: Vec<DropCap>,
}

/// One column's extent. A tuple in the IR; named here because a dump is read by people.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct DumpColumn {
    pub x0: f32,
    pub x1: f32,
}

/// One block, with what the cross-check thought of it.
#[derive(Clone, Debug, Serialize)]
pub struct DumpBlock {
    #[serde(flatten)]
    pub block: Block,
    /// Best IoU against any block the whitespace cover proposed.
    pub agreement_iou: f32,
    pub low_confidence: bool,
    /// The block's text, so that a reading-order question can be answered by reading the dump
    /// rather than by reconstructing it.
    pub text: String,
}

/// Run the stage and describe it.
pub fn dump(
    input: &[PageInput],
    lang: LangTag,
    t: &Thresholds,
) -> Result<(DumpHeader, Vec<DumpPage>), ConservationError> {
    let mut totals = ReasonTotals::default();
    let mut text = text_stage(input, &mut totals, t)?;
    let furniture = furniture_stage(&mut text, lang, &mut totals, t)?;
    let layout = layout_stage(&text, &furniture, &mut totals, t)?;

    Ok((header(&layout), pages(&layout)))
}

/// The document-level half of the dump.
pub fn header(layout: &LayoutStage) -> DumpHeader {
    DumpHeader {
        schema: SCHEMA,
        stage: STAGE,
        ir_version: oc_model::IR_VERSION,
        pages: u32::try_from(layout.pages.len()).unwrap_or(u32::MAX),
        blocks: layout
            .blocks
            .iter()
            .map(|page| u32::try_from(page.len()).unwrap_or(u32::MAX))
            .sum(),
        continuity_held: layout.continuity.held,
        continuity_boundaries: layout.continuity.boundaries,
        column_retries: layout.column_retries,
        low_confidence_blocks: layout
            .agreement
            .iter()
            .map(|page| u32::try_from(page.low_confidence_count()).unwrap_or(u32::MAX))
            .sum(),
        check: layout.check.clone(),
    }
}

/// The per-page half.
pub fn pages(layout: &LayoutStage) -> Vec<DumpPage> {
    layout
        .pages
        .iter()
        .enumerate()
        .map(|(index, page)| DumpPage {
            index: page.page.index,
            width_pt: page.width_pt,
            height_pt: page.height_pt,
            columns: layout
                .columns
                .get(index)
                .map(|layout| {
                    layout
                        .columns
                        .iter()
                        .map(|(x0, x1)| DumpColumn { x0: *x0, x1: *x1 })
                        .collect()
                })
                .unwrap_or_default(),
            gutters: layout
                .columns
                .get(index)
                .map(|layout| layout.gutters.clone())
                .unwrap_or_default(),
            blocks: layout
                .blocks
                .get(index)
                .map(|blocks| {
                    blocks
                        .iter()
                        .enumerate()
                        .map(|(position, block)| DumpBlock {
                            text: crate::pipeline::block_text(block, page),
                            agreement_iou: layout
                                .agreement
                                .get(index)
                                .and_then(|agreement| agreement.iou.get(position).copied())
                                .unwrap_or(0.0),
                            low_confidence: layout
                                .agreement
                                .get(index)
                                .and_then(|agreement| {
                                    agreement.low_confidence.get(position).copied()
                                })
                                .unwrap_or(false),
                            block: block.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            anchors: layout.anchors.get(index).cloned().unwrap_or_default(),
            drop_caps: layout.drop_caps.get(index).cloned().unwrap_or_default(),
        })
        .collect()
}

/// The structural digest of a laid-out document (Phase 3 test 3.16).
///
/// Counts and totals, **no geometry and nothing derived from geometry**. A snapshot of the
/// full dump changes whenever a box moves by a hundredth of a point — which is exactly what a
/// host-dependent glyph box does (`docs/DECISIONS_LOG.md`, 2026-09-13) — so the thing that is
/// asserted across machines is the *shape* of the answer: how many blocks, in how many
/// columns, how many the segmenters disagreed about, what the ledger says.
///
/// The second half of that sentence was learned the hard way. This digest carried the page's
/// worst block-boundary IoU in thousandths, on the reasoning that an integer is stable — but
/// an IoU is a ratio of *areas*, so it inherits every hundredth of a point the boxes carry.
/// `h22` measured 101 on Windows and 102 on Ubuntu and the committed snapshot failed in CI.
/// How many blocks were flagged is the decision the cross-check produced, and it is here;
/// how nearly each one missed is a measurement, and it belongs in the dump, where it is.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Digest {
    pub pages: u32,
    pub blocks: u32,
    pub blocks_per_page: Vec<u32>,
    pub columns_per_page: Vec<u32>,
    pub lines: u32,
    pub paragraph_candidates: u32,
    pub low_confidence_blocks: u32,
    pub continuity_held: u32,
    pub continuity_boundaries: u32,
    pub column_retries: u32,
    pub anchors: u32,
    pub drop_caps: u32,
    pub removed_chars: u64,
    pub added_chars: u64,
}

/// Reduce a laid-out document to its digest.
pub fn digest(layout: &LayoutStage, t: &Thresholds) -> Digest {
    let blocks_per_page: Vec<u32> = layout
        .blocks
        .iter()
        .map(|page| u32::try_from(page.len()).unwrap_or(u32::MAX))
        .collect();
    Digest {
        pages: u32::try_from(layout.pages.len()).unwrap_or(u32::MAX),
        blocks: blocks_per_page.iter().sum(),
        blocks_per_page,
        columns_per_page: layout
            .columns
            .iter()
            .map(|layout| u32::try_from(layout.count()).unwrap_or(u32::MAX))
            .collect(),
        lines: layout
            .blocks
            .iter()
            .flat_map(|page| page.iter())
            .map(|block| u32::try_from(block.lines.len()).unwrap_or(u32::MAX))
            .sum(),
        // Lines indented by at least `paragraph.indent_min_em`, not by *anything at all*.
        // "Indented by more than zero" is a strict comparison on a float that carries every
        // hundredth of a point a glyph box does, and on a document set in a substituted
        // base-14 face those hundredths differ between operating systems. A whole em cannot
        // be crossed by a rounding difference, and it is also what the word "indented" means
        // to the stage that reads it (PIPELINE §7 step 3).
        paragraph_candidates: layout
            .pages
            .iter()
            .zip(&layout.blocks)
            .map(|(page, blocks)| {
                let em = oc_layout::columns::median_height(
                    &page
                        .lines
                        .iter()
                        .map(|line| line.line.bbox)
                        .collect::<Vec<_>>(),
                );
                let minimum = em * t.paragraph.indent_min_em as f32;
                u32::try_from(
                    blocks
                        .iter()
                        .flat_map(|block| block.lines.iter())
                        .filter(|line| line.indent_pt >= minimum)
                        .count(),
                )
                .unwrap_or(u32::MAX)
            })
            .sum(),
        low_confidence_blocks: layout
            .agreement
            .iter()
            .map(|page| u32::try_from(page.low_confidence_count()).unwrap_or(u32::MAX))
            .sum(),
        continuity_held: layout.continuity.held,
        continuity_boundaries: layout.continuity.boundaries,
        column_retries: layout.column_retries,
        anchors: layout
            .anchors
            .iter()
            .map(|page| u32::try_from(page.len()).unwrap_or(u32::MAX))
            .sum(),
        drop_caps: layout
            .drop_caps
            .iter()
            .map(|page| u32::try_from(page.len()).unwrap_or(u32::MAX))
            .sum(),
        removed_chars: layout.check.removed_chars,
        added_chars: layout.check.added_chars,
    }
}

/// A block's box, rounded to whole points — what a snapshot can assert across hosts.
pub fn rounded(bbox: Rect) -> (i32, i32, i32, i32) {
    (
        bbox.x0.round() as i32,
        bbox.y0.round() as i32,
        bbox.x1.round() as i32,
        bbox.y1.round() as i32,
    )
}
