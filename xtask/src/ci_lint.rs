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
const SKIP_DIRS: [&str; 9] = [
    ".git",
    "target",
    "vendor",
    "node_modules",
    // The UI's build output: bundled third-party code, not this repository's source.
    "dist",
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

/// The registry every warning code has to be listed in (R10 §6.20, Phase 6 detail 6).
const REGISTRY: &str = "crates/oc-core/src/warnings/codes.rs";

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

/// Where `unsafe` may appear at all (PHASE 14 row 14.22): the PDFium binding module and the three
/// syscall modules of the sandbox. Everything else is `#![forbid(unsafe_code)]` and must not so much
/// as spell an `unsafe` block — and today none of these four contains one either: the syscalls
/// belong to `pdfium-render`, `rustix`, `landlock` and `win32job` (ARCHITECTURE §5 allows the first
/// only, and the higher authority is kept; see `docs/DECISIONS_LOG.md`).
const UNSAFE_ALLOWED: [&str; 4] = [
    "crates/oc-pdf/src/pdfium/",
    "crates/oc-core/src/sandbox/landlock.rs",
    "crates/oc-core/src/sandbox/rlimit.rs",
    "crates/oc-core/src/sandbox/jobobject.rs",
];

/// Crate roots that cannot forbid `unsafe_code`: libFuzzer's `fuzz_target!` expands to a
/// `#[no_mangle]` entry point, which the lint counts, in a dev-only workspace of its own.
const FORBID_EXEMPT_DIRS: [&str; 1] = ["fuzz/"];

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

    check_warning_registry(workspace_root, &mut findings)?;
    check_unsafe(workspace_root, &mut findings)?;

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

/// Every warning code defined in the tree is in `oc-core`'s registry, and every code in the
/// registry is defined somewhere (R10 §6.20).
///
/// This is the rule that makes the localisation gate total. `oc-core` owns the templates and cannot
/// depend on the crates that raise the warnings — `oc-structure` owns `W_TABLE_AS_IMAGE` because it
/// is what decides a table cannot be recovered — so the registry is written by hand, and a list
/// written by hand drifts. Both directions are checked: a code with no registry entry is a user who
/// gets a bare identifier instead of a sentence, and a registry entry nothing defines is three
/// translations of a claim the software no longer makes.
fn check_warning_registry(workspace_root: &Path, findings: &mut Vec<Finding>) -> Result<()> {
    let registry_path = workspace_root.join(REGISTRY);
    let registry_text = std::fs::read_to_string(&registry_path)
        .with_context(|| format!("cannot read {}", registry_path.display()))?;
    let registered = registry_codes(&registry_text);

    let mut defined: Vec<(String, PathBuf, usize)> = Vec::new();
    for path in source_files(workspace_root)? {
        if path == registry_path || is_self_referential(workspace_root, &path) {
            continue;
        }
        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (number, line) in text.lines().enumerate() {
            if let Some(code) = defined_code(line) {
                defined.push((code, path.clone(), number + 1));
            }
        }
    }

    for (code, path, line) in &defined {
        if !registered.contains(code) {
            findings.push(Finding {
                path: path.clone(),
                line: *line,
                rule:
                    "this warning code is not in crates/oc-core/src/warnings/codes.rs, so it has \
                       no localised template",
                text: code.clone(),
            });
        }
    }
    for code in &registered {
        if !defined.iter().any(|(defined, _, _)| defined == code) {
            findings.push(Finding {
                path: registry_path.clone(),
                line: 0,
                rule: "the registry lists a warning code nothing in the tree defines",
                text: code.clone(),
            });
        }
    }
    Ok(())
}

/// `code: "W_…"` entries of the registry.
fn registry_codes(text: &str) -> Vec<String> {
    const PREFIX: &str = "code: \"";
    text.lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix(PREFIX)?;
            let end = rest.find('"')?;
            Some(rest[..end].to_owned())
        })
        .collect()
}

/// The code a `const … : &str = "W_…";` line declares, if it declares one.
///
/// Matched on the *value* rather than on the constant's name, because two of them are named
/// `WARN_IMAGE_ONLY` and `WARN_BROKEN_TEXT` while their values are `W_…` like every other. What the
/// report carries is the value.
fn defined_code(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if !trimmed.starts_with("const ") && !trimmed.starts_with("pub const ") {
        return None;
    }
    let rest = trimmed.split_once("= \"")?.1;
    let end = rest.find('"')?;
    let value = &rest[..end];
    value.starts_with("W_").then(|| value.to_owned())
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
    const EXTENSIONS: [&str; 8] = ["rs", "toml", "py", "ts", "js", "mjs", "svelte", "yml"];

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

/// PHASE 14 row 14.22: `unsafe` only in [`UNSAFE_ALLOWED`], and every crate root forbids it.
fn check_unsafe(workspace_root: &Path, findings: &mut Vec<Finding>) -> Result<()> {
    for path in source_files(workspace_root)? {
        if path.extension().is_none_or(|ext| ext != "rs")
            || is_self_referential(workspace_root, &path)
        {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let relative = relative(workspace_root, &path);
        for rule_line in unsafe_rule(&relative, &text) {
            findings.push(Finding {
                path: path.clone(),
                line: rule_line.0,
                rule: rule_line.1,
                text: rule_line.2,
            });
        }
    }
    Ok(())
}

fn relative(workspace_root: &Path, path: &Path) -> String {
    path.strip_prefix(workspace_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Whether `relative` is a crate root: `lib.rs`, `main.rs` or a `src/bin/` file.
fn is_crate_root(relative: &str) -> bool {
    relative.ends_with("/src/lib.rs")
        || relative.ends_with("/src/main.rs")
        || relative.contains("/src/bin/")
        || relative.starts_with("fuzz/fuzz_targets/")
}

/// The `unsafe` rule over one file, by its path relative to the workspace root:
/// `(line, rule, text)` for each violation.
fn unsafe_rule(relative: &str, text: &str) -> Vec<(usize, &'static str, String)> {
    const UNSAFE_RULE: &str =
        "`unsafe` outside the declared modules (the PDFium binding, the sandbox's syscall modules)";
    const FORBID_RULE: &str = "a crate root without #![forbid(unsafe_code)]";
    const FORBID: &str = "#![forbid(unsafe_code)]";

    let allowed = UNSAFE_ALLOWED
        .iter()
        .any(|declared| relative.starts_with(declared));
    let mut out = Vec::new();
    for (number, line) in text.lines().enumerate() {
        // Prose about `unsafe` — a doc comment saying there is none — is not a use of it.
        let code = line.split("//").next().unwrap_or_default();
        let uses = [
            "unsafe {",
            "unsafe fn",
            "unsafe impl",
            "unsafe extern",
            "unsafe trait",
        ]
        .iter()
        .any(|form| find_word(code, form).is_some())
            || code.contains("allow(unsafe_code)");
        if uses && !allowed {
            out.push((number + 1, UNSAFE_RULE, line.trim().to_owned()));
        }
    }
    let exempt = FORBID_EXEMPT_DIRS
        .iter()
        .any(|dir| relative.starts_with(dir));
    if is_crate_root(relative) && !exempt && !text.contains(FORBID) {
        out.push((1, FORBID_RULE, relative.to_owned()));
    }
    out
}

/// [`unsafe_rule`] for a test: the rules that fire on `text` at `relative`.
pub fn unsafe_findings_in(relative: &str, text: &str) -> Vec<String> {
    unsafe_rule(relative, text)
        .into_iter()
        .map(|(_, rule, _)| rule.to_owned())
        .collect()
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
