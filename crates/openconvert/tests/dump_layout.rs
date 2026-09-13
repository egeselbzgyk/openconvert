//! Rows 3.15 and 3.16: the layout dump, snapshotted, and the structural digest.
//!
//! Two snapshots rather than one, and the split is the lesson of `docs/DECISIONS_LOG.md`
//! (2026-09-13, the base-14 glyph boxes): a snapshot of geometry is a snapshot of the host's
//! font substitution, and it will differ between Linux and Windows on any document whose fonts
//! are not embedded. So the *canonical JSON* snapshot is taken on `f02`, whose fonts Typst
//! embeds, with boxes rounded to whole points; and the digest — counts, totals, no geometry at
//! all — is what `f01` asserts, because it is the same on every machine by construction.

use oc_core::ledger_check::ReasonTotals;
use oc_core::thresholds::T;
use oc_model::extract::PageRef;
use oc_model::lang::LangTag;
use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;
use openconvert::dump_layout::{digest, header, pages, rounded};
use openconvert::pipeline::{furniture_stage, layout_stage, text_stage, LayoutStage, PageInput};

fn lay_out(relative: &str) -> LayoutStage {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "missing fixture {}: {error}; run `cargo run -p xtask -- fixtures`",
            path.display()
        )
    });
    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(&bytes, None).expect("the fixture opens");
    let input: Vec<PageInput> = (0..document.page_count())
        .map(|index| {
            let geometry = document
                .page_geometry(index)
                .expect("the page has geometry");
            PageInput {
                page: PageRef::new(index),
                width_pt: geometry.width_pt(),
                height_pt: geometry.height_pt(),
                glyphs: document
                    .page_glyphs(index)
                    .expect("the page extracts")
                    .glyphs,
                images: document.page_images(index).unwrap_or_default(),
            }
        })
        .collect();

    let mut totals = ReasonTotals::default();
    let text = text_stage(&input, &mut totals, &T).expect("text conserves");
    let furniture =
        furniture_stage(&text, LangTag::EN, &mut totals, &T).expect("furniture stays in budget");
    layout_stage(&text, &furniture, &mut totals, &T).expect("layout conserves")
}

/// Row 3.15. Page 0 of `f02`: its blocks, their reading indices, their columns, and what the
/// two segmenters made of each — the whole of what `layout` decided about a two-column page
/// with a floating title across it.
#[test]
fn dump_stage_layout_snapshot_f02() {
    let layout = lay_out("../../target/fixtures/f02_two_column.pdf");
    let page = pages(&layout)
        .into_iter()
        .next()
        .expect("the fixture has a first page");

    // Boxes to whole points, and only the fields that are decisions rather than measurements.
    // A snapshot that pinned hundredths would be pinning PDFium's float arithmetic.
    let rows: Vec<String> = page
        .blocks
        .iter()
        .map(|block| {
            let (x0, y0, x1, y1) = rounded(block.block.bbox);
            format!(
                "#{index} col={column} {hint:?} iou={iou:.2} low={low} [{x0},{y0},{x1},{y1}] {text}",
                index = block.block.reading_index,
                column = block.block.column,
                hint = block.block.kind_hint,
                iou = block.agreement_iou,
                low = block.low_confidence,
                text = block.text.chars().take(48).collect::<String>(),
            )
        })
        .collect();
    let columns: Vec<String> = page
        .columns
        .iter()
        .map(|column| format!("[{:.0},{:.0}]", column.x0, column.x1))
        .collect();

    insta::assert_snapshot!(format!(
        "columns {columns:?}\ngutters {}\n{}",
        page.gutters.len(),
        rows.join("\n")
    ));
}

/// And the header, which is the document-level half: how the page set read as a whole.
#[test]
fn dump_stage_layout_header_f02() {
    let layout = lay_out("../../target/fixtures/f02_two_column.pdf");
    let head = header(&layout);
    insta::assert_snapshot!(format!(
        "pages={} blocks={} continuity={}/{} retries={} low_confidence={} removed={} added={}",
        head.pages,
        head.blocks,
        head.continuity_held,
        head.continuity_boundaries,
        head.column_retries,
        head.low_confidence_blocks,
        head.check.removed_chars,
        head.check.added_chars,
    ));
}

/// Row 3.16. The structural digest of `f01`: counts and totals, no geometry, so the assertion
/// means the same thing on every operating system the project builds on.
#[test]
fn digest_f01_layout() {
    let layout = lay_out("../../target/fixtures/f01_prose_single_column.pdf");
    insta::assert_json_snapshot!(digest(&layout));
}

/// The digest of the document whose column hypothesis was withdrawn, because the number that
/// records that withdrawal is the one most likely to change silently.
#[test]
fn digest_h22_layout() {
    let layout = lay_out("../../corpus/fixtures/handmade/h22_false_gutter.pdf");
    insta::assert_json_snapshot!(digest(&layout));
}
