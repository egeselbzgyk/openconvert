//! `openconvert validate` (IMPLEMENTATION_PLAN §2.1).
//!
//! Tier 1 always. Tier 2 — EPUBCheck — only when a jar is named, because it needs a JVM and is
//! never on the default conversion path (D6). A run that asked for tier 2 and did not get it
//! says so rather than reporting a clean tier 1 as if it were the whole answer.

use std::io::Write;
use std::path::PathBuf;

use oc_core::events::EventSink;
use oc_core::exit::ExitCode;
use oc_epub::EpubBytes;
use oc_validate::{validate_tier1, Expectations};

use crate::cli::ValidateArgs;

const E_INPUT: &str = "E_INPUT";
const E_EPUBCHECK: &str = "E_EPUBCHECK";

/// Run the subcommand.
pub fn run<W: Write>(
    args: &ValidateArgs,
    events: &mut EventSink<W>,
    stdout: &mut dyn Write,
) -> ExitCode {
    let bytes = match std::fs::read(&args.input) {
        Ok(bytes) => bytes,
        Err(error) => {
            events.fatal(E_INPUT, &format!("{}: {error}", args.input.display()));
            return ExitCode::Usage;
        }
    };
    let epub = EpubBytes(bytes);

    let report = validate_tier1(&epub, &Expectations::default());
    let mut errors = report.error_count();
    let mut tier2: Option<oc_validate::epubcheck::EpubCheckReport> = None;

    if args.tier >= 2 {
        match run_epubcheck(&args.epubcheck_jar, &args.input) {
            Ok(result) => {
                errors += result.errors.len();
                tier2 = Some(result);
            }
            Err(error) => {
                events.fatal(E_EPUBCHECK, &error);
                eprintln!("error: {error}");
                return ExitCode::Failed;
            }
        }
    }

    if args.json {
        let payload = serde_json::json!({
            "schema": "openconvert.validate/1",
            "input": args.input.to_string_lossy(),
            "tier1": report,
            "tier2": tier2,
        });
        let _ = writeln!(
            stdout,
            "{}",
            serde_json::to_string(&payload).unwrap_or_default()
        );
    } else {
        for finding in &report.findings {
            let _ = writeln!(
                stdout,
                "tier1 {:?} {} {}: {}",
                finding.severity, finding.id, finding.location, finding.detail
            );
        }
        if let Some(tier2) = &tier2 {
            for message in &tier2.errors {
                let _ = writeln!(stdout, "tier2 ERROR {} {}", message.id, message.message);
            }
        }
        let _ = writeln!(stdout, "{errors} error(s)");
    }

    events.done_with(if errors == 0 { "ok" } else { "invalid" }, None);
    if errors == 0 {
        ExitCode::Ok
    } else {
        ExitCode::Failed
    }
}

/// Tier 2, when a jar was named.
fn run_epubcheck(
    jar: &Option<PathBuf>,
    input: &std::path::Path,
) -> Result<oc_validate::epubcheck::EpubCheckReport, String> {
    let Some(jar) = jar else {
        return Err(
            "tier 2 needs --epubcheck-jar; run `cargo xtask fetch-epubcheck` to get one".to_owned(),
        );
    };
    oc_validate::epubcheck::run(jar, input).map_err(|error| error.to_string())
}
