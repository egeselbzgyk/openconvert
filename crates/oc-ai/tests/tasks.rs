//! PHASE 10: the four tasks' own validations and edits, over answers replayed from cassettes.
//!
//! **Where the answers come from.** No model is reachable where Phase 10 was built, so every
//! cassette under `tests/cassettes/<task>/` that a test here names was recorded through the stub:
//! a scripted answer, filed by the production cache key exactly as a model's would be, with
//! `model_id = "stub"` and no llama.cpp build or thread count — honest about what it is. Each test
//! both checks that the committed cassette is what recording its scripted answer would write now,
//! and replays it through [`Replay`] — the provider seam every test tier uses, which holds no
//! transport. Regenerate after a prompt-version bump with
//! `OC_AI_RECORD_SEEDS=1 cargo nextest run -p oc-ai --test tasks` and review the diff; the nightly
//! live job re-records them from a model.

mod common;

use std::path::{Path, PathBuf};

use oc_ai::cassette::{record, Cassette, Params, Recording, Replay, STUB_MODEL};
use oc_ai::digest::hex;
use oc_ai::gates::schema::gate_response;
use oc_ai::gates::GateFailure;
use oc_ai::prompt::v1::metadata::{self, MetadataAnswer, MetadataInput, Size, StyledLine};
use oc_ai::provider::{LlmProvider, LlmRequest, LlmResponse};
use oc_ai::task::metadata::{apply_metadata, validate_metadata, MetadataLimits};
use oc_core::thresholds::T;
use oc_model::doc::{MetaSource, Metadata};
use oc_model::lang::LangTag;

fn cassettes() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cassettes")
}

/// The answer `answer` to `request`, as the committed cassette `<task>__<fixture>__v1` replays it.
///
/// Fails when the committed cassette is not what recording the scripted answer would write now —
/// a stale or hand-edited cassette — and rewrites it instead under `OC_AI_RECORD_SEEDS=1`.
fn replayed(fixture: &str, request: &LlmRequest, answer: &str) -> LlmResponse {
    let fresh = record(
        request,
        &LlmResponse {
            text: answer.to_owned(),
            reasoning: None,
            tokens_in: 0,
            tokens_out: 0,
            cached: false,
            cached_tokens: None,
            finish_reason: Some("stop".to_owned()),
        },
        Recording {
            fixture,
            model_id: STUB_MODEL,
            params: Params {
                temperature: T.llm.temperature as f32,
                max_tokens: request.max_tokens,
                enable_thinking: false,
                top_p: None,
                seed: None,
                n_ctx: None,
                n_parallel: None,
            },
            recorded_at: "2026-09-23T00:00:00Z".to_owned(),
            llama_cpp_build: None,
            threads: None,
            machine: "stub".to_owned(),
        },
    );
    if std::env::var("OC_AI_RECORD_SEEDS").is_ok_and(|value| value == "1") {
        fresh.write(&cassettes()).expect("the cassette is written");
    }
    let task = request.purpose.as_str();
    let key = hex(&request.cache_key(STUB_MODEL));
    let committed: Cassette = serde_json::from_str(
        &std::fs::read_to_string(cassettes().join(task).join(format!("{key}.json")))
            .unwrap_or_else(|_| panic!("no cassette {}; set OC_AI_RECORD_SEEDS=1", fresh.name())),
    )
    .expect("a cassette");
    assert_eq!(
        Cassette {
            recorded_at: fresh.recorded_at.clone(),
            ..committed
        },
        fresh,
        "the cassette {} is stale; set OC_AI_RECORD_SEEDS=1 and review the diff",
        fresh.name()
    );
    Replay::new(cassettes(), STUB_MODEL)
        .complete(request)
        .unwrap_or_else(|error| panic!("{} did not replay: {error}", fresh.name()))
}

fn max_tokens() -> u32 {
    common::max_tokens()
}

// ---------------------------------------------------------------------------
// Task 1 — metadata
// ---------------------------------------------------------------------------

fn metadata_limits() -> MetadataLimits {
    MetadataLimits {
        title_max_chars: usize::try_from(T.metadata.llm_title_max_chars).unwrap_or(usize::MAX),
    }
}

/// A title page whose Info dictionary said `Microsoft Word - Kapitel.docx`: the case the
/// metadata predicate escalates.
fn title_page() -> MetadataInput {
    let line = |size, centered, text: &str| StyledLine {
        size,
        centered,
        text: text.to_owned(),
    };
    MetadataInput {
        lines: vec![
            line(Some(Size::Large), true, "Der Prozess"),
            line(Some(Size::Medium), true, "Roman"),
            line(Some(Size::Small), true, "Franz  Kafka"),
            line(None, false, "Erstes Kapitel"),
            line(
                None,
                false,
                "Jemand mußte Josef K. verleumdet haben, denn ohne daß er",
            ),
        ],
    }
}

fn deterministic_metadata() -> Metadata {
    Metadata {
        title: Some("Kapitel".to_owned()),
        subtitle: None,
        authors: Vec::new(),
        translator: None,
        publisher: None,
        date: None,
        identifier: "urn:uuid:00000000-0000-5000-8000-000000000000".to_owned(),
        language: LangTag::DE,
        source: MetaSource::Heuristic,
    }
}

/// Row 10.3. The model answers with the right title, author and date, and a publisher that is
/// printed nowhere on pages 1–3. The verbatim check refuses the **whole** response — the true title with
/// it — and names the field that was invented.
#[test]
fn metadata_verbatim_substring_rejects_invention() {
    // The same title page with its imprint line: a place and a year, and no publisher.
    let mut input = title_page();
    input.lines.insert(
        3,
        StyledLine {
            size: Some(Size::Small),
            centered: true,
            text: "Berlin 1925".to_owned(),
        },
    );
    let request = metadata::request(&input, max_tokens()).expect("renders");
    let response = replayed(
        "fabricated_publisher",
        &request,
        concat!(
            r#"{"title":"Der Prozess","subtitle":"Roman","authors":["Franz Kafka"],"#,
            r#""translator":null,"publisher":"S. Fischer Verlag","date":"1925"}"#
        ),
    );
    let answer = gate_response::<MetadataAnswer>(&response, &input)
        .expect("the shape is right: gate S admits it");

    let verdict = validate_metadata(&answer, &input.verbatim_text(), &metadata_limits());
    assert_eq!(
        verdict,
        Err(GateFailure::NotVerbatim { field: "publisher" })
    );
    assert_eq!(
        verdict.map_err(|failure| failure.code()),
        Err("V.verbatim"),
        "the code the Decision records"
    );
}

/// Row 10.4. A verbatim title — and an author the page prints with two spaces, which the
/// whitespace normalisation reads as the same name — is accepted, and becomes the book's
/// `dc:title`, with `source = llm` so it is never mistaken for a title the file declared.
#[test]
fn metadata_accepts_exact_title() {
    let input = title_page();
    let request = metadata::request(&input, max_tokens()).expect("renders");
    let response = replayed(
        "exact_title",
        &request,
        concat!(
            r#"{"title":"Der Prozess","subtitle":"Roman","authors":["Franz Kafka"],"#,
            r#""translator":null,"publisher":null,"date":null}"#
        ),
    );
    let answer = gate_response::<MetadataAnswer>(&response, &input).expect("gate S admits it");
    validate_metadata(&answer, &input.verbatim_text(), &metadata_limits())
        .expect("every field is on the page");

    let applied = apply_metadata(&answer, &deterministic_metadata());
    assert_eq!(applied.title.as_deref(), Some("Der Prozess"));
    assert_eq!(applied.subtitle.as_deref(), Some("Roman"));
    assert_eq!(applied.authors, vec!["Franz Kafka".to_owned()]);
    assert_eq!(applied.source, MetaSource::Llm);
    // Never the model's: the identity and the language.
    assert_eq!(applied.identifier, deterministic_metadata().identifier);
    assert_eq!(applied.language, LangTag::DE);
}

/// The check compares the lines' text, not the tags the model was shown beside it, and holds
/// the title to its length bound.
#[test]
fn metadata_check_ignores_the_tags_and_bounds_the_title() {
    let input = title_page();
    let answer = |title: &str| MetadataAnswer {
        title: Some(title.to_owned()),
        subtitle: None,
        authors: Vec::new(),
        translator: None,
        publisher: None,
        date: None,
    };
    assert_eq!(
        validate_metadata(
            &answer("[LARGE][CENTERED] Der Prozess"),
            &input.verbatim_text(),
            &metadata_limits()
        ),
        Err(GateFailure::NotVerbatim { field: "title" })
    );
    // Case-insensitive, as the design says.
    assert!(validate_metadata(
        &answer("DER PROZESS"),
        &input.verbatim_text(),
        &metadata_limits()
    )
    .is_ok());

    let long = "Jemand mußte Josef K. verleumdet haben, denn ohne daß er";
    let tight = MetadataLimits {
        title_max_chars: 10,
    };
    assert!(matches!(
        validate_metadata(&answer(long), &input.verbatim_text(), &tight),
        Err(GateFailure::OutOfRange { field: "title", .. })
    ));
}
