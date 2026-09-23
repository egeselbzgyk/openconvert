//! PHASE 14: the engine's hardening, observed from outside the process — the audit log a
//! conversion leaves alone, the caps a hostile file meets, the sandbox a conversion runs in.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_openconvert"))
}

fn fixture(name: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/fixtures")
        .join(format!("{name}.pdf"));
    assert!(
        path.is_file(),
        "missing fixture {}; run `cargo run -p xtask -- fixtures`",
        path.display()
    );
    path
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("oc-hardening-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

/// Run the engine with its data directory (and so its audit log) inside `home`.
fn engine(home: &Path, args: &[&str]) -> Output {
    Command::new(binary())
        .args(args)
        .env("HOME", home)
        .env("XDG_DATA_HOME", home.join("data"))
        .env("LOCALAPPDATA", home.join("data"))
        .output()
        .expect("the engine runs")
}

/// Row 14.21, the conversion half: a conversion opens no connection, so the audit log is exactly as
/// it was — not appended to, not created, not rotated. The download half is `oc-net`'s
/// `net_audit_log_records_downloads_and_nothing_else`.
#[test]
fn a_conversion_appends_nothing_to_the_network_audit_log() {
    let home = scratch("audit");
    let log = home.join("data/openconvert/network-audit.log");
    std::fs::create_dir_all(log.parent().expect("a parent")).expect("data dir");
    let before = b"{\"ts\":\"2026-09-23T00:00:00Z\",\"host\":\"huggingface.co\",\"purpose\":\"download\",\"bytes\":1,\"outcome\":\"ok\",\"loopback\":false}\n";
    std::fs::write(&log, before).expect("a log with one line");

    let output = home.join("book.epub");
    let converted = engine(
        &home,
        &[
            "convert",
            &fixture("f01_prose_single_column").display().to_string(),
            "-o",
            &output.display().to_string(),
            "--ocr",
            "never",
        ],
    );
    assert_eq!(
        converted.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&converted.stderr)
    );
    assert!(output.is_file());
    assert_eq!(
        std::fs::read(&log).expect("the log"),
        before,
        "a conversion wrote to the network audit log"
    );
    let _ = std::fs::remove_dir_all(&home);
}

// ---------------------------------------------------------------------------
// The caps, the memory limit and the sandbox, through the engine (PHASE 14 rows 14.6 (exit half),
// 14.7, 14.9, 14.10 (the conversion that must still succeed), 14.11 and 14.19; A14.1–A14.5).
// ---------------------------------------------------------------------------

use oc_testkit::hostile;

/// Convert `pdf` in a scratch directory; `extra` goes after the input.
fn convert_bytes(name: &str, pdf: &[u8], extra: &[&str]) -> (Output, PathBuf, std::time::Duration) {
    let dir = scratch(name);
    let input = dir.join("in.pdf");
    std::fs::write(&input, pdf).expect("input");
    let output = dir.join("out.epub");
    let mut args = vec![
        "convert".to_owned(),
        input.display().to_string(),
        "-o".to_owned(),
        output.display().to_string(),
        "--ocr".to_owned(),
        "never".to_owned(),
    ];
    args.extend(extra.iter().map(|arg| (*arg).to_owned()));
    let started = std::time::Instant::now();
    let run = Command::new(binary())
        .args(&args)
        .env("HOME", &dir)
        .env("XDG_DATA_HOME", dir.join("data"))
        .output()
        .expect("the engine runs");
    (run, dir, started.elapsed())
}

fn report_of(dir: &Path) -> serde_json::Value {
    let text = std::fs::read_to_string(dir.join("out.epub.report.json"))
        .unwrap_or_else(|error| panic!("no report in {}: {error}", dir.display()));
    serde_json::from_str(&text).expect("the report is JSON")
}

/// Exit 1, a failure report naming `cap`, and nothing at the output path or beside it.
fn assert_refused(run: &Output, dir: &Path, cap: &str) -> serde_json::Value {
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert_eq!(run.status.code(), Some(1), "{stderr}");
    let report = report_of(dir);
    assert_eq!(report["status"], "failed", "{report}");
    assert_eq!(report["failure"]["code"], "E_LIMIT_EXCEEDED", "{report}");
    assert_eq!(report["failure"]["cap"], cap, "{report}");
    assert!(
        !dir.join("out.epub").exists(),
        "a refused book left an output"
    );
    let leftovers: Vec<String> = std::fs::read_dir(dir)
        .expect("dir")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".oc-tmp-"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "temporaries left behind: {leftovers:?}"
    );
    report
}

/// The peak resident set of a running process, polled from `/proc` until it exits.
#[cfg(target_os = "linux")]
fn run_measuring_peak(mut command: Command) -> (Output, u64) {
    let child = command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("the engine starts");
    let status_path = format!("/proc/{}/status", child.id());
    let peak = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let watcher = {
        let (peak, done) = (peak.clone(), done.clone());
        std::thread::spawn(move || {
            while !done.load(std::sync::atomic::Ordering::SeqCst) {
                if let Ok(status) = std::fs::read_to_string(&status_path) {
                    let kib = status
                        .lines()
                        .find_map(|line| line.strip_prefix("VmHWM:"))
                        .and_then(|value| {
                            value
                                .trim()
                                .trim_end_matches("kB")
                                .trim()
                                .parse::<u64>()
                                .ok()
                        })
                        .unwrap_or(0);
                    peak.fetch_max(kib * 1024, std::sync::atomic::Ordering::SeqCst);
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        })
    };
    let output = child.wait_with_output().expect("the engine exits");
    done.store(true, std::sync::atomic::Ordering::SeqCst);
    let _ = watcher.join();
    (output, peak.load(std::sync::atomic::Ordering::SeqCst))
}

/// Row 14.7. `RLIMIT_AS` is what `--max-memory` said, read back from the kernel at the moment the
/// PDF was opened — not the flag echoed — and the job spec's `limits.max_memory_bytes` reaches it
/// the same way. Without the flag it is the shipped 4 GiB. (Unix; the Windows job-object limit is
/// not implemented, see DECISIONS_LOG.)
#[cfg(unix)]
#[test]
fn memory_cap_is_applied_before_the_pdf_opens() {
    const GIB: u64 = 1 << 30;
    let pdf = std::fs::read(fixture("f01_prose_single_column")).expect("fixture");

    let (run, dir, _) = convert_bytes("memory-flag", &pdf, &["--max-memory", "3GiB"]);
    assert_eq!(
        run.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let memory = report_of(&dir)["sandbox"]["memory"].clone();
    assert_eq!(memory["status"], "applied", "{memory}");
    assert_eq!(memory["requested_bytes"], 3 * GIB);
    assert_eq!(memory["in_force_at_open"], 3 * GIB, "{memory}");
    assert_eq!(memory["flag"], "--max-memory");

    let (run, dir, _) = convert_bytes("memory-default", &pdf, &[]);
    assert_eq!(run.status.code(), Some(0));
    assert_eq!(
        report_of(&dir)["sandbox"]["memory"]["in_force_at_open"],
        oc_core::limits::Limits::default().max_memory_bytes
    );

    // The job spec: the one argument the desktop app passes.
    let dir = scratch("memory-spec");
    let input = dir.join("in.pdf");
    std::fs::write(&input, &pdf).expect("input");
    let spec = dir.join("job.json");
    std::fs::write(
        &spec,
        serde_json::json!({
            "schema": "openconvert.job/1",
            "input": {"path": input},
            "output": {"path": dir.join("out.epub")},
            "limits": {"max_memory_bytes": 2 * GIB},
        })
        .to_string(),
    )
    .expect("spec");
    let run = Command::new(binary())
        .arg(&spec)
        .env("HOME", &dir)
        .env("XDG_DATA_HOME", dir.join("data"))
        .output()
        .expect("the engine runs");
    assert_eq!(
        run.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        report_of(&dir)["sandbox"]["memory"]["in_force_at_open"],
        2 * GIB
    );

    // And an unusable value is a usage error, before anything runs.
    let run = Command::new(binary())
        .args(["convert", "x.pdf", "--max-memory", "4GB"])
        .output()
        .expect("the engine runs");
    assert_eq!(run.status.code(), Some(2));
}

/// Row 14.9, A14.3 (SECURITY §4): three pages declaring forty million glyphs between them. Exit 1,
/// a report naming the cap, well inside the stage deadline, and the engine's resident memory
/// bounded — measured from outside, and under a 1 GiB address-space cap that an over-allocation
/// would have turned into a crash rather than an exit 1. Unguarded, the same file took PDFium past
/// 13 GB.
#[cfg(target_os = "linux")]
#[test]
fn degenerate_40m_glyph_pdf_fails_cleanly() {
    const GIB: u64 = 1 << 30;
    let pdf = hostile::glyph_flood(3, 40_000_000 / 3 + 1);
    let dir = scratch("glyphs");
    let input = dir.join("in.pdf");
    std::fs::write(&input, &pdf).expect("input");
    let mut command = Command::new(binary());
    command
        .args(["convert", &input.display().to_string(), "-o"])
        .arg(dir.join("out.epub"))
        .args(["--ocr", "never", "--max-memory", "1GiB"])
        .env("HOME", &dir)
        .env("XDG_DATA_HOME", dir.join("data"));
    let started = std::time::Instant::now();
    let (run, peak) = run_measuring_peak(command);
    let elapsed = started.elapsed();

    let report = assert_refused(&run, &dir, "max_page_glyphs");
    assert_ne!(run.status.code(), Some(101), "never a panic");
    let deadline =
        std::time::Duration::from_secs(oc_core::limits::Limits::default().stage_deadline_secs);
    assert!(
        elapsed < deadline / 10,
        "{elapsed:?} against a {deadline:?} stage deadline"
    );
    assert!(
        peak > 0 && peak < GIB,
        "peak RSS {peak} bytes against a 1 GiB cap"
    );
    assert!(
        report["failure"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("max_page_glyphs")),
        "{report}"
    );
}

/// Row 14.6's exit half: 5 000 declared pages is exit 1 and a report naming `max_pages`.
#[test]
fn a_declared_5000_page_pdf_exits_1_with_a_report() {
    let (run, dir, _) = convert_bytes("pages", &hostile::declared_pages(5000), &[]);
    let report = assert_refused(&run, &dir, "max_pages");
    assert!(
        report["failure"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("5000")),
        "{report}"
    );
    // `--max-pages` moves the door: the same one-page claim under a larger cap opens (and the one
    // real page converts).
    let (run, _, _) = convert_bytes(
        "pages-raised",
        &hostile::declared_pages(5000),
        &["--max-pages", "6000"],
    );
    assert_ne!(run.status.code(), Some(101));
}

/// A14.1: a 1.6-gigapixel image is refused from its dictionary.
#[test]
fn a_gigapixel_claim_is_refused_from_the_dictionary() {
    let (run, dir, _) = convert_bytes("pixels", &hostile::pixel_claim(40_000, 40_000), &[]);
    let report = assert_refused(&run, &dir, "max_image_pixels");
    assert!(
        report["failure"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("1600000000")),
        "{report}"
    );
}

/// A14.2: a page whose content expands to 300 MiB fails closed at the 256 MiB ceiling, with the
/// engine's memory bounded by the ceiling rather than the bomb.
#[cfg(target_os = "linux")]
#[test]
fn a_decompression_bomb_fails_closed_through_convert() {
    const MIB: u64 = 1 << 20;
    let pdf = hostile::decompression_bomb(&hostile::zlib_zeros(300 * MIB));
    let dir = scratch("bomb");
    let input = dir.join("in.pdf");
    std::fs::write(&input, &pdf).expect("input");
    let mut command = Command::new(binary());
    command
        .args(["convert", &input.display().to_string(), "-o"])
        .arg(dir.join("out.epub"))
        .args(["--ocr", "never"])
        .env("HOME", &dir)
        .env("XDG_DATA_HOME", dir.join("data"));
    let (run, peak) = run_measuring_peak(command);
    assert_refused(&run, &dir, "max_decompressed_stream_bytes");
    assert!(
        peak < 700 * MIB,
        "peak RSS {peak} bytes for a 256 MiB ceiling"
    );
}

/// Row 14.10's other half (the plan's failure mode: "a scope set that forgets the temp directory
/// turns every conversion into a permission error on modern kernels only"): a normal conversion —
/// with OCR, whose `tesseract` runs inside the sandbox — succeeds, and its report says Landlock
/// was applied wherever the kernel has it.
#[cfg(target_os = "linux")]
#[test]
fn a_normal_conversion_succeeds_inside_landlock() {
    let dir = scratch("inside");
    let output = dir.join("book.epub");
    let run = engine(
        &dir,
        &[
            "convert",
            &fixture("f01_prose_single_column").display().to_string(),
            "-o",
            &output.display().to_string(),
        ],
    );
    assert_eq!(
        run.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(output.is_file());
    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("book.epub.report.json")).expect("report"),
    )
    .expect("JSON");
    let landlock = &report["sandbox"]["landlock"];
    let release = std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
    let mut numbers = release
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|n| n.parse::<u32>().ok());
    if (numbers.next().unwrap_or(0), numbers.next().unwrap_or(0)) >= (5, 13) {
        assert_eq!(landlock["status"], "applied", "{landlock}");
    } else {
        assert_eq!(landlock["status"], "unsupported", "{landlock}");
    }
}

/// Row 14.11, A14.5: when Landlock cannot be applied the conversion succeeds all the same, and the
/// report records the skip and why. Exercised on a kernel that has Landlock by switching it off
/// (`OC_LANDLOCK=off`); a kernel without it takes the same branch with the kernel's reason.
#[test]
fn landlock_skips_gracefully_when_unsupported() {
    let dir = scratch("skip");
    let output = dir.join("book.epub");
    let run = Command::new(binary())
        .args([
            "convert",
            &fixture("f01_prose_single_column").display().to_string(),
            "-o",
            &output.display().to_string(),
            "--ocr",
            "never",
        ])
        .env(openconvert::sandbox::LANDLOCK_VAR, "off")
        .env("HOME", &dir)
        .env("XDG_DATA_HOME", dir.join("data"))
        .output()
        .expect("the engine runs");
    assert_eq!(
        run.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        output.is_file(),
        "a missing sandbox never fails a conversion"
    );
    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("book.epub.report.json")).expect("report"),
    )
    .expect("JSON");
    let landlock = &report["sandbox"]["landlock"];
    assert_eq!(landlock["status"], "unsupported", "{landlock}");
    assert!(
        landlock["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("OC_LANDLOCK")),
        "{landlock}"
    );
}

/// Row 14.19: across 1 000 injected cap violations the destination never holds a file. Every
/// violation is a real one, drawn from a seeded generator — a declared page count, an image
/// dictionary, an xref chain too deep or looping, a glyph flood, a bomb, a `--max-pages` below the
/// book's — and each run must end in exit 1 with a report naming the cap, no output and no
/// temporary: never a partial book, never a panic, never a hang.
#[test]
fn no_partial_output_after_any_cap_violation() {
    const RUNS: usize = 1000;
    const THREADS: usize = 4;

    // Built once: the expensive shapes are reused, the cheap ones drawn per run.
    let floods = [
        hostile::glyph_flood(1, 1_000_001),
        hostile::glyph_flood(2, 1_100_000),
        hostile::glyph_flood(3, 1_250_000),
    ];
    let bomb = hostile::decompression_bomb(&hostile::zlib_zeros(270 << 20));
    let f01 = std::fs::read(fixture("f01_prose_single_column")).expect("fixture");

    let next = std::sync::atomic::AtomicUsize::new(0);
    let failures = std::sync::Mutex::new(Vec::<String>::new());
    std::thread::scope(|scope| {
        for _ in 0..THREADS {
            scope.spawn(|| loop {
                let run = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if run >= RUNS {
                    break;
                }
                // A splitmix-style draw, seeded by the run number: the same thousand every time.
                let mut state =
                    (run as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03;
                let mut draw = |bound: u64| {
                    state ^= state >> 31;
                    state = state.wrapping_mul(0xBF58_476D_1CE4_E5B9);
                    state ^= state >> 29;
                    state % bound.max(1)
                };
                let (pdf, extra, cap): (Vec<u8>, Vec<&str>, &str) = match run % 20 {
                    0..=5 => (
                        hostile::declared_pages(3001 + draw(10_000_000)),
                        vec![],
                        "max_pages",
                    ),
                    6..=11 => {
                        let width = 10_001 + draw(90_000);
                        let height = 100_000_000 / width + 1 + draw(50_000);
                        (
                            hostile::pixel_claim(width, height),
                            vec![],
                            "max_image_pixels",
                        )
                    }
                    12..=14 => (
                        hostile::xref_chain(129 + usize::try_from(draw(200)).unwrap_or(0), false),
                        vec![],
                        "max_xref_chain",
                    ),
                    15..=17 => (
                        hostile::xref_chain(1 + usize::try_from(draw(60)).unwrap_or(0), true),
                        vec![],
                        "max_xref_chain",
                    ),
                    18 => (
                        floods[usize::try_from(draw(3)).unwrap_or(0)].clone(),
                        vec![],
                        "max_page_glyphs",
                    ),
                    _ if run % 100 == 19 => (bomb.clone(), vec![], "max_decompressed_stream_bytes"),
                    _ => (f01.clone(), vec!["--max-pages", "1"], "max_pages"),
                };
                let (output, dir, elapsed) = convert_bytes(&format!("prop-{run}"), &pdf, &extra);
                let verdict = std::panic::catch_unwind(|| {
                    assert_refused(&output, &dir, cap);
                    assert!(elapsed < std::time::Duration::from_secs(60), "{elapsed:?}");
                });
                if verdict.is_err() {
                    failures
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .push(format!(
                            "run {run} ({cap}): exit {:?}, {}",
                            output.status.code(),
                            String::from_utf8_lossy(&output.stderr)
                        ));
                } else {
                    let _ = std::fs::remove_dir_all(&dir);
                }
            });
        }
    });
    let failures = failures
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert!(
        failures.is_empty(),
        "{} of {RUNS} runs failed: {failures:#?}",
        failures.len()
    );
}

/// Row 14.8 through the real driver: a stage deadline armed by the stage runner and noticed by the
/// driver's own cancel polls ends the conversion as `Cancelled` with `AbortCause::Deadline` — the
/// cancel's path, not a second one — and names the stage that ran out. Clock-injected: a zero
/// deadline has already passed at the first poll inside the first stage.
#[test]
fn a_stage_deadline_aborts_through_the_cancel_path() {
    use oc_core::cancel::{AbortCause, Cancel};
    use oc_core::deadline::ManualClock;
    use oc_pdf::inspect::PdfOpen;
    use openconvert::convert::{convert_observed, ConvertError, ConvertOptions, Observe};

    let bytes = std::fs::read(fixture("f01_prose_single_column")).expect("fixture");
    let backend = oc_pdf::pdfium::PdfiumBackend::bind().expect("PDFium is vendored");
    let pdf = backend.open(&bytes, None).expect("opens");
    let options = ConvertOptions {
        filename: "f01.pdf".to_owned(),
        language: None,
        preset: oc_model::document::PresetName::Auto,
        epub: oc_epub::EpubOptions {
            split_bytes: usize::MAX,
            max_longest_side_px: u32::MAX,
            jpeg_quality: 1,
            warn_total_bytes: u64::MAX,
            modified: "2026-01-01T00:00:00Z".to_owned(),
        },
        ocr: openconvert::ocr::OcrOptions::off(),
        overrides: None,
        cache_dir: None,
    };

    let clock = std::sync::Arc::new(ManualClock::new(0));
    let cancel = Cancel::with_clock(clock);
    cancel.set_stage_deadline(std::time::Duration::ZERO);
    let silent = oc_core::progress::Silent;
    let result = convert_observed(
        pdf.as_ref(),
        "0",
        &options,
        None,
        &oc_core::thresholds::T,
        Observe {
            progress: &silent,
            cancel: &cancel,
        },
    );
    assert!(
        matches!(result, Err(ConvertError::Cancelled)),
        "a deadline stops the driver"
    );
    match cancel.cause() {
        Some(AbortCause::Deadline(stage)) => assert!(!stage.is_empty()),
        other => panic!("the cause must be the deadline, got {other:?}"),
    }

    // Without a deadline the same flag lets the conversion through.
    let open = Cancel::new();
    let result = convert_observed(
        pdf.as_ref(),
        "0",
        &options,
        None,
        &oc_core::thresholds::T,
        Observe {
            progress: &silent,
            cancel: &open,
        },
    );
    assert!(result.is_ok());
}
