//! Errors from PDF access.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    /// No PDFium library was found at any candidate path. The paths are listed because
    /// "where did it look?" is the only useful thing to say here.
    #[error(
        "no PDFium library found; searched {}. Run `cargo run -p xtask -- vendor-pdfium`, \
         or set OC_PDFIUM_PATH.",
        .searched.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
    )]
    LibraryNotFound { searched: Vec<PathBuf> },

    /// The library exists but could not be loaded.
    #[error("cannot load the PDFium library at {}: {message}", .path.display())]
    Bind { path: PathBuf, message: String },

    /// The library loaded, but the vendored build is not the one the bindings target.
    #[error(
        "PDFium build {found} does not match the {expected} these bindings were built \
         against; re-run `cargo run -p xtask -- vendor-pdfium`"
    )]
    AbiMismatch { expected: u32, found: u32 },

    /// The library loaded but did not behave: it is not the ABI we expect.
    #[error("the PDFium startup probe failed: {message}")]
    ProbeFailed { message: String },
}
