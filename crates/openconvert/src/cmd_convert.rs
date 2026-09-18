//! `openconvert convert` (IMPLEMENTATION_PLAN §2.1).
//!
//! The output is written to `<output>.oc-tmp-<rand>` in the destination directory and renamed
//! atomically on success (D13.2). A conversion that fails halfway therefore leaves no partial
//! EPUB behind for a file manager to show as a book — and the rename is on the same filesystem
//! by construction, because the temporary sits beside the destination rather than in `/tmp`.

use std::io::Write;
use std::path::{Path, PathBuf};

use oc_core::events::EventSink;
use oc_core::exit::ExitCode;
use oc_core::thresholds::T;
use oc_epub::EpubOptions;
use oc_pdf::backend::PdfBackend;
use oc_pdf::pdfium::PdfiumBackend;

use crate::cli::ConvertArgs;
use openconvert::convert::{convert_bytes, ConvertOptions};

const E_PDFIUM: &str = "E_PDFIUM_ABI";
const E_INPUT: &str = "E_INPUT";
const E_PDF: &str = "E_PDF";
const E_OUTPUT: &str = "E_OUTPUT";
const E_CONVERT: &str = "E_CONVERT";
const E_REPORT: &str = "E_REPORT";

/// Run the subcommand, returning the process exit code.
pub fn run<W: Write>(args: &ConvertArgs, events: &mut EventSink<W>) -> ExitCode {
    let backend = match PdfiumBackend::bind() {
        Ok(backend) => backend,
        Err(error) => {
            events.fatal(E_PDFIUM, &error.to_string());
            return ExitCode::Usage;
        }
    };

    let bytes = match std::fs::read(&args.input) {
        Ok(bytes) => bytes,
        Err(error) => {
            events.fatal(E_INPUT, &format!("{}: {error}", args.input.display()));
            return ExitCode::Usage;
        }
    };

    let output = args
        .output
        .clone()
        .unwrap_or_else(|| default_output(&args.input));

    let options = ConvertOptions {
        filename: args
            .input
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        language: args.language.clone(),
        preset: args.preset,
        epub: EpubOptions {
            split_bytes: usize::try_from(T.xhtml.split_bytes).unwrap_or(usize::MAX),
            max_longest_side_px: u32::try_from(T.images.max_longest_side_px).unwrap_or(u32::MAX),
            jpeg_quality: u8::try_from(T.images.jpeg_quality).unwrap_or(85),
            warn_total_bytes: u64::try_from(T.epub.warn_total_bytes).unwrap_or(u64::MAX),
            modified: args.modified.clone().unwrap_or_else(now_utc),
        },
    };

    events.stage("convert", "begin");
    let conversion = match convert_bytes(&backend, &bytes, args.password.as_deref(), &options, &T) {
        Ok(conversion) => conversion,
        Err(error) => {
            let code = match &error {
                openconvert::convert::ConvertError::Pdf(_) => E_PDF,
                _ => E_CONVERT,
            };
            events.fatal(code, &error.to_string());
            eprintln!("error: {error}");
            return ExitCode::Failed;
        }
    };
    events.stage("convert", "end");

    // Every warning the conversion collected, as `code` + `args`. Phase 5 emitted the emitter's
    // codes with an empty argument object; the document now carries the arguments too, and a
    // warning that says "retention 0.950 against a floor of 0.980" is the one a user can act on
    // (PIPELINE §13, R10 §6.20).
    for warning in &conversion.document.warnings {
        let args = serde_json::to_value(&warning.args).unwrap_or_else(|_| serde_json::json!({}));
        events.warning(warning.code, severity_name(warning.severity), args);
    }

    // The report is written **before** the atomic rename, so a report exists even for a conversion
    // that then fails to place its output (PIPELINE §13).
    let report_path = args
        .report
        .clone()
        .unwrap_or_else(|| default_report(&output));
    let report = openconvert::report::report(
        &conversion,
        openconvert::report::ReportInput {
            filename: &options.filename,
            pdfium_version: &backend.version().version,
            producer_family: conversion.producer_family,
            pages: page_count(&conversion),
            page_classes: conversion.page_classes.clone(),
        },
    );
    match openconvert::report::to_json(&report) {
        Ok(json) => {
            if let Err(error) = std::fs::write(&report_path, json) {
                events.fatal(E_REPORT, &format!("{}: {error}", report_path.display()));
                eprintln!("error: {}: {error}", report_path.display());
                return ExitCode::Failed;
            }
        }
        Err(error) => {
            events.fatal(E_REPORT, &error.to_string());
            eprintln!("error: the report could not be serialised: {error}");
            return ExitCode::Failed;
        }
    }

    if let Err(error) = write_atomically(&output, conversion.built.bytes.as_slice()) {
        events.fatal(E_OUTPUT, &format!("{}: {error}", output.display()));
        eprintln!("error: {}: {error}", output.display());
        return ExitCode::Failed;
    }

    // After the repair cap the EPUB is still written and the report says `invalid`; the CLI says so
    // too rather than reporting success (D13.7). Still exit 0: the book exists and the report is
    // where the verdict lives, and a script that treated "slightly invalid" as "no output" would
    // throw away a usable book.
    if report.status == openconvert::report::Status::Invalid {
        eprintln!(
            "warning: the container is not valid: see {}",
            report_path.display()
        );
    }

    events.done_with("ok", Some(&output.to_string_lossy()));
    ExitCode::Ok
}

/// `<output>.report.json` (§2.1).
fn default_report(output: &Path) -> PathBuf {
    let mut name = output.as_os_str().to_os_string();
    name.push(".report.json");
    PathBuf::from(name)
}

/// The NDJSON event schema's severity names (§2.3).
fn severity_name(severity: oc_model::doc::Severity) -> &'static str {
    match severity {
        oc_model::doc::Severity::Info => "info",
        oc_model::doc::Severity::Warn => "warn",
        oc_model::doc::Severity::Error => "error",
    }
}

/// How many pages the book has, counted from the page breaks the document carries.
fn page_count(conversion: &openconvert::convert::Conversion) -> u32 {
    u32::try_from(conversion.document.page_breaks.len()).unwrap_or(u32::MAX)
}

/// `<input>.epub` next to the input.
fn default_output(input: &Path) -> PathBuf {
    input.with_extension("epub")
}

/// Write to a temporary beside the destination and rename over it (D13.2).
fn write_atomically(output: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let directory = output.parent().unwrap_or_else(|| Path::new("."));
    // Enough entropy that two conversions of one book into one directory do not collide, and
    // no more: this is a temporary name, not a secret.
    let token = format!(
        "{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or_default()
    );
    let temporary = directory.join(format!(
        "{}.oc-tmp-{token}",
        output
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "out.epub".to_owned())
    ));

    std::fs::write(&temporary, bytes)?;
    match std::fs::rename(&temporary, output) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = std::fs::remove_file(&temporary);
            Err(error)
        }
    }
}

/// `dcterms:modified`, to the second, in UTC.
fn now_utc() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}
