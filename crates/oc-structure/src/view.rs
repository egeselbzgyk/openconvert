//! What `structure` is handed: the blocks of a laid-out document, in reading order, with
//! everything a semantic rule asks of them already measured.
//!
//! `structure` does not take a `LayoutStage`. It takes this, for two reasons. The first is
//! the dependency graph: the stage wiring lives in the binary crate (see `openconvert`'s
//! `lib.rs`), so a stage crate that took the wiring's types could not be a stage crate. The
//! second is that every rule in PIPELINE §8 asks the same handful of questions — what does
//! this block say, how wide is the column it sits in, how much air is above it, what are its
//! runs — and answering them once at the boundary is cheaper and far easier to test than
//! answering them nine times inside nine rules.

use oc_model::geom::Rect;
use oc_model::ids::BlockId;
use oc_model::layout::BlockKindHint;
use oc_model::text::Run;
use serde::Serialize;

/// One line of a block, with its runs resolved.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LineView {
    pub text: String,
    pub bbox: Rect,
    /// Measured against the *block*, as `layout` leaves it.
    pub indent_pt: f32,
    pub right_gap_pt: f32,
    pub runs: Vec<Run>,
}

impl LineView {
    /// The width of the line's own ink.
    pub fn width_pt(&self) -> f32 {
        self.bbox.x1 - self.bbox.x0
    }

    /// Whether every run of the line is set bold, by `headings.bold_weight_min`.
    pub fn is_bold(&self, t: &oc_core::thresholds::Thresholds) -> bool {
        !self.runs.is_empty()
            && self
                .runs
                .iter()
                .all(|run| i64::from(run.weight) >= t.headings.bold_weight_min)
    }

    /// The size the line is mostly set at: the size of its longest run.
    pub fn size_pt(&self) -> f32 {
        self.runs
            .iter()
            .max_by_key(|run| run.text.chars().count())
            .map_or(0.0, |run| run.size_pt)
    }
}

/// One block, with everything `structure` needs to read it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BlockView {
    pub id: BlockId,
    pub page: u32,
    /// Position in the document's reading order, across pages. Not the page-local one:
    /// `structure` walks the book, not the page.
    pub order: u32,
    pub bbox: Rect,
    pub column: u8,
    pub kind_hint: BlockKindHint,
    /// The block's lines joined by single spaces — the form a `BlockId` is derived from.
    pub text: String,
    pub lines: Vec<LineView>,
    /// The width of the column this block sits in. A heading's "short line" is short
    /// relative to its column, not to the page (PIPELINE §8.2).
    pub column_width_pt: f32,
    /// The vertical gap to the block above it in the same column, in points. Zero for the
    /// first block of a column: there is no gap above the top of a page, and treating the
    /// top margin as one would make every page's first block look like a heading.
    pub space_above_pt: f32,
    /// The page's height, for the band tests that footnotes and furniture share.
    pub page_height_pt: f32,
}

impl BlockView {
    /// How wide the block's widest line is, relative to its column.
    pub fn width_ratio(&self) -> f32 {
        if self.column_width_pt <= 0.0 {
            return 0.0;
        }
        let widest = self
            .lines
            .iter()
            .map(LineView::width_pt)
            .fold(0.0f32, f32::max);
        (widest / self.column_width_pt).clamp(0.0, 1.0)
    }

    /// The size the block is mostly set at.
    pub fn size_pt(&self) -> f32 {
        self.lines
            .iter()
            .max_by_key(|line| line.text.chars().count())
            .map_or(0.0, LineView::size_pt)
    }

    /// Where the block sits down the page, as a fraction: 0 at the top, 1 at the bottom.
    pub fn band(&self) -> f32 {
        if self.page_height_pt <= 0.0 {
            return 0.0;
        }
        (self.bbox.y0 / self.page_height_pt).clamp(0.0, 1.0)
    }

    /// Every run of the block, in order.
    pub fn runs(&self) -> impl Iterator<Item = &Run> {
        self.lines.iter().flat_map(|line| line.runs.iter())
    }

    /// Non-whitespace characters — the conservation quantity.
    pub fn char_count(&self) -> u64 {
        u64::try_from(self.text.chars().filter(|c| !c.is_whitespace()).count()).unwrap_or_default()
    }
}
