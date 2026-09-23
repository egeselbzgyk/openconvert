//! "Fix and rebuild" resumes after `structure` (Phase 12 acceptance A12.4b, ratified R-15).
//!
//! A metadata or TOC correction cannot change a glyph, so the run that applies one starts from
//! what the last full run of the same book saved and runs only `document`, `epub`, `validate`,
//! `repair` and `report`. These run the built binary with `--progress json`, because the claim
//! is about which stages *executed*, and the stage events are how the engine says so.

use std::path::{Path, PathBuf};
use std::process::Command;

use oc_model::overrides::{MetadataPatch, Overrides, TocPatch};
use serde_json::Value;

const CACHE_VAR: &str = "OC_CACHE_DIR";
const MODIFIED: &str = "2026-01-01T00:00:00Z";

/// The stages a rebuild may run, and every stage before them, which it may not.
const DOWNSTREAM: [&str; 5] = ["document", "epub", "validate", "repair", "report"];
const UPSTREAM: [&str; 7] = [
    "inspect",
    "ingest",
    "text",
    "furniture",
    "layout",
    "paragraphs",
    "structure",
];

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

fn scratch(test: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("openconvert-rebuild-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the scratch directory is made");
    directory
}

struct Run {
    report: Value,
    epub: Vec<u8>,
    /// Every stage that began, in order.
    began: Vec<String>,
}

/// `convert` with a fixed `dcterms:modified`, events on stderr, the cache named or not.
fn convert(directory: &Path, name: &str, cache: Option<&Path>, overrides: Option<&Path>) -> Run {
    let epub = directory.join(format!("{name}.epub"));
    let report = directory.join(format!("{name}.report.json"));
    let mut command = Command::new(binary());
    command
        .arg("convert")
        .arg(fixture("f09_novel_structure"))
        .arg("-o")
        .arg(&epub)
        .arg("--report")
        .arg(&report)
        .arg("--modified")
        .arg(MODIFIED)
        .arg("--progress")
        .arg("json");
    if let Some(overrides) = overrides {
        command.arg("--overrides").arg(overrides);
    }
    match cache {
        Some(cache) => command.env(CACHE_VAR, cache),
        None => command.env_remove(CACHE_VAR),
    };
    let output = command.output().expect("the binary runs");
    let stderr = String::from_utf8(output.stderr).expect("UTF-8");
    assert_eq!(output.status.code(), Some(0), "{stderr}");
    let began = stderr
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|event| event["t"] == "stage" && event["phase"] == "begin")
        .filter_map(|event| event["name"].as_str().map(str::to_owned))
        .collect();
    Run {
        report: serde_json::from_slice(&std::fs::read(&report).expect("a report")).expect("JSON"),
        epub: std::fs::read(&epub).expect("an EPUB"),
        began,
    }
}

/// The corrections a user made to f09, keyed off a first run's report.
fn corrections(directory: &Path, first: &Value) -> PathBuf {
    let sha = first["input"]["sha256"].as_str().expect("a digest");
    let chapter_one = first["document"]["toc"]
        .as_array()
        .expect("a toc")
        .iter()
        .find(|entry| entry["title"] == "Chapter One")
        .expect("Chapter One")
        .clone();
    let mut overrides = Overrides::new(sha);
    overrides.metadata = Some(MetadataPatch {
        title: Some("Rebuilt".to_owned()),
        authors: Some(vec!["Ada Reader".to_owned()]),
        language: Some("en".to_owned()),
    });
    overrides.toc = Some(vec![TocPatch {
        heading: chapter_one["heading"]
            .as_str()
            .expect("an id")
            .parse()
            .expect("a block id"),
        title: Some("I. The Beginning".to_owned()),
        level: Some(2),
    }]);
    let path = directory.join("overrides.json");
    std::fs::write(&path, overrides.to_json().expect("serialises")).expect("written");
    path
}

/// The report without what a rebuild is expected to change: the wall-clock.
fn without_timings(report: &Value) -> Value {
    let mut report = report.clone();
    if let Some(object) = report.as_object_mut() {
        object.remove("timings_ms");
    }
    report
}

/// A12.4b — only `document`, `epub`, `validate`, `repair`, `report` re-run, from the cached
/// `structure` output; no stage before `structure` executes. And what they build is the book a
/// full run with the same corrections builds, byte for byte.
#[test]
fn a_rebuild_runs_only_the_stages_after_structure() {
    let directory = scratch("stages");
    let cache = directory.join("cache");

    let first = convert(&directory, "first", Some(&cache), None);
    for stage in ["ingest", "text", "furniture", "layout", "structure"] {
        assert!(
            first.began.iter().any(|began| began == stage),
            "a full run runs {stage}: {:?}",
            first.began
        );
    }
    let saved = std::fs::read_dir(cache.join("structure"))
        .expect("the full run saved what structure settled")
        .count();
    assert_eq!(saved, 1);

    let overrides = corrections(&directory, &first.report);
    let rebuilt = convert(&directory, "rebuilt", Some(&cache), Some(&overrides));
    assert!(
        rebuilt
            .began
            .iter()
            .all(|stage| DOWNSTREAM.contains(&stage.as_str())),
        "only the stages after structure ran: {:?}",
        rebuilt.began
    );
    for stage in UPSTREAM {
        assert!(
            !rebuilt.began.iter().any(|began| began == stage),
            "{stage} executed in a rebuild: {:?}",
            rebuilt.began
        );
    }
    assert_eq!(
        rebuilt.began.first().map(String::as_str),
        Some("document"),
        "the rebuild starts at document"
    );
    assert_eq!(rebuilt.report["document"]["title"], "Rebuilt");
    let timed: Vec<&str> = rebuilt.report["timings_ms"]
        .as_array()
        .expect("timings")
        .iter()
        .filter_map(|pair| pair[0].as_str())
        .collect();
    assert!(
        timed.iter().all(|stage| !UPSTREAM.contains(stage)),
        "the report times only what ran: {timed:?}"
    );

    // The same corrections, applied by a run that reads the PDF from page one.
    let full = convert(&directory, "full", None, Some(&overrides));
    assert!(full.began.iter().any(|stage| stage == "structure"));
    assert!(
        rebuilt.epub == full.epub,
        "the rebuilt EPUB is the fully converted one, byte for byte"
    );
    assert_eq!(
        without_timings(&rebuilt.report),
        without_timings(&full.report),
        "and so is its report, but for the time it took"
    );
}

/// A save that cannot be resumed from is a full run, never a failure: damaged, from another
/// engine, or from another IR version.
#[test]
fn a_save_that_does_not_fit_is_a_full_run() {
    let directory = scratch("miss");
    let cache = directory.join("cache");
    let first = convert(&directory, "first", Some(&cache), None);
    let overrides = corrections(&directory, &first.report);

    let save = std::fs::read_dir(cache.join("structure"))
        .expect("saved")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .next()
        .expect("one save");
    let text = std::fs::read_to_string(&save).expect("reads");
    let mut value: Value = serde_json::from_str(&text).expect("JSON");

    for (label, damage) in [
        ("another engine", "engine_version"),
        ("another IR", "ir_version"),
    ] {
        let mut changed = value.clone();
        changed["key"][damage] = match damage {
            "ir_version" => serde_json::json!(oc_model::IR_VERSION + 1),
            _ => serde_json::json!("0.0.0-other"),
        };
        std::fs::write(&save, serde_json::to_vec(&changed).expect("serialises")).expect("written");
        let run = convert(&directory, "again", Some(&cache), Some(&overrides));
        assert!(
            run.began.iter().any(|stage| stage == "structure"),
            "{label}: converted from page one"
        );
        assert_eq!(run.report["document"]["title"], "Rebuilt", "{label}");
    }

    std::fs::write(&save, "{ not json").expect("written");
    let run = convert(&directory, "damaged", Some(&cache), Some(&overrides));
    assert!(run.began.iter().any(|stage| stage == "structure"));
    // The full run saved a good copy again.
    value = serde_json::from_str(&std::fs::read_to_string(&save).expect("reads"))
        .expect("a good save again");
    assert_eq!(value["key"]["engine_version"], env!("CARGO_PKG_VERSION"));
}
