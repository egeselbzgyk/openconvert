//! Row 2.21 of the Phase 2 table: the `text` stage's dump, as a committed snapshot.
//!
//! A snapshot is the regression artefact for a stage whose output is a *shape* rather than a
//! predicate — runs, their boundaries, the spaces inferred between them, the lines they
//! cluster into and what `N` did on the way. No table of assertions covers that, and a
//! reviewer reading a diff of it can see immediately whether a change to the gap threshold
//! merged two words or split one.
//!
//! Page 0 of `f01` only: one page of ordinary prose with a heading, a running head and a page
//! number is enough shape to catch a regression, and a snapshot big enough that nobody reads
//! it is a snapshot nobody reviews.

use oc_core::thresholds::T;
use oc_model::extract::PageRef;
use oc_model::lang::LangTag;
use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;
use openconvert::dump_text::dump;
use openconvert::pipeline::PageInput;

fn read(relative: &str) -> Vec<PageInput> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "missing fixture {}: {error}; run `cargo run -p xtask -- fixtures`",
            path.display()
        )
    });
    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(&bytes, None).expect("the fixture opens");
    (0..document.page_count())
        .map(|index| PageInput {
            page: PageRef::new(index),
            width_pt: document
                .page_geometry(index)
                .expect("the page has geometry")
                .width_pt(),
            height_pt: document
                .page_geometry(index)
                .expect("the page has geometry")
                .height_pt(),
            glyphs: document
                .page_glyphs(index)
                .expect("the page extracts")
                .glyphs,
            images: document.page_images(index).unwrap_or_default(),
        })
        .collect()
}

#[test]
fn dump_stage_text_snapshot_f01() {
    let input = read("../../target/fixtures/f01_prose_single_column.pdf");
    let (header, pages) = dump(&input, LangTag::EN, &T).expect("f01 conserves");

    let page = pages.first().expect("f01 has pages");
    let rendered = oc_model::canonical::to_canonical_json(page).expect("canonical JSON");

    insta::assert_snapshot!("dump_stage_text_f01_page0", rendered);

    // The header is asserted rather than snapshotted: two of its fields are floats from a
    // language model, and pinning those in a snapshot would make a `whatlang` bump look like
    // a text-assembly regression.
    assert_eq!(header.stage, "text");
    assert_eq!(header.pages, 2);
    assert_eq!(header.language, LangTag::EN);
    assert!(!header.language_fell_back);
    assert!(header.have_frequency_list);
    assert!(header.c_0_total > 500, "{}", header.c_0_total);
}
