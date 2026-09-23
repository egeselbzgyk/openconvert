//! Row 11.10 and acceptance criterion A11.3: the cassette contract, through every adapter.
//!
//! Every committed cassette — the Phase-8 seeds and the Phase-10 scripted answers — is asked
//! through each adapter over the cassette server: the sidecar we own (GBNF), a custom endpoint that
//! takes a JSON Schema, one that takes neither (the schema rides in the prompt), one serving a
//! Qwen-family model (`/no_think` on the prefix), and Ollama (`/api/chat`, `format`). Each one builds
//! its own wire format and reads its own reply shape, and every one of them must come back with
//! exactly what replaying the cassette gives — the same answer, the same reasoning, the same
//! finish — and the seed answers must pass gate S identically. If an adapter needed its own
//! cassettes, the abstraction would not be abstracting.

mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use common::cassette_server::{CassetteServer, OLLAMA_PATH, OPENAI_PATH};
use oc_ai::cassette::{Cassette, Replay, STUB_MODEL};
use oc_ai::gates::schema::gate_response;
use oc_ai::prompt::v1::book_structure::BookStructureAnswer;
use oc_ai::prompt::v1::heading_roles::HeadingRolesAnswer;
use oc_ai::prompt::v1::metadata::MetadataAnswer;
use oc_ai::prompt::v1::verse_quote::VerseQuoteAnswer;
use oc_ai::prompt::{self, PROMPT_VERSION};
use oc_ai::provider::local_sidecar::local_sidecar;
use oc_ai::provider::ollama::{Ollama, OllamaConfig};
use oc_ai::provider::openai_compatible::custom_endpoint;
use oc_ai::provider::{LlmProvider, LlmRequest, ProviderCaps, Purpose, ThinkingControl};
use oc_core::thresholds::T;
use serde_json::Value;

fn cassettes() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cassettes")
}

fn timeout() -> Duration {
    Duration::from_secs(u64::try_from(T.llm.call_timeout_secs).expect("secs"))
}

/// Every committed cassette's question, rebuilt from the current prompt artifacts.
fn recorded_questions() -> Vec<LlmRequest> {
    let mut requests = Vec::new();
    for purpose in Purpose::ALL {
        let directory = cassettes().join(purpose.as_str());
        let mut files: Vec<_> = std::fs::read_dir(&directory)
            .expect("a task directory")
            .map(|entry| entry.expect("an entry").path())
            .filter(|path| path.file_name().is_some_and(|name| name != "index.json"))
            .collect();
        files.sort();
        for file in files {
            let cassette: Cassette =
                serde_json::from_str(&std::fs::read_to_string(&file).expect("read"))
                    .expect("a cassette");
            let artifacts = prompt::artifacts(purpose);
            requests.push(LlmRequest {
                purpose,
                prompt_version: PROMPT_VERSION,
                system_prefix: artifacts.system,
                user: cassette.request.user,
                grammar: artifacts.grammar,
                schema: artifacts.schema,
                max_tokens: cassette.params.max_tokens,
            });
        }
    }
    requests
}

/// The seed answers, parsed by gate S, as one comparable value per task.
fn gated(provider: &dyn LlmProvider) -> Vec<String> {
    let requests = common::requests();
    let answer = |index: usize| provider.complete(&requests[index]).expect("answered");
    vec![
        format!(
            "{:?}",
            gate_response::<MetadataAnswer>(&answer(0), &common::metadata_input())
                .expect("metadata passes gate S")
        ),
        format!(
            "{:?}",
            gate_response::<HeadingRolesAnswer>(&answer(1), &common::heading_roles_input())
                .expect("heading roles pass gate S")
        ),
        format!(
            "{:?}",
            gate_response::<BookStructureAnswer>(&answer(2), &common::book_structure_input())
                .expect("book structure passes gate S")
        ),
        format!(
            "{:?}",
            gate_response::<VerseQuoteAnswer>(&answer(3), &common::verse_quote_input())
                .expect("verse or quote passes gate S")
        ),
    ]
}

/// Row 11.10.
#[test]
fn same_cassettes_pass_on_all_providers() {
    let server = CassetteServer::load(&cassettes());
    let temperature = T.llm.temperature as f32;
    let ollama_config = OllamaConfig {
        model: "qwen3:1.7b".to_owned(),
        temperature,
        timeout: timeout(),
        num_ctx: u32::try_from(T.llm.ollama_num_ctx).expect("a context"),
        keep_alive_secs: u32::try_from(T.llm.ollama_keep_alive_secs).expect("secs"),
        template_overhead_tokens: u32::try_from(T.llm.ollama_template_overhead_tokens)
            .expect("tokens"),
    };

    let sidecar = local_sidecar(
        &server,
        "qwen3-1.7b-q4_k_m".to_owned(),
        temperature,
        timeout(),
    );
    let schema = custom_endpoint(
        &server,
        "lmstudio-model".to_owned(),
        ProviderCaps::json_schema(),
        temperature,
        timeout(),
    );
    let neither = custom_endpoint(
        &server,
        "vllm-model".to_owned(),
        ProviderCaps::neither(),
        temperature,
        timeout(),
    );
    let qwen = custom_endpoint(
        &server,
        "qwen3-8b-instruct".to_owned(),
        ProviderCaps::neither(),
        temperature,
        timeout(),
    );
    assert_eq!(qwen.thinking_control(), ThinkingControl::NoThinkSuffix);
    let ollama = Ollama::new(&server, ollama_config);

    let adapters: [(&str, &dyn LlmProvider, &str); 5] = [
        ("LocalSidecar", &sidecar, OPENAI_PATH),
        ("OpenAiCompatible (json_schema)", &schema, OPENAI_PATH),
        ("OpenAiCompatible (neither)", &neither, OPENAI_PATH),
        ("OpenAiCompatible (qwen, /no_think)", &qwen, OPENAI_PATH),
        ("Ollama", &ollama, OLLAMA_PATH),
    ];

    let replay = Replay::new(cassettes(), STUB_MODEL);
    let questions = recorded_questions();
    assert!(
        questions.len() >= Purpose::ALL.len(),
        "every task has recordings: {}",
        questions.len()
    );
    for request in &questions {
        let recorded = replay.complete(request).expect("the cassette replays");
        for (name, adapter, path) in &adapters {
            let response = adapter
                .complete(request)
                .unwrap_or_else(|error| panic!("{name}, {:?}: {error}", request.purpose));
            assert_eq!(
                response.text, recorded.text,
                "{name}, {:?}",
                request.purpose
            );
            assert_eq!(response.reasoning, recorded.reasoning, "{name}");
            assert_eq!(response.finish_reason, recorded.finish_reason, "{name}");
            assert_eq!(
                (response.tokens_in, response.tokens_out),
                (recorded.tokens_in, recorded.tokens_out),
                "{name}"
            );
            assert_eq!(
                &server.last().path,
                path,
                "{name} speaks its own wire format"
            );
        }
    }

    // Each adapter put the constraint where its server takes it, and nowhere else.
    let request = &common::requests()[1];
    let wire = |adapter: &dyn LlmProvider| -> Value {
        adapter.complete(request).expect("answered");
        server.last().body
    };
    let body = wire(&sidecar);
    assert_eq!(body["grammar"], Value::from(request.grammar));
    let body = wire(&schema);
    assert_eq!(body["response_format"]["type"], "json_schema");
    let body = wire(&neither);
    assert!(body.get("grammar").is_none() && body.get("response_format").is_none());
    assert_ne!(body["messages"][1]["content"], request.user.as_str());
    let body = wire(&ollama);
    assert!(body.get("format").is_some() && body["options"]["num_ctx"].is_u64());

    // And the seed canaries pass gate S with the same answer, whichever adapter carried them.
    let expected = gated(&replay);
    for (name, adapter, _) in &adapters {
        assert_eq!(gated(*adapter), expected, "{name}");
    }
}
