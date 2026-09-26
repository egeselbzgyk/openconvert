//! `openconvert convert` (IMPLEMENTATION_PLAN §2.1).
//!
//! The output is written to `<output>.oc-tmp-<rand>` in the destination directory and renamed
//! atomically on success (D13.2). A conversion that fails halfway therefore leaves no partial
//! EPUB behind for a file manager to show as a book — and the rename is on the same filesystem
//! by construction, because the temporary sits beside the destination rather than in `/tmp`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use oc_core::cancel::{Cancel, Outcome};
use oc_core::progress::StagePhase;

use oc_core::events::EventSink;
use oc_core::exit::ExitCode;
use oc_core::thresholds::T;
use oc_epub::EpubOptions;
use oc_pdf::backend::PdfBackend;
use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;

use crate::cli::{AiArgs, ConvertArgs, Progress};
use openconvert::ai_endpoint::{self, OpenError};
use openconvert::convert::{convert_observed, ConvertError, ConvertOptions, Observe};
use openconvert::sandbox::SandboxReport;

/// The model registry the engine was built with (PHASE 9 detail 6).
const BUNDLED_REGISTRY: &str = include_str!("../../../models.toml");

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
const E_USAGE: &str = "E_USAGE";

/// The environment variable naming the directory where a run saves what `structure` settled, for
/// a later run with the user's corrections to resume from (A12.4b).
pub const CACHE_VAR: &str = "OC_CACHE_DIR";

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
    /// The `overrides.json` the job named (ARCHITECTURE §4.7).
    pub overrides: Option<PathBuf>,
    /// How the model is reached, when it is (PHASE 10). `None` is `ai.enabled = false`.
    pub ai: Option<AiArgs>,
    /// Whether scanned content is read (PHASE 13).
    pub ocr: oc_core::ocr::OcrMode,
    /// An explicit `tesseract`, replacing discovery.
    pub ocr_path: Option<PathBuf>,
    /// Traineddata names; by default the document's language decides.
    pub ocr_lang: Option<oc_core::ocr::lang::LangSpec>,
    /// Whether an OCR sandwich's own layer is replaced (D13.10).
    pub re_ocr: oc_core::ocr::ReOcr,
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
            limits: args.limits,
            job_id: None,
            json_events: args.progress == Progress::Json,
            overrides: args.overrides.clone(),
            ai: args.ai.clone(),
            ocr: args.ocr,
            ocr_path: args.ocr_path.clone(),
            ocr_lang: args.ocr_lang.clone(),
            re_ocr: args.re_ocr,
        }
    }
}

/// Run the subcommand, returning the process exit code.
pub fn run<W: Write + Send>(args: &ConvertArgs, events: &EventSink<W>) -> ExitCode {
    let backend = match PdfiumBackend::bind() {
        Ok(backend) => backend,
        Err(error) => {
            events.fatal(E_PDFIUM, &error.to_string());
            return ExitCode::Usage;
        }
    };
    let job = ConvertJob::from_args(args);
    hello(
        &backend,
        events,
        &ocr_capabilities(job.ocr, job.ocr_path.as_deref()),
    );
    run_job(&job, &backend, events)
}

/// `hello` from a freshly bound backend, or the `fatal` that says why there is none.
///
/// The capabilities are the ones a job spec's run would have — OCR by discovery, as `auto` finds
/// it — so a supervisor's version handshake learns what its conversions can do.
pub(crate) fn announce<W: Write>(events: &EventSink<W>) {
    match PdfiumBackend::bind() {
        Ok(backend) => hello(
            &backend,
            events,
            &ocr_capabilities(oc_core::ocr::OcrMode::Auto, None),
        ),
        Err(error) => events.fatal(E_PDFIUM, &error.to_string()),
    }
}

/// The `hello` event: always the first line of a run (§2.3).
pub(crate) fn hello<W: Write>(backend: &PdfiumBackend, events: &EventSink<W>, extra: &[String]) {
    events.hello_with(
        env!("CARGO_PKG_VERSION"),
        oc_model::IR_VERSION,
        &backend.version().version,
        extra,
    );
}

/// The capabilities this run discovered beyond the ones every engine has: `ocr:tesseract-5.3.4`
/// when a usable Tesseract was found (PHASE 13 detail 1). Discovery runs once, before anything is
/// read, so that `hello` can say it; `--ocr never` asks nothing of the machine at all.
pub(crate) fn ocr_capabilities(mode: oc_core::ocr::OcrMode, path: Option<&Path>) -> Vec<String> {
    if mode == oc_core::ocr::OcrMode::Never {
        return Vec::new();
    }
    oc_core::ocr::discover::discover(path)
        .map(|info| vec![info.capability()])
        .unwrap_or_default()
}

/// How a run ended, decided before the final event is written.
///
/// The final line — `done` or `fatal` — is written only after the heartbeat has stopped, so that it
/// is always the last line on the channel (§2.3). The steps therefore return this instead of
/// writing it.
enum Ending {
    Done {
        report: PathBuf,
        output: PathBuf,
    },
    Cancelled,
    Fatal {
        code: &'static str,
        message: String,
        exit: ExitCode,
    },
}

fn fatal(code: &'static str, message: String, exit: ExitCode) -> Ending {
    Ending::Fatal {
        code,
        message,
        exit,
    }
}

/// Convert one resolved job with a bound backend. `hello` has already been sent.
///
/// Listens on stdin for a cancel for the whole run, and sends a heartbeat every
/// `ipc.heartbeat_secs` until the run has decided how it ends (D13.2).
pub(crate) fn run_job<W: Write + Send>(
    job: &ConvertJob,
    backend: &PdfiumBackend,
    events: &EventSink<W>,
) -> ExitCode {
    // The memory cap first, before the PDF is opened and before any conversion thread exists
    // (PHASE 14 detail 5). Nothing here can fail the run: the record says what was applied.
    let mut sandbox = SandboxReport::start(&job.limits);
    let cancel = Cancel::new();
    // Every stage gets `limits.stage_deadline_secs`, on the same flag a cancel sets (detail 6).
    cancel.set_stage_deadline(Duration::from_secs(job.limits.stage_deadline_secs));
    crate::control::listen(cancel.clone());
    let interval = Duration::from_secs(u64::try_from(T.ipc.heartbeat_secs).unwrap_or(u64::MAX));

    let ending = events.with_heartbeat(interval, || {
        steps(job, backend, events, &cancel, &mut sandbox)
    });
    match ending {
        Ending::Done { report, output } => {
            events.done_job(
                Outcome::Completed.status(),
                &report.to_string_lossy(),
                Some(&output.to_string_lossy()),
            );
            ExitCode::Ok
        }
        Ending::Cancelled => {
            events.done(Outcome::Cancelled.status());
            ExitCode::Cancelled
        }
        Ending::Fatal {
            code,
            message,
            exit,
        } => {
            events.fatal(code, &message);
            exit
        }
    }
}

/// Everything between `hello` and the final event.
fn steps<W: Write + Send>(
    job: &ConvertJob,
    backend: &PdfiumBackend,
    events: &EventSink<W>,
    cancel: &Cancel,
    sandbox: &mut SandboxReport,
) -> Ending {
    let args = job;

    // AI: open the model, or learn why not — before the sandbox, because an engine-owned server is
    // started here and must not inherit it, and before the PDF, because an endpoint off this
    // machine without consent, and an endpoint the engine will not interpret, are refused before
    // anything is read or sent. Anything else that stops a model answering converts the book
    // without one, and says so.
    let (opened, unavailable) = match &job.ai {
        None => (None, None),
        Some(ai) => match ai_endpoint::open(ai, BUNDLED_REGISTRY, &T) {
            Ok(opened) => (Some(opened), None),
            Err(OpenError::Unavailable(reason)) => (None, Some(reason)),
            Err(refused) => {
                let exit = refused.exit_code();
                let (code, message) = refused
                    .fatal()
                    .unwrap_or((E_USAGE, "the model could not be opened".to_owned()));
                return fatal(code, message, exit);
            }
        },
    };

    // OCR's rasters go to a job-private directory beside the output, deleted with the conversion
    // whatever became of it. Discovery runs now, before the sandbox, so the scope knows which
    // program a page may need.
    let output = job.output.clone();
    let workdir = temporary_beside(&output);
    let (ocr, ocr_program) = ocr_options(job, &workdir);
    let llm_cache_dir = openconvert::data_dir::llm_cache();
    let cache_dir = std::env::var_os(CACHE_VAR).map(PathBuf::from);

    // Landlock, after the job is validated and **before the first PDF byte is read** (PHASE 14
    // detail 7): read the input, write beside the output and the report, execute what OCR needs,
    // connect only to the model. Never fatal; the report records what happened.
    let mut writes: Vec<PathBuf> = cache_dir.iter().cloned().collect();
    if opened.is_some() {
        let _ = std::fs::create_dir_all(&llm_cache_dir);
        writes.push(llm_cache_dir.clone());
    }
    let scope = openconvert::sandbox::scope_for(&openconvert::sandbox::ScopeInputs {
        input: job.input.clone(),
        output: output.clone(),
        report: job.report.clone(),
        extra_reads: job.overrides.iter().cloned().collect(),
        extra_writes: writes,
        programs: ocr_program.into_iter().collect(),
        connect_ports: opened
            .as_ref()
            .and_then(|opened| opened.port)
            .into_iter()
            .collect(),
    });
    sandbox.landlock = Some(openconvert::sandbox::restrict(&scope));
    sandbox.memory.observe_at_open();

    let bytes = match std::fs::read(&args.input) {
        Ok(bytes) => bytes,
        Err(error) => {
            return fatal(
                E_INPUT,
                format!("{}: {error}", args.input.display()),
                ExitCode::Usage,
            )
        }
    };
    let sha256 = openconvert::convert::sha256_hex(&bytes);
    if let Some(expected) = &job.expected_sha256 {
        if &sha256 != expected {
            return fatal(
                E_INPUT_CHANGED,
                format!(
                    "{} has SHA-256 {sha256}, the job spec expected {expected}",
                    args.input.display()
                ),
                ExitCode::Usage,
            );
        }
    }

    if !job.overwrite && output.exists() {
        return fatal(
            E_OUTPUT_EXISTS,
            format!(
                "{} exists and the job does not allow replacing it",
                output.display()
            ),
            ExitCode::Usage,
        );
    }

    // The model's time is measured on this clock (D13.6).
    let clock = oc_ai::session::SystemClock::new();

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
        // An unreadable file reads as empty text, which the conversion refuses by name
        // (`W_OVERRIDES_UNREADABLE`) and converts the book without: the job asked for corrections,
        // and the report has to say they were not applied rather than the run pretend none were asked.
        overrides: args
            .overrides
            .as_ref()
            .map(|path| std::fs::read_to_string(path).unwrap_or_default()),
        // The app names its cache directory in the environment of every engine it starts, so a
        // "Fix and rebuild" can resume after `structure` (A12.4b). Unset, nothing is saved.
        cache_dir,
        ocr,
    };

    let failed = |code: &'static str, message: String, cap: Option<&'static str>| {
        failure(job, backend, Some(&sha256), code, message, cap, sandbox)
    };
    let pdf = match backend.open_with_limits(&bytes, args.password.as_deref(), &job.limits) {
        Ok(pdf) => pdf,
        Err(error) => {
            let (code, exit) = open_failure(&error);
            if exit == ExitCode::Failed {
                return failed(code, error.to_string(), error.cap());
            }
            return fatal(code, error.to_string(), exit);
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

    let llm_cache = oc_ai::cache::FileCache::new(llm_cache_dir);
    let context = opened.as_ref().map(|opened| openconvert::ai::AiContext {
        provider: opened.provider.as_ref(),
        cache: Some(&llm_cache),
        clock: &clock,
        all_tasks: job.ai.as_ref().is_some_and(|ai| ai.all_tasks),
    });

    let progress = EventProgress::new(events);
    let observe = Observe {
        progress: &progress,
        cancel,
    };
    let converted = convert_observed(
        pdf.as_ref(),
        &sha256,
        &options,
        context.as_ref(),
        &T,
        observe,
    );
    // The engine-owned server, if any, is not needed past the conversion: stop it now rather than
    // at exit (D8's idle-kill, reached at once). What the report says about the provider — which
    // adapter, and the consent it needed — outlives it.
    let provider_kind = opened.as_ref().map(|opened| opened.kind);
    let consent = opened.as_ref().and_then(|opened| opened.consent.clone());
    drop(opened);
    // The rasters are deleted after every call; the directory goes with the conversion, whatever
    // became of it.
    let _ = std::fs::remove_dir_all(&workdir);
    let mut conversion = match converted {
        Ok(conversion) => conversion,
        // One abort path, two causes (PHASE 14 detail 6): the user's cancel ends as a cancel, a
        // stage's deadline as a failure that names the cap.
        Err(ConvertError::Cancelled) => {
            let limit = cancel.stage_deadline().unwrap_or_default();
            return match cancel.cause().and_then(|cause| cause.violation(limit)) {
                Some(violation) => failed(E_LIMIT, violation.to_string(), Some(violation.cap())),
                None => Ending::Cancelled,
            };
        }
        Err(error) => {
            let (code, cap) = match &error {
                ConvertError::Pdf(pdf) if pdf.cap().is_some() => (E_LIMIT, pdf.cap()),
                ConvertError::Pdf(_) => (E_PDF, None),
                _ => (E_CONVERT, None),
            };
            return failed(code, error.to_string(), cap);
        }
    };

    // AI was asked for and no model could be reached: the banner (RT D20).
    if let Some(reason) = unavailable {
        conversion
            .document
            .warnings
            .push(openconvert::ai::unavailable(reason));
    }
    // One `llm` event per call, cached ones included (D13.2).
    if let Some(outcome) = &conversion.ai {
        for call in &outcome.calls {
            events.emit(
                "llm",
                serde_json::json!({
                    "call_id": call.call_id,
                    "purpose": call.purpose.as_str(),
                    "cached": call.cached,
                    "tokens_in": call.tokens_in,
                    "tokens_out": call.tokens_out,
                    "ms": call.ms,
                }),
            );
        }
    }

    // Every warning the conversion collected, as `code` + `args`. Phase 5 emitted the emitter's
    // codes with an empty argument object; the document now carries the arguments too, and a
    // warning that says "retention 0.950 against a floor of 0.980" is the one a user can act on
    // (PIPELINE §13, R10 §6.20).
    for warning in &conversion.document.warnings {
        let args = serde_json::to_value(&warning.args).unwrap_or_else(|_| serde_json::json!({}));
        events.warning(warning.code, severity_name(warning.severity), args);
    }

    // The last boundary a cancel is honoured at: past it, the book is written and the answer is
    // the book. Nothing has been written to the destination yet — neither the report nor the
    // temporary — so a cancelled job leaves the directory as it found it.
    if cancel.is_cancelled() {
        let limit = cancel.stage_deadline().unwrap_or_default();
        return match cancel.cause().and_then(|cause| cause.violation(limit)) {
            Some(violation) => failed(E_LIMIT, violation.to_string(), Some(violation.cap())),
            None => Ending::Cancelled,
        };
    }

    // The report is written **before** the atomic rename, so a report exists even for a conversion
    // that then fails to place its output (PIPELINE §13).
    let report_path = job.report.clone();
    let record = sandbox.clone();
    let written = oc_core::progress::timed(&progress, "report", || {
        let mut report = openconvert::report::report(
            &conversion,
            openconvert::report::ReportInput {
                filename: &options.filename,
                pdfium_version: &backend.version().version,
                producer_family: conversion.producer_family,
                pages: page_count(&conversion),
                page_classes: conversion.page_classes.clone(),
                provider: provider_kind,
                consent: consent.as_ref(),
            },
        );
        report.sandbox = Some(record);
        match openconvert::report::to_json(&report) {
            Ok(json) => std::fs::write(&report_path, json)
                .map(|()| report)
                .map_err(|error| format!("{}: {error}", report_path.display())),
            Err(error) => Err(error.to_string()),
        }
    });
    let report = match written {
        Ok(report) => report,
        Err(message) => return fatal(E_REPORT, message, ExitCode::Failed),
    };

    if let Err(error) =
        openconvert::deliver::write_atomically(&output, conversion.built.bytes.as_slice())
    {
        return fatal(
            E_OUTPUT,
            format!("{}: {error}", output.display()),
            ExitCode::Failed,
        );
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

    Ending::Done {
        report: report_path,
        output,
    }
}

/// Progress as NDJSON: `stage` edges as they happen, `progress` coalesced (D13.2).
///
/// Coalescing is the sink's job and not the stages' (ARCHITECTURE §8.3): a page loop reports every
/// page, and this lets through at most `ipc.progress_max_per_sec` per stage per second — always
/// including a stage's last unit, so a bar that reaches the end is never left short of it.
struct EventProgress<'a, W: Write + Send> {
    events: &'a EventSink<W>,
    min_gap: Duration,
    last: std::sync::Mutex<std::collections::BTreeMap<String, Instant>>,
}

impl<'a, W: Write + Send> EventProgress<'a, W> {
    fn new(events: &'a EventSink<W>) -> Self {
        let per_second = u32::try_from(T.ipc.progress_max_per_sec)
            .unwrap_or(u32::MAX)
            .max(1);
        Self {
            events,
            min_gap: Duration::from_secs(1) / per_second,
            last: std::sync::Mutex::new(std::collections::BTreeMap::new()),
        }
    }
}

impl<W: Write + Send> oc_core::progress::Progress for EventProgress<'_, W> {
    fn advance(&self, stage: &str, index: u32, total: u32) {
        let done = index.saturating_add(1);
        let now = Instant::now();
        let mut last = self
            .last
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let due = last
            .get(stage)
            .is_none_or(|previous| now.duration_since(*previous) >= self.min_gap);
        if due || done >= total {
            last.insert(stage.to_owned(), now);
            self.events.progress(stage, done, total, "pages");
        }
    }

    fn stage(&self, name: &str, phase: StagePhase) {
        match phase {
            StagePhase::Begin => self.events.stage(name, "begin"),
            StagePhase::End { elapsed_ms } => self.events.stage_end(name, elapsed_ms),
        }
    }
}

/// The fatal code and exit status for a document that would not open.
///
/// A password and a limit are both things the user can act on — the app prompts for the one and
/// names the other — so they get codes of their own, the same ones `inspect` and `dump-stage`
/// use, and exit 2: nothing was attempted. Anything else is a PDF this engine cannot read.
fn open_failure(error: &oc_pdf::error::PdfError) -> (&'static str, ExitCode) {
    match error {
        oc_pdf::error::PdfError::PasswordRequired => (E_PASSWORD, ExitCode::Usage),
        // A cap is a conversion that failed on this file, with a report naming the cap (PHASE 14
        // rows 14.6 and 14.9): exit 1, not a usage error.
        oc_pdf::error::PdfError::LimitExceeded(_) | oc_pdf::error::PdfError::Cap(_) => {
            (E_LIMIT, ExitCode::Failed)
        }
        _ => (E_PDF, ExitCode::Failed),
    }
}

/// End the run as a failed conversion: exit 1, and a report at the report path that says what
/// stopped it (PHASE 14 details 9–10, D13.2 "1 failed (report written)"). No book is written —
/// nothing reaches the destination before a conversion succeeds.
fn failure(
    job: &ConvertJob,
    backend: &PdfiumBackend,
    sha256: Option<&str>,
    code: &'static str,
    message: String,
    cap: Option<&'static str>,
    sandbox: &SandboxReport,
) -> Ending {
    let report = openconvert::report::failure_report(
        &backend.version().version,
        openconvert::report::FailedInput {
            filename: job
                .input
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            sha256: sha256.map(str::to_owned),
        },
        openconvert::report::Failure {
            code,
            message: message.clone(),
            cap,
        },
        Some(sandbox.clone()),
    );
    let message = match openconvert::report::failure_to_json(&report)
        .map_err(|error| error.to_string())
        .and_then(|json| std::fs::write(&job.report, json).map_err(|error| error.to_string()))
    {
        Ok(()) => message,
        Err(error) => format!(
            "{message} (and the report could not be written to {}: {error})",
            job.report.display()
        ),
    };
    fatal(code, message, ExitCode::Failed)
}

/// What the job's OCR settings ask for, with the engine discovered (PHASE 13), and the program a
/// page may start — which the sandbox must let the run execute.
fn ocr_options(
    job: &ConvertJob,
    workdir: &Path,
) -> (openconvert::ocr::OcrOptions, Option<PathBuf>) {
    use oc_core::ocr::discover::{discover, region_deadline};
    use oc_core::ocr::invoke::{OcrEngine, Tesseract};
    use oc_core::ocr::OcrMode;

    if job.ocr == OcrMode::Never {
        return (openconvert::ocr::OcrOptions::off(), None);
    }
    let found = discover(job.ocr_path.as_deref());
    let program = found.as_ref().ok().map(|info| info.path.clone());
    let engine = found.map(|info| {
        std::sync::Arc::new(Tesseract::new(info, workdir, region_deadline()))
            as std::sync::Arc<dyn OcrEngine>
    });
    let mut options = openconvert::ocr::OcrOptions::auto(engine, &T);
    options.mode = job.ocr;
    options.re_ocr = job.re_ocr;
    options.langs = job.ocr_lang.clone();
    (options, program)
}

/// `<output>.oc-tmp-<token>`: a job-private temporary beside the output, on the same filesystem.
fn temporary_beside(output: &Path) -> PathBuf {
    let directory = output.parent().unwrap_or_else(|| Path::new("."));
    let token = format!(
        "{:x}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or_default(),
        std::process::id()
    );
    directory.join(format!(
        "{}.oc-tmp-{token}",
        output
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "out.epub".to_owned())
    ))
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
