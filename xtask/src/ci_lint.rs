//! `cargo xtask ci-lint` — the repository rules that are not expressible as a clippy lint.
//!
//! Each rule here exists because it was decided once and would otherwise erode quietly:
//! a skipped test still reads as a green suite, and an unnumbered `TODO` is a note to
//! nobody. They are cheap to check and expensive to notice by eye, which is what makes them
//! CI's job (IMPLEMENTATION_PLAN §0.2, §0.3).

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

/// Directories never walked: build output, vendored blobs, and the documents that *discuss*
/// these rules and would otherwise trip them.
const SKIP_DIRS: [&str; 8] = [
    ".git",
    "target",
    "vendor",
    "node_modules",
    ".venv",
    "docs",
    "research",
    ".claude",
];

/// Banned outright: a test that does not run is not a test (§0.2). A test that cannot run
/// everywhere is gated behind a cargo feature and named in `docs/TEST_MATRIX.md` instead.
const IGNORE_ATTRIBUTE: &str = "#[ignore";

/// A `TODO` or `FIXME` must carry an issue number, as `TODO(#123):` (§0.3 item 9).
const TODO_MARKERS: [&str; 2] = ["TODO", "FIXME"];

/// `models.toml` may not ship with placeholder values on a release branch (§1.6).
///
/// This prefix is also exempt from the marker rule above: `TODO_SHA256` is a named slot
/// Phase 9 fills, not a note someone forgot to file, and it has this rule of its own.
const MODEL_PLACEHOLDER: &str = "TODO_";

/// The two files that *define and test* these rules, and therefore have to contain the very
/// patterns they ban.
///
/// This is the whole exemption list, and the test asserts that it is: a linter that cannot
/// describe its own rules is not a workable linter, but one that quietly exempts a growing
/// set of files is worse. Everything else in the repository is linted, including the rest of
/// `xtask`.
const SELF_REFERENTIAL_FILES: [&str; 2] = ["xtask/src/ci_lint.rs", "xtask/tests/ci.rs"];

/// One rule violation, with enough location to fix it.
struct Finding {
    path: PathBuf,
    line: usize,
    rule: &'static str,
    text: String,
}

pub fn run(workspace_root: &Path, release_branch: bool) -> Result<()> {
    let mut findings = Vec::new();

    for path in source_files(workspace_root)? {
        if is_self_referential(workspace_root, &path) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            // A non-UTF-8 file is not source; `cargo deny` and the compiler cover the rest.
            continue;
        };
        for (number, line) in text.lines().enumerate() {
            check_line(&path, number + 1, line, &mut findings);
        }
    }

    if release_branch {
        let models = workspace_root.join("models.toml");
        let text = std::fs::read_to_string(&models)
            .with_context(|| format!("cannot read {}", models.display()))?;
        for (number, line) in text.lines().enumerate() {
            if line.contains(MODEL_PLACEHOLDER) {
                findings.push(Finding {
                    path: models.clone(),
                    line: number + 1,
                    rule: "models.toml still has a TODO_ placeholder on a release branch",
                    text: line.trim().to_owned(),
                });
            }
        }
    }

    report(workspace_root, findings)
}

fn check_line(path: &Path, line_number: usize, line: &str, findings: &mut Vec<Finding>) {
    let is_rust = path.extension().is_some_and(|e| e == "rs");

    if is_rust && line.contains(IGNORE_ATTRIBUTE) {
        findings.push(Finding {
            path: path.to_path_buf(),
            line: line_number,
            rule: "#[ignore] is banned; gate the test behind a cargo feature instead",
            text: line.trim().to_owned(),
        });
    }

    for marker in TODO_MARKERS {
        // Only flag a marker that starts a word, so `MITODO` or a URL cannot trip it.
        let Some(offset) = find_word(line, marker) else {
            continue;
        };
        let rest = &line[offset + marker.len()..];
        // `TODO(#123):` is the required form, and `TODO_NAME` is the models.toml placeholder
        // convention, which `--release-branch` checks separately.
        if !rest.starts_with("(#") && !rest.starts_with('_') {
            findings.push(Finding {
                path: path.to_path_buf(),
                line: line_number,
                rule: "a TODO or FIXME needs an issue number, as TODO(#123):",
                text: line.trim().to_owned(),
            });
        }
    }
}

/// Whether this file is one of the two that define and test the rules.
fn is_self_referential(workspace_root: &Path, path: &Path) -> bool {
    let relative = path
        .strip_prefix(workspace_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    SELF_REFERENTIAL_FILES.contains(&relative.as_str())
}

/// The exemption list, so a test can assert it has not grown.
pub fn self_referential_files() -> &'static [&'static str] {
    &SELF_REFERENTIAL_FILES
}

/// Find `needle` at a word boundary, so a marker embedded in a longer word does not match.
fn find_word(haystack: &str, needle: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(offset) = haystack[from..].find(needle) {
        let start = from + offset;
        let before_ok = start == 0
            || !haystack[..start]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_');
        if before_ok {
            return Some(start);
        }
        from = start + needle.len();
    }
    None
}

/// Every file worth linting: source and configuration, never build output.
fn source_files(root: &Path) -> Result<Vec<PathBuf>> {
    const EXTENSIONS: [&str; 6] = ["rs", "toml", "py", "ts", "js", "yml"];

    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries =
            std::fs::read_dir(&dir).with_context(|| format!("cannot read {}", dir.display()))?;
        for entry in entries {
            let path = entry?.path();
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if path.is_dir() {
                if !SKIP_DIRS.contains(&name.as_ref()) {
                    stack.push(path);
                }
            } else if path
                .extension()
                .is_some_and(|e| EXTENSIONS.iter().any(|allowed| e == *allowed))
            {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn report(workspace_root: &Path, findings: Vec<Finding>) -> Result<()> {
    if findings.is_empty() {
        println!("ci-lint: clean");
        return Ok(());
    }
    for finding in &findings {
        let path = finding
            .path
            .strip_prefix(workspace_root)
            .unwrap_or(&finding.path);
        eprintln!(
            "{}:{}: {}\n    {}",
            path.display().to_string().replace('\\', "/"),
            finding.line,
            finding.rule,
            finding.text
        );
    }
    bail!("ci-lint found {} violation(s)", findings.len())
}

/// The rules, applied to a string rather than to the repository, so a test can feed them
/// text that must fail. Public because an integration test compiles this library without
/// `cfg(test)`.
pub fn findings_in(path: &Path, text: &str) -> Vec<String> {
    let mut findings = Vec::new();
    for (number, line) in text.lines().enumerate() {
        check_line(path, number + 1, line, &mut findings);
    }
    findings.into_iter().map(|f| f.rule.to_owned()).collect()
}

/// The workspace's own source files, for a test that wants to walk them.
pub fn workspace_source_files(root: &Path) -> Result<Vec<PathBuf>> {
    source_files(root)
}
