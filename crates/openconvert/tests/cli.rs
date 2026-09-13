//! CLI-level tests: they run the built binary, because the contract being tested is what a
//! supervising process sees, not what a function returns (D13.1, D13.2).

use std::path::PathBuf;
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

#[test]
fn inspect_json_is_valid_and_stable() {
    let fixture = fixture("f01_prose_single_column");

    let run = || {
        let output = Command::new(binary())
            .arg("inspect")
            .arg(&fixture)
            .arg("--json")
            .output()
            .expect("the binary runs");
        assert_eq!(
            output.status.code(),
            Some(0),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    };

    let first = run();
    let second = run();
    assert_eq!(
        first, second,
        "two inspections of the same file must be byte-identical (D13.8)"
    );

    let parsed: serde_json::Value =
        serde_json::from_slice(&first).expect("stdout is one JSON document");
    assert_eq!(parsed["schema"], "openconvert.inspect/1");
    assert_eq!(parsed["document"]["pages"], 2);
    assert_eq!(parsed["document"]["producer_family"], "Typst");
    assert_eq!(parsed["pages"][0]["class"], "text");

    // stdout is the data channel and carries nothing else (RT C2).
    assert!(
        String::from_utf8_lossy(&first)
            .trim_start()
            .starts_with('{'),
        "stdout must be data only"
    );
}

#[test]
fn exit_code_2_on_bad_args() {
    // A missing input is a usage error, not a conversion failure (§2.4).
    let output = Command::new(binary())
        .arg("inspect")
        .arg("no-such-file.pdf")
        .arg("--json")
        .arg("--progress")
        .arg("json")
        .output()
        .expect("the binary runs");

    assert_eq!(output.status.code(), Some(2));
    assert!(
        output.stdout.is_empty(),
        "a failed run writes nothing to the data channel"
    );

    let fatal = String::from_utf8_lossy(&output.stderr)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|event| event["t"] == "fatal")
        .expect("a fatal event on stderr");
    assert_eq!(fatal["code"], "E_INPUT");

    // An unknown subcommand is also a usage error.
    let output = Command::new(binary())
        .arg("frobnicate")
        .output()
        .expect("the binary runs");
    assert_eq!(output.status.code(), Some(2));

    // ...and so is no subcommand at all.
    let output = Command::new(binary()).output().expect("the binary runs");
    assert_eq!(output.status.code(), Some(2));
}

/// `--max-pages` reaches the door, and a refusal is a configuration outcome rather than a
/// conversion failure (§2.4).
///
/// The unit test for the guard itself is `oc_pdf::limits::max_pages_refuses_at_the_door`;
/// this one exists because a flag that parses but is never threaded anywhere would pass that
/// test and still do nothing.
#[test]
fn max_pages_flag_refuses_the_document() {
    let output = Command::new(binary())
        .arg("inspect")
        .arg(fixture("f01_prose_single_column"))
        .arg("--json")
        .arg("--max-pages")
        .arg("1")
        .arg("--progress")
        .arg("json")
        .output()
        .expect("the binary runs");

    assert_eq!(
        output.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E_LIMIT_EXCEEDED"), "stderr: {stderr}");
    assert!(stderr.contains("max_pages"), "stderr: {stderr}");
    assert!(
        output.stdout.is_empty(),
        "a refused document writes no report to stdout"
    );

    // f01 has two pages, so the same file with the allowance it needs still works.
    let ok = Command::new(binary())
        .arg("inspect")
        .arg(fixture("f01_prose_single_column"))
        .arg("--json")
        .arg("--max-pages")
        .arg("2")
        .output()
        .expect("the binary runs");
    assert_eq!(ok.status.code(), Some(0));
}

/// Test 1.13. An encrypted document with a real user password: refused without one, converted
/// with it.
///
/// The exit code is the contract. A supervising UI decides whether to prompt for a password
/// from the code and the `fatal` event, never by reading the message text (D13.2), so this
/// asserts both and says nothing about the wording.
#[test]
fn encrypted_with_password_requires_flag() {
    let encrypted = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/fixtures/mutations/h01__encrypted_password.pdf");
    assert!(
        encrypted.is_file(),
        "missing {}; run `cargo run -p xtask -- mutations`",
        encrypted.display()
    );

    let refused = Command::new(binary())
        .arg("inspect")
        .arg(&encrypted)
        .arg("--json")
        .arg("--progress")
        .arg("json")
        // The environment variable is the other way in, and an inherited one would make this
        // test pass without the flag doing anything.
        .env_remove("OC_PDF_PASSWORD")
        .output()
        .expect("the binary runs");

    assert_eq!(
        refused.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&refused.stderr)
    );
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("E_PASSWORD_REQUIRED"), "stderr: {stderr}");
    assert!(
        refused.stdout.is_empty(),
        "a refused document writes no report"
    );

    let opened = Command::new(binary())
        .arg("inspect")
        .arg(&encrypted)
        .arg("--json")
        .arg("--password")
        .arg("secret")
        .env_remove("OC_PDF_PASSWORD")
        .output()
        .expect("the binary runs");

    assert_eq!(
        opened.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&opened.stderr)
    );
    let parsed: serde_json::Value =
        serde_json::from_slice(&opened.stdout).expect("stdout is one JSON document");
    assert_eq!(parsed["document"]["encrypted"], true);
    assert_eq!(parsed["document"]["pages"], 1);
    // Recorded, not enforced: this fixture permits printing, and h01__encrypted_no_print does
    // not, and both convert.
    assert_eq!(parsed["document"]["permissions"]["print"], true);
}

/// `dump-stage ingest` writes one canonical-JSON object per line, deterministically.
///
/// The unit-level assertion on the dump's *shape* is `oc_pdf::dump`'s snapshot (test 1.17);
/// this is the assertion that the subcommand exists, streams, and keeps stdout a data channel.
#[test]
fn dump_stage_ingest_streams_one_object_per_line() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/fixtures/handmade/h13_outline.pdf");

    let run = || {
        let output = Command::new(binary())
            .arg("dump-stage")
            .arg("ingest")
            .arg(&fixture)
            .output()
            .expect("the binary runs");
        assert_eq!(
            output.status.code(),
            Some(0),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    };

    let first = run();
    assert_eq!(
        first,
        run(),
        "two dumps of the same file must be byte-identical (D13.8)"
    );

    let text = String::from_utf8(first).expect("the dump is UTF-8");
    let lines: Vec<&str> = text.lines().collect();
    // One header plus one line per page, which for h13 is one page.
    assert_eq!(lines.len(), 2, "{lines:?}");

    let header: serde_json::Value = serde_json::from_str(lines[0]).expect("the header is JSON");
    assert_eq!(header["schema"], "openconvert.dump.ingest/1");
    assert_eq!(header["stage"], "ingest");
    assert_eq!(header["outline"].as_array().map(Vec::len), Some(6));
    // `ir_version` leads the object, which is what D13.3 asks of canonical JSON and what a
    // reader needs before it can interpret anything after it.
    assert!(lines[0].starts_with(r#"{"ir_version":"#), "{}", lines[0]);

    let page: serde_json::Value = serde_json::from_str(lines[1]).expect("the page is JSON");
    assert_eq!(page["index"], 0);
    assert_eq!(page["c_raw"]["A"], 1);
}

/// A stage that cannot be dumped yet says so, and says it as a usage error.
///
/// `structure` is the next stage to arrive (Phase 4); `ingest`, `text` and `layout` are
/// implemented, and this test moves to whichever stage is next each time one lands.
#[test]
fn dump_stage_rejects_an_unimplemented_stage() {
    let output = Command::new(binary())
        .arg("dump-stage")
        .arg("structure")
        .arg(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../corpus/fixtures/handmade/h01_two_glyphs.pdf"),
        )
        .arg("--progress")
        .arg("json")
        .output()
        .expect("the binary runs");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E_UNKNOWN_STAGE"), "stderr: {stderr}");
    assert!(output.stdout.is_empty());
}
