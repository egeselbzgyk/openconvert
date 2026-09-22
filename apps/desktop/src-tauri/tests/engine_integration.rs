//! The supervisor against the real engine (Phase 12 rows 12.6 and 12.17).
//!
//! Gated behind `engine-integration` (IMPLEMENTATION_PLAN §0.2): it needs the engine built beside
//! this test (`cargo build -p openconvert`) and the vendored PDFium. The `desktop` CI job builds
//! both and turns the feature on.
#![cfg(feature = "engine-integration")]

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use oc_core::jobspec::JobSpec;
use oc_core::thresholds::T;
use openconvert_desktop::engine::{handshake, Engine, ProcessLauncher, UiError};
use openconvert_desktop::fs_scope::AppDirs;

/// `target/<profile>/openconvert`, beside the directory this test binary is in.
fn engine_binary() -> PathBuf {
    let exe = std::env::current_exe().expect("the test knows where it is");
    let profile_dir = exe
        .parent()
        .and_then(|deps| deps.parent())
        .expect("target/<profile>/deps/<test>");
    let engine = profile_dir.join(format!("openconvert{}", std::env::consts::EXE_SUFFIX));
    assert!(
        engine.is_file(),
        "no engine at {}; run `cargo build -p openconvert` first",
        engine.display()
    );
    engine
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("oc-desktop-it-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("made");
    dir
}

fn seconds(value: i64) -> Duration {
    Duration::from_secs(u64::try_from(value).expect("a positive threshold"))
}

/// 12.6 / A12.2 — Cancel reaches `done{cancelled}` within `ipc.cancel_deadline_secs`, the engine
/// exits 3, and the destination holds no output, no report and no `.oc-tmp-*`.
#[test]
fn cancel_reaches_done_cancelled_and_cleans_temp() {
    let root = scratch("cancel");
    let dirs = AppDirs::under(&root.join("app")).expect("made");
    let books = root.join("books");
    std::fs::create_dir_all(&books).expect("made");

    let pages = usize::try_from(T.perf.bench_reference_pages).expect("positive");
    let input = books.join("long.pdf");
    std::fs::write(&input, oc_testkit::handmade::reference_book(pages)).expect("written");
    let spec = JobSpec::new(input.clone(), books.join("long.epub"));

    let engine = Engine::new(
        dirs.jobs.clone(),
        ProcessLauncher::new(engine_binary(), dirs.jobs.clone()),
    );
    let (lines, received) = mpsc::channel::<String>();
    let mut running = engine
        .start(
            "job-cancel",
            &spec,
            Box::new(move |line| {
                let _ = lines.send(line);
            }),
        )
        .expect("the engine starts");

    let event = |line: String| -> serde_json::Value {
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("{line:?}: {e}"))
    };

    // Cancel once pages are demonstrably being read.
    loop {
        let next = event(
            received
                .recv_timeout(Duration::from_secs(60))
                .expect("progress arrives"),
        );
        assert_ne!(
            next["t"], "done",
            "it finished before it could be cancelled"
        );
        if next["t"] == "progress" {
            break;
        }
    }
    running.cancel().expect("the cancel is written");
    let asked = Instant::now();

    let deadline = seconds(T.ipc.cancel_deadline_secs);
    let done = loop {
        let next = event(
            received
                .recv_timeout(seconds(T.ipc.kill_after_secs))
                .expect("the run ends"),
        );
        if next["t"] == "done" || next["t"] == "fatal" {
            break next;
        }
    };
    let took = asked.elapsed();
    assert_eq!(done["status"], "cancelled", "{done}");
    assert!(took <= deadline, "done{{cancelled}} after {took:?}");

    let exit = loop {
        if let Some(code) = running.try_wait().expect("waits") {
            break code;
        }
        assert!(
            asked.elapsed() <= seconds(T.ipc.kill_after_secs),
            "still running at the kill deadline"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(exit, Some(3), "cancelled is exit 3");

    let mut left: Vec<String> = std::fs::read_dir(&books)
        .expect("reads")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(left, ["long.pdf"], "the destination is clean");
}

/// 12.17 — an engine whose `hello.engine_version` is not the app's blocks startup with a message
/// that names both versions and the fix. The real engine, asked by an app of another version.
#[test]
fn stale_sidecar_is_refused_at_startup() {
    let engine = engine_binary();

    let refused = handshake(&engine, "0.0.0-stale").expect_err("a stale pairing is refused");
    assert_eq!(
        refused,
        UiError::StaleEngine {
            engine: env!("CARGO_PKG_VERSION").to_owned(),
            app: "0.0.0-stale".to_owned(),
        }
    );
    let message = refused.to_string();
    assert!(message.contains(env!("CARGO_PKG_VERSION")) && message.contains("0.0.0-stale"));
    assert!(
        message.contains("stage-sidecars"),
        "the message names the fix"
    );

    // The matched pair starts.
    let hello = handshake(&engine, env!("CARGO_PKG_VERSION")).expect("the matched pair starts");
    assert!(!hello.pdfium_version.is_empty());
}
