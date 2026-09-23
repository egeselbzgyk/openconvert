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

pub mod discover;
pub mod invoke;
pub mod lang;
pub mod merge;
pub mod tsv;

use oc_model::geom::Rect;

/// A page needed OCR and no usable Tesseract 5 was found, so it is carried as an image (D4). The
/// arguments say why (`reason`), how to install one (`hint`) and which pages (`pages`).
pub const W_OCR_ENGINE_MISSING: &str = "W_OCR_ENGINE_MISSING";

/// Tesseract failed on one region — it hung past `ocr.region_deadline_secs`, crashed, or printed
/// something that is not its TSV — and the region is carried as an image instead.
pub const W_OCR_FAILED: &str = "W_OCR_FAILED";

/// A region's mean word confidence is under `ocr.region_conf_min`, so its image is emitted beside
/// the text for a reader who does not trust it (detail 9).
pub const W_OCR_LOW_CONFIDENCE: &str = "W_OCR_LOW_CONFIDENCE";

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

/// The platform a hint is written for. The engine's own platform is [`Os::current`]; the others
/// exist so every hint can be tested on every machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    Linux,
    MacOs,
    Windows,
}

impl Os {
    /// The platform this engine was built for. Anything that is neither macOS nor Windows gets the
    /// Linux hints, which are the ones a BSD user can most easily translate.
    pub fn current() -> Os {
        if cfg!(target_os = "macos") {
            Os::MacOs
        } else if cfg!(windows) {
            Os::Windows
        } else {
            Os::Linux
        }
    }
}

/// Tesseract's page segmentation mode, by the number its `--psm` takes.
///
/// Chosen by page class, never by guesswork (detail 6): a whole page gets automatic segmentation
/// with orientation and script detection, which is where Tesseract's own Leptonica deskew lives; a
/// region the PDF's geometry already established as one block is read as one uniform block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Psm {
    /// `--psm 1`: automatic page segmentation with OSD.
    AutoOsd,
    /// `--psm 4`: a single column of text of variable sizes.
    SingleColumn,
    /// `--psm 6`: a single uniform block of text.
    SingleBlock,
    /// `--psm 11`: sparse text, in no particular order.
    SparseText,
}

impl Psm {
    /// The value of `--psm`. Tesseract's own numbering, not a tunable.
    pub fn number(self) -> u8 {
        match self {
            Psm::AutoOsd => 1,
            Psm::SingleColumn => 4,
            Psm::SingleBlock => 6,
            Psm::SparseText => 11,
        }
    }
}

/// What an OCR call covers, which is what decides its segmentation mode (detail 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OcrScope {
    /// A whole `ImageOnly` (or re-OCR'd sandwich) page: automatic segmentation with OSD.
    FullPage,
    /// One image region of a `Mixed` page that no text covers: the PDF's geometry already says it
    /// is one block.
    ImageRegion,
}

impl OcrScope {
    pub fn psm(self) -> Psm {
        match self {
            OcrScope::FullPage => Psm::AutoOsd,
            OcrScope::ImageRegion => Psm::SingleBlock,
        }
    }
}

/// `--ocr`: whether pages are OCR'd (PHASE 13, §2.1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OcrMode {
    /// Follow the page class: `ImageOnly` pages whole, `Mixed` pages' uncovered image regions.
    #[default]
    Auto,
    /// Never OCR. Scanned pages stay pictures, and no warning says so: the user asked.
    Never,
    /// Read every page whole, in bands that avoid the text it already has (I-6).
    Always,
}

impl OcrMode {
    pub fn parse(value: &str) -> Option<OcrMode> {
        match value {
            "auto" => Some(OcrMode::Auto),
            "never" => Some(OcrMode::Never),
            "always" => Some(OcrMode::Always),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            OcrMode::Auto => "auto",
            OcrMode::Never => "never",
            OcrMode::Always => "always",
        }
    }
}

/// `--re-ocr`: whether an OCR sandwich's existing layer is replaced (D13.10, detail 10).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReOcr {
    /// Use the layer the file carries, with `provenance = OcrLayer`. The default.
    #[default]
    Never,
    /// Re-OCR a sandwich page when its layer's dictionary hit rate is under
    /// `pageclass.broken_text_dict_hit_min`.
    Auto,
    /// Re-OCR every sandwich page.
    Always,
}

impl ReOcr {
    pub fn parse(value: &str) -> Option<ReOcr> {
        match value {
            "never" => Some(ReOcr::Never),
            "auto" => Some(ReOcr::Auto),
            "always" => Some(ReOcr::Always),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ReOcr::Never => "never",
            ReOcr::Auto => "auto",
            ReOcr::Always => "always",
        }
    }
}
