//! PHASE 10: the AI step inside a real conversion — the fixtures, the gates, the budgets, the
//! cache — with an in-process model that answers every task from its own payload.
//!
//! No model is reachable where Phase 10 was built, so the "model" here is [`Echo`]: it reads each
//! question and answers it plausibly, in the task's exact wire shape, the way a cooperative model
//! would. That is what the pipeline needs to be exercised end to end: answers that pass gate S and
//! reach gates L and V and the conservation check. Whether a *real* model's answers are good is
//! the evaluation's question (`docs/AI_EVALUATION.md`), not this file's.

mod common;

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use oc_ai::provider::{
    Constraint, LlmError, LlmProvider, LlmRequest, LlmResponse, ProviderCaps, Purpose,
    ThinkingControl,
};
use oc_ai::session::{Clock, SystemClock};
use oc_core::thresholds::T;
use oc_model::confidence::Method;
use oc_pdf::inspect::PdfOpen;
use openconvert::ai::AiContext;
use openconvert::convert::{convert_prepared, prepare, sha256_hex, Conversion, ConvertOptions};

const FIXTURES: [&str; 10] = [
    "f01_prose_single_column",
    "f02_two_column",
    "f03_image_only",
    "f04_german_prose",
    "f05_turkish_prose",
    "f06_hyphenation_de",
    "f07_verse_and_quote",
    "f08_footnotes",
    "f09_novel_structure",
    "f10_lists_and_table",
];

/// A cooperative model: answers every task in its wire shape, from the question itself.
struct Echo {
    calls: AtomicUsize,
    /// Milliseconds each answer takes on `clock`, when there is one.
    clock: Option<(&'static FakeClock, u64)>,
    purposes: std::sync::Mutex<Vec<Purpose>>,
}

impl Echo {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            clock: None,
            purposes: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn asked(&self, purpose: Purpose) -> usize {
        self.purposes
            .lock()
            .map(|purposes| purposes.iter().filter(|seen| **seen == purpose).count())
            .unwrap_or_default()
    }
}

fn payload(request: &LlmRequest) -> serde_json::Value {
    request
        .user
        .lines()
        .find(|line| line.starts_with('{'))
        .and_then(|line| serde_json::from_str(line).ok())
        .unwrap_or(serde_json::Value::Null)
}

/// A line of the metadata question without its tags.
fn untagged(line: &str) -> &str {
    let mut rest = line;
    while let Some(tail) = rest.strip_prefix('[') {
        match tail.split_once(']') {
            Some((_, after)) => rest = after,
            None => break,
        }
    }
    rest.trim()
}

impl LlmProvider for Echo {
    fn id(&self) -> &str {
        "echo-test-model"
    }
    fn capabilities(&self) -> ProviderCaps {
        ProviderCaps {
            constraint: Constraint::Gbnf,
        }
    }
    fn thinking_control(&self) -> ThinkingControl {
        ThinkingControl::None
    }
    fn complete(&self, request: &LlmRequest) -> Result<LlmResponse, LlmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut purposes) = self.purposes.lock() {
            purposes.push(request.purpose);
        }
        if let Some((clock, ms)) = self.clock {
            clock.advance(ms);
        }
        let text = match request.purpose {
            // The tenth option, whichever it is: asked twice in opposite orders the two answers
            // do not agree, and the page stays as the rules typed it.
            Purpose::FrontPage => "J".to_owned(),
            Purpose::Metadata => {
                let title = request
                    .user
                    .lines()
                    .map(untagged)
                    .find(|line| !line.is_empty() && !line.starts_with("Extract"))
                    .unwrap_or("Untitled");
                serde_json::json!({
                    "title": title, "subtitle": null, "authors": [], "translator": null,
                    "publisher": null, "date": null
                })
                .to_string()
            }
            Purpose::HeadingRoles => {
                let payload = payload(request);
                let role = |value: &serde_json::Value| {
                    let size_z = value["size_z"].as_f64().unwrap_or(0.0);
                    if size_z > 0.5 || value["weight"] == "bold" {
                        "section_heading"
                    } else {
                        "body"
                    }
                };
                let m: Vec<serde_json::Value> = payload["clusters"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|cluster| serde_json::json!({"c": cluster["c"], "r": role(cluster)}))
                    .collect();
                let h: Vec<serde_json::Value> = payload["holdout"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|probe| serde_json::json!({"i": probe["i"], "r": role(probe)}))
                    .collect();
                serde_json::json!({"m": m, "h": h}).to_string()
            }
            Purpose::BookStructure => {
                let n = payload(request)["headings"].as_array().map_or(0, Vec::len);
                serde_json::json!({
                    "frontmatter_end_idx": 0, "part_boundaries": [], "backmatter_start_idx": n
                })
                .to_string()
            }
            Purpose::VerseQuote => {
                let b: Vec<serde_json::Value> = payload(request)["blocks"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|block| serde_json::json!({"id": block["id"], "k": "paragraph"}))
                    .collect();
                serde_json::json!({ "b": b }).to_string()
            }
        };
        Ok(LlmResponse {
            text,
            reasoning: None,
            tokens_in: 400,
            tokens_out: 40,
            cached: false,
            cached_tokens: Some(300),
            finish_reason: Some("stop".to_owned()),
        })
    }
}

/// A model that must not be asked.
struct Panicking;

impl LlmProvider for Panicking {
    fn id(&self) -> &str {
        "echo-test-model"
    }
    fn capabilities(&self) -> ProviderCaps {
        ProviderCaps {
            constraint: Constraint::Gbnf,
        }
    }
    fn thinking_control(&self) -> ThinkingControl {
        ThinkingControl::None
    }
    fn complete(&self, request: &LlmRequest) -> Result<LlmResponse, LlmError> {
        panic!("{:?} reached the model with a warm cache", request.purpose)
    }
}

/// A clock a test moves.
#[derive(Default)]
struct FakeClock(AtomicU64);

impl FakeClock {
    fn advance(&self, ms: u64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for FakeClock {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

fn options(stem: &str) -> ConvertOptions {
    ConvertOptions {
        filename: format!("{stem}.pdf"),
        language: Some(oc_model::lang::LangTag::EN),
        preset: oc_model::document::PresetName::Auto,
        epub: common::epub_options(),
        ocr: openconvert::ocr::OcrOptions::off(),
        overrides: None,
        cache_dir: None,
    }
}

/// How a test wants a fixture's evidence changed before `structure` reads it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Evidence {
    /// As the file has it.
    AsIs,
    /// No outline, and only a Word export's title declared: every book-level predicate fires.
    Stripped,
}

/// Convert a fixture, with the AI step when `ai` is given.
fn convert_fixture(stem: &str, evidence: Evidence, ai: Option<&AiContext<'_>>) -> Conversion {
    let bytes = std::fs::read(common::fixture(stem)).unwrap_or_else(|error| {
        panic!("missing fixture {stem}: {error}; run `cargo run -p xtask -- fixtures`")
    });
    let backend = oc_pdf::pdfium::PdfiumBackend::bind().expect("PDFium is vendored");
    let pdf = backend.open(&bytes, None).expect("the fixture opens");
    let sha = sha256_hex(&bytes);
    let options = options(stem);
    let mut prepared = prepare(pdf.as_ref(), &sha, &options, &T).expect("the fixture prepares");
    if evidence == Evidence::Stripped {
        prepared.structure_input.outline.clear();
        prepared.structure_input.meta.xmp.title = None;
        prepared.structure_input.meta.info.title = Some("Microsoft Word - draft.docx".to_owned());
    }
    convert_prepared(pdf.as_ref(), &sha, &options, prepared, ai, &T)
        .unwrap_or_else(|error| panic!("{stem} did not convert: {error}"))
}

/// Row 10.14 / A10.3. With AI on — every task, every language — I-7 still holds on every
/// fixture, as the file has it and with its outline and title taken away so every book-level
/// predicate fires. And the step is not vacuous: across the fixtures the model's answers are
/// applied, not only refused.
#[test]
fn ai_edits_are_conserving_end_to_end() {
    let clock = SystemClock::new();
    let mut applied = 0;
    for stem in FIXTURES {
        for evidence in [Evidence::AsIs, Evidence::Stripped] {
            let echo = Echo::new();
            let context = AiContext {
                provider: &echo,
                cache: None,
                clock: &clock,
                all_tasks: true,
                mode: oc_core::jobspec::AiMode::default(),
            };
            let conversion = convert_fixture(stem, evidence, Some(&context));
            assert!(
                conversion.structural.i7.holds(),
                "{stem}: I-7 fails with AI on: {} missing, {} extra",
                conversion.structural.i7.missing.total(),
                conversion.structural.i7.extra.total()
            );
            let outcome = conversion.ai.as_ref().expect("the AI step ran");
            assert!(
                outcome.calls.len() <= 8,
                "{stem}: {} calls",
                outcome.calls.len()
            );
            applied += conversion
                .document
                .decisions
                .iter()
                .filter(|decision| decision.method == Method::Llm)
                .count();
        }
    }
    assert!(applied > 0, "no model answer was applied anywhere");
}

/// A10.2. A book with no outline and a Word export's title: with `--ai --ai-all-tasks`, the
/// book asks at most `llm.max_calls_per_book`, stays under the wall-clock share, every escalated
/// choice ends in a `Decision` naming how it was settled, and the title the model read off the
/// title page is the book's `dc:title` — with `source = llm`.
#[test]
fn a_book_with_no_outline_and_boilerplate_metadata() {
    let clock = SystemClock::new();
    let echo = Echo::new();
    let context = AiContext {
        provider: &echo,
        cache: None,
        clock: &clock,
        all_tasks: true,
        mode: oc_core::jobspec::AiMode::default(),
    };
    let conversion = convert_fixture("f09_novel_structure", Evidence::Stripped, Some(&context));
    let outcome = conversion.ai.as_ref().expect("the AI step ran");
    assert!(outcome.calls.len() <= usize::try_from(T.llm.max_calls_per_book).unwrap_or(0));
    assert!(
        echo.asked(Purpose::Metadata) == 1,
        "the title is escalated and asked"
    );

    let kinds: std::collections::BTreeSet<&str> = conversion
        .document
        .decisions
        .iter()
        .map(|decision| decision.kind)
        .collect();
    assert!(kinds.contains("metadata"), "{kinds:?}");
    let metadata = conversion
        .document
        .decisions
        .iter()
        .find(|decision| decision.kind == "metadata")
        .expect("a metadata decision");
    assert_eq!(metadata.method, Method::Llm, "{metadata:?}");
    assert_eq!(
        conversion.document.meta.source,
        oc_model::doc::MetaSource::Llm
    );
    assert!(conversion
        .document
        .meta
        .title
        .as_deref()
        .is_some_and(|title| !title.contains("Microsoft Word")));
    let opf = String::from_utf8_lossy(
        oc_epub::read_entries(&conversion.built.bytes)
            .expect("the container reads back")
            .get(oc_epub::opf::OPF_PATH)
            .map(Vec::as_slice)
            .unwrap_or_default(),
    )
    .into_owned();
    let title = conversion.document.meta.title.clone().unwrap_or_default();
    assert!(opf.contains(&format!(">{title}</dc:title>")), "{opf}");
    assert!(conversion.structural.i7.holds());
}

/// Row 10.2, with AI on: `f07` carries an outline, so book structure is never asked, whatever
/// else is.
#[test]
fn book_structure_is_never_asked_when_an_outline_exists() {
    let clock = SystemClock::new();
    let echo = Echo::new();
    let context = AiContext {
        provider: &echo,
        cache: None,
        clock: &clock,
        all_tasks: true,
        mode: oc_core::jobspec::AiMode::default(),
    };
    let conversion = convert_fixture("f07_verse_and_quote", Evidence::AsIs, Some(&context));
    assert_eq!(echo.asked(Purpose::BookStructure), 0);
    assert!(
        echo.asked(Purpose::VerseQuote) > 0,
        "the ambiguous block is asked about"
    );
    assert!(conversion
        .document
        .decisions
        .iter()
        .all(|decision| decision.kind != "book_structure"));
}

/// Row 10.15. `f10` without its outline and title escalates three tasks. The model takes ten
/// minutes a call on an injected clock, so after its first answer the book's time budget
/// (`llm.quality_max_budget_secs` at most) is spent: the remaining LLM work is abandoned, the conversion
/// completes deterministically, `W_LLM_TIME_EXHAUSTED` is in the report, and the choices left
/// unasked say `budget.time`.
#[test]
fn time_budget_hard_stop() {
    static CLOCK: FakeClock = FakeClock(AtomicU64::new(0));
    let echo = Echo {
        clock: Some((&CLOCK, 600_000)),
        ..Echo::new()
    };
    let context = AiContext {
        provider: &echo,
        cache: None,
        clock: &CLOCK,
        all_tasks: true,
        mode: oc_core::jobspec::AiMode::default(),
    };
    let conversion = convert_fixture("f10_lists_and_table", Evidence::Stripped, Some(&context));
    assert_eq!(
        echo.calls.load(Ordering::SeqCst),
        1,
        "one call, then the stop"
    );
    assert!(conversion
        .document
        .warnings
        .iter()
        .any(|warning| warning.code == "W_LLM_TIME_EXHAUSTED"));
    assert!(conversion
        .document
        .decisions
        .iter()
        .any(|decision| decision.fallback == Some("budget.time")));
    assert!(conversion.structural.i7.holds(), "and the book is whole");
}

/// Row 10.20 (D13.8). Two AI runs of one book with a warm cache produce byte-identical EPUBs:
/// the second run's model panics if it is asked anything, so every answer is the cache's.
#[test]
fn cache_hit_makes_ai_run_byte_identical() {
    let root = std::env::temp_dir().join(format!("oc-ai-cache-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let cache = oc_ai::cache::FileCache::new(&root);
    let clock = SystemClock::new();

    let echo = Echo::new();
    let cold = convert_fixture(
        "f09_novel_structure",
        Evidence::Stripped,
        Some(&AiContext {
            provider: &echo,
            cache: Some(&cache),
            clock: &clock,
            all_tasks: true,
            mode: oc_core::jobspec::AiMode::default(),
        }),
    );
    assert!(echo.calls.load(Ordering::SeqCst) > 0);

    let warm = convert_fixture(
        "f09_novel_structure",
        Evidence::Stripped,
        Some(&AiContext {
            provider: &Panicking,
            cache: Some(&cache),
            clock: &clock,
            all_tasks: true,
            mode: oc_core::jobspec::AiMode::default(),
        }),
    );
    let outcome = warm.ai.as_ref().expect("the AI step ran");
    assert!(outcome.calls.iter().all(|call| call.cached));
    assert_eq!(
        common::sha256_hex_of(cold.built.bytes.as_slice()),
        common::sha256_hex_of(warm.built.bytes.as_slice())
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// `--ai` alone, with the shipped language maps: no task is enabled for any language yet, so
/// nothing is asked, every escalation is recorded with `language.gate`, and the book is the
/// deterministic one byte for byte.
#[test]
fn ai_without_all_tasks_asks_nothing_until_a_language_is_enabled() {
    let clock = SystemClock::new();
    let echo = Echo::new();
    let context = AiContext {
        provider: &echo,
        cache: None,
        clock: &clock,
        all_tasks: false,
        mode: oc_core::jobspec::AiMode::default(),
    };
    let with_ai = convert_fixture("f07_verse_and_quote", Evidence::AsIs, Some(&context));
    let without = convert_fixture("f07_verse_and_quote", Evidence::AsIs, None);
    assert_eq!(echo.calls.load(Ordering::SeqCst), 0);
    let gated: Vec<_> = with_ai
        .document
        .decisions
        .iter()
        .filter(|decision| decision.fallback == Some("language.gate"))
        .collect();
    assert!(!gated.is_empty());
    assert!(gated.iter().all(|decision| decision.llm.is_none()));
    assert_eq!(
        common::sha256_hex_of(with_ai.built.bytes.as_slice()),
        common::sha256_hex_of(without.built.bytes.as_slice())
    );
}

/// Row 14.20 (A14.7, D13.9), the CI gate's test: the `--ai` path completes with no network at all.
///
/// The no-network job runs this file under `unshare -n` with `OC_EXPECT_NO_NETWORK=1`, and the test
/// first proves the namespace is real — an outbound connect fails and loopback is the only
/// interface — so a green job is not a job that happened to have a network. Then the AI path runs twice more from
/// what is on disk: a warm answer cache (the model panics if asked), and the committed cassettes
/// through `Replay`, a provider that holds no transport and answers from files. Both complete, and
/// the warm run is byte-identical to the cold one.
#[test]
fn unshare_n_covers_the_ai_cassette_path() {
    if std::env::var("OC_EXPECT_NO_NETWORK").is_ok_and(|value| value == "1") {
        let outbound = std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::from(([1, 1, 1, 1], 443)),
            std::time::Duration::from_secs(2),
        );
        assert!(
            outbound.is_err(),
            "a socket reached the network: not an empty namespace"
        );
        #[cfg(target_os = "linux")]
        {
            let devices = std::fs::read_to_string("/proc/net/dev").expect("/proc/net/dev");
            let interfaces: Vec<&str> = devices
                .lines()
                .skip(2)
                .filter_map(|line| line.split(':').next())
                .map(str::trim)
                .collect();
            assert_eq!(
                interfaces,
                ["lo"],
                "an interface besides loopback: not an empty namespace"
            );
        }
    }

    let root = std::env::temp_dir().join(format!("oc-ai-offline-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let cache = oc_ai::cache::FileCache::new(&root);
    let clock = SystemClock::new();
    let context = |provider: &'static dyn LlmProvider| AiContext {
        provider,
        cache: Some(&cache),
        clock: &clock,
        all_tasks: true,
        mode: oc_core::jobspec::AiMode::default(),
    };

    // Cold, from the in-process model: fills the cache.
    let echo: &'static Echo = Box::leak(Box::new(Echo::new()));
    let cold = convert_fixture(
        "f09_novel_structure",
        Evidence::Stripped,
        Some(&context(echo)),
    );
    assert!(
        echo.calls.load(Ordering::SeqCst) > 0,
        "the cold run asked the model"
    );

    // Warm: every answer from the cache, none from a model.
    let warm = convert_fixture(
        "f09_novel_structure",
        Evidence::Stripped,
        Some(&context(&Panicking)),
    );
    let outcome = warm.ai.as_ref().expect("the AI step ran");
    assert!(!outcome.calls.is_empty() && outcome.calls.iter().all(|call| call.cached));
    assert_eq!(
        common::sha256_hex_of(cold.built.bytes.as_slice()),
        common::sha256_hex_of(warm.built.bytes.as_slice())
    );

    // The cassettes: whatever they hold and whatever they miss, the conversion completes whole.
    let cassettes =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../oc-ai/tests/cassettes");
    let replay: &'static oc_ai::cassette::Replay = Box::leak(Box::new(
        oc_ai::cassette::Replay::new(&cassettes, oc_ai::cassette::STUB_MODEL),
    ));
    let uncached = AiContext {
        provider: replay,
        cache: None,
        clock: &clock,
        all_tasks: true,
        mode: oc_core::jobspec::AiMode::default(),
    };
    let replayed = convert_fixture("f09_novel_structure", Evidence::Stripped, Some(&uncached));
    assert!(replayed.ai.is_some(), "the AI step ran on the cassettes");
    assert!(replayed.structural.i7.holds(), "and the book is whole");
    let _ = std::fs::remove_dir_all(&root);
}
