//! `openconvert inspect` (IMPLEMENTATION_PLAN §2.1).

use std::io::Write;

use oc_core::events::EventSink;
use oc_core::exit::ExitCode;
use oc_pdf::backend::PdfBackend;
use oc_pdf::inspect::{inspect, InspectOptions};
use oc_pdf::pdfium::PdfiumBackend;

use crate::cli::InspectArgs;

/// Warning and error codes are stable identifiers, localised by the GUI (D13.2).
const E_PDFIUM: &str = "E_PDFIUM_ABI";
const E_INPUT: &str = "E_INPUT";
const E_PDF: &str = "E_PDF";

/// A resource limit refused the document (D13.2, R8 §A2).
///
/// Exit 2 rather than 1, and its own code, because it is a *configuration* outcome: the
/// file is outside the budget this run was given, and the operator's next move is to raise
/// the limit or to reject the file. That is the same decision whether the refusal came at
/// the door or three hundred pages in, which is what makes one code right for both.
const E_LIMIT: &str = "E_LIMIT_EXCEEDED";

/// The document is encrypted with a user password we were not given (D13.11, §2.4).
///
/// Exit 2, like a limit refusal and for the same reason: nothing was attempted, and the
/// next move belongs to whoever launched us — here, to prompt for the password. A UI that
/// had to parse stderr to know that could not be written.
const E_PASSWORD: &str = "E_PASSWORD_REQUIRED";

/// Run the subcommand, returning the process exit code.
///
/// Nothing here returns `Result` to `main`: an error has to reach the caller as an event on
/// stderr *and* an exit code, and doing that in one place keeps the two from disagreeing.
pub fn run<W: Write>(
    args: &InspectArgs,
    events: &mut EventSink<W>,
    stdout: &mut dyn Write,
) -> ExitCode {
    let backend = match PdfiumBackend::bind() {
        Ok(backend) => backend,
        Err(error) => {
            // No `hello` yet: the version it must carry is exactly what could not be read.
            events.fatal(E_PDFIUM, &error.to_string());
            return ExitCode::Usage;
        }
    };

    let version = backend.version();
    events.hello(
        env!("CARGO_PKG_VERSION"),
        oc_model::IR_VERSION,
        &version.version,
    );

    if !args.input.is_file() {
        events.fatal(
            E_INPUT,
            &format!("{} is not a readable file", args.input.display()),
        );
        return ExitCode::Usage;
    }

    let mut limits = oc_core::limits::Limits::default();
    if let Some(max_pages) = args.max_pages {
        limits.max_pages = max_pages;
    }
    let options = InspectOptions {
        limits,
        pages: args.pages.clone(),
        password: args.password.clone(),
    };
    let report = match inspect(&backend, &args.input, &options) {
        Ok(report) => report,
        Err(
            error @ (oc_pdf::error::PdfError::LimitExceeded(_) | oc_pdf::error::PdfError::Cap(_)),
        ) => {
            events.fatal(E_LIMIT, &error.to_string());
            return ExitCode::Usage;
        }
        Err(error @ oc_pdf::error::PdfError::PasswordRequired) => {
            events.fatal(E_PASSWORD, &error.to_string());
            return ExitCode::Usage;
        }
        Err(error) => {
            events.fatal(E_PDF, &error.to_string());
            return ExitCode::Failed;
        }
    };

    // stdout is the data channel and carries the report and nothing else (RT C2). The
    // report is pretty-printed because a human reads it far more often than a machine, and
    // `serde_json`'s pretty printer is deterministic, which is what test 0.17 checks.
    let rendered = match serde_json::to_string_pretty(&report) {
        Ok(rendered) => rendered,
        Err(error) => {
            events.fatal(E_PDF, &error.to_string());
            return ExitCode::Failed;
        }
    };
    if writeln!(stdout, "{rendered}").is_err() {
        events.fatal(E_PDF, "cannot write the report to stdout");
        return ExitCode::Failed;
    }

    events.done("ok");
    ExitCode::Ok
}
