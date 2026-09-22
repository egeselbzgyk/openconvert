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
use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;

use crate::cli::{ConvertArgs, Progress};
use openconvert::convert::{convert, ConvertOptions};

pub(crate) const E_PDFIUM: &str = "E_PDFIUM_ABI";
const E_INPUT: &str = "E_INPUT";
/// The job spec named an input whose bytes are not the ones it hashed.
const E_INPUT_CHANGED: &str = "E_INPUT_CHANGED";
const E_PDF: &str = "E_PDF";
const E_PASSWORD: &str = "E_PASSWORD_REQUIRED";
const E_LIMIT: &str = "E_LIMIT_EXCEEDED";
const E_OUTPUT: &str = "E_OUTPUT";
/// The output exists and the job did not ask for it to be replaced.
const E_OUTPUT_EXISTS: &str = "E_OUTPUT_EXISTS";
const E_CONVERT: &str = "E_CONVERT";
const E_REPORT: &str = "E_REPORT";

/// One conversion, however it was asked for: from `convert`'s flags or from a job spec.
///
/// The two front ends resolve to this and nothing downstream knows which one it was — which is
/// what "one code path serves GUI, CLI, CI and benchmarks" means in practice (D13.1).
#[derive(Clone, Debug)]
pub struct ConvertJob {
    pub input: PathBuf,
    pub output: PathBuf,
    pub report: PathBuf,
    /// Replace an existing output. The CLI always has; a job spec says so explicitly, and the
    /// app never does — it picks a free name instead (the design's "never overwrite" rule).
    pub overwrite: bool,
    /// The input's SHA-256 as the job writer saw it; a mismatch means the file changed between
    /// the drop and the conversion.
    pub expected_sha256: Option<String>,
    pub preset: oc_model::document::PresetName,
    pub language: Option<oc_model::lang::LangTag>,
    pub password: Option<String>,
    pub modified: Option<String>,
    pub locale: oc_core::warnings::Locale,
    pub limits: oc_core::limits::Limits,
    pub job_id: Option<String>,
    /// NDJSON events on stderr. Always true for a job spec, whose only reader is a supervisor.
    pub json_events: bool,
}

impl ConvertJob {
    /// `convert`'s flags, resolved against the documented defaults (§2.1).
    pub fn from_args(args: &ConvertArgs) -> Self {
        let output = args
            .output
            .clone()
            .unwrap_or_else(|| default_output(&args.input));
        Self {
            input: args.input.clone(),
            report: args
                .report
                .clone()
                .unwrap_or_else(|| default_report(&output)),
            output,
            overwrite: true,
            expected_sha256: None,
            preset: args.preset,
            language: args.language.clone(),
            password: args.password.clone(),
            modified: args.modified.clone(),
            locale: args.locale,
            limits: oc_core::limits::Limits::default(),
            job_id: None,
            json_events: args.progress == Progress::Json,
        }
    }
}

/// Run the subcommand, returning the process exit code.
pub fn run<W: Write>(args: &ConvertArgs, events: &mut EventSink<W>) -> ExitCode {
    let backend = match PdfiumBackend::bind() {
        Ok(backend) => backend,
        Err(error) => {
            events.fatal(E_PDFIUM, &error.to_string());
            return ExitCode::Usage;
        }
    };
    hello(&backend, events);
    run_job(&ConvertJob::from_args(args), &backend, events)
}

/// The `hello` event: always the first line of a run (§2.3).
pub(crate) fn hello<W: Write>(backend: &PdfiumBackend, events: &mut EventSink<W>) {
    events.hello(
        env!("CARGO_PKG_VERSION"),
        oc_model::IR_VERSION,
        &backend.version().version,
    );
}

/// Convert one resolved job with a bound backend. `hello` has already been sent.
pub(crate) fn run_job<W: Write>(
    job: &ConvertJob,
    backend: &PdfiumBackend,
    events: &mut EventSink<W>,
) -> ExitCode {
    let args = job;
    let bytes = match std::fs::read(&args.input) {
        Ok(bytes) => bytes,
        Err(error) => {
            events.fatal(E_INPUT, &format!("{}: {error}", args.input.display()));
            return ExitCode::Usage;
        }
    };
    if let Some(expected) = &job.expected_sha256 {
        let actual = openconvert::convert::sha256_hex(&bytes);
        if &actual != expected {
            events.fatal(
                E_INPUT_CHANGED,
                &format!(
                    "{} has SHA-256 {actual}, the job spec expected {expected}",
                    args.input.display()
                ),
            );
            return ExitCode::Usage;
        }
    }

    let output = job.output.clone();
    if !job.overwrite && output.exists() {
        events.fatal(
            E_OUTPUT_EXISTS,
            &format!(
                "{} exists and the job does not allow replacing it",
                output.display()
            ),
        );
        return ExitCode::Usage;
    }

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

    let sha256 = openconvert::convert::sha256_hex(&bytes);
    let pdf = match backend.open_with_limits(&bytes, args.password.as_deref(), &job.limits) {
        Ok(pdf) => pdf,
        Err(error) => {
            let (code, exit) = open_failure(&error);
            events.fatal(code, &error.to_string());
            return exit;
        }
    };
    events.emit(
        "job",
        serde_json::json!({
            "job_id": job.job_id,
            "input_sha256": sha256,
            "pages": pdf.page_count(),
            "phase": "started",
        }),
    );

    events.stage("convert", "begin");
    let conversion = match convert(pdf.as_ref(), &sha256, &options, &T) {
        Ok(conversion) => conversion,
        Err(error) => {
            let code = match &error {
                openconvert::convert::ConvertError::Pdf(_) => E_PDF,
                _ => E_CONVERT,
            };
            events.fatal(code, &error.to_string());
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
    let report_path = job.report.clone();
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
                return ExitCode::Failed;
            }
        }
        Err(error) => {
            events.fatal(E_REPORT, &error.to_string());
            return ExitCode::Failed;
        }
    }

    if let Err(error) = write_atomically(&output, conversion.built.bytes.as_slice()) {
        events.fatal(E_OUTPUT, &format!("{}: {error}", output.display()));
        return ExitCode::Failed;
    }

    // The warnings, as sentences, for a person reading a terminal. Only when stderr is *not* the
    // NDJSON channel: with `--progress json` stderr is one JSON object per line and a line of prose
    // in it would break every reader (§2.3). The GUI localises the codes itself (R10 §6.20); this
    // is the same templates serving the other front end.
    if !job.json_events {
        for warning in &conversion.document.warnings {
            match oc_core::warnings::render(args.locale, warning.code, &warning.args) {
                Some(text) => eprintln!("{}: {text}", severity_name(warning.severity)),
                // A code with no template cannot happen — `xtask ci-lint` holds the registry and the
                // tree in agreement — and if it ever does, the code itself is more use than silence.
                None => eprintln!("{}: {}", severity_name(warning.severity), warning.code),
            }
        }
    }

    // After the repair cap the EPUB is still written and the report says `invalid`; the CLI says so
    // too rather than reporting success (D13.7). Still exit 0: the book exists and the report is
    // where the verdict lives, and a script that treated "slightly invalid" as "no output" would
    // throw away a usable book.
    if report.status == openconvert::report::Status::Invalid && !job.json_events {
        eprintln!(
            "warning: the container is not valid: see {}",
            report_path.display()
        );
    }

    events.done_job(
        "ok",
        &report_path.to_string_lossy(),
        Some(&output.to_string_lossy()),
    );
    ExitCode::Ok
}

/// The fatal code and exit status for a document that would not open.
///
/// A password and a limit are both things the user can act on — the app prompts for the one and
/// names the other — so they get codes of their own, the same ones `inspect` and `dump-stage`
/// use, and exit 2: nothing was attempted. Anything else is a PDF this engine cannot read.
fn open_failure(error: &oc_pdf::error::PdfError) -> (&'static str, ExitCode) {
    match error {
        oc_pdf::error::PdfError::PasswordRequired => (E_PASSWORD, ExitCode::Usage),
        oc_pdf::error::PdfError::LimitExceeded(_) => (E_LIMIT, ExitCode::Usage),
        _ => (E_PDF, ExitCode::Failed),
    }
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
