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

use oc_core::cancel::{Cancel, Outcome};
use oc_core::events::EventSink;
use oc_core::exit::ExitCode;
use oc_core::progress::Progress;
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
    let cancel = Cancel::new();
    crate::control::listen(cancel.clone());

    let known = [
        oc_pdf::dump::STAGE,
        openconvert::dump_text::STAGE,
        openconvert::dump_layout::STAGE,
        openconvert::dump_structure::STAGE,
    ];
    if !known.contains(&args.stage.as_str()) {
        events.fatal(
            E_STAGE,
            &format!(
                "`{}` cannot be dumped yet; {} are implemented",
                args.stage,
                known.join(", ")
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

    // A stage boundary: cancellation asked for while the document was opening is honoured
    // before any page is read, not after all of them (§2.3).
    if cancel.is_cancelled() {
        events.done(Outcome::Cancelled.status());
        return ExitCode::Cancelled;
    }

    let written = if args.stage == openconvert::dump_text::STAGE {
        write_text_dump(document.as_ref(), stdout, &cancel)
    } else if args.stage == openconvert::dump_layout::STAGE {
        write_layout_dump(document.as_ref(), stdout, &cancel)
    } else if args.stage == openconvert::dump_structure::STAGE {
        write_structure_dump(document.as_ref(), &args.input, stdout, &cancel)
    } else {
        write_dump(
            document.as_ref(),
            stdout,
            &cancel,
            &oc_core::progress::Silent,
        )
    };
    match written {
        Ok(Outcome::Completed) => {
            events.done(Outcome::Completed.status());
            ExitCode::Ok
        }
        Ok(Outcome::Cancelled) => {
            events.done(Outcome::Cancelled.status());
            ExitCode::Cancelled
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
    cancel: &Cancel,
    progress: &dyn Progress,
) -> Result<Outcome, String> {
    let total = document.page_count();
    write_line(stdout, &oc_pdf::dump::header(document))?;

    for index in 0..total {
        // Checked *before* each page rather than after, so a cancellation that arrives while
        // page 400 is being extracted stops the loop at 401 instead of running to the end.
        // Between pages is the finest granularity available: extracting one page is a single
        // call into PDFium and cannot be interrupted part-way.
        if cancel.is_cancelled() {
            return Ok(Outcome::Cancelled);
        }
        let page = oc_pdf::dump::page(document, index).map_err(|error| error.to_string())?;
        write_line(stdout, &page)?;
        progress.advance(oc_pdf::dump::STAGE, index, total);
    }
    Ok(Outcome::Completed)
}

/// Stream the `text` stage's dump.
///
/// Unlike `ingest` this cannot start writing at page one: `dc:language` is detected over the
/// whole body and `C_0` is not final until every page has been through `N`, so the stage runs
/// to completion and the pages are written from the result. The memory that costs is bounded
/// and far below the ingest dump's — a book's runs are a fraction of its glyphs, one string
/// and one box per run where there was one of each per character.
fn write_text_dump(
    document: &dyn oc_pdf::inspect::PdfDoc,
    stdout: &mut dyn Write,
    cancel: &Cancel,
) -> Result<Outcome, String> {
    let mut input = Vec::new();
    for index in 0..document.page_count() {
        if cancel.is_cancelled() {
            return Ok(Outcome::Cancelled);
        }
        let glyphs = document
            .page_glyphs(index)
            .map_err(|error| error.to_string())?;
        let geometry = document
            .page_geometry(index)
            .map_err(|error| error.to_string())?;
        input.push(openconvert::pipeline::PageInput {
            page: oc_model::extract::PageRef::new(index),
            width_pt: geometry.width_pt(),
            height_pt: geometry.height_pt(),
            class: glyphs.class,
            glyphs: glyphs.glyphs,
            fonts: glyphs.fonts,
            images: openconvert::input::number_images(
                document.page_images(index).unwrap_or_default(),
            ),
            ocr_runs: Vec::new(),
        });
    }
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }

    let (header, pages) =
        openconvert::dump_text::dump(&input, oc_model::lang::LangTag::EN, &oc_core::thresholds::T)
            .map_err(|error| error.to_string())?;

    write_line(stdout, &header)?;
    for page in &pages {
        write_line(stdout, page)?;
    }
    Ok(Outcome::Completed)
}

/// Stream the `layout` stage's dump.
///
/// Like `text` it runs to completion first, and for a stronger reason: the column hypothesis
/// is checked *across* pages and may be withdrawn document-wide, so page one's answer is not
/// final until page five has been read (PIPELINE §6 step 4).
fn write_layout_dump(
    document: &dyn oc_pdf::inspect::PdfDoc,
    stdout: &mut dyn Write,
    cancel: &Cancel,
) -> Result<Outcome, String> {
    let Some(input) = read_pages(document, cancel)? else {
        return Ok(Outcome::Cancelled);
    };
    let (header, pages) = openconvert::dump_layout::dump(
        &input,
        oc_model::lang::LangTag::EN,
        &oc_core::thresholds::T,
    )
    .map_err(|error| error.to_string())?;

    write_line(stdout, &header)?;
    for page in &pages {
        write_line(stdout, page)?;
    }
    Ok(Outcome::Completed)
}

/// Stream the `structure` stage's dump.
///
/// The header first, because everything in it is about the book rather than about a page:
/// the digest, the metadata, the note match rate and whether escalation is allowed at all.
/// Then one line per top-level section.
fn write_structure_dump(
    document: &dyn oc_pdf::inspect::PdfDoc,
    input: &std::path::Path,
    stdout: &mut dyn Write,
    cancel: &Cancel,
) -> Result<Outcome, String> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let bytes = std::fs::read(input).map_err(|error| error.to_string())?;
    // The identifier is minted from the source bytes, so the dump has to hash them (R5 §A2).
    let sha = {
        use sha2::Digest as _;
        format!("{:x}", sha2::Sha256::digest(&bytes))
    };
    let filename = input
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default();

    let (header, sections) = openconvert::dump_structure::dump(
        document,
        filename,
        &sha,
        oc_model::lang::LangTag::EN,
        &oc_core::thresholds::T,
    )?;
    write_line(stdout, &header)?;
    for section in &sections {
        write_line(stdout, section)?;
    }
    Ok(Outcome::Completed)
}

/// Read every page into the shape the post-`ingest` stages take, or `None` if cancelled.
fn read_pages(
    document: &dyn oc_pdf::inspect::PdfDoc,
    cancel: &Cancel,
) -> Result<Option<Vec<openconvert::pipeline::PageInput>>, String> {
    let mut input = Vec::new();
    for index in 0..document.page_count() {
        if cancel.is_cancelled() {
            return Ok(None);
        }
        let glyphs = document
            .page_glyphs(index)
            .map_err(|error| error.to_string())?;
        let geometry = document
            .page_geometry(index)
            .map_err(|error| error.to_string())?;
        input.push(openconvert::pipeline::PageInput {
            page: oc_model::extract::PageRef::new(index),
            width_pt: geometry.width_pt(),
            height_pt: geometry.height_pt(),
            class: glyphs.class,
            glyphs: glyphs.glyphs,
            fonts: glyphs.fonts,
            images: openconvert::input::number_images(
                document.page_images(index).unwrap_or_default(),
            ),
            ocr_runs: Vec::new(),
        });
    }
    if cancel.is_cancelled() {
        return Ok(None);
    }
    Ok(Some(input))
}

fn write_line<T: serde::Serialize>(stdout: &mut dyn Write, value: &T) -> Result<(), String> {
    let rendered = oc_model::canonical::to_canonical_json(value).map_err(|e| e.to_string())?;
    writeln!(stdout, "{rendered}").map_err(|error| error.to_string())
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Row 1.19 of the Phase 1 table.
// ---------------------------------------------------------------------------

/// A progress reporter that asks for cancellation after a given page.
///
/// This is what makes test 1.19 deterministic instead of a race. Cancelling from another
/// thread after a sleep would work only if the loop were still running when the sleep ended,
/// and 200 empty pages take milliseconds — the test would pass or fail depending on the
/// machine. Cancelling *from inside the loop's own progress callback* pins the moment exactly,
/// and proves the property that matters: the flag is read between pages, not once before them.
#[cfg(test)]
struct CancelAfter {
    after: u32,
    cancel: Cancel,
    seen: std::sync::atomic::AtomicU32,
}

#[cfg(test)]
impl Progress for CancelAfter {
    fn advance(&self, _stage: &str, index: u32, _total: u32) {
        self.seen.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if index == self.after {
            self.cancel.cancel();
        }
    }
}

/// Test 1.19.
///
/// A 200-page ingest, cancelled part-way, stops there and reports `cancelled` well within the
/// two seconds D13.2 allows. Two assertions carry it: the loop stopped *early* — which a
/// check placed after the loop, or only before it, would fail — and the whole thing finished
/// inside the budget.
#[test]
fn cancel_is_observed_inside_page_loop() {
    use oc_pdf::inspect::PdfOpen;
    use oc_pdf::pdfium::PdfiumBackend;

    /// D13.2's budget for reaching `done{cancelled}`.
    const BUDGET: std::time::Duration = std::time::Duration::from_secs(2);
    const PAGES: usize = 200;
    const CANCEL_AFTER: u32 = 3;

    let bytes = oc_testkit::handmade::many_pages(PAGES);
    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(&bytes, None).expect("the fixture opens");
    assert_eq!(document.page_count(), PAGES as u32);

    let cancel = Cancel::new();
    let progress = CancelAfter {
        after: CANCEL_AFTER,
        cancel: cancel.clone(),
        seen: std::sync::atomic::AtomicU32::new(0),
    };

    let mut sink = Vec::new();
    let started = std::time::Instant::now();
    let outcome = write_dump(document.as_ref(), &mut sink, &cancel, &progress)
        .expect("the pages themselves extract fine");
    let elapsed = started.elapsed();

    assert_eq!(outcome, Outcome::Cancelled);
    assert!(elapsed < BUDGET, "took {elapsed:?}, budget is {BUDGET:?}");

    // The loop stopped where it was told to, not at the end. One page more than the cancel
    // point: the flag is read at the top of the next iteration.
    let seen = progress.seen.load(std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        seen,
        CANCEL_AFTER + 1,
        "the loop ran on past the cancellation"
    );
    assert!(
        (seen as usize) < PAGES,
        "the loop ran to the end, so the check is not inside it"
    );
}

/// The same loop, uncancelled, runs every page. Without this the test above would pass against
/// a loop that always stopped after four pages.
#[test]
fn an_uncancelled_loop_writes_every_page() {
    use oc_pdf::inspect::PdfOpen;
    use oc_pdf::pdfium::PdfiumBackend;

    const PAGES: usize = 20;

    let bytes = oc_testkit::handmade::many_pages(PAGES);
    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(&bytes, None).expect("the fixture opens");

    let mut sink = Vec::new();
    let outcome = write_dump(
        document.as_ref(),
        &mut sink,
        &Cancel::new(),
        &oc_core::progress::Silent,
    )
    .expect("the pages extract");

    assert_eq!(outcome, Outcome::Completed);
    let text = String::from_utf8(sink).expect("the dump is UTF-8");
    // One header plus one line per page.
    assert_eq!(text.lines().count(), PAGES + 1);
}
