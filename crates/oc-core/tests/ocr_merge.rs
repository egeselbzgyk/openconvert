//! Rows 13.13 and 13.15, and the merge they check: OCR words become first-class runs with
//! `provenance = Ocr`, one `Added`-only ledger entry per region carries that region, and invariant
//! I-6 is read at region scope (ratified note N-1).

use oc_core::ledger_check::{c_of, check_i6, check_invariants, ConservationError, ReasonTotals};
use oc_core::ocr::merge::{clean_bands, merge_ocr_runs, region_confidence, OcrPage};
use oc_core::ocr::OcrWord;
use oc_core::stages::INGEST;
use oc_core::thresholds::T;
use oc_model::extract::{FontId, PageRef};
use oc_model::geom::Rect;
use oc_model::ledger::{Ledger, LedgerDelta, LedgerEntry, Reason};
use oc_model::text::TextProvenance;

fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
    Rect { x0, y0, x1, y1 }
}

fn word(text: &str, x0: f32, y0: f32, conf: f32, line: u32) -> OcrWord {
    OcrWord {
        text: text.to_owned(),
        bbox: rect(x0, y0, x0 + 8.0 * text.chars().count() as f32, y0 + 12.0),
        conf,
        block: 1,
        par: 1,
        line,
    }
}

fn page() -> OcrPage {
    OcrPage::new(PageRef::new(4), FontId(7))
}

/// Words become one run per Tesseract line, joined with single spaces whatever the pixel gaps
/// were, boxed by the union of their words, and every one of them says it is OCR.
#[test]
fn ocr_words_become_one_run_per_line_with_provenance_ocr() {
    let region = rect(50.0, 100.0, 400.0, 200.0);
    let words = vec![
        word("A", 60.0, 110.0, 0.95, 1),
        word("Scanned", 90.0, 108.0, 0.96, 1),
        word("Chapter", 170.0, 108.0, 0.97, 1),
        word("It", 60.0, 140.0, 0.94, 2),
        word("was", 90.0, 141.0, 0.40, 2),
    ];
    let mut page = page();
    let entries = merge_ocr_runs(&mut page, words, region).expect("merges");

    let texts: Vec<&str> = page.runs.iter().map(|run| run.text.as_str()).collect();
    assert_eq!(texts, ["A Scanned Chapter", "It was"]);
    assert!(page
        .runs
        .iter()
        .all(|run| run.provenance == TextProvenance::Ocr));
    assert!(page.runs.iter().all(|run| run.page.index == 4));
    assert!(page.runs.iter().all(|run| run.font == FontId(7)));
    assert_eq!(page.runs[0].bbox, rect(60.0, 108.0, 226.0, 122.0));
    assert_eq!(page.runs[0].id.0, 0);
    assert_eq!(page.runs[1].id.0, 1);

    // One entry for the region, Added, `Ocr`, carrying the region and every character it added.
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert!(entry.added);
    assert_eq!(entry.reason, Reason::Ocr);
    assert_eq!(entry.stage, INGEST.name);
    assert_eq!(entry.page, 4);
    assert_eq!(entry.region, Some(region));
    assert_eq!(c_of(&entry.text), c_of("A Scanned Chapter It was"));

    // An empty region adds nothing and ledgers nothing.
    let mut empty = OcrPage::new(PageRef::new(5), FontId(0));
    assert!(merge_ocr_runs(&mut empty, Vec::new(), region)
        .expect("merges")
        .is_empty());
    assert!(empty.runs.is_empty());
}

/// Detail 9's inputs: the mean confidence, and how many words fall under `ocr.word_conf_min`.
#[test]
fn region_confidence_is_the_mean_and_the_sub_floor_count() {
    let words = vec![
        word("good", 0.0, 0.0, 0.9, 1),
        word("fine", 0.0, 0.0, 0.7, 1),
        word("poor", 0.0, 0.0, 0.2, 1),
    ];
    let confidence = region_confidence(&words, T.ocr.word_conf_min as f32);
    assert!((confidence.mean - 0.6).abs() < 1e-6, "{confidence:?}");
    assert_eq!(confidence.words, 3);
    assert_eq!(confidence.below_floor, 1);
    assert_eq!(region_confidence(&[], 0.6).words, 0);
}

/// Row 13.13. An `Ocr` entry whose region contains a run that was there before OCR fails I-6, and
/// the message carries the region's box — the page-scoped reading of I-6 that predates N-1 would
/// instead reject every OCR entry on a page with any text at all, which is the `Mixed` case N-1
/// exists to allow.
#[test]
fn i6_region_scope_rejects_overlapping_text() {
    let plate = rect(300.0, 400.0, 500.0, 700.0);
    let entry = LedgerEntry::added(INGEST.name, Reason::Ocr, 2, (0, 5), "Plate".to_owned())
        .with_region(plate);

    // Body text beside the plate, on the same page: region scope admits it.
    let beside = [(2, rect(50.0, 400.0, 280.0, 700.0))];
    check_i6(std::slice::from_ref(&entry), &beside).expect("text beside a region is not in it");
    // Text on another page, over the same coordinates: not this region either.
    check_i6(
        std::slice::from_ref(&entry),
        &[(3, rect(310.0, 410.0, 400.0, 420.0))],
    )
    .expect("other page");

    // A run inside the region: rejected, and the message says which region.
    let inside = [(2, rect(310.0, 690.0, 420.0, 699.0))];
    let error =
        check_i6(std::slice::from_ref(&entry), &inside).expect_err("text inside the region");
    assert!(
        matches!(&error, ConservationError::OcrRegionNotClean { page: 2, region, .. } if *region == plate),
        "{error:?}"
    );
    let message = error.to_string();
    assert!(message.starts_with("I-6"), "{message}");
    assert!(
        message.contains("300") && message.contains("400") && message.contains("500"),
        "the region's box is in the message: {message}"
    );

    // An `Ocr` entry that removes is not OCR; one with no region cannot be checked and is refused.
    let removing = LedgerEntry {
        added: false,
        ..entry.clone()
    };
    assert!(matches!(
        check_i6(&[removing], &[]),
        Err(ConservationError::OcrNotAddedOnly { .. })
    ));
    let unscoped = LedgerEntry {
        region: None,
        ..entry
    };
    assert!(matches!(
        check_i6(&[unscoped], &[]),
        Err(ConservationError::OcrRegionMissing { .. })
    ));
}

/// A whole page with a few stray PDF glyphs on it — a stamped folio on a scan — is read in bands
/// that avoid them, so every region stays clean under I-6 without dropping the page.
#[test]
fn a_full_page_region_is_cut_into_bands_around_existing_text() {
    let page = rect(0.0, 0.0, 600.0, 800.0);
    assert_eq!(clean_bands(page, &[]), [page]);

    let folio = rect(290.0, 760.0, 310.0, 772.0);
    let header = rect(250.0, 20.0, 350.0, 32.0);
    let bands = clean_bands(page, &[folio, header]);
    assert_eq!(
        bands,
        [
            rect(0.0, 0.0, 600.0, 20.0),
            rect(0.0, 32.0, 600.0, 760.0),
            rect(0.0, 772.0, 600.0, 800.0)
        ]
    );
    for band in &bands {
        check_i6(
            &[
                LedgerEntry::added(INGEST.name, Reason::Ocr, 0, (0, 1), "x".to_owned())
                    .with_region(*band),
            ],
            &[(0, folio), (0, header)],
        )
        .expect("every band is clean");
    }
}

/// Row 13.15. OCR-added characters change neither side of the source-retention ratio: the
/// denominator is `C_0` as extraction and normalisation left it, and what OCR invented is taken out
/// of the numerator — at every stage boundary and end to end.
#[test]
fn ocr_regions_excluded_from_source_retention() {
    let source = "Extracted text of the born-digital pages";
    let ocr = "Read off a scanned plate";
    let c0 = c_of(source);

    // The `ingest` check that adds OCR text, then a Conserving stage after it: retention stays 1.
    let mut totals = ReasonTotals::new(&c0);
    let delta = LedgerDelta::new(vec![LedgerEntry::added(
        INGEST.name,
        Reason::Ocr,
        0,
        (0, 24),
        ocr.to_owned(),
    )
    .with_region(rect(0.0, 0.0, 100.0, 100.0))]);
    let with_ocr = c_of(&format!("{source} {ocr}"));
    let check = check_invariants(&c0, &with_ocr, &delta, INGEST, &mut totals)
        .expect("an Added-only OCR entry balances I-1");
    assert_eq!(
        totals.c0_total(),
        c0.total(),
        "the denominator is unchanged by OCR"
    );
    assert_eq!(totals.ocr_added(), c_of(ocr).total());
    assert!((check.retention - 1.0).abs() < 1e-6, "{check:?}");
    assert_eq!(totals.non_ocr_removed(), 0, "OCR is not a removal");

    let later = check_invariants(
        &with_ocr,
        &with_ocr,
        &LedgerDelta::default(),
        oc_core::stages::LAYOUT,
        &mut totals,
    )
    .expect("layout conserves");
    assert!((later.retention - 1.0).abs() < 1e-6, "{later:?}");

    // End to end, over the document's ledger: `ocr_added` is exactly what OCR put in.
    let mut ledger = Ledger {
        c_0: c0.clone(),
        c_raw: c0.clone(),
        ..Ledger::default()
    };
    ledger.push_stage(&delta, check);
    assert_eq!(ledger.ocr_added(), c_of(ocr));
    assert_eq!(ledger.c_0, c0);
}

/// An OCR line's size is its median word's height, not its box's — a skewed scan's line box
/// grows with the climb — and body lines within `ocr.line_size_snap_ratio` of the median are one
/// size, so a heading is the one that stands out (the scanned fixtures' 13.20 depends on it).
#[test]
fn ocr_line_sizes_are_word_heights_snapped_to_one_body_size() {
    use oc_core::ocr::merge::snap_line_sizes;

    // A line climbing 8 pt across the page, each word 7 pt tall: size 7, not 15.
    let climbing: Vec<OcrWord> = (0..6)
        .map(|i| {
            let x = 50.0 + i as f32 * 50.0;
            let y = 100.0 + i as f32 * 1.6;
            OcrWord {
                text: "word".to_owned(),
                bbox: rect(x, y, x + 40.0, y + 7.0),
                conf: 0.9,
                block: 1,
                par: 1,
                line: 1,
            }
        })
        .collect();
    let mut page = page();
    merge_ocr_runs(&mut page, climbing, rect(0.0, 0.0, 600.0, 800.0)).expect("merges");
    assert!(
        (page.runs[0].size_pt - 7.0).abs() < 1e-4,
        "{:?}",
        page.runs[0]
    );
    assert!(
        page.runs[0].bbox.y1 - page.runs[0].bbox.y0 > 14.0,
        "the box still spans the climb"
    );

    // Body lines at 7.2 and 8.9 are one size; the 13.7 pt heading is not.
    let mut runs = page.runs.clone();
    for (index, size) in [13.7f32, 7.2, 8.9, 8.9, 7.2, 8.9].into_iter().enumerate() {
        let mut run = page.runs[0].clone();
        run.size_pt = size;
        run.id = oc_model::text::RunId(u32::try_from(index).expect("small"));
        runs.push(run);
    }
    runs.remove(0);
    snap_line_sizes(&mut runs, T.ocr.line_size_snap_ratio as f32);
    let sizes: Vec<f32> = runs.iter().map(|run| run.size_pt).collect();
    assert_eq!(sizes, [13.7, 8.9, 8.9, 8.9, 8.9, 8.9]);
}
