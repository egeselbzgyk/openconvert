//! The repository-level CI gates, asserted against the repository itself.
//!
//! This file and `xtask/src/ci_lint.rs` are the only two the linter skips, because they have
//! to contain the patterns it bans. The first test below asserts that the list is still
//! exactly those two.

use std::path::{Path, PathBuf};
use std::process::Command;

use xtask::{ci_lint, thresholds_lint};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .expect("xtask has a parent directory")
}

fn xtask(task: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg(task)
        .current_dir(workspace_root())
        .output()
        .expect("xtask runs")
}

/// The banned attribute, assembled at runtime so this file does not contain it literally.
/// The exemption above would cover it, but a test that depends on its own exemption proves
/// less than one that does not.
fn ignore_attribute() -> String {
    format!("#[{}]", "ignore")
}

fn marker(word: &str) -> String {
    word.to_owned()
}

#[test]
fn no_ignored_tests() {
    let output = xtask("ci-lint");
    assert_eq!(
        output.status.code(),
        Some(0),
        "ci-lint failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("clean"));

    // The exemption list is bounded and has not grown.
    assert_eq!(
        ci_lint::self_referential_files(),
        ["xtask/src/ci_lint.rs", "xtask/tests/ci.rs"],
        "only the files that define and test the rules may be exempt"
    );

    // The rules are also checked against text that must fail them, so a linter that quietly
    // stopped finding anything cannot pass this test.
    let rust = Path::new("demo.rs");
    let skipped = format!("#[test]\n{}\nfn skipped() {{}}\n", ignore_attribute());
    assert!(
        !ci_lint::findings_in(rust, &skipped).is_empty(),
        "a skipped test must be found"
    );
    assert!(
        !ci_lint::findings_in(rust, &format!("// {}: fix this later\n", marker("TODO"))).is_empty(),
        "an unnumbered marker must be found"
    );
    assert!(
        !ci_lint::findings_in(rust, &format!("// {} this is wrong\n", marker("FIXME"))).is_empty(),
        "an unnumbered fix-me must be found"
    );
    assert!(
        ci_lint::findings_in(rust, &format!("// {}(#123): later\n", marker("TODO"))).is_empty(),
        "a numbered marker is allowed"
    );

    // A marker embedded in a longer word is not a marker.
    assert!(
        ci_lint::findings_in(rust, &format!("let mi{}s = 1;\n", marker("todo"))).is_empty(),
        "a lowercase word must not trip the rule"
    );
    assert!(
        ci_lint::findings_in(rust, &format!("let MI{}S = 1;\n", marker("TODO"))).is_empty(),
        "a marker embedded in a word must not trip the rule"
    );

    // `models.toml` placeholders have their own rule, checked only on a release branch.
    assert!(
        ci_lint::findings_in(Path::new("models.toml"), "revision = \"TODO_COMMIT_SHA\"\n")
            .is_empty(),
        "a models.toml placeholder is not an unnumbered marker"
    );

    // The skipped-test rule is a Rust concept; the same text in another language is not.
    assert!(
        ci_lint::findings_in(
            Path::new("demo.py"),
            &format!("# see {} in the Rust code\n", ignore_attribute())
        )
        .is_empty(),
        "the skipped-test rule applies to Rust source only"
    );
}

#[test]
fn thresholds_pass_their_own_provenance_rule() {
    let output = xtask("thresholds-lint");
    assert_eq!(
        output.status.code(),
        Some(0),
        "thresholds-lint failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("clean"), "{stdout}");
    // It reports the date it judged expiry against, so nobody has to guess which clock ran.
    assert!(
        stdout.contains(&thresholds_lint::today_for_test()),
        "expected today's date in {stdout:?}"
    );
}
