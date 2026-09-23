//! PHASE 11 detail 2: Ollama, through its own `/api/chat`.
//!
//! Ollama's default context is 2 048 tokens, and a prompt longer than the context is cut from the
//! front without a word — the worst failure mode available here, because the answer that comes back
//! is well-formed and wrong. So every request states its context, large enough for the prompt it
//! carries, and tells Ollama to refuse rather than truncate or shift.

mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use common::cassette_server::{CassetteServer, OLLAMA_PATH};
use oc_ai::cassette::{Replay, STUB_MODEL};
use oc_ai::gates::schema::gate_response;
use oc_ai::prompt::v1::heading_roles::HeadingRolesAnswer;
use oc_ai::provider::ollama::{prompt_token_bound, Ollama, OllamaConfig};
use oc_ai::provider::{LlmProvider, LlmRequest, ProviderCaps, ThinkingControl};
use oc_ai::transport::{Transport, TransportError};
use oc_core::thresholds::T;
use serde_json::Value;

fn cassettes() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cassettes")
}

fn config(model: &str) -> OllamaConfig {
    OllamaConfig {
        model: model.to_owned(),
        temperature: T.llm.temperature as f32,
        timeout: Duration::from_secs(u64::try_from(T.llm.call_timeout_secs).expect("secs")),
        num_ctx: u32::try_from(T.llm.ollama_num_ctx).expect("a context"),
        keep_alive_secs: u32::try_from(T.llm.ollama_keep_alive_secs).expect("secs"),
        template_overhead_tokens: u32::try_from(T.llm.ollama_template_overhead_tokens)
            .expect("tokens"),
    }
}

/// The seed questions, and questions far larger than the configured context — in one-byte and in
/// multi-byte text, since a token is never shorter than a byte of UTF-8.
fn questions() -> Vec<LlmRequest> {
    let mut requests = common::requests();
    let configured = usize::try_from(T.llm.ollama_num_ctx).expect("a context");
    for filler in ["x", "ö", "漢"] {
        let mut large = common::requests()[2].clone();
        large.user.push_str(&filler.repeat(configured * 3));
        requests.push(large);
    }
    requests
}

/// Answers `/api/chat` with a fixed body and keeps what it was sent.
struct Fixed(&'static str, std::sync::Mutex<Vec<String>>);

impl Transport for &Fixed {
    fn post_json(&self, path: &str, body: &str, _: Duration) -> Result<String, TransportError> {
        assert_eq!(path, OLLAMA_PATH, "Ollama's own chat endpoint");
        self.1.lock().expect("not poisoned").push(body.to_owned());
        Ok(self.0.to_owned())
    }
}

const OK_REPLY: &str = r#"{"model":"qwen3:1.7b","created_at":"2026-09-23T00:00:00Z","message":{"role":"assistant","content":"{}"},"done":true,"done_reason":"stop","prompt_eval_count":10,"eval_count":2}"#;

/// Row 11.2. Every request body sets `options.num_ctx`, never below the configured context and
/// never below what the rendered prompt can occupy plus the answer's token cap — and asks Ollama to
/// fail rather than truncate the prompt or shift the context.
#[test]
fn ollama_num_ctx_is_always_overridden() {
    let transport = Fixed(OK_REPLY, std::sync::Mutex::new(Vec::new()));
    let ollama = Ollama::new(&transport, config("qwen3:1.7b"));
    for request in questions() {
        ollama.complete(&request).expect("answered");
        let body: Value = serde_json::from_str(
            transport
                .1
                .lock()
                .expect("not poisoned")
                .last()
                .expect("sent"),
        )
        .expect("JSON");
        let num_ctx = body["options"]["num_ctx"]
            .as_u64()
            .expect("num_ctx is set on every request");

        let rendered = request.system_prefix.len() + request.user.len();
        let floor = u64::try_from(rendered).expect("fits") + u64::from(request.max_tokens);
        assert!(
            num_ctx >= floor,
            "num_ctx {num_ctx} < {rendered} prompt bytes + {} answer tokens",
            request.max_tokens
        );
        assert!(
            num_ctx >= u64::try_from(T.llm.ollama_num_ctx).expect("fits"),
            "never below the configured context"
        );
        assert!(
            u64::from(prompt_token_bound(
                &request,
                config("m").template_overhead_tokens
            )) >= u64::try_from(rendered).expect("fits"),
            "the bound counts every byte of the prompt"
        );
        assert_eq!(body["options"]["num_predict"], request.max_tokens);
        assert_eq!(body["truncate"], false, "refuse, never truncate");
        assert_eq!(body["shift"], false, "never shift the prompt out");
        assert_eq!(body["stream"], false);
        assert_eq!(body["think"], false);
        assert_eq!(
            body["keep_alive"],
            u64::try_from(T.llm.ollama_keep_alive_secs).expect("fits"),
            "set explicitly: Ollama's default unloads the model after five minutes"
        );
    }

    // A small book asks for the same context every time, so Ollama does not reload the model.
    let contexts: std::collections::BTreeSet<u32> = common::requests()
        .iter()
        .map(|request| ollama.num_ctx_for(request))
        .collect();
    assert_eq!(
        contexts,
        [u32::try_from(T.llm.ollama_num_ctx).expect("fits")].into()
    );
}

/// Row 11.3. The request carries `format` with the task's own JSON Schema, and nothing else is
/// asked of the prompt: the question goes verbatim. Replayed through the committed cassettes, the
/// answer is the recorded one, and it passes gate S.
#[test]
fn ollama_uses_format_schema() {
    let server = CassetteServer::load(&cassettes());
    let ollama = Ollama::new(&server, config("qwen3:1.7b"));
    assert_eq!(ollama.capabilities(), ProviderCaps::json_schema());
    assert_eq!(ollama.thinking_control(), ThinkingControl::OllamaThink);
    assert_eq!(ollama.id(), "qwen3:1.7b");

    let replay = Replay::new(cassettes(), STUB_MODEL);
    for request in common::requests() {
        let response = ollama.complete(&request).expect("the cassette answers");
        let asked = server.last();
        assert_eq!(asked.path, OLLAMA_PATH);
        let schema: Value = serde_json::from_str(request.schema).expect("the schema is JSON");
        assert_eq!(asked.body["format"], schema, "{:?}", request.purpose);
        assert_eq!(asked.body["messages"][0]["content"], request.system_prefix);
        assert_eq!(asked.body["messages"][1]["content"], request.user.as_str());
        assert!(asked.body.get("grammar").is_none());
        assert!(asked.body.get("response_format").is_none());

        let recorded = replay.complete(&request).expect("the seed replays");
        assert_eq!(response.text, recorded.text, "{:?}", request.purpose);
        assert!(!response.cached, "a server answered, not the cache");
    }

    let response = ollama
        .complete(&common::requests()[1])
        .expect("the cassette answers");
    assert!(gate_response::<HeadingRolesAnswer>(&response, &common::heading_roles_input()).is_ok());
}

/// Ollama's reply, read: the answer, its token counts, why it stopped — and its thinking, which gate
/// S refuses wherever the server put it. A reply that is not a chat response is a protocol error.
#[test]
fn ollama_reply_is_read_and_thinking_is_kept_for_gate_s() {
    let thinking = Fixed(
        r#"{"model":"qwen3:1.7b","message":{"role":"assistant","content":"{}","thinking":"let me see"},"done":true,"done_reason":"length","prompt_eval_count":321,"prompt_eval_cached_count":300,"eval_count":12}"#,
        std::sync::Mutex::new(Vec::new()),
    );
    let response = Ollama::new(&thinking, config("qwen3:1.7b"))
        .complete(&common::requests()[1])
        .expect("answered");
    assert_eq!(response.reasoning.as_deref(), Some("let me see"));
    assert_eq!((response.tokens_in, response.tokens_out), (321, 12));
    assert_eq!(response.cached_tokens, Some(300));
    assert_eq!(response.finish_reason.as_deref(), Some("length"));
    assert!(
        gate_response::<HeadingRolesAnswer>(&response, &common::heading_roles_input()).is_err()
    );

    for reply in ["{}", r#"{"error":"model not found"}"#, "<html>502</html>"] {
        let garbage = Fixed(reply, std::sync::Mutex::new(Vec::new()));
        assert!(
            matches!(
                Ollama::new(&garbage, config("qwen3:1.7b")).complete(&common::requests()[1]),
                Err(oc_ai::provider::LlmError::Protocol(_))
            ),
            "{reply}"
        );
    }
}
