//! The document outline: PDF bookmarks, read depth-first (Phase 1 detail 6).
//!
//! The outline is the one piece of structure a producer writes on purpose. Everything else
//! Phase 4 infers — headings from font clusters, sections from gaps — but a bookmark tree is
//! someone's own statement of how the book is organised, which is why D13.10 prefers it over
//! every heuristic when the two disagree.
//!
//! It is read by walking `first_child` and `next_sibling` rather than through
//! `PdfBookmarks::iter()`, which flattens the tree and drops the depth. The depth is half the
//! information.

use oc_model::extract::OutlineEntry;
use pdfium_render::prelude::{PdfBookmark, PdfDocument};

/// Read the whole outline in depth-first prefix order.
///
/// Bounded twice, because a `/First`/`/Next` chain is a linked structure in a file anyone can
/// write: a cycle would otherwise walk forever, and neither bound can be hit by an outline a
/// human wrote.
pub fn read_outline(
    document: &PdfDocument<'_>,
    limits: &oc_core::limits::Limits,
) -> Vec<OutlineEntry> {
    let Some(root) = document.bookmarks().root() else {
        return Vec::new();
    };

    let mut entries = Vec::new();
    // The stack holds siblings still to visit, deepest last, so popping gives prefix order.
    let mut stack: Vec<(PdfBookmark<'_>, u16)> = vec![(root, 0)];

    while let Some((bookmark, level)) = stack.pop() {
        if entries.len() >= usize::try_from(limits.max_outline_entries).unwrap_or(usize::MAX) {
            break;
        }
        if let Some(title) = bookmark.title() {
            entries.push(OutlineEntry {
                title,
                level,
                page: bookmark
                    .destination()
                    .and_then(|destination| destination.page_index().ok())
                    .and_then(|index| u32::try_from(index).ok()),
            });
        }

        // Siblings first, so that they sit *under* this node's children on the stack and are
        // therefore visited after them: that is what makes the walk prefix rather than level
        // order. Pushed in reverse so the first sibling comes off first.
        let mut siblings: Vec<PdfBookmark<'_>> = Vec::new();
        let mut next = bookmark.next_sibling();
        while let Some(sibling) = next {
            if siblings.len() >= usize::try_from(limits.max_outline_entries).unwrap_or(usize::MAX) {
                break;
            }
            next = sibling.next_sibling();
            siblings.push(sibling);
        }
        for sibling in siblings.into_iter().rev() {
            stack.push((sibling, level));
        }

        if let Some(child) = bookmark.first_child() {
            stack.push((child, level.saturating_add(1)));
        }
    }
    entries
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Row 1.15 of the Phase 1 table.
// ---------------------------------------------------------------------------

/// Test 1.15.
///
/// `h13` carries a tree of six entries over three levels with two roots. Three levels because
/// a one-level tree cannot tell a depth-first walk from a breadth-first one; two roots because
/// one cannot tell a walk that comes back up from one that stops at the first leaf.
#[test]
fn outline_is_read_depth_first() {
    use crate::inspect::PdfOpen;
    use crate::pdfium::PdfiumBackend;

    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/fixtures/handmade/h13_outline.pdf");
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "missing fixture {}: {error}; run `cargo run -p xtask -- handmade-fixtures`",
            path.display()
        )
    });

    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(&bytes, None).expect("the fixture opens");
    let outline = document.outline();

    let expected: Vec<(String, u16)> = oc_testkit::handmade::OUTLINE_TREE
        .iter()
        .map(|(title, level)| ((*title).to_owned(), *level))
        .collect();
    let got: Vec<(String, u16)> = outline
        .iter()
        .map(|entry| (entry.title.clone(), entry.level))
        .collect();
    assert_eq!(got, expected);
}

/// The same reader against a real producer's output, which is the case that matters.
///
/// `f01` is compiled by Typst and carries one bookmark, "Chapter 3", pointing at page 0. It
/// cannot distinguish a depth-first walk from any other — that is what `h13` is for — but it
/// is the assertion that the reader works on a file we did not write.
#[test]
fn outline_is_read_from_a_real_producer() {
    use crate::inspect::PdfOpen;
    use crate::pdfium::PdfiumBackend;

    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/fixtures/f01_prose_single_column.pdf");
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "missing fixture {}: {error}; run `cargo run -p xtask -- fixtures`",
            path.display()
        )
    });

    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(&bytes, None).expect("the fixture opens");
    let outline = document.outline();

    assert_eq!(outline.len(), 1, "{outline:?}");
    assert_eq!(outline[0].title, "Chapter 3");
    assert_eq!(outline[0].level, 0);
    assert_eq!(outline[0].page, Some(0));
}

/// A document with no outline reports none, rather than an entry with an empty title.
#[test]
fn no_outline_is_an_empty_outline() {
    use crate::inspect::PdfOpen;
    use crate::pdfium::PdfiumBackend;

    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/fixtures/handmade/h01_two_glyphs.pdf");
    let bytes = std::fs::read(path).expect("h01 is committed");

    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(&bytes, None).expect("the fixture opens");
    assert!(document.outline().is_empty());
}
