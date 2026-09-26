//! PHASE 11: the provider adapters — the sidecar we own, a custom OpenAI-compatible endpoint, and
//! Ollama — each over an in-process transport, so nothing here opens a socket.

mod common;

use std::sync::Mutex;
use std::time::Duration;

use oc_ai::gates::schema::gate_response;
use oc_ai::prompt::v1::heading_roles::HeadingRolesAnswer;
use oc_ai::provider::local_sidecar::local_sidecar;
use oc_ai::provider::openai_compatible::{custom_endpoint, ClientConfig, OpenAiCompatible};
use oc_ai::provider::{
    Constraint, LlmProvider, ProviderCaps, ProviderKind, ThinkingControl, W_LLM_UNCONSTRAINED,
};
use oc_ai::session::{Asker, Session, SystemClock};
use oc_ai::transport::{Transport, TransportError};
use oc_core::thresholds::T;
use serde_json::Value;

/// Answers every chat completion with `content`, and keeps every body it was sent.
struct Canned {
    content: String,
    bodies: Mutex<Vec<String>>,
}

impl Canned {
    fn new(content: &str) -> Self {
        Self {
            content: content.to_owned(),
            bodies: Mutex::new(Vec::new()),
        }
    }

    fn last(&self) -> Value {
        let bodies = self.bodies.lock().expect("not poisoned");
        serde_json::from_str(bodies.last().expect("a request was sent")).expect("JSON")
    }
}

impl Transport for &Canned {
    fn post_json(&self, path: &str, body: &str, _: Duration) -> Result<String, TransportError> {
        assert_eq!(path, "/v1/chat/completions");
        self.bodies
            .lock()
            .expect("not poisoned")
            .push(body.to_owned());
        Ok(serde_json::json!({
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": self.content },
                "finish_reason": "stop"
            }],
            "usage": { "prompt_tokens": 400, "completion_tokens": 40 }
        })
        .to_string())
    }
}

fn timeout() -> Duration {
    Duration::from_secs(u64::try_from(T.llm.call_timeout_secs).expect("a timeout"))
}

/// Row 11.4. An endpoint that takes neither a grammar nor a JSON Schema gets the schema in the
/// prompt instead, the book's report says the answers were unconstrained — once, however many
/// calls — and gate S holds the answer to exactly the same shape as it would a grammar's.
#[test]
fn openai_compatible_without_grammar_warns() {
    let request = &common::requests()[1];
    let input = common::heading_roles_input();

    let good = Canned::new(common::HEADING_ROLES_ANSWER);
    let provider = custom_endpoint(
        &good,
        "qwen3-8b-instruct".to_owned(),
        ProviderCaps::neither(),
        T.llm.temperature as f32,
        timeout(),
    );
    assert_eq!(provider.capabilities(), ProviderCaps::neither());
    assert_eq!(provider.thinking_control(), ThinkingControl::NoThinkSuffix);

    let clock = SystemClock::new();
    let mut session = Session::new(&provider, None, &clock, 8, u64::MAX);
    for _ in 0..2 {
        let asked = session.ask(request).expect("answered");
        assert!(
            gate_response::<HeadingRolesAnswer>(&asked.response, &input).is_ok(),
            "a well-formed unconstrained answer is admitted"
        );
    }
    let unconstrained: Vec<_> = session
        .warnings()
        .iter()
        .filter(|warning| warning.code == W_LLM_UNCONSTRAINED)
        .collect();
    assert_eq!(unconstrained.len(), 1, "{:?}", session.warnings());
    assert_eq!(
        unconstrained[0].args.get("model").map(String::as_str),
        Some("qwen3-8b-instruct")
    );

    // Nothing on the wire claims a constraint, and the schema rides in the user message instead.
    let body = good.last();
    assert!(body.get("grammar").is_none() && body.get("response_format").is_none());
    let user = body["messages"][1]["content"].as_str().expect("text");
    assert!(user.starts_with(&request.user), "the question comes first");
    assert!(user.ends_with(request.schema.trim_end()), "then the schema");

    // Gate S is not relaxed for an unconstrained answer: the adversarial ones still fail it.
    for (answer, code) in [
        (
            format!("Sure! {}", common::HEADING_ROLES_ANSWER),
            "S.unparseable",
        ),
        (
            format!("```json\n{}\n```", common::HEADING_ROLES_ANSWER),
            "S.unparseable",
        ),
        (
            common::HEADING_ROLES_ANSWER.replace("\"running_head\"", "\"page_header\""),
            "S.enum",
        ),
        (
            format!("<think>hm</think>{}", common::HEADING_ROLES_ANSWER),
            "S.thinking",
        ),
    ] {
        let bad = Canned::new(&answer);
        let provider = custom_endpoint(
            &bad,
            "qwen3-8b-instruct".to_owned(),
            ProviderCaps::neither(),
            T.llm.temperature as f32,
            timeout(),
        );
        let response = provider.complete(request).expect("answered");
        let failure =
            gate_response::<HeadingRolesAnswer>(&response, &input).expect_err("gate S refuses it");
        assert_eq!(failure.code(), code, "{answer}");
    }
}

/// A provider that constrains the answer raises no such warning.
#[test]
fn a_constrained_provider_does_not_warn() {
    let request = &common::requests()[1];
    let good = Canned::new(common::HEADING_ROLES_ANSWER);
    let sidecar = local_sidecar(
        &good,
        "qwen3-1.7b-q4_k_m".to_owned(),
        T.llm.temperature as f32,
        timeout(),
    );
    assert_eq!(sidecar.capabilities(), ProviderCaps::grammar());
    assert_eq!(
        sidecar.thinking_control(),
        ThinkingControl::ChatTemplateKwargs
    );
    let clock = SystemClock::new();
    let mut session = Session::new(&sidecar, None, &clock, 8, u64::MAX);
    session.ask(request).expect("answered");
    assert!(session.warnings().is_empty(), "{:?}", session.warnings());
    let body = good.last();
    assert_eq!(body["grammar"], Value::from(request.grammar));
    assert_eq!(body["messages"][1]["content"], request.user.as_str());
}

/// D10's thinking lever for a custom endpoint: `/no_think` for a Qwen-family model, nothing for any
/// other — whose thinking, if it comes back, gate S refuses.
#[test]
fn a_custom_endpoint_turns_thinking_off_only_for_the_qwen_family() {
    let transport = Canned::new("{}");
    for (model, thinking) in [
        ("qwen3:1.7b", ThinkingControl::NoThinkSuffix),
        ("Qwen/Qwen3-4B-Instruct", ThinkingControl::NoThinkSuffix),
        ("llama3.2:3b", ThinkingControl::None),
        ("gpt-oss-20b", ThinkingControl::None),
    ] {
        let provider = custom_endpoint(
            &transport,
            model.to_owned(),
            ProviderCaps::json_schema(),
            T.llm.temperature as f32,
            timeout(),
        );
        assert_eq!(provider.thinking_control(), thinking, "{model}");
    }
}

/// The kinds, as the report and the command line name them.
#[test]
fn provider_kinds_have_stable_names() {
    assert_eq!(ProviderKind::LocalSidecar.as_str(), "local_sidecar");
    assert_eq!(ProviderKind::OpenAiCompatible.as_str(), "openai_compatible");
    assert_eq!(ProviderKind::Ollama.as_str(), "ollama");
    // The client every OpenAI-compatible adapter is built from is still constructible directly.
    let transport = Canned::new("{}");
    let client = OpenAiCompatible::new(
        &transport,
        ClientConfig {
            model_id: "m".to_owned(),
            constraint: Constraint::JsonSchema,
            thinking: ThinkingControl::None,
            temperature: T.llm.temperature as f32,
            timeout: timeout(),
        },
    );
    assert_eq!(client.capabilities(), ProviderCaps::json_schema());
}
