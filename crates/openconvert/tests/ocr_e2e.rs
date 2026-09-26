//! Rows 13.11, 13.12, 13.14, 13.16, 13.17, 13.18 and 13.22: OCR inside `ingest`, end to end.
//!
//! The engine here is `common::ocr::ScriptedEngine`, in process, so these run on every platform and
//! on machines without Tesseract. The real engine is exercised behind the `tesseract` feature
//! (`ocr_tesseract.rs`), and the process itself by `oc-testkit`'s `ocr_invoke`.

mod common;

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use common::ocr::ScriptedEngine;
use oc_core::ledger_check::{c_of, ReasonTotals};
use oc_core::ocr::discover::{engine_install_hint, OcrUnavailable};
use oc_core::ocr::invoke::{OcrEngine, OcrError};
use oc_core::ocr::W_OCR_LOW_CONFIDENCE;
use oc_core::ocr::{OcrMode, Os, Psm, ReOcr, W_OCR_ENGINE_MISSING, W_OCR_FAILED};
use oc_core::thresholds::T;
use oc_model::ledger::Reason;
use oc_model::text::TextProvenance;
use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;
use openconvert::ocr::{ocr_stage, OcrOptions, RegionOutcome};
use openconvert::pipeline::text_stage;

const SCAN_LINES: [&str; 3] = [
    "A Scanned Chapter",
    "It was a dark and stormy night",
    "the rain fell in torrents",
];

fn typst(stem: &str) -> PathBuf {
    common::fixture(stem)
}

fn options(engine: &Arc<ScriptedEngine>) -> OcrOptions {
    OcrOptions::auto(Ok(engine.clone() as Arc<dyn OcrEngine>), &T)
}

/// `ingest` and `text` over one PDF, with OCR as given.
fn ingest_and_text(
    path: &std::path::Path,
    ocr: &OcrOptions,
) -> (openconvert::ocr::OcrStage, openconvert::pipeline::TextStage) {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let pdf = backend.open(&bytes, None).expect("opens");
    let mut input = openconvert::input::page_inputs(pdf.as_ref()).expect("extracts");
    let stage = ocr_stage(pdf.as_ref(), &mut input, ocr, None, &T).expect("ingest conserves");
    let mut totals =
        ReasonTotals::default().with_ocr_added(stage.delta.reason_added(Reason::Ocr).total());
    let text = text_stage(&input, &mut totals, &T).expect("text conserves");
    (stage, text)
}

/// Row 13.11. Every run OCR produced says `Ocr`, and no run extraction produced does — on a wholly
/// scanned book and on a page that has both.
#[test]
fn ocr_runs_carry_provenance_ocr() {
    let engine = Arc::new(ScriptedEngine::new(&SCAN_LINES));
    let (_, scanned) = ingest_and_text(&typst("f03_image_only"), &options(&engine));
    let runs: Vec<_> = scanned.pages.iter().flat_map(|page| &page.runs).collect();
    assert!(!runs.is_empty(), "OCR produced runs on f03");
    assert!(
        runs.iter().all(|run| run.provenance == TextProvenance::Ocr),
        "an image-only book's every run is OCR's"
    );
    let text: String = runs
        .iter()
        .map(|run| run.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("A Scanned Chapter"), "{text}");

    let engine = Arc::new(ScriptedEngine::new(&SCAN_LINES));
    let (_, mixed) = ingest_and_text(&typst("f11_mixed_plate"), &options(&engine));
    let (_, plain) = ingest_and_text(&typst("f11_mixed_plate"), &OcrOptions::off());
    let ocr_text: Vec<&str> = mixed
        .pages
        .iter()
        .flat_map(|page| &page.runs)
        .filter(|run| run.provenance == TextProvenance::Ocr)
        .map(|run| run.text.as_str())
        .collect();
    assert_eq!(ocr_text, SCAN_LINES, "OCR's runs are exactly what it read");
    let extracted: Vec<_> = mixed
        .pages
        .iter()
        .flat_map(|page| &page.runs)
        .filter(|run| run.provenance != TextProvenance::Ocr)
        .collect();
    let without: Vec<_> = plain.pages.iter().flat_map(|page| &page.runs).collect();
    assert_eq!(
        extracted.len(),
        without.len(),
        "extraction produced the same runs with OCR on"
    );
    assert!(
        extracted
            .iter()
            .zip(&without)
            .all(|(with, without)| with.text == without.text
                && with.provenance == TextProvenance::Pdf)
    );
}

/// Row 13.12. The `ingest` ledger has `Ocr` entries, every one `Added` and region-scoped, and no
/// `Removed` entry at all without `--re-ocr`; the book balances end to end (I-7) and OCR's
/// characters inflate neither side of retention.
#[test]
fn ledger_ocr_entries_are_added_only() {
    let engine = Arc::new(ScriptedEngine::new(&SCAN_LINES));
    let built = common::build_path_with_ocr(&typst("f03_image_only"), options(&engine));
    let ledger = &built.document.ledger;
    let ingest: Vec<_> = ledger
        .entries
        .iter()
        .filter(|entry| entry.stage == "ingest")
        .collect();

    assert_eq!(ingest.len(), 2, "one region per page: {ingest:?}");
    for entry in &ingest {
        assert_eq!(entry.reason, Reason::Ocr);
        assert!(entry.added, "OCR only adds: {entry:?}");
        assert!(entry.region.is_some(), "and says where: {entry:?}");
    }
    assert!(
        !ledger
            .entries
            .iter()
            .any(|entry| entry.stage == "ingest" && !entry.added),
        "no Removed counterpart without --re-ocr"
    );
    assert_eq!(
        ledger.per_stage_checks.first().map(|check| check.stage),
        Some("ingest"),
        "ingest's check is recorded first"
    );

    // I-7 holds with OCR's additions in it, and the retention ratio has nothing to divide: the book
    // had no source text, and OCR's is not counted as retained.
    let structural = &built.structural;
    assert!(structural.i7.holds(), "{:?}", structural.i7);
    assert_eq!(structural.i7.c0_chars, 0);
    assert_eq!(structural.i7.ocr_chars, c_of(&engine.text()).total() * 2);
    assert_eq!(ledger.ocr_added(), c_of(&format!("{0} {0}", engine.text())));

    // The pages are text now: the pictures OCR replaced are gone, and the book says what it read.
    assert!(
        built.built.emitted.used_images.is_empty(),
        "the scans were replaced by their text"
    );
    let body: String = built
        .content_documents()
        .iter()
        .map(|(_, xhtml)| xhtml.clone())
        .collect();
    assert!(body.contains("It was a dark and stormy night"), "{body}");
    let report = built.ocr.as_ref().expect("the report has an OCR section");
    assert_eq!(report.regions.len(), 2);
    assert!(report
        .regions
        .iter()
        .all(|region| region.outcome == RegionOutcome::Text));
    assert!(report.pages_as_images.is_empty());
    // Full pages are read with automatic segmentation, at `ocr.render_dpi`.
    let asked = engine.asked();
    assert!(asked.iter().all(|asked| asked.psm == Psm::AutoOsd));
    assert!(asked
        .iter()
        .all(|asked| i64::from(asked.dpi) == T.ocr.render_dpi));
}

/// Row 13.14. On a `mixed` page the text is left exactly as extraction found it, and OCR reads
/// only the one image region no text run touches — as one block, `--psm 6`.
#[test]
fn mixed_page_ocrs_only_uncovered_regions() {
    let engine = Arc::new(ScriptedEngine::new(&SCAN_LINES));
    let built = common::build_path_with_ocr(&typst("f11_mixed_plate"), options(&engine));
    let plain = common::build_path_with_ocr(&typst("f11_mixed_plate"), OcrOptions::off());

    let asked = engine.asked();
    assert_eq!(asked.len(), 1, "one uncovered image region: {asked:?}");
    let plate = asked[0].region;
    assert_eq!(asked[0].psm, Psm::SingleBlock);

    // The plate is below the text, and no text run is inside it.
    let bytes = std::fs::read(typst("f11_mixed_plate")).expect("f11");
    let backend = PdfiumBackend::bind().expect("PDFium");
    let pdf = backend.open(&bytes, None).expect("opens");
    let glyphs = pdf.page_glyphs(0).expect("glyphs").glyphs;
    assert!(!glyphs.is_empty());
    assert!(glyphs
        .iter()
        .all(|glyph| !oc_core::ledger_check::overlaps(&glyph.bbox, &plate)));

    // Every OCR entry is the plate's, and I-6 held (the conversion would have stopped otherwise).
    let ocr: Vec<_> = built
        .document
        .ledger
        .entries
        .iter()
        .filter(|entry| entry.reason == Reason::Ocr)
        .collect();
    assert_eq!(ocr.len(), 1);
    let region = ocr[0].region.expect("a region");
    assert!(region.y0 >= plate.y0 - 1.0 && region.y1 <= plate.y1 + 1.0);

    // Text untouched: the book with OCR is the book without it, plus the plate's text.
    let without = oc_validate::epub_chars(&plain.built.bytes).expect("reads");
    let with = oc_validate::epub_chars(&built.built.bytes).expect("reads");
    assert_eq!(with.difference(&without), c_of(&engine.text()));
    assert!(
        without.difference(&with).is_empty(),
        "nothing extracted was lost"
    );
    assert!(built.structural.i7.holds(), "{:?}", built.structural.i7);
    assert!(
        (built.structural.retention - plain.structural.retention).abs() < 1e-6,
        "the plate's text does not change retention: {} vs {}",
        built.structural.retention,
        plain.structural.retention
    );
    assert!(
        built.built.emitted.used_images.is_empty(),
        "the plate was read, so it is text and not also a picture"
    );
    assert_eq!(plain.built.emitted.used_images.len(), 1);
}

/// Row 13.16. An OCR sandwich keeps its own layer by default, as `OcrLayer`, and asks the engine
/// nothing. With `--re-ocr always` the layer is removed under `OcrLayerDuplicate` and the new
/// reading added under `Ocr` — the one place a `Removed` and an `Added` entry are coupled — and I-1
/// balances across the pair.
#[test]
fn re_ocr_replaces_sandwich_layer_conservingly() {
    let sandwich = common::handmade("h05_invisible_layer");

    // Default: the layer is the text.
    let engine = Arc::new(ScriptedEngine::new(&["a new reading"]));
    let (stage, text) = ingest_and_text(&sandwich, &options(&engine));
    assert!(
        engine.asked().is_empty(),
        "--re-ocr never asks the engine nothing"
    );
    assert!(stage.delta.is_empty());
    let runs: Vec<_> = text.pages.iter().flat_map(|page| &page.runs).collect();
    assert!(!runs.is_empty());
    assert!(
        runs.iter()
            .all(|run| run.provenance == TextProvenance::OcrLayer),
        "{runs:?}"
    );

    // `--re-ocr always`: the layer goes, the reading comes, and the pair balances.
    let engine = Arc::new(ScriptedEngine::new(&["a new reading"]));
    let mut re_ocr = options(&engine);
    re_ocr.re_ocr = ReOcr::Always;
    let (stage, text) = ingest_and_text(&sandwich, &re_ocr);
    assert_eq!(engine.asked().len(), 1);
    let entries = stage.delta.entries();
    let removed: Vec<_> = entries.iter().filter(|entry| !entry.added).collect();
    let added: Vec<_> = entries.iter().filter(|entry| entry.added).collect();
    assert_eq!(removed.len(), 1, "{entries:?}");
    assert_eq!(removed[0].reason, Reason::OcrLayerDuplicate);
    assert_eq!(c_of(&removed[0].text), c_of("abcdefghijklmnopqrstuvwxyz"));
    assert_eq!(added.len(), 1);
    assert_eq!(added[0].reason, Reason::Ocr);
    assert_eq!(stage.check.removed_chars, 26);
    assert_eq!(stage.check.added_chars, c_of("a new reading").total());
    let runs: Vec<_> = text.pages.iter().flat_map(|page| &page.runs).collect();
    assert!(runs.iter().all(|run| run.provenance == TextProvenance::Ocr));

    // End to end: the book reads the new text, and I-7 holds — the layer's removal is folded into
    // `C_0` as dedup is (ARCHITECTURE §5.2), and the reading is an addition.
    let built = common::build_path_with_ocr(&sandwich, re_ocr);
    assert!(built.structural.i7.holds(), "{:?}", built.structural.i7);
    let body: String = built
        .content_documents()
        .iter()
        .map(|(_, xhtml)| xhtml.clone())
        .collect();
    assert!(body.contains("a new reading"), "{body}");
    assert!(
        !body.contains("abcdefghijklmnopqrstuvwxyz"),
        "the old layer is gone"
    );

    // A failed re-OCR leaves the layer where it was: never a page with neither.
    let failing = Arc::new(ScriptedEngine::new(&["unused"]).failing(OcrError::Exit {
        status: "exit status: 1".to_owned(),
    }));
    let mut options = options(&failing);
    options.re_ocr = ReOcr::Always;
    let (stage, text) = ingest_and_text(&sandwich, &options);
    assert!(stage.delta.is_empty());
    assert!(text
        .pages
        .iter()
        .flat_map(|page| &page.runs)
        .all(|run| run.provenance == TextProvenance::OcrLayer));
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("openconvert-ocr-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

fn events_of(stderr: &[u8]) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(stderr)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|e| panic!("{line:?}: {e}")))
        .collect()
}

/// Row 13.17 and A13.2. No Tesseract: the conversion still succeeds (exit 0), the scanned pages
/// are in the book as pictures, and `W_OCR_ENGINE_MISSING` carries this platform's install hint.
#[test]
fn missing_engine_emits_install_hint_and_page_images() {
    let dir = scratch("missing");
    let output = dir.join("out.epub");
    let run = Command::new(env!("CARGO_BIN_EXE_openconvert"))
        .arg("convert")
        .arg(typst("f03_image_only"))
        .args(["-o", &output.display().to_string()])
        // An explicit path replaces discovery, so a machine that has Tesseract still has none here.
        .args([
            "--ocr-path",
            &dir.join("nowhere/tesseract").display().to_string(),
        ])
        .args(["--progress", "json", "--modified", common::FIXED_MODIFIED])
        .output()
        .expect("the binary runs");
    assert_eq!(
        run.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );

    let events = events_of(&run.stderr);
    assert_eq!(events[0]["t"], "hello");
    let capabilities = events[0]["capabilities"].as_array().expect("capabilities");
    assert!(
        !capabilities
            .iter()
            .any(|c| c.as_str().is_some_and(|c| c.starts_with("ocr:"))),
        "no engine, no OCR capability: {capabilities:?}"
    );
    let missing: Vec<_> = events
        .iter()
        .filter(|event| event["t"] == "warning" && event["code"] == W_OCR_ENGINE_MISSING)
        .collect();
    assert_eq!(missing.len(), 1, "{events:?}");
    assert_eq!(
        missing[0]["args"]["hint"],
        engine_install_hint(Os::current()),
        "a copy-pasteable hint for this platform"
    );
    assert_eq!(missing[0]["args"]["pages"], "1, 2");
    assert_eq!(
        missing[0]["args"]["reason"],
        OcrUnavailable::NotFound.to_string()
    );

    // The pages are pictures in the book.
    let bytes = std::fs::read(&output).expect("the EPUB was written");
    let entries = oc_epub::read_entries(&oc_epub::EpubBytes(bytes)).expect("reads");
    let images = entries
        .keys()
        .filter(|path| path.ends_with(".jpg") || path.ends_with(".png"))
        // The cover is a rendering of the first page, not one of the pages carried as a picture.
        .filter(|path| !path.starts_with("images/cover."))
        .count();
    assert_eq!(images, 2, "{:?}", entries.keys().collect::<Vec<_>>());
    let report: serde_json::Value = serde_json::from_slice(
        &std::fs::read(dir.join("out.epub.report.json")).expect("the report"),
    )
    .expect("json");
    assert_eq!(report["ocr"]["pages_as_images"], serde_json::json!([0, 1]));
    assert!(report["ocr"]["engine"].is_null());

    // Every platform's hint is a different, concrete instruction.
    let hints = [Os::Linux, Os::MacOs, Os::Windows].map(engine_install_hint);
    assert!(hints[0].contains("apt install tesseract-ocr"));
    assert!(hints[1].contains("brew install tesseract"));
    assert!(hints[2].contains("UB-Mannheim"));

    // And `--ocr never` asks for nothing and warns about nothing: the user said so.
    let quiet = Command::new(env!("CARGO_BIN_EXE_openconvert"))
        .arg("convert")
        .arg(typst("f03_image_only"))
        .args(["-o", &dir.join("never.epub").display().to_string()])
        .args(["--ocr", "never", "--progress", "json"])
        .output()
        .expect("the binary runs");
    assert_eq!(quiet.status.code(), Some(0));
    assert!(!events_of(&quiet.stderr)
        .iter()
        .any(|event| event["code"] == W_OCR_ENGINE_MISSING));
}

/// A region under `ocr.region_conf_min` keeps its text and gets its picture back beside it
/// (detail 9); a region the engine failed on stays a picture with `W_OCR_FAILED`, and the book
/// completes either way.
#[test]
fn a_doubtful_or_failed_region_keeps_its_picture() {
    let doubtful = Arc::new(ScriptedEngine::new(&SCAN_LINES).with_conf(0.3));
    let built = common::build_path_with_ocr(&typst("f03_image_only"), options(&doubtful));
    assert_eq!(
        built.built.emitted.used_images.len(),
        2,
        "both pictures stay"
    );
    let body: String = built
        .content_documents()
        .iter()
        .map(|(_, xhtml)| xhtml.clone())
        .collect();
    // (The first line is the same on both pages, so `furniture` rightly takes it as a running
    // head; the body line is what shows the text stayed.)
    assert!(
        body.contains("It was a dark and stormy night"),
        "and so does the text: {body}"
    );
    let low: Vec<_> = built
        .document
        .warnings
        .iter()
        .filter(|w| w.code == W_OCR_LOW_CONFIDENCE)
        .collect();
    assert_eq!(low.len(), 2);
    assert_eq!(
        low[0].args.get("confidence").map(String::as_str),
        Some("0.30")
    );

    let failing = Arc::new(ScriptedEngine::new(&SCAN_LINES).failing(OcrError::Exit {
        status: "signal: 11".to_owned(),
    }));
    let built = common::build_path_with_ocr(&typst("f03_image_only"), options(&failing));
    assert_eq!(built.built.emitted.used_images.len(), 2);
    assert_eq!(
        built
            .document
            .warnings
            .iter()
            .filter(|w| w.code == W_OCR_FAILED)
            .count(),
        2
    );
    assert!(built.document.ledger.ocr_added().is_empty());

    // `--ocr never` with an engine present: nothing is read.
    let engine = Arc::new(ScriptedEngine::new(&SCAN_LINES));
    let mut never = options(&engine);
    never.mode = OcrMode::Never;
    let built = common::build_path_with_ocr(&typst("f03_image_only"), never);
    assert!(engine.asked().is_empty());
    assert_eq!(built.built.emitted.used_images.len(), 2);
    assert!(built.ocr.is_none());
}

/// Row 13.18 and A13.5: a `tesseract` that hangs is killed at the deadline, its region stays a
/// picture, the book completes, and no process is left behind. Unix: the hanging engine is a script.
#[cfg(unix)]
#[test]
fn hung_tesseract_is_killed_at_deadline() {
    use oc_core::ocr::discover::{DiscoverySource, TesseractInfo, Version};
    use oc_core::ocr::invoke::Tesseract;
    use oc_testkit::fake_tesseract::{FakeConfig, FakeTesseract};

    let dir = scratch("hung");
    let fake = FakeTesseract::install(
        &dir.join("bin"),
        &FakeConfig {
            sleep_secs: 120,
            ..FakeConfig::default()
        },
    );
    let info = TesseractInfo {
        path: fake.path.clone(),
        version: Version {
            major: 5,
            minor: 3,
            patch: 4,
        },
        langs: ["eng".to_owned()].into(),
        source: DiscoverySource::ConfigPath,
    };
    // The pipeline's deadline is `ocr.region_deadline_secs`; a test waits a fraction of it.
    let deadline = std::time::Duration::from_millis(600);
    assert!(deadline.as_secs() < u64::try_from(T.ocr.region_deadline_secs).unwrap_or(0));
    let engine: Arc<dyn OcrEngine> = Arc::new(Tesseract::new(info, &dir.join("work"), deadline));

    let started = std::time::Instant::now();
    let built =
        common::build_path_with_ocr(&typst("f03_image_only"), OcrOptions::auto(Ok(engine), &T));
    let took = started.elapsed();

    assert_eq!(
        built.built.emitted.used_images.len(),
        2,
        "each region degraded to its picture"
    );
    let failed: Vec<_> = built
        .document
        .warnings
        .iter()
        .filter(|w| w.code == W_OCR_FAILED)
        .collect();
    assert_eq!(failed.len(), 2);
    assert!(
        failed[0]
            .args
            .get("reason")
            .is_some_and(|reason| reason.contains("still running")),
        "{failed:?}"
    );
    assert!(
        took < std::time::Duration::from_secs(60),
        "the book completed: {took:?}"
    );
    let pid = fake.last_pid().expect("the fake ran");
    let gone = (0..100).any(|_| {
        std::thread::sleep(std::time::Duration::from_millis(20));
        !std::path::Path::new(&format!("/proc/{pid}")).exists()
            || std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| {
                stat.rsplit_once(") ")
                    .is_some_and(|(_, rest)| rest.starts_with('Z'))
            })
    });
    #[cfg(target_os = "linux")]
    assert!(gone, "the hung tesseract was not left running");
    let _ = gone;
    assert!(
        std::fs::read_dir(dir.join("work")).map_or(true, |mut dir| dir.next().is_none()),
        "no raster left behind"
    );
}

/// A model that answers nothing and counts what it was asked: the stand-in for `--ai` in 13.22.
struct CountingModel {
    calls: std::sync::atomic::AtomicUsize,
}

impl oc_ai::provider::LlmProvider for CountingModel {
    fn id(&self) -> &str {
        "counting-test-model"
    }
    fn capabilities(&self) -> oc_ai::provider::ProviderCaps {
        oc_ai::provider::ProviderCaps {
            constraint: oc_ai::provider::Constraint::Gbnf,
        }
    }
    fn thinking_control(&self) -> oc_ai::provider::ThinkingControl {
        oc_ai::provider::ThinkingControl::None
    }
    fn complete(
        &self,
        _request: &oc_ai::provider::LlmRequest,
    ) -> Result<oc_ai::provider::LlmResponse, oc_ai::provider::LlmError> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err(oc_ai::provider::LlmError::Protocol(
            "the test model answers nothing".to_owned(),
        ))
    }
}

/// An OCR engine that records, at every call, how many model calls had been made by then.
struct Witness<'a> {
    inner: ScriptedEngine,
    model: &'a CountingModel,
    seen: std::sync::Mutex<Vec<usize>>,
}

impl OcrEngine for Witness<'static> {
    fn capability(&self) -> String {
        self.inner.capability()
    }
    fn langs(&self) -> &std::collections::BTreeSet<String> {
        self.inner.langs()
    }
    fn recognize(
        &self,
        request: &oc_core::ocr::invoke::OcrRequest,
    ) -> Result<Vec<oc_core::ocr::OcrWord>, OcrError> {
        let so_far = self.model.calls.load(std::sync::atomic::Ordering::SeqCst);
        self.seen.lock().expect("the log").push(so_far);
        self.inner.recognize(request)
    }
}

/// Row 13.22 (D16, R10 §6.14). With `--ai` — every task, every language — and a scanned book, no
/// model call originates from `ingest`: every OCR call happens with the model not yet asked
/// anything, OCR has no path to a provider at all (neither OCR module names one, and `oc-core`,
/// where the engine adapter lives, does not depend on `oc-ai`), and no `ingest` decision carries a
/// model trace.
#[test]
fn ocr_never_calls_the_llm() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut sources = vec![root.join("crates/openconvert/src/ocr.rs")];
    for entry in std::fs::read_dir(root.join("crates/oc-core/src/ocr")).expect("the OCR module") {
        sources.push(entry.expect("an entry").path());
    }
    sources.push(root.join("crates/oc-core/src/sidecar/tesseract.rs"));
    for path in &sources {
        let text = std::fs::read_to_string(path).expect("readable");
        for banned in [
            "oc_ai",
            "LlmProvider",
            "llm::",
            "OpenAiCompatible",
            "cassette",
        ] {
            assert!(
                !text.contains(banned),
                "{} mentions {banned}: OCR must not reach a model",
                path.display()
            );
        }
    }
    let lock = std::fs::read_to_string(root.join("Cargo.lock")).expect("Cargo.lock");
    let core = lock
        .split("[[package]]")
        .find(|package| package.contains("name = \"oc-core\""))
        .expect("oc-core is in the lock");
    assert!(
        !core.contains("\"oc-ai\""),
        "oc-core depends on oc-ai:\n{core}"
    );

    // `--ai --ai-all-tasks` over a scanned book.
    let model: &'static CountingModel = Box::leak(Box::new(CountingModel {
        calls: std::sync::atomic::AtomicUsize::new(0),
    }));
    let witness = Arc::new(Witness {
        inner: ScriptedEngine::new(&SCAN_LINES),
        model,
        seen: std::sync::Mutex::new(Vec::new()),
    });
    let clock = oc_ai::session::SystemClock::new();
    let context = openconvert::ai::AiContext {
        provider: model,
        cache: None,
        clock: &clock,
        started_ms: oc_ai::session::Clock::now_ms(&clock),
        all_tasks: true,
    };
    let path = typst("f03_image_only");
    let bytes = std::fs::read(&path).expect("f03");
    let backend = PdfiumBackend::bind().expect("PDFium");
    let pdf = backend.open(&bytes, None).expect("opens");
    let options = openconvert::convert::ConvertOptions {
        filename: "f03_image_only.pdf".to_owned(),
        language: Some(oc_model::lang::LangTag::EN),
        preset: oc_model::document::PresetName::Auto,
        epub: common::epub_options(),
        ocr: OcrOptions::auto(Ok(witness.clone() as Arc<dyn OcrEngine>), &T),
        overrides: None,
        cache_dir: None,
    };
    let conversion = openconvert::convert::convert_with_ai(
        pdf.as_ref(),
        &openconvert::convert::sha256_hex(&bytes),
        &options,
        Some(&context),
        &T,
    )
    .expect("converts");

    let seen = witness.seen.lock().expect("the log").clone();
    assert_eq!(seen.len(), 2, "both pages were read");
    assert!(
        seen.iter().all(|calls| *calls == 0),
        "the model was asked something before OCR finished: {seen:?}"
    );
    assert!(conversion.ai.is_some(), "the AI step ran");
    assert!(
        !conversion
            .document
            .decisions
            .iter()
            .any(|decision| decision.stage == "ingest"),
        "{:?}",
        conversion.document.decisions
    );
    assert!(
        conversion.structural.i7.holds(),
        "{:?}",
        conversion.structural.i7
    );
}

/// PHASE 13 detail 1: discovery runs once, before anything is read, and `hello` says what it found
/// — `ocr:tesseract-<version>` — so a supervisor knows whether a scanned book will be read before it
/// asks. Unix: the engine on `--ocr-path` is the fake script.
#[cfg(unix)]
#[test]
fn hello_reports_the_discovered_engine() {
    use oc_testkit::fake_tesseract::{tsv_line, FakeConfig, FakeTesseract};

    let dir = scratch("hello");
    let fake = FakeTesseract::install(
        &dir.join("bin"),
        &FakeConfig {
            banner: "tesseract 5.4.1".to_owned(),
            tsv: tsv_line(&[("Scanned", 95.0), ("text", 94.0)]),
            ..FakeConfig::default()
        },
    );
    let run = Command::new(env!("CARGO_BIN_EXE_openconvert"))
        .arg("convert")
        .arg(typst("f03_image_only"))
        .args(["-o", &dir.join("out.epub").display().to_string()])
        .args(["--ocr-path", &fake.path.display().to_string()])
        .args(["--ocr-lang", "deu+eng", "--progress", "json"])
        .output()
        .expect("the binary runs");
    assert_eq!(
        run.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let events = events_of(&run.stderr);
    assert_eq!(events[0]["t"], "hello");
    assert!(
        events[0]["capabilities"]
            .as_array()
            .is_some_and(|all| all.iter().any(|c| c == "ocr:tesseract-5.4.1")),
        "{:?}",
        events[0]
    );
    // Both pages were read, with the languages asked for, and the work directory is gone.
    let calls = fake.ocr_calls();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert!(calls
        .iter()
        .all(|argv| argv.windows(2).any(|w| w == ["-l", "deu+eng"])));
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .expect("dir")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains(".oc-tmp-"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

/// A13.7: a Tesseract 4.x is refused as too old, and the book converts exactly as if there were no
/// engine — exit 0, pages as pictures, `W_OCR_ENGINE_MISSING` saying which version was found — and
/// the old binary is asked for its version and nothing else. Unix: the old engine is the fake script.
#[cfg(unix)]
#[test]
fn an_old_tesseract_converts_as_if_none_existed() {
    use oc_testkit::fake_tesseract::{FakeConfig, FakeTesseract};

    let dir = scratch("old-engine");
    let fake = FakeTesseract::install(
        &dir.join("bin"),
        &FakeConfig {
            banner: "tesseract 4.1.1".to_owned(),
            ..FakeConfig::default()
        },
    );
    let run = Command::new(env!("CARGO_BIN_EXE_openconvert"))
        .arg("convert")
        .arg(typst("f03_image_only"))
        .args(["-o", &dir.join("out.epub").display().to_string()])
        .args(["--ocr-path", &fake.path.display().to_string()])
        .args(["--progress", "json"])
        .output()
        .expect("the binary runs");
    assert_eq!(
        run.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let events = events_of(&run.stderr);
    let missing: Vec<_> = events
        .iter()
        .filter(|event| event["code"] == W_OCR_ENGINE_MISSING)
        .collect();
    assert_eq!(missing.len(), 1, "{events:?}");
    assert!(
        missing[0]["args"]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("4.1.1")),
        "{:?}",
        missing[0]
    );
    assert_eq!(fake.invocations(), [vec!["--version".to_owned()]]);
    let bytes = std::fs::read(dir.join("out.epub")).expect("written");
    let entries = oc_epub::read_entries(&oc_epub::EpubBytes(bytes)).expect("reads");
    assert_eq!(
        entries.keys().filter(|path| path.ends_with(".jpg")).count(),
        2
    );
}
