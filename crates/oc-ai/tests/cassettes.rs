//! Cassettes: record once, replay offline, forever (IMPLEMENTATION_PLAN Appendix B, R9 §C.4).
//!
//! The committed cassettes under `tests/cassettes/` are the **seeds** — Appendix A.3's worked
//! answers, one per task, recorded through the stub rather than a model, because there is no model
//! until Phase 9. They are honest about it: `model_id` and `machine` are `stub`, and the llama.cpp
//! build and thread count are absent. The nightly `live-llm-cassette-refresh` job records real ones
//! with the same code (Appendix B.4). Every seed is also a **canary** (B.4 rule 5): it must replay,
//! and its answer must pass gate S, whatever edit the prompts receive next.

mod common;

use std::path::{Path, PathBuf};

use oc_ai::cassette::{record, Cassette, Params, Recording, Replay, STUB_MODEL};
use oc_ai::digest::hex;
use oc_ai::gates::schema::gate_response;
use oc_ai::prompt::v1::book_structure::BookStructureAnswer;
use oc_ai::prompt::v1::heading_roles::{HeadingRole, HeadingRolesAnswer};
use oc_ai::prompt::v1::metadata::MetadataAnswer;
use oc_ai::prompt::v1::verse_quote::{BlockKind, VerseQuoteAnswer};
use oc_ai::prompt::PROMPT_VERSION;
use oc_ai::provider::{LlmError, LlmProvider, LlmResponse};
use oc_core::thresholds::T;

fn cassettes() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cassettes")
}

/// The readable name of a seed, as Appendix B.1 spells names: `<task>__<fixture>__v<N>`.
fn seed_name(task: &str) -> String {
    format!("{task}__a3__v{PROMPT_VERSION}")
}

/// Test 8.11. Replay needs no transport: the provider is built from files, answers every task's
/// worked question, and there is nothing in it that could reach a model or a socket (Appendix B.3
/// rule 1; test 8.15 is what shows the crate cannot). The replayed answers are the canaries: each
/// one passes gate S and says what the worked example says.
#[test]
fn cassette_replay_is_offline() {
    let replay = Replay::new(cassettes(), STUB_MODEL);
    let requests = common::requests();
    let responses: Vec<LlmResponse> = requests
        .iter()
        .map(|request| {
            replay
                .complete(request)
                .unwrap_or_else(|error| panic!("{:?} did not replay: {error}", request.purpose))
        })
        .collect();
    assert!(responses.iter().all(|response| response.cached));

    let metadata = gate_response::<MetadataAnswer>(&responses[0], &common::metadata_input())
        .expect("the metadata canary passes gate S");
    assert_eq!(metadata.title.as_deref(), Some("Die Verwandlung"));

    let roles = gate_response::<HeadingRolesAnswer>(&responses[1], &common::heading_roles_input())
        .expect("the heading-roles canary passes gate S");
    assert_eq!(roles.clusters.get(&2), Some(&HeadingRole::RunningHead));

    let structure =
        gate_response::<BookStructureAnswer>(&responses[2], &common::book_structure_input())
            .expect("the book-structure canary passes gate S");
    assert_eq!(structure.backmatter_start_idx, 4);

    let verse = gate_response::<VerseQuoteAnswer>(&responses[3], &common::verse_quote_input())
        .expect("the verse canary passes gate S");
    assert_eq!(
        verse.kinds.get(&common::verse_block_id()),
        Some(&BlockKind::Verse)
    );
}

/// One keying rule in the system (Appendix B.1): a cassette's file name is the production cache
/// key of its question, and the index maps each readable name to it.
#[test]
fn a_cassette_is_filed_under_the_cache_key() {
    for request in common::requests() {
        let task = request.purpose.as_str();
        let key = hex(&request.cache_key(STUB_MODEL));
        let path = cassettes().join(task).join(format!("{key}.json"));
        assert!(path.is_file(), "{} is missing", path.display());

        let index: std::collections::BTreeMap<String, String> = serde_json::from_str(
            &std::fs::read_to_string(cassettes().join(task).join("index.json")).expect("index"),
        )
        .expect("the index is a map of names to keys");
        assert_eq!(index.get(&seed_name(task)), Some(&key));
    }
}

/// Every cassette file is one an index names, and every index entry names a file: a cassette left
/// behind by a prompt-version bump is an inventory nobody can interpret, and B.4 rule 3 deletes it
/// in the bump's own commit.
#[test]
fn every_cassette_file_is_indexed() {
    let mut tasks = 0;
    for entry in std::fs::read_dir(cassettes()).expect("the cassette directory") {
        let directory = entry.expect("an entry").path();
        if !directory.is_dir() {
            continue;
        }
        tasks += 1;
        let index: std::collections::BTreeMap<String, String> = serde_json::from_str(
            &std::fs::read_to_string(directory.join("index.json")).expect("an index"),
        )
        .expect("a map");
        let indexed: std::collections::BTreeSet<String> =
            index.values().map(|key| format!("{key}.json")).collect();
        let files: std::collections::BTreeSet<String> = std::fs::read_dir(&directory)
            .expect("the task directory")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .filter(|name| name != "index.json")
            .collect();
        assert_eq!(files, indexed, "{}", directory.display());
    }
    assert_eq!(tasks, 4, "one directory per task");
}

/// A miss is a failure naming the key that was looked for and the recording nearest to the live
/// question — never a silent fallback and never a "closest cassette" answer (Appendix B.3 rule 2).
#[test]
fn a_miss_names_the_key_and_the_nearest_recording() {
    let replay = Replay::new(cassettes(), STUB_MODEL);
    let mut request = common::requests().remove(1);
    request.user = request.user.replace("Kapitel Drei", "Kapitel Neun");
    let expected = hex(&request.cache_key(STUB_MODEL));

    match replay.complete(&request) {
        Err(LlmError::CassetteMiss { key, nearest }) => {
            assert_eq!(key, expected);
            let nearest = nearest.expect("the task has recordings to compare with");
            assert_eq!(nearest.name, seed_name("heading_roles"));
            let diverges = request.user.find("Neun").expect("the edit");
            assert_eq!(nearest.diverges_at, diverges, "the first byte that differs");
        }
        other => panic!("a changed question replayed: {other:?}"),
    }
}

/// A cassette whose prompt or grammar hash disagrees with the current artifacts fails even when
/// its key matched (Appendix B.3 rule 3): that is what a hand-edited cassette looks like.
#[test]
fn a_hand_edited_cassette_is_refused() {
    let root = std::env::temp_dir().join(format!("oc-ai-edited-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let request = common::requests().remove(3);
    let task = request.purpose.as_str();
    let key = hex(&request.cache_key(STUB_MODEL));
    std::fs::create_dir_all(root.join(task)).expect("mkdir");

    let original =
        std::fs::read_to_string(cassettes().join(task).join(format!("{key}.json"))).expect("seed");
    let mut cassette: Cassette = serde_json::from_str(&original).expect("a cassette");
    cassette.prompt_sha256 = "0".repeat(64);
    std::fs::write(
        root.join(task).join(format!("{key}.json")),
        serde_json::to_string_pretty(&cassette).expect("serialises"),
    )
    .expect("write");

    let replay = Replay::new(&root, STUB_MODEL);
    assert!(matches!(
        replay.complete(&request),
        Err(LlmError::StaleCassette { .. })
    ));
    let _ = std::fs::remove_dir_all(&root);
}

/// The regression artefact: each committed seed is exactly what recording its worked answer would
/// write now. After a deliberate prompt change — which is a new prompt version — regenerate them
/// with `OC_AI_RECORD_SEEDS=1 cargo nextest run -p oc-ai -E 'test(seeds)'` and review the diff.
/// `recorded_at` is the one field not compared: cassettes expire on a version, not on a clock
/// (Appendix B.3 rule 4).
#[test]
fn the_committed_seeds_are_the_worked_answers() {
    let rerecord = std::env::var("OC_AI_RECORD_SEEDS").is_ok_and(|value| value == "1");
    for (request, answer) in common::requests().iter().zip(common::answers()) {
        let response = LlmResponse {
            text: answer,
            reasoning: None,
            tokens_in: 0,
            tokens_out: 0,
            cached: false,
            finish_reason: Some("stop".to_owned()),
        };
        let fresh = record(
            request,
            &response,
            Recording {
                fixture: "a3",
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
                recorded_at: "2026-09-22T00:00:00Z".to_owned(),
                llama_cpp_build: None,
                threads: None,
                machine: "stub".to_owned(),
            },
        );
        if rerecord {
            fresh.write(&cassettes()).expect("the seed is written");
            continue;
        }
        let task = request.purpose.as_str();
        let key = hex(&request.cache_key(STUB_MODEL));
        let committed: Cassette = serde_json::from_str(
            &std::fs::read_to_string(cassettes().join(task).join(format!("{key}.json")))
                .unwrap_or_else(|_| panic!("no seed for {task}; set OC_AI_RECORD_SEEDS=1")),
        )
        .expect("a cassette");
        assert_eq!(
            Cassette {
                recorded_at: fresh.recorded_at.clone(),
                ..committed
            },
            fresh,
            "the {task} seed is stale; set OC_AI_RECORD_SEEDS=1 and review the diff"
        );
    }
}
