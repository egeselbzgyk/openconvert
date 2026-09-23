//! The scanned fixtures' ground truth (PHASE 13 detail 12), held to the pipeline that defines it.
//!
//! A scan's ground truth is what a perfect OCR of it would give this pipeline: the text the same
//! pipeline extracts from the born-digital source with OCR off, furniture and all removed exactly
//! as they are removed from the scan. So it is not typed in by hand and it is not the source PDF's
//! raw text layer; it is this conversion's output, committed as `<id>.gt.txt` so the Python eval
//! harness can score the nightly corpus run against it without building anything, and checked
//! here so it cannot drift from the pipeline. `OC_UPDATE_SCAN_GT=1` rewrites it.

mod common;

use openconvert::ocr::OcrOptions;

#[test]
fn scanned_ground_truth_is_the_born_digital_text() {
    let update = std::env::var_os("OC_UPDATE_SCAN_GT").is_some();
    for (id, source) in common::SCANNED {
        let clean = common::build_path_with(
            &common::fixture(source),
            OcrOptions::off(),
            common::scanned_lang(id),
        );
        let truth = common::reading_text(&clean) + "\n";
        let path = common::scanned(&format!("{id}.gt.txt"));
        if update {
            std::fs::write(&path, &truth).expect("writes the ground truth");
            continue;
        }
        let committed = std::fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!(
                "{}: {error}; run with OC_UPDATE_SCAN_GT=1 to write it",
                path.display()
            )
        });
        assert_eq!(
            committed, truth,
            "{id}: the committed ground truth is not what the pipeline extracts from {source}"
        );
        assert!(truth.len() > 100, "{id}: a page of text, not a fragment");
    }
}

/// Every scanned fixture is an image-only PDF, and without OCR the book says so: its pages are
/// pictures, nothing is invented, and I-7 holds on an empty ledger.
#[test]
fn scanned_fixtures_are_image_only_without_ocr() {
    for (id, _) in common::SCANNED {
        let built =
            common::build_path_with_ocr(&common::scanned(&format!("{id}.pdf")), OcrOptions::off());
        assert!(
            built.page_classes.keys().all(|class| class == "image_only"),
            "{id}: {:?}",
            built.page_classes
        );
        assert!(!built.built.emitted.used_images.is_empty(), "{id}");
        assert!(
            built.structural.i7.holds(),
            "{id}: {:?}",
            built.structural.i7
        );

        // Its assertions parse here, in the fast tier, so a typo in one is not found only by the
        // job that has Tesseract.
        let path = common::scanned(&format!("{id}.assert.json"));
        let text = std::fs::read_to_string(&path).expect("the assertions");
        let assertions = oc_testkit::assertions::parse(&text)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        assert!(!assertions.is_empty(), "{id}");
    }
}
