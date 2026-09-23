//! `cargo bench -p openconvert --bench end_to_end` — the whole pipeline, on the reference book.
//!
//! IMPLEMENTATION_PLAN PHASE 7 detail 7. The *gate* is
//! `crates/openconvert/tests/perf_budget.rs`, which asserts seconds per page against
//! `perf.seconds_per_page_max`; this is the trend beside it, in criterion's own
//! statistics-and-history form, so a change that stays inside the budget and is nevertheless a
//! 40 % regression is visible rather than merely permitted.
//!
//! The plan's file list puts these under `crates/oc-core/benches/`. They are here instead
//! because `convert` lives in `openconvert` and `oc-core` cannot depend on it without a cycle
//! — see `docs/DECISIONS_LOG.md`, 2026-09-20.

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use oc_core::thresholds::T;
use oc_epub::EpubOptions;
use oc_pdf::pdfium::PdfiumBackend;
use openconvert::convert::{convert_bytes, ConvertOptions};

/// How many pages the bench converts.
///
/// Shorter than `perf.bench_reference_pages`: criterion wants many samples and the budget gate
/// already runs the full three hundred. What this measures is the per-page cost, and that does
/// not need three hundred pages to be visible.
const BENCH_PAGES: usize = 40;

fn options() -> ConvertOptions {
    ConvertOptions {
        filename: "reference_book.pdf".to_owned(),
        language: None,
        preset: oc_model::document::PresetName::default(),
        epub: EpubOptions {
            split_bytes: usize::try_from(T.xhtml.split_bytes).unwrap_or(usize::MAX),
            max_longest_side_px: u32::try_from(T.images.max_longest_side_px).unwrap_or(u32::MAX),
            jpeg_quality: u8::try_from(T.images.jpeg_quality).unwrap_or(85),
            warn_total_bytes: u64::try_from(T.epub.warn_total_bytes).unwrap_or(u64::MAX),
            // Fixed, so two runs differ in timing and in nothing else.
            modified: "2026-01-01T00:00:00Z".to_owned(),
        },
        overrides: None,
        cache_dir: None,
    }
}

fn end_to_end(criterion: &mut Criterion) {
    let bytes = oc_testkit::handmade::reference_book(BENCH_PAGES);
    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let options = options();

    let mut group = criterion.benchmark_group("end_to_end");
    group.throughput(criterion::Throughput::Elements(BENCH_PAGES as u64));
    group.bench_function("convert_reference_book", |bencher| {
        bencher.iter_batched(
            || bytes.clone(),
            |input| convert_bytes(&backend, &input, None, &options, &T).expect("the book converts"),
            BatchSize::SmallInput,
        );
    });
    group.finish();
}

criterion_group!(benches, end_to_end);
criterion_main!(benches);
