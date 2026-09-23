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

    /// The file could not be read.
    #[error("cannot read {}: {message}", .path.display())]
    Io { path: PathBuf, message: String },

    /// The bytes are not a PDF this backend can open: malformed, or truncated.
    #[error("cannot open the PDF: {message}")]
    Open { message: String },

    /// The document is encrypted with a user password that was not supplied, or the one
    /// supplied was wrong.
    ///
    /// Distinct from [`PdfError::Open`] because a supervisor that cannot tell "this needs a
    /// password" from "this is not a PDF" cannot prompt the user for one — which is the whole
    /// contract of test 1.13.
    #[error("this PDF needs a password; pass --password or set OC_PDF_PASSWORD")]
    PasswordRequired,

    /// A declared or reached quantity exceeded what this conversion is allowed to consume
    /// (D13.2, R8 §A2). Both numbers are carried so the message says whether to raise the
    /// limit or to distrust the file.
    #[error("{0}")]
    LimitExceeded(#[from] oc_core::limits::LimitExceeded),

    /// A Phase 14 cap fired: checked before the work it bounds, and naming where — the page, the
    /// object, the section — as well as how much.
    #[error("{0}")]
    Cap(#[from] oc_core::limits::CapViolation),

    /// One page could not be read. The index is included because a document that fails on
    /// page 812 of 900 is a different problem from one that fails on page 0.
    #[error("page {index}: {message}")]
    Page { index: u32, message: String },
}

impl PdfError {
    /// The name of the resource cap that refused the document, if one did.
    ///
    /// Phase 1's checks report [`oc_core::limits::LimitExceeded`] and Phase 14's report
    /// [`oc_core::limits::CapViolation`]; a caller deciding "was this a cap?" should not have to
    /// know which phase wrote the check.
    pub fn cap(&self) -> Option<&'static str> {
        match self {
            PdfError::LimitExceeded(exceeded) => Some(exceeded.limit),
            PdfError::Cap(violation) => Some(violation.cap()),
            _ => None,
        }
    }
}
