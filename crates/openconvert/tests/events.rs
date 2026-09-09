//! The NDJSON event protocol on stderr (§2.3, RT C2).

use std::path::PathBuf;
use std::process::Command;

#[test]
fn hello_is_first_stderr_line() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/fixtures/f01_prose_single_column.pdf");
    assert!(
        fixture.is_file(),
        "missing fixture {}; run `cargo run -p xtask -- fixtures`",
        fixture.display()
    );

    let output = Command::new(env!("CARGO_BIN_EXE_openconvert"))
        .arg("inspect")
        .arg(&fixture)
        .arg("--json")
        .arg("--progress")
        .arg("json")
        .output()
        .expect("the binary runs");
    assert_eq!(output.status.code(), Some(0));

    let stderr = String::from_utf8(output.stderr).expect("stderr is UTF-8");
    let events: Vec<serde_json::Value> = stderr
        .lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|e| panic!("line {line:?}: {e}")))
        .collect();
    assert!(!events.is_empty(), "--progress json must emit events");

    // `hello` is always the first line, so a supervisor can check the protocol version
    // before interpreting anything else.
    assert_eq!(events[0]["v"], 1);
    assert_eq!(events[0]["t"], "hello");
    assert_eq!(events[0]["seq"], 0);
    assert!(events[0]["engine_version"].is_string());
    assert_eq!(events[0]["ir_version"], 1);
    assert_eq!(events[0]["protocol"], 1);
    assert!(
        events[0]["pdfium_version"]
            .as_str()
            .is_some_and(|v| !v.is_empty()),
        "hello carries the bound PDFium version: {:?}",
        events[0]
    );

    // `seq` is strictly monotone from zero, and every line fits the 8 KiB cap.
    for (expected, event) in events.iter().enumerate() {
        assert_eq!(
            event["seq"], expected as u64,
            "seq must be strictly monotone"
        );
        assert_eq!(event["v"], 1);
    }
    for line in stderr.lines() {
        assert!(line.len() <= 8 * 1024, "event line exceeds the 8 KiB cap");
    }

    // The run ends with `done`, and never parses as anything but events.
    let last = events.last().expect("at least one event");
    assert_eq!(last["t"], "done");
    assert_eq!(last["status"], "ok");
}
