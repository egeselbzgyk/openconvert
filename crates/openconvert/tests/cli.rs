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
