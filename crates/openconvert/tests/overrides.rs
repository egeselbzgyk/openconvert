//! The user's corrections, end to end (Phase 12 row 12.10, ARCHITECTURE §4.7).
//!
//! What the desktop app does, done here with the built binary: convert, read the report, write an
//! `overrides.json` of metadata and TOC corrections, convert again with it named in the job spec,
//! and check that the book, the report and the ledger all say what was corrected.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use oc_model::ids::BlockId;
use oc_model::overrides::{MetadataPatch, Overrides, TocPatch};
use serde_json::{json, Value};

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
        "openconvert-overrides-{test}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the scratch directory is made");
    directory
}

struct Converted {
    report: Value,
    epub: PathBuf,
}

/// One conversion through a job spec, as the app runs it, with `overrides` named when given.
fn convert(directory: &Path, input: &Path, name: &str, overrides: Option<&Path>) -> Converted {
    let epub = directory.join(format!("{name}.epub"));
    let report = directory.join(format!("{name}.report.json"));
    let mut spec = json!({
        "schema": "openconvert.job/1",
        "input": {"path": input},
        "output": {"path": epub, "report_path": report},
        "locale": "en",
    });
    if let Some(overrides) = overrides {
        spec["overrides_path"] = json!(overrides);
    }
    let spec_path = directory.join(format!("{name}.job.json"));
    std::fs::write(&spec_path, serde_json::to_vec(&spec).expect("serialises")).expect("written");

    let output = Command::new(binary())
        .arg(&spec_path)
        .output()
        .expect("the binary runs");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value =
        serde_json::from_slice(&std::fs::read(&report).expect("a report")).expect("JSON");
    Converted { report, epub }
}

fn entry(epub: &Path, name: &str) -> String {
    let mut archive =
        zip::ZipArchive::new(std::fs::File::open(epub).expect("opens")).expect("a zip");
    let mut text = String::new();
    archive
        .by_name(name)
        .unwrap_or_else(|e| panic!("{name}: {e}"))
        .read_to_string(&mut text)
        .expect("UTF-8");
    text
}

/// The content document whose body has `needle` in it.
fn chapter_with(epub: &Path, needle: &str) -> String {
    let mut archive =
        zip::ZipArchive::new(std::fs::File::open(epub).expect("opens")).expect("a zip");
    let names: Vec<String> = archive
        .file_names()
        .filter(|name| name.starts_with("text/"))
        .map(str::to_owned)
        .collect();
    for name in names {
        let mut text = String::new();
        archive
            .by_name(&name)
            .expect("listed")
            .read_to_string(&mut text)
            .expect("UTF-8");
        if text.contains(needle) {
            return text;
        }
    }
    panic!("no content document has {needle:?}");
}

fn heading(report: &Value, title: &str) -> Value {
    report["document"]["toc"]
        .as_array()
        .expect("a toc")
        .iter()
        .find(|entry| entry["title"] == title)
        .unwrap_or_else(|| panic!("no heading {title:?} in {}", report["document"]["toc"]))
        .clone()
}

fn id(entry: &Value) -> BlockId {
    entry["heading"]
        .as_str()
        .expect("an id")
        .parse()
        .expect("a block id")
}

fn non_whitespace(text: &str) -> u64 {
    text.chars().filter(|c| !c.is_whitespace()).count() as u64
}

/// 12.10 — edit → save → re-convert applies the patch; ledger reason `UserOverride`.
#[test]
fn overrides_roundtrip_metadata_and_toc() {
    let directory = scratch("roundtrip");
    let input = fixture("f09_novel_structure");

    // Converted as the pipeline sees it.
    let first = convert(&directory, &input, "first", None);
    let sha = first.report["input"]["sha256"]
        .as_str()
        .expect("a digest")
        .to_owned();
    let chapter_one = heading(&first.report, "Chapter One");
    let chapter_two = heading(&first.report, "Chapter Two");
    assert_eq!(chapter_two["level"], 1);

    // Edited, and saved the way the app saves it.
    let mut overrides = Overrides::new(&sha);
    overrides.metadata = Some(MetadataPatch {
        title: Some("A Short Novel, Corrected".to_owned()),
        authors: Some(vec!["Ada Reader".to_owned(), "O. Convert".to_owned()]),
        // The book is detected as English; the user says British English.
        language: Some("en-GB".to_owned()),
    });
    overrides.toc = Some(vec![
        TocPatch {
            heading: id(&chapter_one),
            title: Some("I. The Beginning".to_owned()),
            level: None,
        },
        TocPatch {
            heading: id(&chapter_two),
            title: None,
            level: Some(2),
        },
    ]);
    let saved = directory.join("overrides.json");
    std::fs::write(&saved, overrides.to_json().expect("serialises")).expect("written");
    let read_back = std::fs::read_to_string(&saved).expect("reads");
    assert_eq!(
        Overrides::parse(&read_back, &sha),
        Ok(overrides.clone()),
        "the saved file reads back as the corrections that were made"
    );

    // Converted again, with the corrections.
    let second = convert(&directory, &input, "second", Some(&saved));
    let report = &second.report;
    assert_eq!(report["status"], "ok");
    assert_eq!(report["document"]["title"], "A Short Novel, Corrected");
    assert_eq!(
        report["document"]["authors"],
        json!(["Ada Reader", "O. Convert"])
    );
    assert_eq!(report["document"]["language"], "en-gb");

    let renamed = heading(report, "I. The Beginning");
    assert_eq!(
        renamed["heading"], chapter_one["heading"],
        "same heading, new words"
    );
    assert_eq!(heading(report, "Chapter Two")["level"], 2);
    assert_eq!(
        report["document"]["toc"].as_array().map(Vec::len),
        first.report["document"]["toc"].as_array().map(Vec::len),
        "no heading gained or lost"
    );

    // The ledger: the printed words out, the user's in, as `UserOverride`, from `document` alone —
    // and the end-to-end equation still holds with them in it.
    let per_reason = report["conservation"]["per_reason"]
        .as_array()
        .expect("per reason");
    let user = per_reason
        .iter()
        .find(|total| total["reason"] == "user_override")
        .expect("a user_override total");
    assert_eq!(user["removed_chars"], non_whitespace("Chapter One"));
    assert_eq!(user["added_chars"], non_whitespace("I. The Beginning"));
    assert!(
        user["budget_group"].is_null(),
        "a user's correction draws on no budget"
    );
    assert!(first.report["conservation"]["per_reason"]
        .as_array()
        .expect("per reason")
        .iter()
        .all(|total| total["reason"] != "user_override"));
    assert_eq!(report["conservation"]["i7"]["holds"], true);
    let document_check = report["conservation"]["per_stage"]
        .as_array()
        .expect("per stage")
        .iter()
        .find(|check| check["stage"] == "document")
        .expect("document was checked")
        .clone();
    assert_eq!(document_check["kind"], "budgeted");
    assert_eq!(
        report["conservation"]["epub_chars"].as_u64(),
        first.report["conservation"]["epub_chars"]
            .as_u64()
            .map(|chars| chars - non_whitespace("Chapter One") + non_whitespace("I. The Beginning")),
        "nothing else in the book changed"
    );

    // Each correction is a decision the user made, against what the pipeline had chosen.
    let decisions: Vec<(&str, &str, Value)> = report["decisions"]
        .as_array()
        .expect("decisions")
        .iter()
        .filter(|decision| decision["method"] == "user")
        .map(|decision| {
            (
                decision["kind"].as_str().unwrap_or_default(),
                decision["chosen"].as_str().unwrap_or_default(),
                decision["alternatives"].clone(),
            )
        })
        .collect();
    assert_eq!(
        decisions,
        vec![
            (
                "metadata_title",
                "A Short Novel, Corrected",
                json!(["A Short Novel"])
            ),
            (
                "metadata_authors",
                "Ada Reader; O. Convert",
                json!(["O. Convert"])
            ),
            ("metadata_language", "en-gb", json!(["en"])),
            (
                "toc_heading_text",
                "I. The Beginning",
                json!(["Chapter One"])
            ),
            ("toc_heading_level", "2", json!(["1"])),
        ]
    );

    // And the book says so.
    let opf = entry(&second.epub, "content.opf");
    assert!(
        opf.contains(">A Short Novel, Corrected</dc:title>"),
        "{opf}"
    );
    assert!(opf.contains(">Ada Reader</dc:creator>"), "{opf}");
    assert!(opf.contains(">en-gb</dc:language>"), "{opf}");
    let nav = entry(&second.epub, "nav.xhtml");
    assert!(
        nav.contains("I. The Beginning") && !nav.contains("Chapter One"),
        "{nav}"
    );
    // The heading, not the printed contents page, which lists "Chapter Two" too.
    let two = chapter_with(&second.epub, ">Chapter Two</h");
    assert!(
        two.contains(">Chapter Two</h2>"),
        "Chapter Two is now a level-2 heading: {two}"
    );
}

/// A file from another IR version converts the book without it, and the report names why
/// (ARCHITECTURE §4.6): refused, not silently ignored, and not a reason to withhold the book.
#[test]
fn a_stale_overrides_file_is_named_and_the_book_converts_without_it() {
    let directory = scratch("stale");
    let input = fixture("f09_novel_structure");
    let first = convert(&directory, &input, "first", None);
    let sha = first.report["input"]["sha256"].as_str().expect("a digest");

    let stale = directory.join("overrides.json");
    let older = oc_model::IR_VERSION - 1;
    std::fs::write(
        &stale,
        format!(r#"{{"ir_version": {older}, "source_sha256": "{sha}", "metadata": {{"title": "Stale"}}}}"#),
    )
    .expect("written");

    let second = convert(&directory, &input, "second", Some(&stale));
    let report = &second.report;
    assert_eq!(
        report["document"]["title"],
        first.report["document"]["title"]
    );
    let warning = report["warnings"]
        .as_array()
        .expect("warnings")
        .iter()
        .find(|warning| warning["code"] == "W_OVERRIDES_STALE")
        .expect("the refusal is named")
        .clone();
    assert_eq!(warning["args"]["file_ir"], older.to_string());
    assert_eq!(
        warning["args"]["engine_ir"],
        oc_model::IR_VERSION.to_string()
    );
    assert!(report["decisions"]
        .as_array()
        .expect("decisions")
        .iter()
        .all(|decision| decision["method"] != "user"));
}

/// `convert --overrides` takes the same file the job spec names (§2.1): the CLI and the app are one
/// code path (D13.1).
#[test]
fn the_convert_command_takes_the_same_corrections() {
    let directory = scratch("cli");
    let input = fixture("f09_novel_structure");
    let first = convert(&directory, &input, "first", None);
    let sha = first.report["input"]["sha256"].as_str().expect("a digest");

    let mut overrides = Overrides::new(sha);
    overrides.metadata = Some(MetadataPatch {
        title: Some("From the Command Line".to_owned()),
        ..MetadataPatch::default()
    });
    let saved = directory.join("overrides.json");
    std::fs::write(&saved, overrides.to_json().expect("serialises")).expect("written");

    let report = directory.join("cli.report.json");
    let output = Command::new(binary())
        .arg("convert")
        .arg(&input)
        .arg("-o")
        .arg(directory.join("cli.epub"))
        .arg("--report")
        .arg(&report)
        .arg("--overrides")
        .arg(&saved)
        .output()
        .expect("the binary runs");
    assert_eq!(output.status.code(), Some(0));
    let report: Value =
        serde_json::from_slice(&std::fs::read(&report).expect("a report")).expect("JSON");
    assert_eq!(report["document"]["title"], "From the Command Line");
}
