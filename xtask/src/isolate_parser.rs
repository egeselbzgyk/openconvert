//! `cargo xtask isolate-parser-spike` — the time-boxed `--isolate-parser` spike (PHASE 14 detail 13,
//! D16, SECURITY §5).
//!
//! The question: if each page range were parsed by a child process — so a PDFium crash on one range
//! costs that range, not the book — what would it cost, and would the output change? The shape is
//! the plan's: one child per page range, the glyph batches sent back over a pipe as length-prefixed
//! CBOR, the parent reassembling them. Measured on the fast corpus (`target/fixtures`), against the
//! same extraction in process and against the whole conversion.
//!
//! **The child is this binary** (`xtask __parse-range <pdf> <first> <last>`), not a hidden
//! subcommand of the shipped engine: the per-child cost being measured — process start, PDFium
//! bind, document open, serialisation — is the same either way, and a no-go spike leaves nothing
//! half-built in the engine (detail 13: "it does not linger"). The verdict and the numbers are in
//! `docs/DECISIONS_LOG.md`.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use oc_model::extract::{FontInfo, Glyph};
use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;

/// The hidden child subcommand.
pub const CHILD: &str = "__parse-range";
/// Pages per child: the plan's "page range". Eight keeps a 300-page book at ~38 children.
const RANGE_PAGES: u32 = 8;
/// Repetitions per fixture; the median is reported.
const REPEATS: usize = 5;
/// The go threshold (detail 13): overhead under 15 %.
const GO_OVERHEAD: f64 = 0.15;
/// A frame's length prefix: four bytes, big-endian.
const PREFIX: usize = 4;

/// One page's extraction, as it crosses the pipe.
type Batch = (u32, Vec<Glyph>, Vec<FontInfo>);

/// The child: extract pages `first..=last` of `pdf` and write one frame per page to stdout.
pub fn child(pdf: &Path, first: u32, last: u32) -> Result<()> {
    let backend = PdfiumBackend::bind().context("PDFium is vendored")?;
    let bytes = std::fs::read(pdf).with_context(|| format!("cannot read {}", pdf.display()))?;
    let document = backend.open(&bytes, None).context("the PDF opens")?;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for page in first..=last.min(document.page_count().saturating_sub(1)) {
        let glyphs = document.page_glyphs(page)?;
        let batch: Batch = (page, glyphs.glyphs, glyphs.fonts);
        let mut frame = Vec::new();
        ciborium::into_writer(&batch, &mut frame).context("CBOR")?;
        let length = u32::try_from(frame.len()).context("a frame over 4 GiB")?;
        out.write_all(&length.to_be_bytes())?;
        out.write_all(&frame)?;
    }
    out.flush()?;
    Ok(())
}

/// What one fixture measured.
struct Measured {
    name: String,
    pages: u32,
    children: u32,
    in_process: Duration,
    isolated: Duration,
    conversion: Duration,
    identical: bool,
}

/// Run the spike over `fixtures` (all of `target/fixtures` when empty) and print the table.
/// Fails when any fixture's output differs; the verdict is printed, not enforced.
pub fn run(root: &Path, only: Option<&str>, repeats: Option<usize>) -> Result<()> {
    let repeats = repeats.unwrap_or(REPEATS).max(1);
    // A path instead of a stem measures that file alone (a long book, say).
    let explicit = only.map(PathBuf::from).filter(|path| path.is_file());
    let mut fixtures: Vec<PathBuf> = match explicit {
        Some(path) => vec![path],
        None => std::fs::read_dir(root.join("target/fixtures"))
            .context("run `cargo run -p xtask -- fixtures` first")?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.extension().is_some_and(|ext| ext == "pdf"))
            .filter(|path| {
                only.is_none_or(|only| path.file_stem().is_some_and(|stem| stem == only))
            })
            .collect(),
    };
    fixtures.sort();
    if fixtures.is_empty() {
        bail!("no fixtures to measure");
    }
    let me = std::env::current_exe().context("the xtask binary")?;
    let backend = PdfiumBackend::bind().context("PDFium is vendored")?;

    let mut rows = Vec::new();
    for path in &fixtures {
        let bytes = std::fs::read(path)?;
        let name = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();

        let mut in_process = Vec::new();
        let mut isolated = Vec::new();
        let mut conversion = Vec::new();
        let mut identical = true;
        let mut pages = 0;
        let mut children = 0;
        for _ in 0..repeats {
            let started = Instant::now();
            let document = backend.open(&bytes, None)?;
            pages = document.page_count();
            let mut local: Vec<Batch> = Vec::new();
            for page in 0..pages {
                let glyphs = document.page_glyphs(page)?;
                local.push((page, glyphs.glyphs, glyphs.fonts));
            }
            in_process.push(started.elapsed());

            let started = Instant::now();
            let mut remote: Vec<Batch> = Vec::new();
            children = 0;
            let mut first = 0;
            while first < pages {
                let last = (first + RANGE_PAGES - 1).min(pages - 1);
                remote.extend(parse_range(&me, path, first, last)?);
                children += 1;
                first = last + 1;
            }
            isolated.push(started.elapsed());
            identical &= remote == local;

            let started = Instant::now();
            convert(&backend, &bytes, &name)?;
            conversion.push(started.elapsed());
        }
        rows.push(Measured {
            name,
            pages,
            children,
            in_process: median(&mut in_process),
            isolated: median(&mut isolated),
            conversion: median(&mut conversion),
            identical,
        });
    }

    println!("fixture                              pages children  in-proc   isolated  convert   extra/convert");
    let (mut extra, mut total) = (Duration::ZERO, Duration::ZERO);
    for row in &rows {
        let added = row.isolated.saturating_sub(row.in_process);
        extra += added;
        total += row.conversion;
        println!(
            "{:<36} {:>5} {:>8} {:>8.1}ms {:>8.1}ms {:>8.1}ms {:>7.1}% {}",
            row.name,
            row.pages,
            row.children,
            ms(row.in_process),
            ms(row.isolated),
            ms(row.conversion),
            100.0 * ms(added) / ms(row.conversion).max(f64::EPSILON),
            if row.identical {
                "identical"
            } else {
                "DIFFERS"
            }
        );
    }
    let overhead = ms(extra) / ms(total).max(f64::EPSILON);
    let identical = rows.iter().all(|row| row.identical);
    println!(
        "overall: +{:.1} ms over {:.1} ms of conversion = {:.1}% overhead; output {}; verdict: {}",
        ms(extra),
        ms(total),
        100.0 * overhead,
        if identical { "identical" } else { "DIFFERS" },
        if identical && overhead < GO_OVERHEAD {
            "GO"
        } else {
            "NO-GO"
        }
    );
    if !identical {
        bail!("the isolated extraction differs from the in-process one");
    }
    Ok(())
}

/// Start one child for `first..=last` and read its frames back.
fn parse_range(me: &Path, pdf: &Path, first: u32, last: u32) -> Result<Vec<Batch>> {
    let mut child = Command::new(me)
        .arg(CHILD)
        .arg(pdf)
        .arg(first.to_string())
        .arg(last.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("the child starts")?;
    let mut stdout = child.stdout.take().context("the child's stdout")?;
    let mut batches = Vec::new();
    let mut prefix = [0_u8; PREFIX];
    loop {
        match stdout.read_exact(&mut prefix) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(error) => return Err(error.into()),
        }
        let length = usize::try_from(u32::from_be_bytes(prefix))?;
        let mut frame = vec![0_u8; length];
        stdout.read_exact(&mut frame)?;
        batches.push(ciborium::from_reader(frame.as_slice()).context("a CBOR frame")?);
    }
    let status = child.wait()?;
    if !status.success() {
        bail!(
            "the child for pages {first}-{last} of {} failed: {status}",
            pdf.display()
        );
    }
    Ok(batches)
}

/// The whole conversion in process, the denominator the overhead is judged against.
fn convert(backend: &PdfiumBackend, bytes: &[u8], name: &str) -> Result<()> {
    let t = &oc_core::thresholds::T;
    let pdf = backend.open(bytes, None)?;
    openconvert::convert::convert(
        pdf.as_ref(),
        &openconvert::convert::sha256_hex(bytes),
        &openconvert::convert::ConvertOptions {
            filename: format!("{name}.pdf"),
            language: None,
            preset: oc_model::document::PresetName::Auto,
            epub: oc_epub::EpubOptions {
                split_bytes: usize::try_from(t.xhtml.split_bytes).unwrap_or(usize::MAX),
                max_longest_side_px: u32::try_from(t.images.max_longest_side_px)
                    .unwrap_or(u32::MAX),
                jpeg_quality: u8::try_from(t.images.jpeg_quality).unwrap_or(u8::MAX),
                warn_total_bytes: u64::MAX,
                modified: "2026-01-01T00:00:00Z".to_owned(),
            },
            overrides: None,
            cache_dir: None,
            ocr: openconvert::ocr::OcrOptions::off(),
        },
        t,
    )
    .map(|_| ())
    .map_err(|error| anyhow::anyhow!("{name}: {error}"))
}

fn median(samples: &mut [Duration]) -> Duration {
    samples.sort();
    samples.get(samples.len() / 2).copied().unwrap_or_default()
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}
