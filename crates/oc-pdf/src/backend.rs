//! The backend-agnostic face of PDF access (D3).
//!
//! Everything above this trait is written against the trait, not against PDFium, so a
//! second backend can be added post-v1 without touching the pipeline.

use std::path::PathBuf;

/// What a backend reports about itself, and what the `hello` event carries (D13.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendVersion {
    /// Which backend this is, e.g. `"pdfium"`.
    pub backend: &'static str,
    /// The library's own version string, e.g. `"151.0.7881.0"`, or `"unknown"` when the
    /// library was supplied without the metadata that carries it.
    pub version: String,
    /// The Chromium build number, when it could be determined.
    pub build: Option<u32>,
    /// The library that was actually loaded. Reported because "which PDFium am I running?"
    /// is the first question of every binding bug report.
    pub library_path: PathBuf,
}

impl std::fmt::Display for BackendVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.backend, self.version)
    }
}

/// A source of PDF documents.
pub trait PdfBackend {
    fn version(&self) -> BackendVersion;
}
