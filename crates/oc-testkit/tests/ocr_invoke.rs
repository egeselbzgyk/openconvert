//! Rows 13.10 and 13.19, and the invocation half of 13.18: what `tesseract` is asked, how long it
//! is given, and that it never outlives the engine.
//!
//! Unix only — the fake engine is a shell script (see `oc_testkit::fake_tesseract`).
#![cfg(unix)]

use std::collections::BTreeSet;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use image::GrayImage;
use oc_core::ocr::discover::{DiscoverySource, TesseractInfo, Version};
use oc_core::ocr::invoke::{OcrEngine, OcrError, OcrRequest, Tesseract};
use oc_core::ocr::lang::LangSpec;
use oc_core::ocr::OcrScope;
use oc_core::sidecar::tesseract::RunError;
use oc_model::geom::Rect;
use oc_testkit::fake_tesseract::{tsv_line, FakeConfig, FakeTesseract};

/// A13.5 / row 13.19: "no orphan process", within the same 2 s A9.4 gives the model server.
const TEARDOWN: Duration = Duration::from_secs(2);

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("oc-ocr-invoke-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

fn info(fake: &FakeTesseract) -> TesseractInfo {
    TesseractInfo {
        path: fake.path.clone(),
        version: Version {
            major: 5,
            minor: 3,
            patch: 4,
        },
        langs: BTreeSet::from(["deu".to_owned(), "eng".to_owned()]),
        source: DiscoverySource::ConfigPath,
    }
}

fn request(scope: OcrScope, langs: &str) -> OcrRequest {
    OcrRequest {
        raster: GrayImage::from_pixel(40, 20, image::Luma([255])),
        dpi: 300,
        psm: scope.psm(),
        langs: LangSpec::parse(langs).expect("a spec"),
        region_pt: Rect {
            x0: 72.0,
            y0: 144.0,
            x1: 81.6,
            y1: 148.8,
        },
        page_index: 3,
    }
}

/// Whether a process with this pid still exists. A zombie counts as gone: it holds nothing.
fn alive(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            Ok(stat) => !stat
                .rsplit_once(") ")
                .is_some_and(|(_, rest)| rest.starts_with('Z')),
            Err(_) => false,
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }
}

fn gone_within(pid: u32, deadline: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < deadline {
        if !alive(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    !alive(pid)
}

fn wait_for_pid(fake: &FakeTesseract) -> u32 {
    let start = Instant::now();
    loop {
        if let Some(pid) = fake.last_pid() {
            return pid;
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "the fake never started"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Row 13.10, by argv spy. A full page is read with `--psm 1` (automatic segmentation with OSD), a
/// `Mixed` page's image region with `--psm 6` (one uniform block); `--oem` is never passed, and the
/// whole vector is the fixed one detail 2 names — no shell, no extra flag.
#[test]
fn psm_follows_page_class() {
    let dir = scratch("argv");
    let fake = FakeTesseract::install(
        &dir.join("bin"),
        &FakeConfig {
            tsv: tsv_line(&[("Chapter", 91.0), ("One", 88.0)]),
            ..FakeConfig::default()
        },
    );
    let engine = Tesseract::new(info(&fake), &dir.join("work"), Duration::from_secs(30));

    let full = engine
        .recognize(&request(OcrScope::FullPage, "deu"))
        .expect("full page");
    let region = engine
        .recognize(&request(OcrScope::ImageRegion, "deu+eng"))
        .expect("region");
    assert_eq!(full.len(), 2);
    assert_eq!(region.len(), 2);

    let calls = fake.ocr_calls();
    assert_eq!(calls.len(), 2, "{calls:?}");
    for (argv, psm, langs) in [(&calls[0], "1", "deu"), (&calls[1], "6", "deu+eng")] {
        assert!(argv[0].ends_with(".png"), "{argv:?}");
        assert!(argv[0].starts_with(&dir.join("work").display().to_string()));
        assert_eq!(
            argv[1..],
            [
                "stdout",
                "-l",
                langs,
                "--psm",
                psm,
                "--dpi",
                "300",
                "-c",
                "preserve_interword_spaces=1",
                "tsv"
            ],
            "{argv:?}"
        );
        assert!(
            !argv.iter().any(|arg| arg == "--oem"),
            "--oem is never passed: {argv:?}"
        );
    }

    // The raster is removed after each call.
    let left: Vec<_> = std::fs::read_dir(dir.join("work"))
        .expect("the work directory")
        .collect();
    assert!(left.is_empty(), "rasters left behind: {left:?}");

    // Words come back in page space: 300 dpi, region origin (72, 144).
    let first = &full[0];
    assert_eq!(first.text, "Chapter");
    assert!((first.bbox.x0 - (72.0 + 100.0 * 72.0 / 300.0)).abs() < 1e-3);
    assert!((first.bbox.y0 - (144.0 + 100.0 * 72.0 / 300.0)).abs() < 1e-3);
    assert!((first.conf - 0.91).abs() < 1e-6);
}

/// The invocation half of row 13.18: a call still running at its deadline is killed, reported as a
/// deadline, and leaves neither a process nor a raster behind.
#[test]
fn a_hung_call_is_killed_at_its_deadline() {
    let dir = scratch("hung");
    let fake = FakeTesseract::install(
        &dir.join("bin"),
        &FakeConfig {
            sleep_secs: 120,
            ..FakeConfig::default()
        },
    );
    let deadline = Duration::from_millis(700);
    let engine = Tesseract::new(info(&fake), &dir.join("work"), deadline);

    let started = Instant::now();
    let result = engine.recognize(&request(OcrScope::FullPage, "eng"));
    let took = started.elapsed();

    assert_eq!(
        result,
        Err(OcrError::Run(RunError::Deadline(deadline))),
        "a hang is a deadline, not an empty page"
    );
    assert!(
        took >= deadline && took < deadline + TEARDOWN,
        "killed at the deadline, not after the hang: {took:?}"
    );
    let pid = fake.last_pid().expect("the fake started");
    assert!(
        gone_within(pid, TEARDOWN),
        "the hung tesseract is not left running"
    );
    assert!(std::fs::read_dir(dir.join("work"))
        .expect("work")
        .next()
        .is_none());

    // A crash is a failed call too, and says how it ended.
    let crashing = FakeTesseract::install(
        &dir.join("crash"),
        &FakeConfig {
            exit_code: 3,
            ..FakeConfig::default()
        },
    );
    let engine = Tesseract::new(info(&crashing), &dir.join("work"), deadline);
    assert!(matches!(
        engine.recognize(&request(OcrScope::FullPage, "eng")),
        Err(OcrError::Exit { .. })
    ));
    // And output that is not Tesseract's TSV is the schema error, not a silent empty page.
    let garbled = FakeTesseract::install(
        &dir.join("garbled"),
        &FakeConfig {
            tsv: "this is not a tsv\n".to_owned(),
            ..FakeConfig::default()
        },
    );
    let engine = Tesseract::new(info(&garbled), &dir.join("work"), deadline);
    assert!(matches!(
        engine.recognize(&request(OcrScope::FullPage, "eng")),
        Err(OcrError::Tsv(_))
    ));
}

fn run_engine(mode: &str, fake: &FakeTesseract, work: &std::path::Path) -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_oc-ocr-engine"))
        .args([
            mode,
            &fake.path.display().to_string(),
            &work.display().to_string(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the engine starts")
}

/// Row 13.19: a `tesseract` the engine started does not outlive it. A SIGTERM (a cancel) reaches it
/// through the signal handler, a panic through the panic hook. An engine killed outright (SIGKILL, a
/// segfault) needs `PR_SET_PDEATHSIG` or a Windows job object, which are Phase 14's, as for the
/// model server (`docs/DECISIONS_LOG.md`, 2026-09-23).
#[test]
fn ocr_child_dies_with_the_engine() {
    // Cancelled: SIGTERM to the engine, exit code 3, and the child gone.
    let dir = scratch("teardown-term");
    let fake = FakeTesseract::install(
        &dir.join("bin"),
        &FakeConfig {
            sleep_secs: 120,
            ..FakeConfig::default()
        },
    );
    let mut engine = run_engine("wait", &fake, &dir.join("work"));
    let pid = wait_for_pid(&fake);
    assert!(alive(pid), "tesseract is running before the engine ends");
    let sent = Command::new("kill")
        .args(["-TERM", &engine.id().to_string()])
        .status()
        .expect("kill runs");
    assert!(sent.success());
    let status = engine.wait().expect("the engine exits");
    assert_eq!(status.code(), Some(3), "a signal is a cancel: {status:?}");
    assert!(
        gone_within(pid, TEARDOWN),
        "tesseract outlived a cancelled engine"
    );

    // Panicked: the hook runs before unwinding, so the child goes even though no destructor runs.
    let dir = scratch("teardown-panic");
    let fake = FakeTesseract::install(
        &dir.join("bin"),
        &FakeConfig {
            sleep_secs: 120,
            ..FakeConfig::default()
        },
    );
    let mut engine = run_engine("panic", &fake, &dir.join("work"));
    let pid = wait_for_pid(&fake);
    assert!(alive(pid));
    engine
        .stdin
        .take()
        .expect("stdin")
        .write_all(b"go\n")
        .expect("go");
    let status = engine.wait().expect("the engine exits");
    assert_eq!(status.code(), Some(101), "a panic is exit code 101 (D13.2)");
    assert!(
        gone_within(pid, TEARDOWN),
        "tesseract outlived a panicked engine"
    );
}

/// Carried over from Phase 13 (row 13.19) into Phase 14: a `tesseract` the engine started does not
/// outlive an engine killed with `SIGKILL`, which runs neither the signal handler nor the panic
/// hook. Linux ends it through `PR_SET_PDEATHSIG` (`oc_core::sidecar::orphan`).
#[cfg(target_os = "linux")]
#[test]
fn ocr_child_does_not_outlive_a_sigkilled_engine() {
    let dir = scratch("teardown-kill");
    let fake = FakeTesseract::install(
        &dir.join("bin"),
        &FakeConfig {
            sleep_secs: 120,
            ..FakeConfig::default()
        },
    );
    let mut engine = run_engine("wait", &fake, &dir.join("work"));
    let pid = wait_for_pid(&fake);
    assert!(alive(pid), "tesseract is running before the engine ends");
    engine.kill().expect("SIGKILL to the engine alone");
    let _ = engine.wait();
    assert!(
        gone_within(pid, TEARDOWN),
        "tesseract {pid} outlived an engine killed with SIGKILL"
    );
}
