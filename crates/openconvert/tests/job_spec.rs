//! The desktop app's form of the command line: one argument, a job spec (D13.2, RT B15).
//!
//! These run the built binary, because what is under test is what a supervisor sees: the exit
//! code, the NDJSON on stderr, and the files on disk.

use std::path::{Path, PathBuf};
use std::process::Command;

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

/// A fresh scratch directory per test.
fn scratch(test: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("openconvert-jobspec-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the scratch directory is made");
    directory
}

/// Run the engine with exactly one argument and parse every stderr line as an event.
fn run_spec(spec: &Path) -> (Option<i32>, Vec<serde_json::Value>) {
    let output = Command::new(binary())
        .arg(spec)
        .output()
        .expect("the binary runs");
    let stderr = String::from_utf8(output.stderr).expect("stderr is UTF-8");
    let events = stderr
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("stderr carries only events; {line:?}: {e}"))
        })
        .collect();
    (output.status.code(), events)
}

fn write_spec(directory: &Path, spec: &serde_json::Value) -> PathBuf {
    let path = directory.join("job.json");
    std::fs::write(&path, serde_json::to_vec_pretty(spec).expect("serialises"))
        .expect("the spec is written");
    path
}

#[test]
fn a_job_spec_path_is_the_only_argument_the_engine_needs() {
    let directory = scratch("ok");
    let output = directory.join("book.epub");
    let spec = write_spec(
        &directory,
        &serde_json::json!({
            "schema": "openconvert.job/1",
            "job_id": "job-1",
            "input": {"path": fixture("f01_prose_single_column")},
            "output": {"path": output},
            "preset": "novel",
            "locale": "de"
        }),
    );

    let (code, events) = run_spec(&spec);
    assert_eq!(code, Some(0), "events: {events:#?}");

    assert_eq!(events[0]["t"], "hello", "hello is always the first line");
    let job = events
        .iter()
        .find(|event| event["t"] == "job")
        .expect("a job event");
    assert_eq!(job["job_id"], "job-1");
    assert_eq!(job["phase"], "started");
    assert!(job["pages"].as_u64().is_some_and(|pages| pages > 0));

    let done = events.last().expect("events");
    assert_eq!(done["t"], "done");
    assert_eq!(done["status"], "ok");
    let report = PathBuf::from(done["report_path"].as_str().expect("done names the report"));
    assert!(
        report.is_file(),
        "the report is where done says it is: {report:?}"
    );
    assert_eq!(done["output_path"], output.to_string_lossy().as_ref());
    assert!(output.is_file());
}

#[test]
fn an_invalid_job_spec_is_exit_2_with_e_jobspec() {
    let directory = scratch("invalid");
    let output = directory.join("book.epub");
    // `--model-path` smuggled in as a field is exactly what the schema's closed top level is for.
    let spec = write_spec(
        &directory,
        &serde_json::json!({
            "schema": "openconvert.job/1",
            "input": {"path": fixture("f01_prose_single_column")},
            "output": {"path": output},
            "model_path": "/tmp/evil.gguf"
        }),
    );

    let (code, events) = run_spec(&spec);
    assert_eq!(code, Some(2));
    assert_eq!(
        events[0]["t"], "hello",
        "the supervisor can still check versions"
    );
    let fatal = events.last().expect("events");
    assert_eq!(fatal["t"], "fatal");
    assert_eq!(fatal["code"], "E_JOBSPEC");
    assert!(
        !output.exists(),
        "nothing was attempted for an invalid spec"
    );
}

#[test]
fn a_job_spec_never_replaces_an_existing_output_unless_it_says_so() {
    let directory = scratch("exists");
    let output = directory.join("book.epub");
    std::fs::write(&output, b"somebody's earlier book").expect("written");
    let spec = write_spec(
        &directory,
        &serde_json::json!({
            "schema": "openconvert.job/1",
            "input": {"path": fixture("f01_prose_single_column")},
            "output": {"path": output}
        }),
    );

    let (code, events) = run_spec(&spec);
    assert_eq!(code, Some(2));
    assert_eq!(events.last().expect("events")["code"], "E_OUTPUT_EXISTS");
    assert_eq!(
        std::fs::read(&output).expect("still there"),
        b"somebody's earlier book"
    );
}

#[test]
fn a_job_spec_whose_input_changed_is_refused() {
    let directory = scratch("changed");
    let spec = write_spec(
        &directory,
        &serde_json::json!({
            "schema": "openconvert.job/1",
            "input": {"path": fixture("f01_prose_single_column"), "sha256": "0".repeat(64)},
            "output": {"path": directory.join("book.epub")}
        }),
    );

    let (code, events) = run_spec(&spec);
    assert_eq!(code, Some(2));
    assert_eq!(events.last().expect("events")["code"], "E_INPUT_CHANGED");
}

#[test]
fn a_mistyped_subcommand_is_still_a_usage_error() {
    let output = Command::new(binary())
        .arg("convrt")
        .output()
        .expect("the binary runs");
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown subcommand"));
}
