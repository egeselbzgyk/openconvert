//! One OCR call: a raster in, words in page space out (PHASE 13 detail 2).
//!
//! The raster is written as a grayscale PNG into the job's own temporary directory, `tesseract`
//! reads it under [`crate::sidecar::tesseract`]'s deadline and ownership, and the PNG is removed
//! whatever happened. stdout is the TSV; stderr is logged at debug level and never shown, because
//! Tesseract's chatter ("Estimating resolution as …") is not something a reader can act on.
//!
//! [`OcrEngine`] is the seam the pipeline is written against, so that its routing, merging and
//! degradation are testable on every platform with an in-process engine; [`Tesseract`] is the one
//! real implementation. Platform engines (Apple Vision, `Windows.Media.Ocr`) are post-v1 extension
//! points behind the same trait.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use image::GrayImage;
use oc_model::geom::Rect;

use super::discover::TesseractInfo;
use super::lang::LangSpec;
use super::tsv::{parse_tsv, TsvError};
use super::{OcrWord, Psm};
use crate::sidecar::tesseract::{command, run_captured, OcrArgs, RunError};

/// What one OCR call is given.
#[derive(Clone, Debug)]
pub struct OcrRequest {
    /// The region, rasterized in grayscale at `dpi`.
    pub raster: GrayImage,
    pub dpi: u32,
    pub psm: Psm,
    pub langs: LangSpec,
    /// Where the raster sits on the page, in normalised page space.
    pub region_pt: Rect,
    pub page_index: u32,
}

/// Why an OCR call produced no words. Every one of these degrades the region to an image; none of
/// them fails the book (D4, A13.5).
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum OcrError {
    #[error("the raster could not be written: {0}")]
    Image(String),
    #[error(transparent)]
    Run(#[from] RunError),
    #[error("tesseract exited with {status}")]
    Exit { status: String },
    #[error(transparent)]
    Tsv(#[from] TsvError),
}

/// An OCR engine, as the pipeline sees it.
pub trait OcrEngine: Send + Sync {
    /// The `hello` capability string, e.g. `ocr:tesseract-5.3.4`.
    fn capability(&self) -> String;
    /// The traineddata the engine has.
    fn langs(&self) -> &BTreeSet<String>;
    /// Read one region.
    fn recognize(&self, request: &OcrRequest) -> Result<Vec<OcrWord>, OcrError>;
}

/// The user's Tesseract, run once per region.
#[derive(Debug)]
pub struct Tesseract {
    info: TesseractInfo,
    workdir: PathBuf,
    deadline: Duration,
    calls: AtomicU64,
}

impl Tesseract {
    /// An engine that writes its rasters into `workdir` and kills any call still running after
    /// `deadline` (`ocr.region_deadline_secs` in the pipeline; shorter in tests).
    pub fn new(info: TesseractInfo, workdir: &Path, deadline: Duration) -> Self {
        Self {
            info,
            workdir: workdir.to_path_buf(),
            deadline,
            calls: AtomicU64::new(0),
        }
    }

    pub fn info(&self) -> &TesseractInfo {
        &self.info
    }
}

impl OcrEngine for Tesseract {
    fn capability(&self) -> String {
        self.info.capability()
    }

    fn langs(&self) -> &BTreeSet<String> {
        &self.info.langs
    }

    fn recognize(&self, request: &OcrRequest) -> Result<Vec<OcrWord>, OcrError> {
        run(
            &self.info,
            request,
            &self.workdir,
            self.deadline,
            &self.calls,
        )
    }
}

/// Run one OCR call. The PNG is named for the page and a per-engine counter, so concurrent calls
/// never share a file, and it is removed before this returns.
pub fn run(
    info: &TesseractInfo,
    request: &OcrRequest,
    workdir: &Path,
    deadline: Duration,
    calls: &AtomicU64,
) -> Result<Vec<OcrWord>, OcrError> {
    std::fs::create_dir_all(workdir).map_err(|error| OcrError::Image(error.to_string()))?;
    let call = calls.fetch_add(1, Ordering::Relaxed);
    let image = workdir.join(format!("ocr-p{}-{call}.png", request.page_index));
    request
        .raster
        .save_with_format(&image, image::ImageFormat::Png)
        .map_err(|error| OcrError::Image(error.to_string()))?;

    let args = OcrArgs {
        langs: request.langs.clone(),
        psm: request.psm,
        dpi: request.dpi,
    };
    let outcome = run_captured(command(&info.path, &image, &args), deadline);
    // Removed whatever happened: a hung call that was killed leaves no raster of the reader's
    // book behind in a temporary directory.
    let _ = std::fs::remove_file(&image);
    let captured = outcome?;

    if !captured.stderr.is_empty() {
        tracing::debug!(
            page = request.page_index,
            stderr = %String::from_utf8_lossy(&captured.stderr),
            "tesseract"
        );
    }
    if !captured.success() {
        return Err(OcrError::Exit {
            status: captured
                .status
                .map_or_else(|| "no status".to_owned(), |status| status.to_string()),
        });
    }
    Ok(parse_tsv(&captured.stdout, request.dpi, request.region_pt)?)
}
