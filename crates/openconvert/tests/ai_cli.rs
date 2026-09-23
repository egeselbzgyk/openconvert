//! PHASE 10 at the command line: `--ai` is an opt-in that never fails a conversion, and without
//! it nothing about AI happens at all. These run the built binary, because what is being tested
//! is what a user and the desktop app get.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_openconvert"))
}

fn fixture(stem: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/fixtures")
        .join(format!("{stem}.pdf"))
}

/// A scratch directory of its own per test, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("oc-ai-cli-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("a scratch directory");
        Self(path)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `convert` a fixture into `scratch` with `extra` flags, the timestamp pinned and the data
/// directory — where the answer cache lives — inside the scratch directory.
fn convert(scratch: &Scratch, stem: &str, name: &str, extra: &[&str]) -> Output {
    let epub = scratch.join(&format!("{name}.epub"));
    let report = scratch.join(&format!("{name}.report.json"));
    Command::new(binary())
        .arg("convert")
        .arg(fixture(stem))
        .arg("-o")
        .arg(&epub)
        .arg("--report")
        .arg(&report)
        .args(["--modified", "2026-01-01T00:00:00Z", "--lang", "en"])
        .args(extra)
        .env("XDG_DATA_HOME", scratch.join("data"))
        .output()
        .expect("the binary runs")
}

fn report(path: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).expect("the report was written"))
        .expect("the report is JSON")
}

fn warning_codes(report: &serde_json::Value) -> Vec<String> {
    report["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|warning| warning["code"].as_str().map(str::to_owned))
        .collect()
}

/// Row 10.16 (RT D20). `--ai` with an endpoint nothing listens on — port 1 of this machine —
/// and every task enabled, so the step does try to ask: the conversion completes, exits 0, the
/// book is the deterministic book byte for byte, and the report carries the banner warning
/// `W_LLM_UNAVAILABLE` rather than pretending a model ran. Then the same with no endpoint and no
/// `llama-server` to start: the engine-owned sidecar is missing, and the answer is the same.
#[test]
fn missing_sidecar_degrades_gracefully() {
    let scratch = Scratch::new("unavailable");
    let plain = convert(&scratch, "f07_verse_and_quote", "plain", &["--no-ai"]);
    assert!(
        plain.status.success(),
        "{}",
        String::from_utf8_lossy(&plain.stderr)
    );

    let unreachable = convert(
        &scratch,
        "f07_verse_and_quote",
        "unreachable",
        &[
            "--ai",
            "--ai-all-tasks",
            "--llm-endpoint",
            "http://127.0.0.1:1",
        ],
    );
    assert_eq!(
        unreachable.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&unreachable.stderr)
    );
    let unreachable_report = report(&scratch.join("unreachable.report.json"));
    assert!(warning_codes(&unreachable_report).contains(&"W_LLM_UNAVAILABLE".to_owned()));
    assert_eq!(
        std::fs::read(scratch.join("unreachable.epub")).expect("an EPUB"),
        std::fs::read(scratch.join("plain.epub")).expect("an EPUB"),
        "the book is the deterministic one"
    );
    assert!(String::from_utf8_lossy(&unreachable.stderr).contains("no model could be reached"));

    let missing = Command::new(binary())
        .arg("convert")
        .arg(fixture("f07_verse_and_quote"))
        .arg("-o")
        .arg(scratch.join("missing.epub"))
        .arg("--report")
        .arg(scratch.join("missing.report.json"))
        .args(["--modified", "2026-01-01T00:00:00Z", "--lang", "en", "--ai"])
        .env("XDG_DATA_HOME", scratch.join("data"))
        .env("OC_LLAMA_SERVER", scratch.join("no-such-llama-server"))
        .output()
        .expect("the binary runs");
    assert_eq!(missing.status.code(), Some(0));
    let missing_report = report(&scratch.join("missing.report.json"));
    let banner = missing_report["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|warning| warning["code"] == "W_LLM_UNAVAILABLE")
        .expect("the banner");
    assert_eq!(banner["args"]["reason"], "no llama-server was found");
    assert_eq!(
        std::fs::read(scratch.join("missing.epub")).expect("an EPUB"),
        std::fs::read(scratch.join("plain.epub")).expect("an EPUB")
    );
}

/// Row 10.19 / A10.1 (D17). With no flags, `ai.enabled` is false and nothing about AI happens:
/// no `llm` event on the NDJSON channel, no `ai` section and no prompt version in the report, no
/// decision carrying a model's trace — and the escalations are still recorded.
#[test]
fn ai_default_is_off() {
    assert!(
        !oc_core::thresholds::T.ai.enabled,
        "v1 ships ai.enabled = false"
    );

    let scratch = Scratch::new("default");
    let output = convert(
        &scratch,
        "f07_verse_and_quote",
        "default",
        &["--progress", "json"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let events = String::from_utf8_lossy(&output.stderr);
    assert!(
        events
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .all(|event| event["t"] != "llm"),
        "an llm event with AI off"
    );

    let report = report(&scratch.join("default.report.json"));
    assert!(report.get("ai").is_none(), "no ai section with AI off");
    assert!(report["engine"]["prompt_version"].is_null());
    assert!(report["decisions"]
        .as_array()
        .into_iter()
        .flatten()
        .all(|decision| decision["llm"].is_null() && decision["fallback"].is_null()));
    assert!(
        report["escalations"]
            .as_array()
            .is_some_and(|records| !records.is_empty()),
        "the calibration record is written with AI off"
    );
}

/// An endpoint that is not this machine is a usage error: nothing is converted and nothing is
/// sent (D10; Phase 11 adds the consent that could allow it).
#[test]
fn an_endpoint_off_this_machine_is_refused() {
    let scratch = Scratch::new("remote");
    let output = convert(
        &scratch,
        "f01_prose_single_column",
        "remote",
        &["--ai", "--llm-endpoint", "https://api.example.com/v1"],
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("api.example.com"));
    assert!(!scratch.join("remote.epub").exists());
}

/// The AI-only flags mean nothing without `--ai`, and are refused rather than ignored; `--no-ai`
/// wins over `--ai`.
#[test]
fn ai_flags_need_ai_and_no_ai_wins() {
    let scratch = Scratch::new("flags");
    let lonely = convert(
        &scratch,
        "f01_prose_single_column",
        "lonely",
        &["--ai-all-tasks"],
    );
    assert_eq!(lonely.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&lonely.stderr).contains("--ai"));

    let both = convert(
        &scratch,
        "f01_prose_single_column",
        "both",
        &[
            "--ai",
            "--no-ai",
            "--llm-endpoint",
            "https://api.example.com/v1",
        ],
    );
    assert_eq!(
        both.status.code(),
        Some(0),
        "--no-ai wins, so nothing is opened"
    );
    assert!(report(&scratch.join("both.report.json"))
        .get("ai")
        .is_none());
}

/// A10.2 and A10.3 against a **real** model (nightly, `--features live-llm`): the engine starts
/// the pinned `llama-server` (`OC_LLAMA_SERVER`) on the model `OC_LIVE_MODEL` names, converts three
/// fixtures that escalate a task each — `f03` no title, `f07` an ambiguous block, `f10` several
/// heading styles — with every task enabled, and every book comes back whole: exit 0, at most
/// `llm.max_calls_per_book` calls, no `W_LLM_UNAVAILABLE`, and I-7 holding.
///
/// With the feature on and either variable unset this fails, as the Phase 9 live tests do: a live
/// test that passes without a model is not a live test.
#[cfg(feature = "live-llm")]
#[test]
fn ai_against_a_live_model_conserves_every_book() {
    let required = |name: &str| {
        std::env::var_os(name).unwrap_or_else(|| panic!("--features live-llm needs {name}"))
    };
    let server = required("OC_LLAMA_SERVER");
    let model = required("OC_LIVE_MODEL");
    let scratch = Scratch::new("live");
    for stem in [
        "f03_image_only",
        "f07_verse_and_quote",
        "f10_lists_and_table",
    ] {
        let output = Command::new(binary())
            .arg("convert")
            .arg(fixture(stem))
            .arg("-o")
            .arg(scratch.join(&format!("{stem}.epub")))
            .arg("--report")
            .arg(scratch.join(&format!("{stem}.report.json")))
            .args(["--lang", "en", "--ai", "--ai-all-tasks", "--model-path"])
            .arg(&model)
            .env("XDG_DATA_HOME", scratch.join("data"))
            .env("OC_LLAMA_SERVER", &server)
            .output()
            .expect("the binary runs");
        assert_eq!(
            output.status.code(),
            Some(0),
            "{stem}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report = report(&scratch.join(&format!("{stem}.report.json")));
        assert!(
            !warning_codes(&report).contains(&"W_LLM_UNAVAILABLE".to_owned()),
            "{stem}"
        );
        let calls = report["ai"]["calls"].as_u64().unwrap_or(u64::MAX);
        assert!(
            calls <= u64::try_from(oc_core::thresholds::T.llm.max_calls_per_book).unwrap_or(0),
            "{stem}: {calls} calls"
        );
        assert_eq!(report["conservation"]["i7"]["holds"], true, "{stem}: I-7");
    }
}
