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
use oc_pdf::pdfium::PdfiumBackend;

use crate::cli::ConvertArgs;
use openconvert::convert::{convert_bytes, ConvertOptions};

const E_PDFIUM: &str = "E_PDFIUM_ABI";
const E_INPUT: &str = "E_INPUT";
const E_PDF: &str = "E_PDF";
const E_OUTPUT: &str = "E_OUTPUT";
const E_CONVERT: &str = "E_CONVERT";

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

    for warning in &conversion.built.warnings {
        events.warning(warning, "warn", serde_json::json!({}));
    }

    if let Err(error) = write_atomically(&output, conversion.built.bytes.as_slice()) {
        events.fatal(E_OUTPUT, &format!("{}: {error}", output.display()));
        eprintln!("error: {}: {error}", output.display());
        return ExitCode::Failed;
    }

    events.done_with("ok", Some(&output.to_string_lossy()));
    ExitCode::Ok
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
