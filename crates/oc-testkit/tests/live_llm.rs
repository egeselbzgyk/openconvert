//! Rows 9.15 and 9.16, against a real `llama-server` and a real model (nightly, `--features
//! live-llm`).
//!
//! `OC_LLAMA_SERVER` names the server binary (`cargo xtask fetch-llama-server` stages the pinned
//! build) and `OC_LIVE_MODEL` the GGUF (`openconvert model pull qwen3-1.7b-q4_k_m`). The server is
//! started exactly as the engine starts it — `oc_core::sidecar`, loopback, a per-run key, one slot —
//! and asked the four tasks' worked examples (IMPLEMENTATION_PLAN Appendix A.3) through the real
//! client, the real transport and the real grammars.

#![cfg(feature = "live-llm")]

#[path = "../../oc-ai/tests/common/mod.rs"]
mod common;

use std::path::PathBuf;
use std::time::Duration;

use oc_ai::prefix;
use oc_ai::provider::openai_compatible::{ClientConfig, OpenAiCompatible};
use oc_ai::provider::{Constraint, LlmProvider, ThinkingControl};
use oc_core::sidecar::llama::ServerSpec;
use oc_core::sidecar::server::{Health, OwnedServer};
use oc_core::thresholds::T;
use oc_net::transport::HttpTransport;

/// A GGUF of 1–3 GB loads in seconds from a local disk; a CI runner's disk is slower.
const LOAD: Duration = Duration::from_secs(300);
/// One CPU generation of at most `llm.max_output_tokens_per_call` tokens.
const CALL: Duration = Duration::from_secs(300);
/// Row 9.15's count.
const GENERATIONS: usize = 200;

fn required(name: &str) -> PathBuf {
    PathBuf::from(
        std::env::var_os(name).unwrap_or_else(|| {
            panic!("--features live-llm needs {name}; see the header of this file")
        }),
    )
}

fn start() -> OwnedServer {
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let spec = ServerSpec {
        model: required("OC_LIVE_MODEL"),
        context: 8192,
        threads: u32::try_from(threads).expect("a thread count"),
        cache_reuse: true,
        context_checkpoints: None,
    };
    let port = oc_net::loopback::free_port().expect("a free port");
    let server = OwnedServer::spawn(&required("OC_LLAMA_SERVER"), &spec, port).expect("spawn");
    server
        .wait_healthy(
            &|server: &OwnedServer| match HttpTransport::new(&server.base_url(), None)
                .expect("a base")
                .get("/health", Duration::from_secs(2))
            {
                Ok(_) => Health::Ready,
                Err(_) => Health::NotYet,
            },
            LOAD,
        )
        .expect("the model loads");
    server
}

fn client(server: &OwnedServer) -> OpenAiCompatible<HttpTransport> {
    OpenAiCompatible::new(
        HttpTransport::new(&server.base_url(), Some(server.api_key().clone())).expect("a base"),
        ClientConfig {
            model_id: "live".to_owned(),
            constraint: Constraint::Gbnf,
            thinking: ThinkingControl::ChatTemplateKwargs,
            temperature: T.llm.temperature as f32,
            timeout: CALL,
        },
    )
}

/// Row 9.15: no `<think>` block, in the text or separated out by the server, in 200
/// grammar-constrained generations across all four production grammars.
#[test]
fn thinking_is_absent_in_200_generations() {
    let server = start();
    let client = client(&server);
    let requests = common::requests();
    let mut thinking = Vec::new();
    for i in 0..GENERATIONS {
        let request = &requests[i % requests.len()];
        let response = client.complete(request).expect("a completion");
        if response.text.contains("<think>") || response.reasoning.is_some() {
            thinking.push((i, request.purpose.as_str(), response.text));
        }
    }
    assert!(
        thinking.is_empty(),
        "{} of {GENERATIONS} generations thought: {thinking:?}",
        thinking.len()
    );
}

/// Row 9.16: the second call of a book finds the shared prefix in the KV cache.
#[test]
fn prefix_is_cached_on_second_call() {
    let server = start();
    let client = client(&server);
    let requests = common::requests();
    let first = client.complete(&requests[0]).expect("call 1");
    prefix::check(0, requests[0].purpose, &first).expect("call 1 is allowed to be cold");
    let second = client.complete(&requests[1]).expect("call 2");
    if let Err(warning) = prefix::check(1, requests[1].purpose, &second) {
        panic!(
            "call 2 did not reuse the prefix ({:?} cached tokens): {} {:?}",
            second.cached_tokens, warning.code, warning.args
        );
    }
}
