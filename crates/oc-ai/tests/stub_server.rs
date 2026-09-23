//! The stub server (IMPLEMENTATION_PLAN Phase 8, detail 6): an in-process `Transport` that answers
//! the way an OpenAI-compatible server does, and can be told to answer badly — malformed JSON, a
//! preamble, a code fence, a duplicated id, an out-of-enum value, a thinking block.
//!
//! What these tests demonstrate is acceptance criterion A8.1 end to end: whatever the model says,
//! the question goes out as the prompt artifacts wrote it, the answer comes back through the real
//! client, and anything malformed or adversarial leaves the deterministic answer standing with a
//! `Decision` that names the gate. No socket is involved anywhere: the stub is a struct.

mod common;

use std::sync::Mutex;
use std::time::Duration;

use oc_ai::gates::fallback::{settle, Choice};
use oc_ai::gates::schema::gate_response;
use oc_ai::prompt::v1::heading_roles::HeadingRolesAnswer;
use oc_ai::provider::openai_compatible::{ClientConfig, OpenAiCompatible};
use oc_ai::provider::{trace, Constraint, LlmError, LlmProvider, ThinkingControl};
use oc_ai::transport::{Transport, TransportError};
use oc_core::thresholds::T;
use oc_model::confidence::Method;
use serde_json::Value;

/// What the stub says in answer to any question.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// The worked answer, as a well-behaved model would give it.
    Canned,
    Malformed,
    Preamble,
    Fence,
    DuplicateId,
    OutOfEnum,
    Thinking,
    /// A well-formed answer, with the model's reasoning separated out by the server into
    /// `reasoning_content` — llama-server does this with `--reasoning-format`.
    SeparatedReasoning,
    /// The endpoint cannot be reached at all.
    Down,
}

struct Stub {
    mode: Mode,
    bodies: Mutex<Vec<String>>,
}

impl Stub {
    fn new(mode: Mode) -> Self {
        Self {
            mode,
            bodies: Mutex::new(Vec::new()),
        }
    }

    fn content(&self) -> String {
        let good = common::HEADING_ROLES_ANSWER;
        match self.mode {
            Mode::Canned | Mode::SeparatedReasoning | Mode::Down => good.to_owned(),
            Mode::Malformed => good[..good.len() - 7].to_owned(),
            Mode::Preamble => format!("Here is the mapping you asked for:\n{good}"),
            Mode::Fence => format!("```json\n{good}\n```"),
            Mode::DuplicateId => good.replace("{\"c\":1,", "{\"c\":0,"),
            Mode::OutOfEnum => good.replace("\"running_head\"", "\"page_header\""),
            Mode::Thinking => {
                format!("<think>\nCluster 0 is centred and bold.\n</think>\n\n{good}")
            }
        }
    }
}

impl Transport for Stub {
    fn post_json(
        &self,
        path: &str,
        body: &str,
        _timeout: Duration,
    ) -> Result<String, TransportError> {
        assert_eq!(
            path, "/v1/chat/completions",
            "one wire format for every provider (D10)"
        );
        self.bodies
            .lock()
            .expect("not poisoned")
            .push(body.to_owned());
        if self.mode == Mode::Down {
            return Err(TransportError::Unreachable("connection refused".to_owned()));
        }
        let mut message = serde_json::json!({ "role": "assistant", "content": self.content() });
        if self.mode == Mode::SeparatedReasoning {
            message["reasoning_content"] = Value::from("Cluster 0 is centred and bold.");
        }
        Ok(serde_json::json!({
            "id": "chatcmpl-stub",
            "object": "chat.completion",
            "choices": [{ "index": 0, "message": message, "finish_reason": "stop" }],
            "usage": { "prompt_tokens": 412, "completion_tokens": 38, "total_tokens": 450 }
        })
        .to_string())
    }
}

fn config(constraint: Constraint, thinking: ThinkingControl) -> ClientConfig {
    ClientConfig {
        model_id: "qwen3-1.7b-q4_k_m".to_owned(),
        constraint,
        thinking,
        temperature: T.llm.temperature as f32,
        timeout: Duration::from_secs(30),
    }
}

fn client(mode: Mode) -> OpenAiCompatible<Stub> {
    OpenAiCompatible::new(
        Stub::new(mode),
        config(Constraint::Gbnf, ThinkingControl::ChatTemplateKwargs),
    )
}

fn sent(client: &OpenAiCompatible<Stub>) -> Value {
    let bodies = client.transport().bodies.lock().expect("not poisoned");
    serde_json::from_str(bodies.last().expect("a request was sent")).expect("the body is JSON")
}

/// A well-behaved server's answer comes back verbatim, with its token counts.
#[test]
fn the_client_reads_an_answer_the_way_a_server_gives_it() {
    let client = client(Mode::Canned);
    let response = client.complete(&common::requests()[1]).expect("answered");
    assert_eq!(response.text, common::HEADING_ROLES_ANSWER);
    assert_eq!((response.tokens_in, response.tokens_out), (412, 38));
    assert_eq!(response.reasoning, None);
    assert_eq!(client.id(), "qwen3-1.7b-q4_k_m");
}

/// A8.1, end to end: every adversarial answer the stub can give leaves the deterministic answer
/// standing, and the decision names the gate that refused it.
#[test]
fn every_adversarial_answer_leaves_the_deterministic_answer_standing() {
    let request = &common::requests()[1];
    let input = common::heading_roles_input();
    for (mode, code) in [
        (Mode::Malformed, "S.unparseable"),
        (Mode::Preamble, "S.unparseable"),
        (Mode::Fence, "S.unparseable"),
        (Mode::DuplicateId, "S.bijection"),
        (Mode::OutOfEnum, "S.enum"),
        (Mode::Thinking, "S.thinking"),
        (Mode::SeparatedReasoning, "S.thinking"),
    ] {
        let client = client(mode);
        let response = client.complete(request).expect("the stub answers");
        let verdict = gate_response::<HeadingRolesAnswer>(&response, &input);
        let decision = settle(
            Choice {
                stage: "structure",
                kind: "heading_roles",
                subject: None,
                deterministic: "size_rank".to_owned(),
                alternatives: vec!["llm_mapping".to_owned()],
            },
            trace(client.id(), request, &response.text, false, 0),
            verdict.map(|_| "llm_mapping".to_owned()),
        );
        assert_eq!(decision.method, Method::Deterministic, "{mode:?}");
        assert_eq!(decision.chosen, "size_rank", "{mode:?}");
        assert_eq!(decision.fallback, Some(code), "{mode:?}");
    }

    let response = client(Mode::Canned).complete(request).expect("answered");
    assert!(
        gate_response::<HeadingRolesAnswer>(&response, &input).is_ok(),
        "and the well-behaved answer passes"
    );
}

/// The grammar goes on the wire as GBNF to a server that takes it, as JSON Schema to one that
/// prefers that (ARCHITECTURE §9.2), and not at all to one that takes neither — gate S is the check
/// in every case, so a provider that cannot constrain degrades rather than fails.
#[test]
fn the_body_carries_the_grammar_in_the_form_the_server_takes() {
    let request = &common::requests()[1];

    let gbnf = OpenAiCompatible::new(
        Stub::new(Mode::Canned),
        config(Constraint::Gbnf, ThinkingControl::None),
    );
    gbnf.complete(request).expect("answered");
    let body = sent(&gbnf);
    assert_eq!(body["grammar"], Value::from(request.grammar));
    assert!(body.get("response_format").is_none());

    let schema = OpenAiCompatible::new(
        Stub::new(Mode::Canned),
        config(Constraint::JsonSchema, ThinkingControl::None),
    );
    schema.complete(request).expect("answered");
    let body = sent(&schema);
    assert_eq!(body["response_format"]["type"], "json_schema");
    assert_eq!(
        body["response_format"]["json_schema"]["name"],
        "heading_roles"
    );
    assert_eq!(body["response_format"]["json_schema"]["strict"], true);
    let expected: Value = serde_json::from_str(request.schema).expect("the schema is JSON");
    assert_eq!(body["response_format"]["json_schema"]["schema"], expected);
    assert!(body.get("grammar").is_none());

    let neither = OpenAiCompatible::new(
        Stub::new(Mode::Canned),
        config(Constraint::None, ThinkingControl::None),
    );
    neither.complete(request).expect("answered");
    let body = sent(&neither);
    assert!(body.get("grammar").is_none() && body.get("response_format").is_none());
}

/// Thinking is turned off in whatever words the provider understands (D10): chat-template kwargs for
/// the sidecar, `think: false` for Ollama, `/no_think` appended to the shared prefix for a generic
/// endpoint — and appended to the prefix of all four tasks alike, so it is still one prefix.
#[test]
fn thinking_is_turned_off_in_each_providers_own_words() {
    let request = &common::requests()[1];
    let body_for = |thinking| {
        let client =
            OpenAiCompatible::new(Stub::new(Mode::Canned), config(Constraint::Gbnf, thinking));
        client.complete(request).expect("answered");
        sent(&client)
    };

    let sidecar = body_for(ThinkingControl::ChatTemplateKwargs);
    assert_eq!(sidecar["chat_template_kwargs"]["enable_thinking"], false);
    assert_eq!(sidecar["messages"][0]["content"], request.system_prefix);

    let ollama = body_for(ThinkingControl::OllamaThink);
    assert_eq!(ollama["think"], false);

    let generic = body_for(ThinkingControl::NoThinkSuffix);
    let system = generic["messages"][0]["content"].as_str().expect("text");
    assert!(system.starts_with(request.system_prefix) && system.ends_with("/no_think"));

    let prefixes: std::collections::BTreeSet<String> = common::requests()
        .iter()
        .map(|request| {
            let client = OpenAiCompatible::new(
                Stub::new(Mode::Canned),
                config(Constraint::Gbnf, ThinkingControl::NoThinkSuffix),
            );
            client.complete(request).expect("answered");
            sent(&client)["messages"][0]["content"]
                .as_str()
                .expect("text")
                .to_owned()
        })
        .collect();
    assert_eq!(
        prefixes.len(),
        1,
        "one prefix on the wire for all four tasks"
    );
}

/// Greedy decoding and a bounded answer: `temperature` from `llm.temperature`, `max_tokens` from the
/// request, and the user message exactly as rendered.
#[test]
fn the_body_is_greedy_bounded_and_verbatim() {
    let request = &common::requests()[0];
    let client = client(Mode::Canned);
    client.complete(request).expect("answered");
    let body = sent(&client);
    assert_eq!(body["model"], "qwen3-1.7b-q4_k_m");
    assert_eq!(
        body["temperature"],
        serde_json::json!(T.llm.temperature as f32)
    );
    assert_eq!(body["max_tokens"], request.max_tokens);
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["messages"][1]["role"], "user");
    assert_eq!(body["messages"][1]["content"], request.user.as_str());
}

/// An endpoint that cannot be reached is not an answer and is not a gate failure: the client says
/// so, and the caller converts deterministically (RT D20).
#[test]
fn an_unreachable_endpoint_is_an_error_not_an_answer() {
    let result = client(Mode::Down).complete(&common::requests()[1]);
    assert!(matches!(
        result,
        Err(LlmError::Transport(TransportError::Unreachable(_)))
    ));
}

/// A reply that is not a chat completion at all — the server's error page, an empty object — is a
/// protocol failure, distinguishable from a model that answered badly.
#[test]
fn a_reply_that_is_not_a_completion_is_a_protocol_error() {
    struct Garbage(&'static str);
    impl Transport for Garbage {
        fn post_json(&self, _: &str, _: &str, _: Duration) -> Result<String, TransportError> {
            Ok(self.0.to_owned())
        }
    }
    for reply in ["<html>502 Bad Gateway</html>", "{}", "{\"choices\":[]}"] {
        let client = OpenAiCompatible::new(
            Garbage(reply),
            config(Constraint::Gbnf, ThinkingControl::None),
        );
        assert!(
            matches!(
                client.complete(&common::requests()[1]),
                Err(LlmError::Protocol(_))
            ),
            "{reply}"
        );
    }
}
