//! `openconvert <JOB.json>`: the desktop app's one argument (D13.2, RT B15).
//!
//! The app writes a job spec into a directory it controls and spawns the engine with that path
//! and nothing else. Everything is read from the file, validated against the committed schema
//! before the PDF is touched, and resolved into the same [`ConvertJob`] the `convert` flags
//! resolve to — so the GUI's conversions and the CLI's are one code path (D13.1).
//!
//! Events are always NDJSON on stderr here: the only reader of this form is a supervisor. A spec
//! that does not validate is exit code 2 with `fatal{code: "E_JOBSPEC"}` (§2.2), after `hello`,
//! so the supervisor has already been able to check it is talking to the engine it expects.

use std::io::Write;
use std::path::{Path, PathBuf};

use oc_core::events::EventSink;
use oc_core::exit::ExitCode;
use oc_core::jobspec::{self, JobSpec};
use oc_core::limits::Limits;
use oc_pdf::pdfium::PdfiumBackend;

use crate::cli::AiArgs;
use crate::cmd_convert::{hello, ocr_capabilities, run_job, ConvertJob, E_PDFIUM};

/// Every refusal of the spec itself, whatever the reason (§2.2).
pub const E_JOBSPEC: &str = "E_JOBSPEC";

/// The environment variable a password may arrive in (D13.11).
const PASSWORD_VAR: &str = "OC_PDF_PASSWORD";

/// Run one job spec, returning the process exit code.
pub fn run<W: Write + Send>(path: &Path, events: &EventSink<W>) -> ExitCode {
    let backend = match PdfiumBackend::bind() {
        Ok(backend) => backend,
        Err(error) => {
            events.fatal(E_PDFIUM, &error.to_string());
            return ExitCode::Usage;
        }
    };
    // A spec has no OCR settings: its run reads scanned pages the way `convert`'s default does,
    // with whatever Tesseract discovery finds (PHASE 13).
    hello(
        &backend,
        events,
        &ocr_capabilities(oc_core::ocr::OcrMode::Auto, None),
    );

    let job = match read(path) {
        Ok(job) => job,
        Err(message) => {
            events.fatal(E_JOBSPEC, &message);
            return ExitCode::Usage;
        }
    };
    run_job(&job, &backend, events)
}

/// Read, validate and resolve a spec file. The error is the fatal's message.
fn read(path: &Path) -> Result<ConvertJob, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read the job spec {}: {error}", path.display()))?;
    let spec = jobspec::parse(&text).map_err(|error| error.to_string())?;
    resolve(&spec)
}

/// Turn a valid spec into a job, refusing what this engine cannot honour.
///
/// **Refused, not ignored.** A spec field the engine does not act on yet — threshold overrides,
/// stage dumps — is a refusal naming the field. Ignoring it would convert a book the user asked to
/// be converted differently and report success, which is the one outcome worse than an error.
pub fn resolve(spec: &JobSpec) -> Result<ConvertJob, String> {
    if spec
        .threshold_overrides
        .as_ref()
        .is_some_and(|overrides| !overrides.is_empty())
    {
        return Err(unsupported("threshold_overrides"));
    }
    if spec
        .dump_stages
        .as_ref()
        .is_some_and(|stages| !stages.is_empty())
    {
        return Err(unsupported("dump_stages"));
    }
    // A password comes from a file the spec names, or from `OC_PDF_PASSWORD` — the same variable
    // `convert` reads (D13.11) — and never from the spec itself or the command line, where anything
    // that can list processes or read the job directory would see it. The desktop app uses the
    // variable for the one job a user unlocks, so the password never touches the disk.
    let password = match &spec.input.password_file {
        Some(file) => Some(read_password(file)?),
        None => std::env::var(PASSWORD_VAR).ok(),
    };

    let mut limits = Limits::default();
    if let Some(requested) = spec.limits {
        if let Some(pages) = requested.max_pages {
            limits.max_pages = u32::try_from(pages).map_err(|_| {
                format!("limits.max_pages {pages} is larger than this engine reads")
            })?;
        }
        if let Some(bytes) = requested.max_memory_bytes {
            limits.max_memory_bytes = bytes;
        }
        if let Some(seconds) = requested.stage_deadline_secs {
            limits.stage_deadline_secs = seconds;
        }
    }

    let output = spec.output.path.clone();
    Ok(ConvertJob {
        input: spec.input.path.clone(),
        report: spec
            .output
            .report_path
            .clone()
            .unwrap_or_else(|| report_beside(&output)),
        output,
        overwrite: spec.output.overwrite,
        expected_sha256: spec.input.sha256.clone(),
        preset: spec.preset.unwrap_or_default(),
        language: None,
        password,
        modified: None,
        locale: oc_core::warnings::Locale::from_tag(spec.locale.as_deref().unwrap_or_default()),
        limits,
        job_id: spec.job_id.clone(),
        json_events: true,
        overrides: spec.overrides_path.clone(),
        ai: spec.ai.as_ref().and_then(ai_args),
        ocr: oc_core::ocr::OcrMode::Auto,
        ocr_path: None,
        ocr_lang: None,
        re_ocr: oc_core::ocr::ReOcr::Never,
    })
}

/// The spec's `ai` object as `convert --ai` and its flags (PHASE 10, PHASE 11): `None` unless it is
/// switched on. `non_loopback_consent` is consent to the endpoint's own host — the host the app's
/// consent dialog named — and to no other ([`AiArgs::consenting_to_the_endpoint`]). The validator
/// has already refused an endpoint off this machine without it (D10); the engine checks again
/// before it sends anything, and a host nothing consented to is `E_CONSENT_REQUIRED`, exit 2.
fn ai_args(ai: &oc_core::jobspec::AiSpec) -> Option<AiArgs> {
    if !ai.enabled {
        return None;
    }
    let args = AiArgs {
        endpoint: ai.endpoint.clone(),
        api_key_file: ai.api_key_file.clone(),
        model_path: ai.model_path.clone(),
        model: ai.model_id.clone(),
        ..AiArgs::default()
    };
    Some(if ai.non_loopback_consent {
        args.consenting_to_the_endpoint()
    } else {
        args
    })
}

fn unsupported(field: &str) -> String {
    format!("`{field}` is valid in a job spec but this engine does not implement it yet")
}

/// The password file's contents, without the line ending an editor adds.
fn read_password(file: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(file)
        .map_err(|error| format!("cannot read the password file {}: {error}", file.display()))?;
    Ok(text.trim_end_matches(['\r', '\n']).to_owned())
}

/// `<output>.report.json`, the same default as `convert --report` (§2.1).
fn report_beside(output: &Path) -> PathBuf {
    let mut name = output.as_os_str().to_os_string();
    name.push(".report.json");
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> JobSpec {
        JobSpec::new("/in/book.pdf".into(), "/out/book.epub".into())
    }

    #[test]
    fn a_minimal_spec_resolves_to_the_documented_defaults() {
        let job = resolve(&spec()).expect("resolves");
        assert_eq!(job.report, PathBuf::from("/out/book.epub.report.json"));
        assert!(
            !job.overwrite,
            "a job spec never replaces an output unless it says so"
        );
        assert!(job.json_events, "the supervisor always gets events");
        assert_eq!(job.limits, Limits::default());
    }

    #[test]
    fn spec_limits_override_the_defaults() {
        let mut spec = spec();
        spec.limits = Some(oc_core::jobspec::LimitsSpec {
            max_pages: Some(7),
            max_memory_bytes: None,
            stage_deadline_secs: Some(9),
        });
        let job = resolve(&spec).expect("resolves");
        assert_eq!(job.limits.max_pages, 7);
        assert_eq!(job.limits.stage_deadline_secs, 9);
        assert_eq!(
            job.limits.max_memory_bytes,
            Limits::default().max_memory_bytes
        );
    }

    /// The desktop app's `ai` object is `convert --ai` and its flags, field for field (PHASE 11's
    /// hand-off): `endpoint` → `--llm-endpoint`, `api_key_file` → `--llm-api-key-file`,
    /// `model_path` → `--model-path`, `model_id` → `--llm-model`.
    #[test]
    fn an_ai_spec_resolves_to_the_arguments_convert_takes() {
        let mut on = spec();
        on.ai = Some(oc_core::jobspec::AiSpec {
            enabled: true,
            endpoint: Some("http://127.0.0.1:8123".to_owned()),
            api_key_file: Some("/run/oc/llm.key".into()),
            model_path: Some("/models/qwen3.gguf".into()),
            model_id: Some("qwen3-1.7b".to_owned()),
            non_loopback_consent: false,
        });
        let job = resolve(&on).expect("resolves");
        assert_eq!(
            job.ai,
            Some(crate::cli::AiArgs {
                endpoint: Some("http://127.0.0.1:8123".to_owned()),
                api_key_file: Some("/run/oc/llm.key".into()),
                model_path: Some("/models/qwen3.gguf".into()),
                model: Some("qwen3-1.7b".to_owned()),
                ..Default::default()
            })
        );
    }

    /// `non_loopback_consent: true` is consent to the endpoint's own host and no other — the host
    /// the app's consent dialog named (D10) — as `--llm-allow-host <that host>` would be.
    #[test]
    fn consent_in_a_spec_is_consent_to_its_endpoints_own_host() {
        let mut remote = spec();
        remote.ai = Some(oc_core::jobspec::AiSpec {
            enabled: true,
            endpoint: Some("https://LLM.example.org/v1".to_owned()),
            non_loopback_consent: true,
            ..Default::default()
        });
        let job = resolve(&remote).expect("resolves");
        let ai = job.ai.expect("AI is on");
        assert_eq!(ai.allow_host.as_deref(), Some("llm.example.org"));

        let mut local = spec();
        local.ai = Some(oc_core::jobspec::AiSpec {
            enabled: true,
            endpoint: Some("http://localhost:11434".to_owned()),
            ..Default::default()
        });
        let job = resolve(&local).expect("resolves");
        assert_eq!(
            job.ai.expect("AI is on").allow_host,
            None,
            "loopback needs no consent, and none is invented"
        );
    }

    /// `ai.enabled = false` is the v1 default, whatever else the object carries: the app keeps its
    /// provider settings in every spec, and only the switch decides.
    #[test]
    fn ai_off_or_absent_resolves_to_no_model() {
        assert_eq!(resolve(&spec()).expect("resolves").ai, None);
        let mut off = spec();
        off.ai = Some(oc_core::jobspec::AiSpec {
            enabled: false,
            endpoint: Some("http://127.0.0.1:8123".to_owned()),
            ..Default::default()
        });
        assert_eq!(resolve(&off).expect("resolves").ai, None);
    }

    #[test]
    fn fields_the_engine_cannot_honour_are_refused_by_name() {
        let mut thresholds = spec();
        thresholds.threshold_overrides = Some([("x.y".to_owned(), 1.0)].into_iter().collect());
        assert!(resolve(&thresholds)
            .expect_err("refused")
            .contains("threshold_overrides"));
    }
}
