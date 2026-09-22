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

use crate::cmd_convert::{hello, run_job, ConvertJob, E_PDFIUM};

/// Every refusal of the spec itself, whatever the reason (§2.2).
pub const E_JOBSPEC: &str = "E_JOBSPEC";

/// Run one job spec, returning the process exit code.
pub fn run<W: Write>(path: &Path, events: &mut EventSink<W>) -> ExitCode {
    let backend = match PdfiumBackend::bind() {
        Ok(backend) => backend,
        Err(error) => {
            events.fatal(E_PDFIUM, &error.to_string());
            return ExitCode::Usage;
        }
    };
    hello(&backend, events);

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
/// **Refused, not ignored.** A spec field the engine does not act on yet — AI, threshold
/// overrides, stage dumps — is a refusal naming the field. Ignoring it would convert a book the
/// user asked to be converted differently and report success, which is the one outcome worse than
/// an error.
pub fn resolve(spec: &JobSpec) -> Result<ConvertJob, String> {
    if spec.ai.as_ref().is_some_and(|ai| ai.enabled) {
        return Err(unsupported("ai.enabled"));
    }
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
    if spec.overrides_path.is_some() {
        return Err(unsupported("overrides_path"));
    }

    let password = match &spec.input.password_file {
        Some(file) => Some(read_password(file)?),
        None => None,
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

    #[test]
    fn fields_the_engine_cannot_honour_are_refused_by_name() {
        let mut ai = spec();
        ai.ai = Some(oc_core::jobspec::AiSpec {
            enabled: true,
            ..Default::default()
        });
        assert!(resolve(&ai).expect_err("refused").contains("ai.enabled"));

        let mut thresholds = spec();
        thresholds.threshold_overrides = Some([("x.y".to_owned(), 1.0)].into_iter().collect());
        assert!(resolve(&thresholds)
            .expect_err("refused")
            .contains("threshold_overrides"));
    }
}
