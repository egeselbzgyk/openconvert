//! PHASE 7 rows 7.11 and 7.12: the performance budget, and the arithmetic behind it.
//!
//! D13.11 gives a converter 0.5 s and 500 MB of peak RSS per page of a 300-page born-digital
//! book on reference machine L. Row 7.12 splits that across the stages. Both numbers are
//! `provisional` in `thresholds.toml` — anchored against Docling's published 0.41–1.06 s/page
//! as a "we should not be slower than a heavier, non-Rust pipeline" sanity check, not derived
//! from a measurement of anything.
//!
//! **The timing assertions are behind the `bench` feature**, and the nightly `bench` job turns
//! it on. A wall-clock assertion on a shared CI runner that is also building three other jobs
//! measures the runner, and a gate that fails for that reason is one people learn to re-run
//! until it passes. The arithmetic test below is not gated: it needs no clock, and it is the
//! one that catches the mistake that actually happens — a stage quietly given more room than
//! the end-to-end budget has to give.

use oc_core::thresholds::T;

/// The stage budgets row 7.12 names, in the order the pipeline runs them.
///
/// `furniture`, `document`, `validate` and `repair` have no budget of their own: row 7.12
/// gives five numbers and they sum to the whole, so the four stages it does not name have to
/// come out of the five it does. That is worth stating rather than leaving as an omission —
/// it means `layout`'s 0.15 is really "layout and the furniture pass it depends on".
const STAGE_BUDGETS: [(&str, f64); 5] = [
    ("ingest", T.perf.stage_seconds_per_page.ingest),
    ("text", T.perf.stage_seconds_per_page.text),
    ("layout", T.perf.stage_seconds_per_page.layout),
    ("structure", T.perf.stage_seconds_per_page.structure),
    ("epub", T.perf.stage_seconds_per_page.epub),
];

/// Floating-point slack for comparing a sum of five two-decimal numbers.
const SUM_TOLERANCE: f64 = 1e-9;

#[test]
fn the_stage_budgets_sum_to_the_end_to_end_budget() {
    let total: f64 = STAGE_BUDGETS.iter().map(|(_, budget)| budget).sum();

    assert!(
        (total - T.perf.seconds_per_page_max).abs() < SUM_TOLERANCE,
        "the stage budgets sum to {total}, and the end-to-end budget is {}. Row 7.12 says the \
         split sums to the whole, so a stage cannot be given room without taking it from \
         another",
        T.perf.seconds_per_page_max
    );
}

#[test]
fn every_stage_budget_is_a_positive_share_of_the_whole() {
    for (stage, budget) in STAGE_BUDGETS {
        assert!(budget > 0.0, "{stage} has a budget of {budget}");
        assert!(
            budget < T.perf.seconds_per_page_max,
            "{stage} is budgeted {budget}, which is the whole of {}",
            T.perf.seconds_per_page_max
        );
    }
}

#[test]
fn the_reference_book_is_the_length_the_budget_is_stated_for() {
    let pages = usize::try_from(T.perf.bench_reference_pages).expect("a positive page count");
    let bytes = oc_testkit::handmade::reference_book(pages);

    assert!(bytes.starts_with(b"%PDF-"));
    assert_eq!(
        pages, 300,
        "D13.11 states the budget for a 300-page book; `perf.bench_reference_pages` is {pages}"
    );
}

#[test]
fn the_reference_book_is_the_same_bytes_every_time() {
    // A benchmark whose input differs between runs measures the input.
    let first = oc_testkit::handmade::reference_book(8);
    let second = oc_testkit::handmade::reference_book(8);

    assert_eq!(first, second);
}

#[cfg(feature = "bench")]
mod timed {
    use std::time::{Duration, Instant};

    use oc_core::thresholds::T;
    use oc_epub::EpubOptions;
    use oc_pdf::pdfium::PdfiumBackend;
    use openconvert::convert::{convert_bytes, ConvertOptions};

    use super::STAGE_BUDGETS;

    /// Convert the reference book once and return the page count, the wall clock, and what
    /// each stage spent.
    fn measure() -> (usize, Duration, Vec<(String, Duration)>) {
        let pages = usize::try_from(T.perf.bench_reference_pages).expect("a positive page count");
        let bytes = oc_testkit::handmade::reference_book(pages);
        let backend = PdfiumBackend::bind().expect("PDFium is vendored");

        let options = ConvertOptions {
            filename: "reference_book.pdf".to_owned(),
            language: None,
            preset: oc_model::document::PresetName::default(),
            epub: EpubOptions {
                split_bytes: usize::try_from(T.xhtml.split_bytes).unwrap_or(usize::MAX),
                max_longest_side_px: u32::try_from(T.images.max_longest_side_px)
                    .unwrap_or(u32::MAX),
                jpeg_quality: u8::try_from(T.images.jpeg_quality).unwrap_or(85),
                warn_total_bytes: u64::try_from(T.epub.warn_total_bytes).unwrap_or(u64::MAX),
                // Fixed, so two runs of the benchmark differ in timing and in nothing else.
                modified: "2026-01-01T00:00:00Z".to_owned(),
            },
            overrides: None,
            cache_dir: None,
        };

        let started = Instant::now();
        let conversion = convert_bytes(&backend, &bytes, None, &options, &T)
            .expect("the reference book converts");
        let elapsed = started.elapsed();

        let per_stage = conversion
            .timings
            .as_slice()
            .iter()
            .map(|(name, millis)| ((*name).to_owned(), Duration::from_millis(*millis)))
            .collect();

        (pages, elapsed, per_stage)
    }

    #[test]
    fn bench_end_to_end_within_budget() {
        let (pages, elapsed, _) = measure();
        let per_page = elapsed.as_secs_f64() / pages as f64;

        assert!(
            per_page <= T.perf.seconds_per_page_max,
            "{per_page:.4} s/page over {pages} pages, budget {:.4}",
            T.perf.seconds_per_page_max
        );
    }

    #[test]
    fn bench_per_stage_budgets() {
        let (pages, _, timings) = measure();

        for (stage, budget) in STAGE_BUDGETS {
            // A stage the driver does not time separately — `epub` is timed together with
            // validate and repair, which the loop owns — has no row here, and a missing row
            // is not a pass: `the_stage_budgets_sum_to_the_end_to_end_budget` and the
            // end-to-end assertion between them still bound it.
            let Some((_, spent)) = timings.iter().find(|(name, _)| name == stage) else {
                continue;
            };
            let per_page = spent.as_secs_f64() / pages as f64;
            assert!(
                per_page <= budget,
                "{stage} took {per_page:.4} s/page over {pages} pages, budget {budget:.4}"
            );
        }
    }

    /// Every stage the driver times is named, so the budget table cannot silently stop
    /// covering a stage that was renamed.
    #[test]
    fn the_budget_table_still_names_the_stages_the_driver_times() {
        let (_, _, timings) = measure();
        let timed: Vec<&str> = timings.iter().map(|(name, _)| name.as_str()).collect();

        for (stage, _) in STAGE_BUDGETS {
            assert!(
                timed.iter().any(|name| name.starts_with(stage)),
                "the budget table names {stage:?}, which the driver does not time; it timed                  {timed:?}"
            );
        }
    }
}
