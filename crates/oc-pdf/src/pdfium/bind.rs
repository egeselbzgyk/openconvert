//! Binding PDFium at runtime, and proving the binding works before anything relies on it.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use pdfium_render::prelude::Pdfium;

use crate::backend::{BackendVersion, PdfBackend};
use crate::error::PdfError;

/// The Chromium build number of the PDFium the bindings target.
///
/// Must equal the `pdfium_<build>` feature selected for `pdfium-render` in the workspace
/// manifest and the `build` pinned in `xtask/pdfium.lock`. `xtask vendor-pdfium` refuses to
/// run when the manifest and the lock disagree; test 0.7 checks this constant against the
/// lock and against the library it actually bound.
pub const EXPECTED_PDFIUM_BUILD: u32 = 7881;

/// Environment variable that overrides library discovery entirely.
const LIBRARY_PATH_VAR: &str = "OC_PDFIUM_PATH";

/// Where `xtask vendor-pdfium` puts the library, relative to the workspace root.
const VENDOR_DIR: &str = "vendor/pdfium";

/// How far to walk up from the working directory looking for `VENDOR_DIR`. A test runs with
/// its crate directory as the working directory, the CLI usually with the workspace root;
/// four levels covers both without turning into a filesystem search.
const VENDOR_SEARCH_ANCESTORS: usize = 4;

/// A one-page PDF used to prove the loaded library actually works. 437 bytes, hand-built,
/// with a correct cross-reference table; verified to load as exactly one page.
const PROBE_PDF: &[u8] = include_bytes!("probe.pdf");

/// How many pages [`PROBE_PDF`] must report. A library that loads but miscounts is an ABI
/// mismatch dressed up as success, which is the failure this probe exists to catch.
const PROBE_PAGE_COUNT: i32 = 1;

/// PDFium, loaded from a shared library at runtime (D3).
pub struct PdfiumBackend {
    /// Leaked deliberately. A `PdfDocument` borrows from the `Pdfium` that produced it, and
    /// `PdfOpen::open` has to hand back an owned `Box<dyn PdfDoc>` - so the library has to
    /// outlive every document, which for a process that binds one library and then converts
    /// is the same as living forever. The alternatives are a self-referential struct or a
    /// lifetime in a public API, and IMPLEMENTATION_PLAN 0.1 rules the latter out.
    pdfium: &'static Pdfium,
    version: BackendVersion,
}

impl PdfiumBackend {
    /// Load PDFium and prove it works.
    ///
    /// Resolution order: `OC_PDFIUM_PATH`, then beside the running executable, then
    /// `vendor/pdfium/<triple>/` at or above the working directory. The first candidate
    /// that exists is the one that must bind: falling through to another after a real
    /// library failed to load would hide the reason.
    ///
    /// `OC_PDFIUM_PATH` is authoritative — when it is set, nothing else is tried. An
    /// override that silently does not take effect is worse than one that fails loudly.
    ///
    /// **Idempotent.** PDFium initialises global state, so a second `bind_to_library` in one
    /// process fails with `PdfiumLibraryBindingsAlreadyInitialized`. A caller should not have
    /// to know that: the first successful bind is cached and every later call returns it. A
    /// *failed* bind is not cached, so fixing `OC_PDFIUM_PATH` and retrying still works.
    pub fn bind() -> Result<Self, PdfError> {
        static BOUND: OnceLock<(&'static Pdfium, BackendVersion)> = OnceLock::new();
        if let Some((pdfium, version)) = BOUND.get() {
            return Ok(Self {
                pdfium,
                version: version.clone(),
            });
        }

        let bound = Self::bind_uncached()?;
        // A race here means two threads both bound successfully, which PDFium does not allow,
        // so in practice one wins and the loser reuses its result - which is the point.
        let (pdfium, version) = BOUND.get_or_init(|| (bound.pdfium, bound.version.clone()));
        Ok(Self {
            pdfium,
            version: version.clone(),
        })
    }

    /// The library path `bind` would use, without loading it.
    ///
    /// Separate from `bind` so the resolution rules can be tested on their own: once a
    /// process has bound successfully, `bind` returns the cached library and no longer
    /// consults the environment, which would make the override rule untestable through it.
    pub fn resolve_library() -> Result<PathBuf, PdfError> {
        let candidates = candidates();
        candidates
            .iter()
            .find(|path| path.is_file())
            .cloned()
            .ok_or(PdfError::LibraryNotFound {
                searched: candidates,
            })
    }

    fn bind_uncached() -> Result<Self, PdfError> {
        let library = &Self::resolve_library()?;

        let bindings = Pdfium::bind_to_library(library).map_err(|source| PdfError::Bind {
            path: library.clone(),
            message: source.to_string(),
        })?;
        let pdfium: &'static Pdfium = Box::leak(Box::new(Pdfium::new(bindings)));

        let version = read_version(library, library.clone());
        probe(pdfium, &version)?;

        Ok(Self { pdfium, version })
    }
}

impl PdfBackend for PdfiumBackend {
    fn version(&self) -> BackendVersion {
        self.version.clone()
    }
}

impl crate::inspect::PdfOpen for PdfiumBackend {
    fn open(
        &self,
        bytes: &[u8],
        password: Option<&str>,
    ) -> Result<Box<dyn crate::inspect::PdfDoc>, PdfError> {
        Ok(Box::new(crate::pdfium::PdfiumDoc::open(
            self.pdfium,
            bytes,
            password,
        )?))
    }
}

/// Open the probe document through the loaded library and check that it behaves.
///
/// This is the ABI check D3 calls for. `has_unicode_map_error()` — the API the plan
/// originally reached for — does not exist in `pdfium-render` (RT B1), and no version
/// symbol is exported either, so the check is behavioural: if a document that is known to
/// have one page does not load with one page through this library, the bindings and the
/// library do not agree and nothing further should be attempted.
fn probe(pdfium: &Pdfium, version: &BackendVersion) -> Result<(), PdfError> {
    let document = pdfium
        .load_pdf_from_byte_slice(PROBE_PDF, None)
        .map_err(|source| PdfError::ProbeFailed {
            message: source.to_string(),
        })?;
    let pages = document.pages().len();
    if pages != PROBE_PAGE_COUNT {
        return Err(PdfError::ProbeFailed {
            message: format!("the probe document reported {pages} pages, not {PROBE_PAGE_COUNT}"),
        });
    }

    match version.build {
        Some(build) if build != EXPECTED_PDFIUM_BUILD => Err(PdfError::AbiMismatch {
            expected: EXPECTED_PDFIUM_BUILD,
            found: build,
        }),
        // No VERSION file beside the library: the caller supplied their own PDFium through
        // `OC_PDFIUM_PATH` and the behavioural probe above is all the assurance there is.
        _ => Ok(()),
    }
}

/// Read the `VERSION` file bblanchon's archives ship beside the library.
fn read_version(library: &Path, library_path: PathBuf) -> BackendVersion {
    let text = library
        .parent()
        .map(|dir| dir.join("VERSION"))
        .and_then(|path| std::fs::read_to_string(path).ok());

    let field = |name: &str| -> Option<u32> {
        let text = text.as_deref()?;
        text.lines()
            .find_map(|line| line.strip_prefix(name))?
            .trim()
            .parse()
            .ok()
    };

    match (
        field("MAJOR="),
        field("MINOR="),
        field("BUILD="),
        field("PATCH="),
    ) {
        (Some(major), Some(minor), Some(build), Some(patch)) => BackendVersion {
            backend: BACKEND_NAME,
            version: format!("{major}.{minor}.{build}.{patch}"),
            build: Some(build),
            library_path,
        },
        _ => BackendVersion {
            backend: BACKEND_NAME,
            version: UNKNOWN_VERSION.to_owned(),
            build: None,
            library_path,
        },
    }
}

const BACKEND_NAME: &str = "pdfium";
const UNKNOWN_VERSION: &str = "unknown";

/// Every path the library may live at, in the order they are tried.
fn candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let file_name = Pdfium::pdfium_platform_library_name();

    if let Some(explicit) = std::env::var_os(LIBRARY_PATH_VAR) {
        let explicit = PathBuf::from(explicit);
        // Accept either the library itself or the directory holding it, and stop there.
        return vec![if explicit.is_dir() {
            explicit.join(&file_name)
        } else {
            explicit
        }];
    }

    if let Some(beside_exe) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(&file_name)))
    {
        paths.push(beside_exe);
    }

    if let Ok(cwd) = std::env::current_dir() {
        for ancestor in cwd.ancestors().take(VENDOR_SEARCH_ANCESTORS + 1) {
            paths.push(ancestor.join(VENDOR_DIR).join(HOST_TRIPLE).join(&file_name));
        }
    }

    paths
}

/// The triple this crate was built for, captured by `build.rs`, so the vendored directory
/// name matches what `xtask vendor-pdfium` wrote.
const HOST_TRIPLE: &str = env!("OC_PDF_TARGET_TRIPLE");
