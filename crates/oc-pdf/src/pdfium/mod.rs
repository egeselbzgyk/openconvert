//! The PDFium backend: the only place `libloading` is used, and the only module in the
//! workspace permitted to contain `unsafe` (D3, IMPLEMENTATION_PLAN §0.1).
//!
//! PDFium is bound at runtime rather than linked, so the application binary stays small and
//! the library can be updated independently (D3). The price is that an ABI mismatch is a
//! runtime error, which is why binding always ends with a probe.

mod bind;

pub use bind::{PdfiumBackend, EXPECTED_PDFIUM_BUILD};

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Row 0.7 of the Phase 0 table.
// ---------------------------------------------------------------------------

/// The pinned release, as `xtask vendor-pdfium` reads it. Test 0.7 checks it against
/// [`EXPECTED_PDFIUM_BUILD`] so the two cannot drift apart unnoticed.
#[cfg(test)]
const PDFIUM_LOCK: &str = include_str!("../../../../xtask/pdfium.lock");

#[test]
fn binds_and_reports_version() {
    use crate::backend::PdfBackend;
    use crate::pdfium::{PdfiumBackend, EXPECTED_PDFIUM_BUILD};

    let backend = PdfiumBackend::bind().expect(
        "PDFium must be vendored before this test can run: \
         `cargo run -p xtask -- vendor-pdfium`",
    );

    let version = backend.version();
    assert_eq!(version.backend, "pdfium");
    assert!(
        !version.version.is_empty(),
        "the backend must report a non-empty version string"
    );
    assert!(
        version.library_path.exists(),
        "the reported library path must be the one that was bound: {}",
        version.library_path.display()
    );

    // `bind` already ran the ABI probe — it returns `Err` otherwise — so reaching here
    // means a real one-page document was opened through the loaded library.
    assert_eq!(
        version.build,
        Some(EXPECTED_PDFIUM_BUILD),
        "vendored PDFium build differs from the one the bindings target"
    );

    // The build number lives in three places: this constant, `xtask/pdfium.lock`, and the
    // `pdfium_<n>` feature selected in the workspace manifest. The first two are checked
    // here; the third is checked by `xtask vendor-pdfium` itself.
    assert!(
        PDFIUM_LOCK.contains(&format!("build = {EXPECTED_PDFIUM_BUILD}")),
        "xtask/pdfium.lock does not pin build {EXPECTED_PDFIUM_BUILD}"
    );

    // OC_PDFIUM_PATH is authoritative: pointing it somewhere empty must fail rather than
    // quietly fall back to the vendored copy, which would make the override untrustworthy.
    // This also proves the test above is not passing vacuously - binding really can fail.
    std::env::set_var("OC_PDFIUM_PATH", "definitely-not-a-pdfium-library");
    let refused = PdfiumBackend::bind();
    std::env::remove_var("OC_PDFIUM_PATH");
    match refused {
        Err(crate::error::PdfError::LibraryNotFound { searched }) => {
            assert_eq!(
                searched,
                vec![std::path::PathBuf::from("definitely-not-a-pdfium-library")],
                "an explicit override must not fall back to other candidates"
            );
        }
        Err(other) => panic!("expected LibraryNotFound, got {other}"),
        Ok(_) => panic!("an OC_PDFIUM_PATH pointing at nothing must not bind"),
    }
}
