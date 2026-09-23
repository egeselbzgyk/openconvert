//! The OCR engine adapter (D4, IMPLEMENTATION_PLAN PHASE 13).
//!
//! **No OCR code lives in `oc-pdf`; `oc-pdf` only rasterizes.** This module owns everything between
//! a grayscale raster and a list of words in normalised page space: finding the user's Tesseract
//! ([`discover`]), running it under a deadline ([`invoke`]), reading what it printed ([`tsv`]),
//! choosing its language data ([`lang`]) and turning its words into first-class runs with
//! `provenance = Ocr` and the ledger entries that account for them ([`merge`]).
//!
//! What it never does is ask a language model to correct the result. LLM post-correction
//! measurably degrades OCR text across fourteen models and eight languages, German among them
//! (D16, R10 §6.14), and nothing in this module has a path to one.

pub mod tsv;

use oc_model::geom::Rect;

/// One word Tesseract read, already in the pipeline's units.
///
/// `bbox` is in normalised page space — points, origin top-left, y down, after `/Rotate` and the
/// CropBox offset (D13.3) — and `conf` is in `0.0..=1.0`, so nothing downstream has to remember
/// that Tesseract speaks pixels and percent.
#[derive(Clone, Debug, PartialEq)]
pub struct OcrWord {
    pub text: String,
    pub bbox: Rect,
    /// Tesseract's word confidence, scaled from `0..100` to `0.0..=1.0` at the boundary.
    pub conf: f32,
    /// Tesseract's own segmentation, which is what groups words into lines (detail 7).
    pub block: u32,
    pub par: u32,
    pub line: u32,
}
