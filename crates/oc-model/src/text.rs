//! The text layer: runs and lines, what the `text` stage produces (IR_SKETCH, PIPELINE §4).
//!
//! A **run** is a maximal stretch of one line drawn in one style. A **line** is a set of runs
//! sharing a baseline. Neither is a paragraph and neither knows about columns: reading order
//! and segmentation are `layout`'s, in Phase 3.

use serde::Serialize;

use crate::extract::{FontId, PageRef};
use crate::geom::Rect;

/// A run's index within its document. Stable within one extraction, and no more than that —
/// a `BlockId` is what survives a re-run (D13.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct RunId(pub u32);

/// Where a run's text came from.
///
/// Load-bearing for the source-retention metric, not decoration: text this pipeline invented
/// with OCR must not be counted as text it retained from the document (ARCHITECTURE §5.4,
/// I-6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TextProvenance {
    /// Drawn by the PDF.
    Pdf,
    /// From an OCR layer the document already carried, over a scanned image.
    OcrLayer,
    /// Produced by this pipeline's own OCR.
    Ocr,
}

/// One stretch of one line in one style.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Run {
    pub id: RunId,
    pub page: PageRef,
    /// NFC, ligatures expanded, no U+00AD — normalisation `N` has been applied, once.
    pub text: String,
    pub bbox: Rect,
    pub baseline_y: f32,
    pub font: FontId,
    pub size_pt: f32,
    pub weight: u16,
    pub italic: bool,
    /// Read from geometry **before** `N`, and never altered by it (ARCHITECTURE §5.1).
    pub superscript: bool,
    pub subscript: bool,
    pub provenance: TextProvenance,
    /// The run's glyphs, as a half-open range into the assembly order that produced it —
    /// **not** into the backend's glyph order, which is not reading order (Phase 1 item 1.3).
    /// [`crate::text::Run`]'s producer returns that order alongside the runs.
    pub glyph_range: (u32, u32),
}

/// One baseline's worth of runs.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Line {
    pub runs: Vec<RunId>,
    pub bbox: Rect,
    pub baseline_y: f32,
    /// The line's last character is a hyphen, so a word may be broken across it. Whether it
    /// *is* broken, and whether rejoining is right, is Phase 3's under invariant I-5.
    pub ends_with_hyphen: bool,
    /// How far the line starts inside the text block's left edge.
    pub indent_pt: f32,
    /// How far the line stops short of the text block's right edge.
    pub right_gap_pt: f32,
}
