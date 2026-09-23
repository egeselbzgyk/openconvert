//! `cargo xtask fuzz-seeds` — the fuzz targets' seed corpora, from the committed fixtures (PHASE 14
//! detail 8: "corpora seed from the committed fixtures").
//!
//! - `ir_deserialize`: the semantic layer of three converted Typst fixtures, as the property reads
//!   it (`oc_testkit::fuzz_props::SemanticIr`), plus the empty object.
//! - `job_spec`: the job specs the engine's own tests write, the minimal one, and two refused
//!   shapes (a relative path, a `..`).
//! - `xhtml_opf_roundtrip`: the property draws a document from the bytes themselves, so its seeds
//!   are byte strings that decode to a paragraph-heavy book, a nested one and markup-heavy text.
//!
//! Written to `fuzz/corpus/<target>/`, committed, and run by the ordinary suite
//! (`crates/oc-testkit/tests/fuzz_props.rs`) as well as by `cargo fuzz`.

use std::path::Path;

use anyhow::{Context, Result};
use oc_model::document::PresetName;
use oc_model::lang::LangTag;
use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;
use openconvert::convert::{convert, sha256_hex, ConvertOptions};

const FIXTURES: [&str; 3] = [
    "f01_prose_single_column",
    "f07_verse_and_quote",
    "f10_lists_and_table",
];
const MODIFIED: &str = "2026-01-01T00:00:00Z";

pub fn run(root: &Path) -> Result<()> {
    let corpus = root.join("fuzz/corpus");
    let ir = corpus.join("ir_deserialize");
    let spec = corpus.join("job_spec");
    let xhtml = corpus.join("xhtml_opf_roundtrip");
    for dir in [&ir, &spec, &xhtml] {
        std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }

    let backend =
        PdfiumBackend::bind().context("PDFium is vendored (cargo xtask vendor-pdfium)")?;
    for stem in FIXTURES {
        let path = root.join("target/fixtures").join(format!("{stem}.pdf"));
        let bytes = std::fs::read(&path).with_context(|| {
            format!("cannot read {}; run `cargo xtask fixtures`", path.display())
        })?;
        let pdf = backend
            .open(&bytes, None)
            .with_context(|| format!("{stem} does not open"))?;
        let t = &oc_core::thresholds::T;
        let conversion = convert(
            pdf.as_ref(),
            &sha256_hex(&bytes),
            &ConvertOptions {
                filename: format!("{stem}.pdf"),
                language: Some(LangTag::EN),
                preset: PresetName::Auto,
                epub: oc_epub::EpubOptions {
                    split_bytes: usize::try_from(t.xhtml.split_bytes).unwrap_or(usize::MAX),
                    max_longest_side_px: u32::try_from(t.images.max_longest_side_px)
                        .unwrap_or(u32::MAX),
                    jpeg_quality: u8::try_from(t.images.jpeg_quality).unwrap_or(u8::MAX),
                    warn_total_bytes: u64::MAX,
                    modified: MODIFIED.to_owned(),
                },
                overrides: None,
                cache_dir: None,
                ocr: openconvert::ocr::OcrOptions::off(),
            },
            t,
        )
        .with_context(|| format!("{stem} does not convert"))?;
        let document = conversion.document;
        let semantic = oc_testkit::fuzz_props::SemanticIr {
            meta: document.meta,
            language: document.language,
            sections: document.sections,
            notes: document.notes,
            figures: document.figures,
            tables: document.tables,
            page_breaks: document.page_breaks,
        };
        let json = oc_model::canonical::to_canonical_json(&semantic)
            .map_err(|error| anyhow::anyhow!("{stem}: {error}"))?;
        std::fs::write(ir.join(format!("{stem}.json")), json)?;
    }
    std::fs::write(ir.join("empty-object.json"), "{}")?;

    let specs = [
        (
            "minimal.json",
            r#"{"schema":"openconvert.job/1","input":{"path":"/in/book.pdf"},"output":{"path":"/out/book.epub"}}"#,
        ),
        (
            "full.json",
            r#"{"schema":"openconvert.job/1","job_id":"job-1","input":{"path":"/in/book.pdf","sha256":"0000000000000000000000000000000000000000000000000000000000000000","password_file":"/in/pw"},"output":{"path":"/out/book.epub","report_path":"/out/book.json","overwrite":true},"preset":"novel","ai":{"enabled":true,"endpoint":"http://127.0.0.1:8080","api_key_file":"/k","model_path":"/m.gguf","model_id":"m","non_loopback_consent":false},"limits":{"max_pages":10,"max_memory_bytes":268435456,"stage_deadline_secs":5},"overrides_path":"/o.json","threshold_overrides":{"a.b":1.0},"locale":"de","dump_stages":["text"]}"#,
        ),
        (
            "relative.json",
            r#"{"schema":"openconvert.job/1","input":{"path":"book.pdf"},"output":{"path":"/out/book.epub"}}"#,
        ),
        (
            "climbing.json",
            r#"{"schema":"openconvert.job/1","input":{"path":"/in/../etc/passwd"},"output":{"path":"/out/book.epub"}}"#,
        ),
    ];
    for (name, text) in specs {
        std::fs::write(spec.join(name), text)?;
    }

    let books: [(&str, Vec<u8>); 3] = [
        ("paragraphs.bin", (0..=255_u8).cycle().take(1024).collect()),
        ("nested.bin", [8_u8, 0, 1, 2].repeat(128)),
        ("markup.bin", b"&<>\"'&<>\"'&<>\"'".repeat(32)),
    ];
    for (name, bytes) in books {
        std::fs::write(xhtml.join(name), bytes)?;
    }
    println!("fuzz seeds written to {}", corpus.display());
    Ok(())
}
