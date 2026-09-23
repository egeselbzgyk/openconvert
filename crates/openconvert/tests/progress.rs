//! Real progress and a real cancel, as a supervisor sees them (D13.2, RT C2, Phase 12 detail 3–4).
//!
//! The desktop app renders only what the engine reports. These tests hold the engine to reporting
//! it: the stage names it actually runs, page counts that reach their total, and a cancel that ends
//! the run with `done{cancelled}`, exit 3 and an untouched destination — inside the deadline.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use oc_core::thresholds::T;

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
    // A job spec names files exactly: absolute, with no `..` (PHASE 14 row 14.14).
    std::fs::canonicalize(&path).unwrap_or(path)
}

fn scratch(test: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "openconvert-progress-{test}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the scratch directory is made");
    directory
}

fn write_spec(directory: &Path, input: &Path, output: &Path) -> PathBuf {
    let path = directory.join("job.json");
    let spec = serde_json::json!({
        "schema": "openconvert.job/1",
        "job_id": "progress",
        "input": {"path": input},
        "output": {"path": output}
    });
    std::fs::write(&path, serde_json::to_vec(&spec).expect("serialises")).expect("written");
    path
}

fn events_of(stderr: &[u8]) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(stderr)
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).unwrap_or_else(|e| panic!("{line:?}: {e}")))
        .collect()
}

#[test]
fn a_conversion_reports_the_stages_it_runs_by_their_real_names() {
    let directory = scratch("stages");
    let output = directory.join("book.epub");
    let spec = write_spec(&directory, &fixture("f09_novel_structure"), &output);

    let run = Command::new(binary()).arg(&spec).output().expect("runs");
    assert_eq!(run.status.code(), Some(0));
    let events = events_of(&run.stderr);

    // The order in which stages first begin is the pipeline's order.
    let mut begun: Vec<String> = Vec::new();
    for event in events
        .iter()
        .filter(|e| e["t"] == "stage" && e["phase"] == "begin")
    {
        let name = event["name"].as_str().expect("a name").to_owned();
        if !begun.contains(&name) {
            begun.push(name);
        }
    }
    assert_eq!(
        begun,
        [
            "ingest",
            "text",
            "furniture",
            "layout",
            "structure",
            "document",
            "epub",
            "validate",
            "report"
        ],
        "only the twelve names, only for stages that ran, in order"
    );

    // Every begin has an end carrying its wall-clock.
    for name in &begun {
        let begins = events
            .iter()
            .filter(|e| e["t"] == "stage" && e["name"] == name.as_str() && e["phase"] == "begin")
            .count();
        let ends: Vec<&serde_json::Value> = events
            .iter()
            .filter(|e| e["t"] == "stage" && e["name"] == name.as_str() && e["phase"] == "end")
            .collect();
        assert_eq!(begins, ends.len(), "{name}: every begin ends");
        assert!(ends.iter().all(|e| e["elapsed_ms"].is_u64()));
    }

    // Page progress never overshoots and always arrives at its total.
    let pages: Vec<&serde_json::Value> = events
        .iter()
        .filter(|e| e["t"] == "progress" && e["stage"] == "ingest")
        .collect();
    assert!(!pages.is_empty(), "ingest reports pages");
    for event in &pages {
        assert!(event["done"].as_u64() <= event["total"].as_u64());
        assert_eq!(event["unit"], "pages");
    }
    let last = pages.last().expect("at least one");
    assert_eq!(last["done"], last["total"], "the bar reaches the end");

    assert_eq!(events.last().expect("events")["t"], "done");
}

/// A12.2: a running conversion, a cancel on stdin — `done{cancelled}` within the deadline, exit 3,
/// and nothing at the destination: no output, no report, no `.oc-tmp-*`.
#[test]
fn a_cancel_on_stdin_ends_the_run_within_the_deadline_and_leaves_nothing() {
    let directory = scratch("cancel");
    // Long enough that the cancel lands mid-run on any machine: the reference book the
    // performance budget is stated for.
    let pages = usize::try_from(T.perf.bench_reference_pages).expect("positive");
    let input = directory.join("long.pdf");
    std::fs::write(&input, oc_testkit::handmade::reference_book(pages)).expect("written");
    let output = directory.join("long.epub");
    let spec = write_spec(&directory, &input, &output);

    let mut child = Command::new(binary())
        .arg(&spec)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawns");
    let stderr = child.stderr.take().expect("piped");
    let (lines, received) = mpsc::channel::<serde_json::Value>();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if let Ok(event) = serde_json::from_str(&line) {
                if lines.send(event).is_err() {
                    return;
                }
            }
        }
    });

    // Wait until the engine is demonstrably working through pages, then cancel.
    let started = Instant::now();
    loop {
        let event = received
            .recv_timeout(Duration::from_secs(60))
            .expect("the engine reports progress");
        if event["t"] == "progress" {
            break;
        }
        assert_ne!(event["t"], "done", "finished before it could be cancelled");
        assert!(started.elapsed() < Duration::from_secs(60));
    }
    let mut stdin = child.stdin.take().expect("piped");
    writeln!(stdin, r#"{{"t":"cancel"}}"#).expect("the engine reads stdin");
    stdin.flush().expect("flushed");
    let cancelled_at = Instant::now();

    let deadline =
        Duration::from_secs(u64::try_from(T.ipc.cancel_deadline_secs).expect("positive"));
    let done = loop {
        let event = received
            .recv_timeout(deadline + Duration::from_secs(5))
            .expect("the run ends");
        if event["t"] == "done" || event["t"] == "fatal" {
            break event;
        }
    };
    let took = cancelled_at.elapsed();
    let status = child.wait().expect("exits");
    drop(stdin);
    reader.join().expect("the reader ends");

    assert_eq!(done["t"], "done");
    assert_eq!(done["status"], "cancelled");
    assert!(
        took <= deadline,
        "done{{cancelled}} after {took:?}, deadline {deadline:?}"
    );
    assert_eq!(status.code(), Some(3), "cancelled is exit 3");

    let mut left: Vec<String> = std::fs::read_dir(&directory)
        .expect("reads")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(
        left,
        ["job.json", "long.pdf"],
        "no output, no report, no temporary"
    );
}

/// RT C2: a run longer than the heartbeat period beats, and keeps working after it beats.
///
/// The heartbeat is written from a thread of its own. When the process held stderr's lock on its
/// main thread for the whole run, the first heartbeat blocked on that lock while holding the event
/// sink's, and the next event the conversion wrote blocked on the sink: every run longer than
/// `ipc.heartbeat_secs` hung, silently, for ever — exactly the "hung process" a heartbeat exists
/// to expose. The short fixtures never ran long enough to beat.
#[test]
fn a_run_longer_than_the_heartbeat_period_beats_and_keeps_reporting() {
    let directory = scratch("heartbeat");
    let pages = usize::try_from(T.perf.bench_reference_pages).expect("positive");
    let input = directory.join("long.pdf");
    std::fs::write(&input, oc_testkit::handmade::reference_book(pages)).expect("written");
    let output = directory.join("long.epub");
    let spec = write_spec(&directory, &input, &output);

    let mut child = Command::new(binary())
        .arg(&spec)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawns");
    let stderr = child.stderr.take().expect("piped");
    let (lines, received) = mpsc::channel::<serde_json::Value>();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if let Ok(event) = serde_json::from_str(&line) {
                if lines.send(event).is_err() {
                    return;
                }
            }
        }
    });

    let period = Duration::from_secs(u64::try_from(T.ipc.heartbeat_secs).expect("positive"));
    let patience = period * 5;
    let mut beat = false;
    let mut after_beat = false;
    let mut finished = false;
    while !(beat && after_beat) {
        let Ok(event) = received.recv_timeout(patience) else {
            break;
        };
        match event["t"].as_str() {
            Some("heartbeat") => beat = true,
            Some("done" | "fatal") => {
                finished = true;
                break;
            }
            _ if beat => after_beat = true,
            _ => {}
        }
    }
    // Stop the rest of the book: what is under test is that the run beat and went on.
    let mut stdin = child.stdin.take().expect("piped");
    let _ = writeln!(stdin, r#"{{"t":"cancel"}}"#);
    let _ = stdin.flush();
    let ended = (0..100).any(|_| {
        std::thread::sleep(Duration::from_millis(100));
        matches!(child.try_wait(), Ok(Some(_)))
    });
    if !ended {
        let _ = child.kill();
    }
    let _ = child.wait();
    drop(stdin);
    reader.join().expect("the reader ends");

    assert!(
        !finished || beat,
        "the book finished inside one heartbeat period; it cannot show a beat"
    );
    assert!(beat, "no heartbeat within {patience:?}");
    assert!(
        after_beat,
        "nothing after the first heartbeat: the run hung"
    );
    assert!(ended, "the engine did not end after a cancel");
}
