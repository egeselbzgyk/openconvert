//! `cargo bench -p openconvert --bench stages` — where the time in a conversion goes.
//!
//! IMPLEMENTATION_PLAN PHASE 7 row 7.12 budgets the stages separately, so they are measured
//! separately. One conversion is run per sample and its own `Timings` are reported, rather
//! than each stage being driven in isolation: a stage's input is the previous stage's output,
//! and benchmarking `layout` on a hand-built input would measure a document the pipeline never
//! produces.
//!
//! The gate is `crates/openconvert/tests/perf_budget.rs`. This is the trend.

use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use oc_core::thresholds::T;
use oc_epub::EpubOptions;
use oc_pdf::pdfium::PdfiumBackend;
use openconvert::convert::{convert_bytes, ConvertOptions};

const BENCH_PAGES: usize = 40;

fn stages(criterion: &mut Criterion) {
    let bytes = oc_testkit::handmade::reference_book(BENCH_PAGES);
    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let options = ConvertOptions {
        filename: "reference_book.pdf".to_owned(),
        language: None,
        preset: oc_model::document::PresetName::default(),
        epub: EpubOptions {
            split_bytes: usize::try_from(T.xhtml.split_bytes).unwrap_or(usize::MAX),
            max_longest_side_px: u32::try_from(T.images.max_longest_side_px).unwrap_or(u32::MAX),
            jpeg_quality: u8::try_from(T.images.jpeg_quality).unwrap_or(85),
            warn_total_bytes: u64::try_from(T.epub.warn_total_bytes).unwrap_or(u64::MAX),
            modified: "2026-01-01T00:00:00Z".to_owned(),
        },
        overrides: None,
        cache_dir: None,
    };

    // One conversion, read once: what each stage spent, per page, printed where a reader of
    // the bench output will see it beside criterion's own numbers.
    let conversion = convert_bytes(&backend, &bytes, None, &options, &T).expect("it converts");
    println!("per-stage, seconds per page over {BENCH_PAGES} pages:");
    for (stage, millis) in conversion.timings.as_slice() {
        let per_page = Duration::from_millis(*millis).as_secs_f64() / BENCH_PAGES as f64;
        println!("  {stage:<24} {per_page:.5}");
    }

    let mut group = criterion.benchmark_group("stages");
    group.throughput(criterion::Throughput::Elements(BENCH_PAGES as u64));
    group.bench_function("ingest_to_epub", |bencher| {
        bencher.iter(|| convert_bytes(&backend, &bytes, None, &options, &T).expect("it converts"));
    });
    group.finish();
}

criterion_group!(benches, stages);
criterion_main!(benches);
