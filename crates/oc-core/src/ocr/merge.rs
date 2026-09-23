//! OCR words into first-class runs (PHASE 13 details 7 and 8).
//!
//! **One run per Tesseract line.** Words are grouped by Tesseract's own `(block, par, line)` and
//! joined with a single space — never with the TSV's spacing, which measures pixel gaps and not
//! text. The run's box is the union of its words' boxes, and its provenance is `Ocr`, which is what
//! keeps the source-retention metric honest downstream.
//!
//! **Added-only, one entry per region.** The merge creates text from nothing, so its whole ledger
//! is one `Added` entry per region under `Reason::Ocr`, carrying the region's box; there is no
//! `Removed` counterpart. Invariant I-6 then checks that box against the runs the page had before
//! ([`crate::ledger_check::check_i6`]).
//!
//! **Regions are clean by construction.** A whole-page region on a scan that also carries a few PDF
//! glyphs — a stamped folio — is cut into horizontal [`clean_bands`] that avoid them, and a word
//! that straddles a cut is dropped: it is the text the PDF already carries.

use std::collections::BTreeMap;

use oc_model::extract::{FontId, PageRef};
use oc_model::geom::Rect;
use oc_model::ledger::{LedgerEntry, Reason};
use oc_model::text::{Run, RunId, TextProvenance};

use super::OcrWord;
use crate::stages::INGEST;

/// OpenType's "normal" weight. OCR sees no weights; normal is what a reader would assume.
const NORMAL_WEIGHT: u16 = 400;

/// The OCR runs one page has gained so far, and the font they are drawn in.
#[derive(Clone, Debug)]
pub struct OcrPage {
    pub page: PageRef,
    /// The font every OCR run on the page points at: a synthetic entry the caller adds to the
    /// page's font table, because OCR recovers text and not typefaces.
    pub font: FontId,
    /// The runs, numbered from zero in the order they were merged. `text` renumbers them after the
    /// runs it assembles from glyphs.
    pub runs: Vec<Run>,
    /// Characters added on this page so far, the offset the next ledger span starts at.
    added_chars: u32,
}

impl OcrPage {
    pub fn new(page: PageRef, font: FontId) -> Self {
        Self {
            page,
            font,
            runs: Vec::new(),
            added_chars: 0,
        }
    }
}

/// Why a region could not be merged.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum MergeError {
    /// A region with no area cannot carry a ledger entry I-6 can check.
    #[error("the OCR region {0:?} has no area")]
    DegenerateRegion(Rect),
}

/// Merge the words read from `region_pt` into `page` as runs, returning the ledger entries that
/// account for them: one `Added` `Ocr` entry carrying the region, or none when there were no words.
pub fn merge_ocr_runs(
    page: &mut OcrPage,
    words: Vec<OcrWord>,
    region_pt: Rect,
) -> Result<Vec<LedgerEntry>, MergeError> {
    if region_pt.x1 <= region_pt.x0 || region_pt.y1 <= region_pt.y0 {
        return Err(MergeError::DegenerateRegion(region_pt));
    }
    if words.is_empty() {
        return Ok(Vec::new());
    }

    // Grouped by Tesseract's line key, in the order the lines first appear — which is Tesseract's
    // reading order — so the result does not depend on how a map orders its keys.
    let mut order: Vec<(u32, u32, u32)> = Vec::new();
    let mut lines: BTreeMap<(u32, u32, u32), Vec<OcrWord>> = BTreeMap::new();
    for word in words {
        let key = (word.block, word.par, word.line);
        if !lines.contains_key(&key) {
            order.push(key);
        }
        lines.entry(key).or_default().push(word);
    }

    let mut texts = Vec::with_capacity(order.len());
    for key in order {
        let Some(words) = lines.remove(&key) else {
            continue;
        };
        let Some(run) = line_run(page, &words) else {
            continue;
        };
        texts.push(run.text.clone());
        page.runs.push(run);
    }

    let text = texts.join("\n");
    let width = u32::try_from(text.chars().count()).unwrap_or(u32::MAX);
    let start = page.added_chars;
    page.added_chars = start.saturating_add(width);
    Ok(vec![LedgerEntry::added(
        INGEST.name,
        Reason::Ocr,
        page.page.index,
        (start, start.saturating_add(width)),
        text,
    )
    .with_region(region_pt)])
}

/// One Tesseract line as one run.
fn line_run(page: &OcrPage, words: &[OcrWord]) -> Option<Run> {
    let first = words.first()?;
    let bbox = words.iter().skip(1).fold(first.bbox, |acc, word| Rect {
        x0: acc.x0.min(word.bbox.x0),
        y0: acc.y0.min(word.bbox.y0),
        x1: acc.x1.max(word.bbox.x1),
        y1: acc.y1.max(word.bbox.y1),
    });
    let text = words
        .iter()
        .map(|word| word.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let id = RunId(u32::try_from(page.runs.len()).unwrap_or(u32::MAX));
    Some(Run {
        id,
        page: page.page.clone(),
        text,
        bbox,
        baseline_y: baseline(words),
        font: page.font,
        size_pt: bbox.y1 - bbox.y0,
        weight: NORMAL_WEIGHT,
        italic: false,
        superscript: false,
        subscript: false,
        provenance: TextProvenance::Ocr,
        glyph_range: (0, 0),
    })
}

/// The line's baseline: the median of its words' bottoms. Most words have no descender, so the
/// median sits on the baseline where the lowest bottom would sit on a `g` or a `p`.
fn baseline(words: &[OcrWord]) -> f32 {
    let mut bottoms: Vec<f32> = words.iter().map(|word| word.bbox.y1).collect();
    bottoms.sort_by(f32::total_cmp);
    bottoms.get(bottoms.len() / 2).copied().unwrap_or_default()
}

/// A region's confidence, in the terms detail 9 decides on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RegionConfidence {
    /// Mean word confidence, `0.0` for a region with no words.
    pub mean: f32,
    pub words: usize,
    /// Words under `ocr.word_conf_min`: kept, flagged, and never dehyphenated.
    pub below_floor: usize,
}

/// The confidence of a region's words against `floor` (`ocr.word_conf_min`).
pub fn region_confidence(words: &[OcrWord], floor: f32) -> RegionConfidence {
    let total: f32 = words.iter().map(|word| word.conf).sum();
    RegionConfidence {
        mean: if words.is_empty() {
            0.0
        } else {
            total / words.len() as f32
        },
        words: words.len(),
        below_floor: words.iter().filter(|word| word.conf < floor).count(),
    }
}

/// `region` cut into full-width horizontal bands that overlap none of `obstacles` — the text runs
/// already on the page. With no obstacles, the region itself.
pub fn clean_bands(region: Rect, obstacles: &[Rect]) -> Vec<Rect> {
    let mut cuts: Vec<(f32, f32)> = obstacles
        .iter()
        .filter(|rect| crate::ledger_check::overlaps(rect, &region))
        .map(|rect| (rect.y0.max(region.y0), rect.y1.min(region.y1)))
        .collect();
    cuts.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut bands = Vec::new();
    let mut top = region.y0;
    for (y0, y1) in cuts {
        if y0 > top {
            bands.push(Rect {
                x0: region.x0,
                y0: top,
                x1: region.x1,
                y1: y0,
            });
        }
        top = top.max(y1);
    }
    if region.y1 > top {
        bands.push(Rect {
            x0: region.x0,
            y0: top,
            x1: region.x1,
            y1: region.y1,
        });
    }
    bands
}

/// Assign each word to the band that wholly contains it, vertically. A word that sits in no band
/// straddles a cut — it overlaps text the page already had — and is dropped; the count is returned
/// so the caller can report it.
pub fn split_by_bands(words: Vec<OcrWord>, bands: &[Rect]) -> (Vec<Vec<OcrWord>>, usize) {
    let mut out: Vec<Vec<OcrWord>> = vec![Vec::new(); bands.len()];
    let mut dropped = 0;
    for word in words {
        let home = bands
            .iter()
            .position(|band| word.bbox.y0 >= band.y0 && word.bbox.y1 <= band.y1);
        match home.and_then(|index| out.get_mut(index)) {
            Some(slot) => slot.push(word),
            None => dropped += 1,
        }
    }
    (out, dropped)
}
