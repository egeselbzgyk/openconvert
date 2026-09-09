//! Encrypted documents: opening them, and what their permission flags mean (Phase 1 detail 7,
//! D13.11, RT D12).
//!
//! Two rules, and the second one is the one that matters.
//!
//! **Try the empty user password first.** Most encrypted PDFs in circulation are not locked
//! at all — they are encrypted so that they can carry permission flags, and their user
//! password is empty. A reader opens them without asking anyone anything, and so must this.
//!
//! **Permissions are recorded, never enforced.** A permission bit is a request from the
//! publisher to a viewer; it is not a lock, it is not enforced by the format, and every PDF
//! reader in existence can ignore it. A converter that obeyed `print = false` by refusing to
//! convert would be refusing to convert a book its user legitimately owns, on the strength of
//! a bit that anyone can flip with a text editor. So the flags land in the report, where the
//! user can see what the file asked for, and nothing branches on them.

use serde::Serialize;

/// What a document's permission flags say a viewer may do.
///
/// Reported, not obeyed. Every field is what the *file* claims, and the report is where a
/// user reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Permissions {
    /// Whether printing is permitted at all, at any quality.
    pub print: bool,
    /// Whether printing at full quality is permitted.
    pub print_high_quality: bool,
    /// Whether text and graphics may be extracted — the flag a converter would look at, if it
    /// were the kind of converter that looked.
    pub copy: bool,
    pub modify: bool,
    pub annotate: bool,
    /// Whether pages may be inserted, rotated or deleted.
    pub assemble: bool,
}

impl Permissions {
    /// The permissions of an unencrypted document: everything.
    ///
    /// A file with no `/Encrypt` dictionary makes no request at all, which is not the same as
    /// requesting nothing — so the honest default is the permissive one.
    pub fn unrestricted() -> Self {
        Self {
            print: true,
            print_high_quality: true,
            copy: true,
            modify: true,
            annotate: true,
            assemble: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Rows 1.12 and 1.14; row 1.13 is a CLI
// test and lives in `crates/openconvert/tests/cli.rs`.
// ---------------------------------------------------------------------------

#[cfg(test)]
fn mutation(name: &str) -> Vec<u8> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/fixtures/mutations")
        .join(format!("{name}.pdf"));
    std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "missing mutation {}: {error}; run `cargo run -p xtask -- mutations`",
            path.display()
        )
    })
}

/// Test 1.12.
///
/// An AES-128 document whose user password is empty opens with no password supplied, and the
/// text inside it is the text that was there before it was encrypted — the encryption is a
/// wrapper, not a change to the content.
#[test]
fn encrypted_empty_user_password_opens() {
    use crate::inspect::PdfOpen;
    use crate::pdfium::PdfiumBackend;

    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend
        .open(&mutation("h01__encrypted_empty_user"), None)
        .expect("an empty user password must not need a --password");

    assert!(
        document.doc_info().encrypted,
        "the file really is encrypted"
    );

    // h01 is two glyphs, and they survive the round trip through AES-128.
    let page = document.page_glyphs(0).expect("the page extracts");
    assert_eq!(page.glyphs.iter().map(|g| g.ch).collect::<String>(), "AB");
}

/// Test 1.14.
///
/// The file says printing is forbidden. We record that and convert it anyway, which is D13.11
/// as written: the bit is a request to a viewer, not a lock, and a converter that honoured it
/// would be refusing a user their own book on the strength of a flag anyone can edit.
#[test]
fn owner_password_permissions_recorded_not_enforced() {
    use crate::inspect::PdfOpen;
    use crate::pdfium::PdfiumBackend;

    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend
        .open(&mutation("h01__encrypted_no_print"), None)
        .expect("a permission flag must not stop the document opening");

    let info = document.doc_info();
    assert!(info.encrypted);
    assert!(
        !info.permissions.print,
        "the flag must be read, not assumed: {:?}",
        info.permissions
    );
    assert!(
        !info.permissions.print_high_quality,
        "{:?}",
        info.permissions
    );

    // Not enforced: extraction proceeds exactly as it does for the unencrypted h01.
    let page = document.page_glyphs(0).expect("the page extracts");
    assert_eq!(page.glyphs.iter().map(|g| g.ch).collect::<String>(), "AB");

    // And the flag really is specific to this fixture rather than a constant — the same
    // document encrypted with printing allowed reports the opposite.
    let allowed = backend
        .open(&mutation("h01__encrypted_empty_user"), None)
        .expect("opens");
    assert!(allowed.doc_info().permissions.print);
}

/// A document with no `/Encrypt` dictionary asks for nothing, and is reported as unrestricted
/// rather than as a document that forbade everything.
#[test]
fn unencrypted_documents_are_unrestricted() {
    use crate::inspect::PdfOpen;
    use crate::pdfium::PdfiumBackend;

    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/fixtures/handmade/h01_two_glyphs.pdf");
    let bytes = std::fs::read(path).expect("h01 is committed");

    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(&bytes, None).expect("opens");
    let info = document.doc_info();

    assert!(!info.encrypted);
    assert_eq!(info.permissions, Permissions::unrestricted());
}

/// The wrong password is refused as a *password* problem, not as a malformed file.
///
/// The distinction is the whole of test 1.13's contract: a supervisor that cannot tell "this
/// needs a password" from "this is not a PDF" cannot prompt the user for one.
#[test]
fn wrong_password_is_reported_as_a_password_problem() {
    use crate::error::PdfError;
    use crate::inspect::PdfOpen;
    use crate::pdfium::PdfiumBackend;

    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let bytes = mutation("h01__encrypted_password");

    for (label, password) in [("no password", None), ("the wrong one", Some("not-it"))] {
        match backend.open(&bytes, password) {
            Err(PdfError::PasswordRequired) => {}
            Err(other) => panic!("{label}: expected PasswordRequired, got {other:?}"),
            Ok(_) => panic!("{label}: a protected file must not open"),
        }
    }

    let document = backend
        .open(&bytes, Some("secret"))
        .expect("the right password opens it");
    let page = document.page_glyphs(0).expect("the page extracts");
    assert_eq!(page.glyphs.iter().map(|g| g.ch).collect::<String>(), "AB");
}
