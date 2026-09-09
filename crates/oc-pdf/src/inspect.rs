//! `openconvert inspect` — what a PDF is, before anything tries to convert it.
//!
//! Everything here is counting and classification. No text is assembled, no `Glyph` is
//! materialised, no geometry beyond the page box is produced: those belong to Phase 1's
//! ingestion. The point of `inspect` is to answer "what am I about to be handed?" cheaply
//! and to make the answer reviewable as a committed snapshot.

use std::path::Path;

use serde::Serialize;

use crate::classify::{classify_page, PageCharStats, PageClass, PageImageStats};
use crate::error::PdfError;
use crate::producer::{producer_family, ProducerFamily};

/// The schema tag carried by every report, so a consumer can refuse a shape it does not know.
const SCHEMA: &str = "openconvert.inspect/1";

/// Emitted once when any page turns out to be `image_only`: without OCR those pages become
/// images in the EPUB, which is a thing the user should hear about before converting.
const WARN_IMAGE_ONLY: &str = "W_IMAGE_ONLY_PAGES";

/// Emitted when any page's text does not decode. Same reasoning.
const WARN_BROKEN_TEXT: &str = "W_BROKEN_TEXT_PAGES";

/// What `inspect` looks at.
#[derive(Clone, Debug, Default)]
pub struct InspectOptions {
    /// Zero-based page indices to report, or every page when empty.
    pub pages: Vec<u32>,
    /// The user password, for an encrypted document.
    pub password: Option<String>,
    /// What this run is allowed to consume. `Default` is the shipped set from
    /// `thresholds.toml`; `--max-pages` overrides one field of it.
    pub limits: oc_core::limits::Limits,
}

/// The machine-readable answer to "what is this file?".
#[derive(Clone, Debug, Serialize)]
pub struct InspectReport {
    pub schema: &'static str,
    pub engine_version: &'static str,
    pub ir_version: u32,
    pub source: SourceInfo,
    pub document: DocumentInfo,
    pub pages: Vec<PageInfo>,
    pub warnings: Vec<Warning>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SourceInfo {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct DocumentInfo {
    pub pages: u32,
    pub encrypted: bool,
    pub producer: Option<String>,
    pub creator: Option<String>,
    pub producer_family: ProducerFamily,
    pub has_struct_tree: bool,
    pub title: Option<String>,
    pub author: Option<String>,
    /// What the file asks a viewer to allow. Reported so a user can see it; nothing in the
    /// pipeline branches on it (D13.11).
    pub permissions: crate::encrypt::Permissions,
}

#[derive(Clone, Debug, Serialize)]
pub struct PageInfo {
    pub index: u32,
    pub width_pt: f32,
    pub height_pt: f32,
    pub rotate: u16,
    pub class: PageClass,
    pub class_confidence: f32,
    pub visible_chars: u32,
    pub invisible_chars: u32,
    pub replacement_chars: u32,
    pub pua_chars: u32,
    /// Control characters other than tab, line feed and carriage return — the shape a
    /// stripped `/ToUnicode` on a CID font takes. Reported so that a `broken_text` verdict
    /// says which of the three undecodable kinds it was reached by.
    pub control_chars: u32,
    pub image_count: u32,
    pub image_area_ratio: f32,
    pub glyphless_font: bool,
}

/// A code plus arguments, never a sentence: the GUI localises (D13.2).
#[derive(Clone, Debug, Serialize)]
pub struct Warning {
    pub code: &'static str,
    pub severity: &'static str,
    pub args: serde_json::Value,
}

/// An open PDF, as much of one as `inspect` needs.
pub trait PdfDoc {
    fn page_count(&self) -> u32;
    fn doc_info(&self) -> DocMetadata;
    fn page_geometry(&self, index: u32) -> Result<crate::geom::PageGeometry, PdfError>;
    fn page_char_stats(&self, index: u32) -> Result<PageCharStats, PdfError>;
    fn page_image_stats(&self, index: u32) -> Result<PageImageStats, PdfError>;
    /// The Stage-1 extraction layer for one page (Phase 1).
    fn page_glyphs(&self, index: u32) -> Result<crate::glyphs::PageGlyphs, PdfError>;
    /// The images one page draws (Phase 1 detail 4).
    fn page_images(&self, index: u32) -> Result<Vec<oc_model::extract::ImageRef>, PdfError>;
}

/// The document-level metadata `inspect` reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocMetadata {
    pub producer: Option<String>,
    pub creator: Option<String>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub encrypted: bool,
    pub has_struct_tree: bool,
    /// What the file's permission flags request. Recorded, never enforced (D13.11).
    pub permissions: crate::encrypt::Permissions,
}

impl Default for DocMetadata {
    /// An unencrypted document, which asks for nothing and therefore restricts nothing.
    fn default() -> Self {
        Self {
            producer: None,
            creator: None,
            title: None,
            author: None,
            encrypted: false,
            has_struct_tree: false,
            permissions: crate::encrypt::Permissions::unrestricted(),
        }
    }
}

/// Inspect a PDF file.
pub fn inspect(
    backend: &dyn PdfOpen,
    path: &Path,
    options: &InspectOptions,
) -> Result<InspectReport, PdfError> {
    use sha2::{Digest, Sha256};

    let bytes = std::fs::read(path).map_err(|source| PdfError::Io {
        path: path.to_path_buf(),
        message: source.to_string(),
    })?;
    let digest: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();

    let document =
        backend.open_with_limits(&bytes, options.password.as_deref(), &options.limits)?;
    let metadata = document.doc_info();
    let page_count = document.page_count();

    let wanted: Vec<u32> = if options.pages.is_empty() {
        (0..page_count).collect()
    } else {
        options
            .pages
            .iter()
            .copied()
            .filter(|i| *i < page_count)
            .collect()
    };

    let thresholds = &oc_core::thresholds::T;
    let mut pages = Vec::with_capacity(wanted.len());
    for index in wanted {
        let geometry = document.page_geometry(index)?;
        let chars = document.page_char_stats(index)?;
        let images = document.page_image_stats(index)?;
        // The dictionary-hit-rate arm is Phase 2's; Phase 0 has no word assembly to feed it.
        let (class, class_confidence) = classify_page(&chars, &images, None, thresholds);
        pages.push(PageInfo {
            index,
            width_pt: round_to_report(geometry.width_pt()),
            height_pt: round_to_report(geometry.height_pt()),
            rotate: geometry.rotate_degrees(),
            class,
            class_confidence,
            visible_chars: chars.visible,
            invisible_chars: chars.invisible,
            replacement_chars: chars.replacement,
            pua_chars: chars.pua,
            control_chars: chars.control,
            image_count: images.count,
            image_area_ratio: round_to_report(images.covered_area_ratio),
            glyphless_font: chars.glyphless_font,
        });
    }

    let warnings = warnings_for(&pages);

    Ok(InspectReport {
        schema: SCHEMA,
        engine_version: env!("CARGO_PKG_VERSION"),
        ir_version: oc_model::IR_VERSION,
        source: SourceInfo {
            path: path.to_string_lossy().replace('\\', "/"),
            sha256: digest,
            bytes: bytes.len() as u64,
        },
        document: DocumentInfo {
            pages: page_count,
            encrypted: metadata.encrypted,
            producer_family: producer_family(
                metadata.producer.as_deref(),
                metadata.creator.as_deref(),
            ),
            producer: metadata.producer,
            creator: metadata.creator,
            has_struct_tree: metadata.has_struct_tree,
            title: metadata.title,
            author: metadata.author,
            permissions: metadata.permissions,
        },
        pages,
        warnings,
    })
}

/// Anything that can open a PDF. Separate from [`crate::backend::PdfBackend`] so that
/// `inspect` can be tested against a stub without a library on disk.
pub trait PdfOpen {
    /// Open a document under an explicit resource budget.
    ///
    /// This is the door: the page-count guard is applied here, before a single page is
    /// touched, because the cost of a degenerate document is paid per page and the only
    /// useful moment to decline is the one before the first one (Phase 1 detail 8).
    fn open_with_limits(
        &self,
        bytes: &[u8],
        password: Option<&str>,
        limits: &oc_core::limits::Limits,
    ) -> Result<Box<dyn PdfDoc>, PdfError>;

    /// Open under the shipped defaults.
    fn open(&self, bytes: &[u8], password: Option<&str>) -> Result<Box<dyn PdfDoc>, PdfError> {
        self.open_with_limits(bytes, password, &oc_core::limits::Limits::default())
    }
}

/// Geometry reaches the report at two decimals, the same precision canonical JSON uses
/// (ARCHITECTURE §4.3), so a snapshot does not churn on the last bit of an `f32`.
fn round_to_report(value: f32) -> f32 {
    const SCALE: f32 = 100.0;
    (value * SCALE).round() / SCALE
}

fn warnings_for(pages: &[PageInfo]) -> Vec<Warning> {
    let mut warnings = Vec::new();
    let count = |class: PageClass| pages.iter().filter(|p| p.class == class).count();

    for (code, class) in [
        (WARN_IMAGE_ONLY, PageClass::ImageOnly),
        (WARN_BROKEN_TEXT, PageClass::BrokenText),
    ] {
        let n = count(class);
        if n > 0 {
            warnings.push(Warning {
                code,
                severity: "warn",
                args: serde_json::json!({ "count": n }),
            });
        }
    }
    warnings
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Rows 0.14–0.16 of the Phase 0 table.
// ---------------------------------------------------------------------------

/// Compiled fixtures live in `target/fixtures/`, produced by `cargo run -p xtask -- fixtures`.
#[cfg(test)]
fn fixture(name: &str) -> std::path::PathBuf {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/fixtures")
        .join(format!("{name}.pdf"));
    assert!(
        path.is_file(),
        "missing fixture {}; run `cargo run -p xtask -- fixtures`",
        path.display()
    );
    path
}

#[cfg(test)]
fn inspect_fixture(name: &str) -> crate::inspect::InspectReport {
    use crate::inspect::{inspect, InspectOptions};
    use crate::pdfium::PdfiumBackend;

    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    inspect(&backend, &fixture(name), &InspectOptions::default()).expect("the fixture inspects")
}

/// Snapshot settings shared by 0.14–0.16.
///
/// `source.path`, `source.sha256`, `source.bytes` and `engine_version` are redacted because
/// they move with the machine and the release, and the producer string is redacted because
/// it moves with the Typst toolchain. `visible_chars` is deliberately **not** redacted: it
/// is the assertion.
#[cfg(test)]
fn snapshot_settings() -> insta::Settings {
    let mut settings = insta::Settings::clone_current();
    settings.add_redaction(".source.path", "[path]");
    settings.add_redaction(".source.sha256", "[sha256]");
    settings.add_redaction(".source.bytes", "[bytes]");
    settings.add_redaction(".engine_version", "[version]");
    settings.add_redaction(".document.creator", "[creator]");
    settings
}

#[test]
fn inspect_f01_prose_single_column() {
    use crate::classify::PageClass;
    use crate::producer::ProducerFamily;

    let report = inspect_fixture("f01_prose_single_column");

    assert_eq!(report.document.pages, 2);
    assert!(!report.document.encrypted);
    assert_eq!(report.document.producer_family, ProducerFamily::Typst);
    assert!(
        !report.document.has_struct_tree,
        "xtask fixtures strips tagging (D18)"
    );
    assert!(report.pages.iter().all(|p| p.class == PageClass::Text));
    // A prose page carries far more than the text threshold; the exact count is the snapshot's.
    assert!(report.pages[0].visible_chars > 500, "{:?}", report.pages[0]);
    assert_eq!(report.pages[0].image_count, 0);

    snapshot_settings().bind(|| insta::assert_json_snapshot!(report));
}

#[test]
fn inspect_f02_two_column() {
    use crate::classify::PageClass;
    use crate::producer::ProducerFamily;

    let report = inspect_fixture("f02_two_column");

    assert_eq!(report.document.pages, 2);
    assert_eq!(report.document.producer_family, ProducerFamily::Typst);
    assert!(report.pages.iter().all(|p| p.class == PageClass::Text));
    // A4 portrait, in points, after /Rotate and the CropBox offset.
    assert_eq!(report.pages[0].width_pt, 595.28);
    assert_eq!(report.pages[0].height_pt, 841.89);

    snapshot_settings().bind(|| insta::assert_json_snapshot!(report));
}

#[test]
fn inspect_f03_image_only() {
    use crate::classify::PageClass;

    let report = inspect_fixture("f03_image_only");

    assert_eq!(report.document.pages, 2);
    assert!(
        report.pages.iter().all(|p| p.class == PageClass::ImageOnly),
        "{:?}",
        report.pages
    );
    assert!(report.pages.iter().all(|p| p.visible_chars == 0));
    assert!(report.pages.iter().all(|p| p.image_count == 1));
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.code == "W_IMAGE_ONLY_PAGES"),
        "{:?}",
        report.warnings
    );

    snapshot_settings().bind(|| insta::assert_json_snapshot!(report));
}

/// A mutated fixture, written by `cargo run -p xtask -- mutations`.
///
/// Committed, unlike the Typst fixtures it is derived from, because a mutation is only a
/// regression artefact if everyone's copy is the same file.
#[cfg(test)]
fn mutation(name: &str) -> std::path::PathBuf {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/fixtures/mutations")
        .join(format!("{name}.pdf"));
    assert!(
        path.is_file(),
        "missing mutation {}; run `cargo run -p xtask -- mutations`",
        path.display()
    );
    path
}

/// Test 1.8. Strip `/ToUnicode` from every font and the page still draws — a reader sees the
/// same ink — but nothing can say what the characters *are*. That is the single most common
/// way a PDF is unconvertible (R2 §B.8), and the whole point of classifying it is to route
/// the page to OCR instead of emitting mojibake into a book.
///
/// `f01` is the right subject because unmutated it classifies `text` with high confidence,
/// so this asserts the classifier can be moved, not merely that it agrees with itself.
#[test]
fn stripped_tounicode_page_classifies_broken_text() {
    use crate::classify::PageClass;
    use crate::inspect::{inspect, InspectOptions};
    use crate::pdfium::PdfiumBackend;

    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let report = inspect(
        &backend,
        &mutation("f01__strip_tounicode"),
        &InspectOptions::default(),
    )
    .expect("the mutated fixture inspects");

    assert_eq!(report.document.pages, 2);
    assert!(
        report
            .pages
            .iter()
            .all(|p| p.class == PageClass::BrokenText),
        "{:?}",
        report.pages
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.code == "W_BROKEN_TEXT_PAGES"),
        "{:?}",
        report.warnings
    );
}
