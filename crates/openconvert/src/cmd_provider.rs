//! `openconvert provider detect | check <URL> | probe <URL>` (PHASE 11): what the desktop app's
//! Provider settings read (UI_UX §2.4), and what a user can ask from a terminal.
//!
//! - **`detect`** — is Ollama on `localhost:11434`, and which models does it serve? Loopback, so no
//!   consent; nothing there is `"ollama": null`, never an error.
//! - **`check <URL>`** — does this endpoint need consent, and would the engine use it at all? Sends
//!   nothing: it is how the settings page decides to show the consent dialog that names the host.
//! - **`probe <URL>`** — what would `convert --ai` open there: the adapter, what it can constrain,
//!   the model. It is `convert --ai`'s own opening, under the same consent rule (exit 2,
//!   `E_CONSENT_REQUIRED`), so the answer cannot drift from what a conversion does. It asks the
//!   endpoint what it is and sends no question and no document text.
//!
//! Output is data on stdout (`--json` for machines); events, including `fatal`, on stderr.

use std::io::Write;
use std::time::Duration;

use oc_ai::provider::{Constraint, ThinkingControl};
use oc_core::events::EventSink;
use oc_core::exit::ExitCode;
use oc_core::thresholds::T;
use oc_net::consent::{self, ConsentRecord, ConsentScope};
use oc_net::detect;
use openconvert::ai_endpoint::{self, OpenError};
use serde_json::{json, Value};

use crate::cli::{ProviderAction, ProviderArgs};

/// The model registry the engine was built with; `probe` opens exactly as `convert` does.
const BUNDLED_REGISTRY: &str = include_str!("../../../models.toml");

const E_USAGE: &str = "E_USAGE";

pub fn run<W: Write>(
    args: &ProviderArgs,
    events: &mut EventSink<W>,
    stdout: &mut dyn Write,
) -> ExitCode {
    let url = args.ai.endpoint.clone().unwrap_or_default();
    let (code, answer) = match args.action {
        ProviderAction::Detect => (ExitCode::Ok, detect_ollama()),
        ProviderAction::Check => match check(&url) {
            Some(answer) => (ExitCode::Ok, answer),
            None => {
                let message = format!("`{url}` is not an endpoint the engine will use");
                events.fatal(E_USAGE, &message);
                eprintln!("error: {message}");
                return ExitCode::Usage;
            }
        },
        ProviderAction::Probe => match ai_endpoint::open(&args.ai, BUNDLED_REGISTRY, &T) {
            Ok(opened) => (
                ExitCode::Ok,
                json!({
                    "available": true,
                    "url": url,
                    "host": consent::host_of(&url).ok(),
                    "provider": opened.kind.as_str(),
                    "constraint": constraint_name(opened.provider.capabilities().constraint),
                    "thinking": thinking_name(opened.provider.thinking_control()),
                    "model": opened.provider.id(),
                    "models": opened.models,
                    "consent": opened.consent.as_ref().map(consent_json),
                }),
            ),
            Err(OpenError::Unavailable(reason)) => (
                ExitCode::Failed,
                json!({ "available": false, "url": url, "reason": reason }),
            ),
            Err(refused) => {
                let (code, message) = refused
                    .fatal()
                    .unwrap_or((E_USAGE, "the endpoint was refused".to_owned()));
                events.fatal(code, &message);
                eprintln!("error: {message}");
                return refused.exit_code();
            }
        },
    };
    let text = if args.json {
        format!("{answer}\n")
    } else {
        human(&answer)
    };
    if stdout.write_all(text.as_bytes()).is_err() {
        return ExitCode::Failed;
    }
    code
}

/// Ollama on its default endpoint, or `null`.
fn detect_ollama() -> Value {
    let timeout = Duration::from_millis(
        u64::try_from(T.llm.provider_probe_timeout_millis).unwrap_or_default(),
    );
    let found = detect::ollama_transport()
        .ok()
        .and_then(|transport| detect::detect_ollama(&transport, timeout));
    json!({
        "ollama": found.map(|info| json!({
            "url": detect::OLLAMA_DEFAULT_URL,
            "models": info.models,
        })),
    })
}

/// What the engine would make of `url`, without sending anything; `None` when it is not a URL the
/// engine will use at all.
fn check(url: &str) -> Option<Value> {
    let host = consent::host_of(url).ok()?;
    let loopback = consent::is_loopback(&host);
    // Usable once consented to: the only other refusal is plain http off this machine.
    let usable = consent::authorize(url, Some(&ConsentRecord::grant(&host, ConsentScope::Run)));
    Some(json!({
        "url": url,
        "host": host,
        "loopback": loopback,
        "requires_consent": !loopback,
        "usable": usable.is_ok(),
        "reason": usable.err().map(|error| error.to_string()),
    }))
}

fn consent_json(record: &ConsentRecord) -> Value {
    json!({
        "host": record.host,
        "granted_at": record.granted_at_rfc3339(),
        "scope": record.scope.as_str(),
    })
}

fn constraint_name(constraint: Constraint) -> &'static str {
    match constraint {
        Constraint::Gbnf => "gbnf",
        Constraint::JsonSchema => "json_schema",
        Constraint::None => "none",
    }
}

fn thinking_name(thinking: ThinkingControl) -> &'static str {
    match thinking {
        ThinkingControl::ChatTemplateKwargs => "chat_template_kwargs",
        ThinkingControl::OllamaThink => "ollama_think",
        ThinkingControl::NoThinkSuffix => "no_think_suffix",
        ThinkingControl::None => "none",
    }
}

/// One `key: value` line per field, for a person at a terminal.
fn human(answer: &Value) -> String {
    let mut text = String::new();
    if let Value::Object(fields) = answer {
        for (key, value) in fields {
            let value = match value {
                Value::String(text) => text.clone(),
                Value::Null => "-".to_owned(),
                Value::Array(items) => items
                    .iter()
                    .map(|item| {
                        item.as_str()
                            .map_or_else(|| item.to_string(), str::to_owned)
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
                other => other.to_string(),
            };
            text.push_str(&format!("{key}: {value}\n"));
        }
    }
    text
}
