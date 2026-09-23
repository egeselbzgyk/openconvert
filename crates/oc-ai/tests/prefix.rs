//! PHASE 9 detail 4 and the offline half of row 9.16: the reply says whether the prefix was warm,
//! and a cold prefix after a book's first call is a warning, not a silence.

mod common;

use std::sync::Mutex;
use std::time::Duration;

use oc_ai::prefix::{check, W_LLM_PREFIX_COLD};
use oc_ai::provider::openai_compatible::{ClientConfig, OpenAiCompatible};
use oc_ai::provider::{Constraint, LlmProvider, LlmResponse, Purpose, ThinkingControl};
use oc_ai::transport::{Transport, TransportError};

/// Answers every request with a fixed reply body.
struct Fixed(Mutex<String>);

impl Transport for Fixed {
    fn post_json(&self, _: &str, _: &str, _: Duration) -> Result<String, TransportError> {
        Ok(self.0.lock().expect("reply").clone())
    }
}

fn reply(extra: &str) -> String {
    format!(r#"{{"choices":[{{"message":{{"content":"{{}}"}},"finish_reason":"stop"}}],{extra}}}"#)
}

fn complete(body: String) -> LlmResponse {
    let client = OpenAiCompatible::new(
        Fixed(Mutex::new(body)),
        ClientConfig {
            model_id: "m".to_owned(),
            constraint: Constraint::Gbnf,
            thinking: ThinkingControl::ChatTemplateKwargs,
            temperature: 0.0,
            timeout: Duration::from_secs(1),
        },
    );
    client
        .complete(&common::requests()[1])
        .expect("a completion")
}

#[test]
fn cached_prompt_tokens_are_read_from_the_reply() {
    // llama-server's own timings.
    let warm = complete(reply(
        r#""usage":{"prompt_tokens":1200,"completion_tokens":40},"timings":{"cache_n":1024,"prompt_n":176}"#,
    ));
    assert_eq!(warm.cached_tokens, Some(1024));
    // The OpenAI-compatible field.
    let openai = complete(reply(
        r#""usage":{"prompt_tokens":1200,"completion_tokens":40,"prompt_tokens_details":{"cached_tokens":896}}"#,
    ));
    assert_eq!(openai.cached_tokens, Some(896));
    // Neither: unknown, not zero.
    let silent = complete(reply(
        r#""usage":{"prompt_tokens":1200,"completion_tokens":40}"#,
    ));
    assert_eq!(silent.cached_tokens, None);
}

fn response(cached_tokens: Option<u32>) -> LlmResponse {
    LlmResponse {
        text: "{}".to_owned(),
        reasoning: None,
        tokens_in: 1200,
        tokens_out: 40,
        cached: false,
        cached_tokens,
        finish_reason: Some("stop".to_owned()),
    }
}

#[test]
fn prefix_cold_is_warned_after_the_first_call() {
    // The first call of a book fills the cache: cold is expected.
    assert!(check(0, Purpose::Metadata, &response(Some(0))).is_ok());
    assert!(check(0, Purpose::Metadata, &response(None)).is_ok());
    // Every later call must find it warm.
    assert!(check(1, Purpose::HeadingRoles, &response(Some(1024))).is_ok());
    for cold in [Some(0), None] {
        let warning = check(1, Purpose::HeadingRoles, &response(cold))
            .expect_err("a cold prefix on the second call");
        assert_eq!(warning.code, W_LLM_PREFIX_COLD);
        let args = serde_json::to_value(&warning.args).expect("args");
        assert_eq!(args["call"], "2", "one-based, as a reader counts");
        assert_eq!(args["task"], "heading_roles");
    }
}
