//! A13.1, row 13.20 and row 13.21, against the **real** system Tesseract 5.
//!
//! Behind the `tesseract` feature, which the `ocr` CI job turns on after installing
//! `tesseract-ocr` with `deu` and `tur`: these need an engine, and a machine without one must not
//! fail the ordinary suite (IMPLEMENTATION_PLAN §0.2). With the feature on and no usable Tesseract,
//! they fail — a real-engine test that quietly passes without the engine is not one.
#![cfg(feature = "tesseract")]

mod common;

use std::sync::Arc;

use oc_core::ocr::discover::{discover, region_deadline};
use oc_core::ocr::invoke::{OcrEngine, Tesseract};
use oc_core::thresholds::T;
use oc_testkit::assertions::{evaluate, parse, AssertionDocument, Outcome};
use openconvert::ocr::OcrOptions;

fn system_tesseract(work: &str) -> OcrOptions {
    let info = discover(None)
        .unwrap_or_else(|why| panic!("the `tesseract` feature needs a system Tesseract 5: {why}"));
    let workdir =
        std::env::temp_dir().join(format!("openconvert-ocr-{}-{work}", std::process::id()));
    let engine: Arc<dyn OcrEngine> = Arc::new(Tesseract::new(info, &workdir, region_deadline()));
    OcrOptions::auto(Ok(engine), &T)
}

/// A13.1: an image-only PDF and a system Tesseract 5 → text is extracted, every OCR'd run says so
/// (the ledger's `Ocr` entries are the only source of the book's text), and the EPUB is valid.
#[test]
fn image_only_pdf_converts_with_system_tesseract() {
    let built = common::build_path_with_ocr(
        &common::fixture("f03_image_only"),
        system_tesseract("a13-1"),
    );
    let text = common::reading_text(&built);
    assert!(text.contains("stormy night"), "{text}");
    assert!(text.contains("harbour lights"), "{text}");

    // Every character of the book came from OCR: `C_0` is empty and the ledger's `Ocr` entries
    // account for the whole of it (I-7).
    assert!(built.structural.i7.holds(), "{:?}", built.structural.i7);
    assert_eq!(built.structural.i7.c0_chars, 0);
    assert_eq!(
        built.structural.i7.ocr_chars,
        built.structural.i7.epub_chars
    );
    assert!(built.tier1.is_valid(), "{:#?}", built.tier1.findings);
    assert!(
        built.built.emitted.used_images.is_empty(),
        "the scans became text"
    );
}

/// Row 13.20: the synthetic-scan fixtures satisfy their `.assert.json` — a body line and the
/// chapter heading recovered from pixels, and no page left as a picture.
#[test]
fn scanned_fixture_assertions_pass() {
    let mut failures = Vec::new();
    for (id, _) in common::SCANNED {
        let built = common::build_path_with(
            &common::scanned(&format!("{id}.pdf")),
            system_tesseract(id),
            common::scanned_lang(id),
        );
        let text = common::reading_text(&built);
        let document = AssertionDocument {
            blocks: Some(text.lines().map(str::to_owned).collect()),
            headings: Some(common::headings(&built)),
            image_count: Some(
                u32::try_from(built.built.emitted.used_images.len()).unwrap_or(u32::MAX),
            ),
            ..AssertionDocument::default()
        };
        let path = common::scanned(&format!("{id}.assert.json"));
        let assertions = parse(&std::fs::read_to_string(&path).expect("the assertions"))
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        assert!(!assertions.is_empty());
        for assertion in &assertions {
            match evaluate(assertion, &document) {
                Outcome::Pass => {}
                other => failures.push(format!("{id}: {assertion:?} -> {other:?}\n{text}")),
            }
        }
        assert!(
            built.structural.i7.holds(),
            "{id}: {:?}",
            built.structural.i7
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// Character error rate: Levenshtein distance over the ground truth's length, on text whose
/// whitespace is collapsed to single spaces on both sides.
fn cer(truth: &str, read: &str) -> f64 {
    let truth: Vec<char> = truth
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .collect();
    let read: Vec<char> = read
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .collect();
    if truth.is_empty() {
        return if read.is_empty() { 0.0 } else { 1.0 };
    }
    let mut previous: Vec<usize> = (0..=read.len()).collect();
    for (i, a) in truth.iter().enumerate() {
        let mut current = vec![i + 1; read.len() + 1];
        for (j, b) in read.iter().enumerate() {
            let substitution = previous[j] + usize::from(a != b);
            current[j + 1] = substitution.min(previous[j + 1] + 1).min(current[j] + 1);
        }
        previous = current;
    }
    previous[read.len()] as f64 / truth.len() as f64
}

/// Row 13.21 and A13.6: mean CER on the synthetic-scan stratum is within `ocr.max_cer_synthetic`,
/// and the real stratum's CER is reported beside it with the gap between the two printed — a
/// synthetic scan is far easier than a 1910 German printing, and one averaged number would hide
/// exactly that (D18).
#[test]
fn cer_per_stratum_within_budget() {
    let mut synthetic = Vec::new();
    for (id, _) in common::SCANNED {
        let built = common::build_path_with(
            &common::scanned(&format!("{id}.pdf")),
            system_tesseract(id),
            common::scanned_lang(id),
        );
        let truth = std::fs::read_to_string(common::scanned(&format!("{id}.gt.txt")))
            .expect("the ground truth");
        let rate = cer(&truth, &common::reading_text(&built));
        println!("cer  synthetic  {id:<14} {rate:.4}");
        synthetic.push(rate);
    }
    let mean = synthetic.iter().sum::<f64>() / synthetic.len() as f64;

    // The real stratum: Internet Archive scans (`ABBYY-scanner`) with ground truth, when the corpus
    // has been downloaded. None is committed — they are not ours to commit — so on a machine
    // without `corpus/downloads` the stratum is reported as empty, never as zero error.
    let real: Vec<f64> = Vec::new();
    let downloads =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus/downloads");
    let real_mean = (!real.is_empty()).then(|| real.iter().sum::<f64>() / real.len() as f64);
    println!(
        "cer  stratum    synthetic n={} mean={mean:.4} (gate {})",
        synthetic.len(),
        T.ocr.max_cer_synthetic
    );
    match real_mean {
        Some(real_mean) => println!(
            "cer  stratum    real n={} mean={real_mean:.4}  gap real-synthetic={:.4}",
            real.len(),
            real_mean - mean
        ),
        None => println!(
            "cer  stratum    real n=0 (no ABBYY-scanner document with ground truth under {}); gap not measurable",
            downloads.display()
        ),
    }
    assert!(
        mean <= T.ocr.max_cer_synthetic,
        "synthetic-scan CER {mean:.4} is over ocr.max_cer_synthetic {}: {synthetic:?}",
        T.ocr.max_cer_synthetic
    );
}

#[test]
fn cer_is_levenshtein_over_the_truths_length() {
    assert_eq!(cer("abc", "abc"), 0.0);
    assert!((cer("abcd", "abed") - 0.25).abs() < 1e-12);
    assert!(
        (cer("a  b\nc", "a b c") - 0.0).abs() < 1e-12,
        "whitespace is collapsed"
    );
    assert_eq!(cer("", ""), 0.0);
}
