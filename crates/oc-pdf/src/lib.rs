#![forbid(unsafe_code)]
//! PDF access: the `PdfBackend` trait, the PDFium implementation, `lopdf` object
//! access, per-page classification and `inspect` (D3, D13.10).
//!
//! `unsafe` is forbidden crate-wide; the pdfium binding module re-enables it locally
//! with `#[allow(unsafe_code)]` on that module alone (IMPLEMENTATION_PLAN §0.1).

pub mod backend;
pub mod classify;
pub mod dump;
pub mod encrypt;
pub mod error;
pub mod geom;
pub mod glyphs;
pub mod images;
pub mod inspect;
pub mod limits;
pub mod meta;
pub mod outline;
pub mod pdfium;
pub mod producer;
pub mod render;
