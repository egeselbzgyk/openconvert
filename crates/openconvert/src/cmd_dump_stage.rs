//! `openconvert dump-stage <stage> <input>` (IMPLEMENTATION_PLAN §2.1, Phase 1 detail 9).
//!
//! The debugging surface: everything a stage produced, as canonical JSON, in a form that
//! diffs. When a book converts wrongly the first question is what was actually extracted, and
//! this is the only answer that is not a guess.
//!
//! **Written a page at a time.** RT B4 puts a real book's extraction layer at tens of
//! megabytes, so nothing is assembled whole: the header goes out, then each page as it is
//! extracted, then the footer. That keeps peak memory at one page regardless of the book, and
//! it means a dump of a nine-hundred-page book starts appearing immediately instead of after
//! a minute of silence.

use std::io::Write;

use oc_core::events::EventSink;
use oc_core::exit::ExitCode;
use oc_pdf::backend::PdfBackend;
use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;

use crate::cli::DumpStageArgs;

const E_PDFIUM: &str = "E_PDFIUM_ABI";
const E_INPUT: &str = "E_INPUT";
const E_PDF: &str = "E_PDF";
const E_LIMIT: &str = "E_LIMIT_EXCEEDED";
const E_PASSWORD: &str = "E_PASSWORD_REQUIRED";

/// A stage name that is not one of the twelve, or one whose dump is not implemented yet.
const E_STAGE: &str = "E_UNKNOWN_STAGE";

pub fn run<W: Write>(
    args: &DumpStageArgs,
    events: &mut EventSink<W>,
    stdout: &mut dyn Write,
) -> ExitCode {
    if args.stage != oc_pdf::dump::STAGE {
        events.fatal(
            E_STAGE,
            &format!(
                "`{}` cannot be dumped yet; only `{}` is implemented",
                args.stage,
                oc_pdf::dump::STAGE
            ),
        );
        return ExitCode::Usage;
    }

    let backend = match PdfiumBackend::bind() {
        Ok(backend) => backend,
        Err(error) => {
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

    let bytes = match std::fs::read(&args.input) {
        Ok(bytes) => bytes,
        Err(error) => {
            events.fatal(
                E_INPUT,
                &format!("cannot read {}: {error}", args.input.display()),
            );
            return ExitCode::Usage;
        }
    };

    let document = match backend.open_with_limits(&bytes, args.password.as_deref(), &args.limits) {
        Ok(document) => document,
        Err(oc_pdf::error::PdfError::LimitExceeded(exceeded)) => {
            events.fatal(E_LIMIT, &exceeded.to_string());
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

    match write_dump(document.as_ref(), stdout) {
        Ok(()) => {
            events.done("ok");
            ExitCode::Ok
        }
        Err(message) => {
            events.fatal(E_PDF, &message);
            ExitCode::Failed
        }
    }
}

/// Stream the dump: header, then one page per line, then the footer.
///
/// NDJSON-shaped rather than one JSON document, and deliberately: a nine-hundred-page book's
/// dump has to be readable with `head`, greppable by page, and streamable by the writer. A
/// single top-level array would force the writer to hold everything or to hand-roll the commas,
/// and the reader to parse the lot before seeing page one.
fn write_dump(
    document: &dyn oc_pdf::inspect::PdfDoc,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    write_line(stdout, &oc_pdf::dump::header(document))?;
    for index in 0..document.page_count() {
        let page = oc_pdf::dump::page(document, index).map_err(|error| error.to_string())?;
        write_line(stdout, &page)?;
    }
    Ok(())
}

fn write_line<T: serde::Serialize>(stdout: &mut dyn Write, value: &T) -> Result<(), String> {
    let rendered = oc_model::canonical::to_canonical_json(value).map_err(|e| e.to_string())?;
    writeln!(stdout, "{rendered}").map_err(|error| error.to_string())
}
