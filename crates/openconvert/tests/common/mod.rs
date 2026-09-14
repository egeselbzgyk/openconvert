//! Driving a fixture the whole way to a container, the way a conversion does.
//!
//! Thin on purpose: the pipeline itself lives in `openconvert::convert`, which is what the CLI
//! drives, so a test here is looking at exactly what a user gets. A second driver that only
//! tests took would be a second set of bugs.

#![allow(dead_code)]

use oc_core::thresholds::T;
use oc_epub::EpubOptions;
use oc_model::document::PresetName;
use oc_model::lang::LangTag;
use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;
use openconvert::convert::{convert, sha256_hex, Conversion, ConvertOptions};

/// A fixture converted all the way to a container.
pub struct Built {
    pub conversion: Conversion,
    /// The container read back: every file by its path inside the archive.
    pub entries: std::collections::BTreeMap<String, Vec<u8>>,
}

impl Built {
    /// One file of the container, as text.
    pub fn text_file(&self, path: &str) -> String {
        self.entries
            .get(path)
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
            .unwrap_or_else(|| panic!("the container has no {path}: {:?}", self.paths()))
    }

    pub fn paths(&self) -> Vec<&str> {
        self.entries.keys().map(String::as_str).collect()
    }

    /// Every content document, in spine order.
    pub fn content_documents(&self) -> Vec<(String, String)> {
        self.conversion
            .built
            .emitted
            .files
            .iter()
            .map(|file| (file.path.clone(), self.text_file(&file.path)))
            .collect()
    }
}

/// The conversion's parts, by the names the tests use.
impl std::ops::Deref for Built {
    type Target = Conversion;

    fn deref(&self) -> &Conversion {
        &self.conversion
    }
}

/// `dcterms:modified` held still, so that two builds of one book differ in nothing.
pub const FIXED_MODIFIED: &str = "2026-01-01T00:00:00Z";

/// The path of a built Typst fixture, by its stem.
pub fn fixture(stem: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/fixtures")
        .join(format!("{stem}.pdf"))
}

/// The emitter options, from `thresholds.toml`, with the timestamp pinned.
pub fn epub_options() -> EpubOptions {
    EpubOptions {
        split_bytes: usize::try_from(T.xhtml.split_bytes).unwrap_or(usize::MAX),
        max_longest_side_px: u32::try_from(T.images.max_longest_side_px).unwrap_or(u32::MAX),
        jpeg_quality: u8::try_from(T.images.jpeg_quality).unwrap_or(85),
        warn_total_bytes: u64::try_from(T.epub.warn_total_bytes).unwrap_or(u64::MAX),
        modified: FIXED_MODIFIED.to_owned(),
    }
}

/// Convert a fixture and emit its container.
pub fn build(stem: &str) -> Built {
    build_with(stem, epub_options())
}

/// The same, with the emitter configured differently — a smaller split bound, say.
pub fn build_with(stem: &str, epub: EpubOptions) -> Built {
    let path = fixture(stem);
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "missing fixture {}: {error}; run `cargo run -p xtask -- fixtures`",
            path.display()
        )
    });

    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let pdf = backend.open(&bytes, None).expect("the fixture opens");
    let conversion = convert(
        pdf.as_ref(),
        &sha256_hex(&bytes),
        &ConvertOptions {
            filename: format!("{stem}.pdf"),
            language: Some(LangTag::EN),
            preset: PresetName::Auto,
            epub,
        },
        &T,
    )
    .expect("the fixture converts and conserves");

    let entries = oc_epub::read_entries(&conversion.built.bytes).expect("the container reads back");
    Built {
        conversion,
        entries,
    }
}

/// The lowercase hex SHA-256 of some bytes.
pub fn sha256_hex_of(bytes: &[u8]) -> String {
    sha256_hex(bytes)
}
