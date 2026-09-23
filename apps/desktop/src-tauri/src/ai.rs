//! What AI assistance means for one job: the job spec's `ai` object, or why there is none (PHASE 10,
//! PHASE 11, PHASE 12 part B2, UI_UX §2.4 and §4).
//!
//! The switch and the provider are the user's settings; a job takes them when it is queued. Then:
//!
//! - **Off** — the v1 default (D17). The spec has no `ai` object.
//! - **Built-in** — the app's own `llama-server` ([`crate::llm`]) for the installed default model,
//!   leased when the job starts and released when it ends ([`ModelServer`]). A job waits for the
//!   server to load — a 1 GB model, once per batch — before its engine starts.
//! - **Ollama / custom endpoint** — the endpoint, model and key file written into the spec as they
//!   are; the engine probes what it is (job-spec v1 has no provider field). A host off this computer
//!   is sent text only with the consent the dialog recorded for that host, written as
//!   `non_loopback_consent`; without it the job never starts and the dialog opens again (D10).
//!
//! **Fail-open.** Anything else that stops a model answering — no model installed, no server in
//! this build, a server that does not come up, an endpoint that is not a usable URL — converts the
//! book without AI, and the row says so in the non-modal banner (UI_UX §4). The engine's own
//! `W_LLM_UNAVAILABLE` says the same for what only it can find out.

use std::path::PathBuf;

use oc_core::jobspec::AiSpec;
use serde::Serialize;

use crate::settings::{endpoint_host, Provider, Settings};

/// What a job's AI settings are, fixed when it is queued.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobAi {
    Off,
    /// The app's own server: leased when the job starts.
    Builtin,
    /// An endpoint the user configured, written into the spec as it is.
    Endpoint {
        provider: Provider,
        spec: AiSpec,
    },
    /// A custom endpoint off this computer that no consent names. The job never starts.
    ConsentRequired {
        host: String,
    },
    /// AI is on and cannot be used for this job: it converts without, and says why.
    Unusable {
        provider: Provider,
        why: AiUnavailable,
    },
}

/// Why a job that asked for AI assistance converted without it — the app's half of the banner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AiUnavailable {
    /// Built-in, and the default model is not installed.
    NoModel,
    /// Built-in, and this build has no `llama-server` beside the app.
    NoServer,
    /// Built-in, and the server did not start or did not answer `/health` in time.
    ServerFailed,
    /// A custom endpoint that is not an `http://` or `https://` URL the engine will use.
    NoEndpoint,
    /// A custom endpoint off this computer over plain `http://`: text is sent off this computer only
    /// over `https://` (D10).
    PlainHttp,
}

/// What a row shows about AI: which provider, and whether it could be used.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AiView {
    pub provider: Provider,
    pub unavailable: Option<AiUnavailable>,
}

impl JobAi {
    /// The job the settings describe.
    pub fn plan(settings: &Settings) -> JobAi {
        if !settings.ai_enabled {
            return JobAi::Off;
        }
        match settings.provider {
            Provider::Builtin => JobAi::Builtin,
            Provider::Ollama => JobAi::Endpoint {
                provider: Provider::Ollama,
                spec: AiSpec {
                    enabled: true,
                    endpoint: Some(oc_net::detect::OLLAMA_DEFAULT_URL.to_owned()),
                    model_id: settings.ollama_model.clone(),
                    ..AiSpec::default()
                },
            },
            Provider::Custom => custom(settings),
        }
    }

    /// The row's view of it; `None` with AI off.
    pub fn view(&self, unavailable: Option<AiUnavailable>) -> Option<AiView> {
        let provider = match self {
            JobAi::Off => return None,
            JobAi::Builtin => Provider::Builtin,
            JobAi::Endpoint { provider, .. } | JobAi::Unusable { provider, .. } => *provider,
            JobAi::ConsentRequired { .. } => Provider::Custom,
        };
        let why = match self {
            JobAi::Unusable { why, .. } => Some(*why),
            _ => unavailable,
        };
        Some(AiView {
            provider,
            unavailable: why,
        })
    }
}

fn custom(settings: &Settings) -> JobAi {
    let endpoint = settings.custom.endpoint.trim();
    let unusable = |why| JobAi::Unusable {
        provider: Provider::Custom,
        why,
    };
    let Some(host) = endpoint_host(endpoint) else {
        return unusable(AiUnavailable::NoEndpoint);
    };
    let loopback = oc_net::consent::is_loopback(&host);
    let consented = settings
        .custom
        .consent
        .as_ref()
        .is_some_and(|consent| consent.host == host);
    if !loopback && !consented {
        return JobAi::ConsentRequired { host };
    }
    if !loopback {
        // The consent is the user's; whether the URL may carry text at all is not.
        let record =
            oc_net::consent::ConsentRecord::grant(&host, oc_net::consent::ConsentScope::Run);
        if oc_net::consent::authorize(endpoint, Some(&record)).is_err() {
            return unusable(AiUnavailable::PlainHttp);
        }
    }
    let model = settings.custom.model.trim();
    JobAi::Endpoint {
        provider: Provider::Custom,
        spec: AiSpec {
            enabled: true,
            endpoint: Some(endpoint.to_owned()),
            api_key_file: settings.custom.api_key_file.clone(),
            model_id: (!model.is_empty()).then(|| model.to_owned()),
            non_loopback_consent: !loopback,
            ..AiSpec::default()
        },
    }
}

/// The app's own model server, as the queue sees it: started (or reused) for a job, and released
/// when the job ends. [`crate::llm::AppModelServer`] in the app; a double in the tests.
pub trait ModelServer: Send + Sync {
    /// The server for the installed default model, started if it is not running. Blocks while the
    /// model loads, so the queue calls it off its own lock.
    fn acquire(&self) -> Result<ServerLease, AiUnavailable>;
    /// A job that held a lease has ended.
    fn release(&self);
}

/// What a job's spec needs from the app's server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerLease {
    /// `http://127.0.0.1:<port>`.
    pub endpoint: String,
    /// The run's key, in a file only this user can read.
    pub api_key_file: PathBuf,
    /// The registry id of the model it serves.
    pub model_id: String,
}

impl ServerLease {
    /// The spec's `ai` object for this lease.
    pub fn spec(&self) -> AiSpec {
        AiSpec {
            enabled: true,
            endpoint: Some(self.endpoint.clone()),
            api_key_file: Some(self.api_key_file.clone()),
            model_id: Some(self.model_id.clone()),
            ..AiSpec::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Consent;

    fn on(provider: Provider) -> Settings {
        Settings {
            ai_enabled: true,
            provider,
            ..Settings::default()
        }
    }

    /// Off is the default, and off means no `ai` object whatever the provider settings hold.
    #[test]
    fn ai_off_is_the_default_and_writes_nothing() {
        assert_eq!(JobAi::plan(&Settings::default()), JobAi::Off);
        let mut off = on(Provider::Ollama);
        off.ai_enabled = false;
        assert_eq!(JobAi::plan(&off), JobAi::Off);
        assert_eq!(JobAi::Off.view(None), None);
    }

    /// Ollama is `localhost:11434` with the chosen model; a custom endpoint on this computer is
    /// written as it is, with its key file and no consent.
    #[test]
    fn ollama_and_a_local_endpoint_are_written_as_configured() {
        let mut ollama = on(Provider::Ollama);
        ollama.ollama_model = Some("qwen3:1.7b".to_owned());
        let JobAi::Endpoint { provider, spec } = JobAi::plan(&ollama) else {
            panic!("an endpoint");
        };
        assert_eq!(provider, Provider::Ollama);
        assert_eq!(spec.endpoint.as_deref(), Some("http://localhost:11434"));
        assert_eq!(spec.model_id.as_deref(), Some("qwen3:1.7b"));
        assert!(!spec.non_loopback_consent);

        let mut local = on(Provider::Custom);
        local.custom.endpoint = "http://127.0.0.1:1234/v1".to_owned();
        local.custom.model = "qwen3-4b".to_owned();
        local.custom.api_key_file = Some(PathBuf::from("/keys/lm.key"));
        let JobAi::Endpoint { spec, .. } = JobAi::plan(&local) else {
            panic!("an endpoint");
        };
        assert_eq!(spec.endpoint.as_deref(), Some("http://127.0.0.1:1234/v1"));
        assert_eq!(spec.api_key_file, Some(PathBuf::from("/keys/lm.key")));
        assert_eq!(spec.model_id.as_deref(), Some("qwen3-4b"));
        assert!(!spec.non_loopback_consent, "this computer needs no consent");
    }

    /// D10: a host off this computer is sent text only with the consent that names it — then the
    /// spec says so; without it, or with consent to another host, the job never starts.
    #[test]
    fn a_host_off_this_computer_needs_the_consent_that_names_it() {
        let mut remote = on(Provider::Custom);
        remote.custom.endpoint = "https://llm.example.org/v1".to_owned();
        assert_eq!(
            JobAi::plan(&remote),
            JobAi::ConsentRequired {
                host: "llm.example.org".to_owned()
            }
        );

        remote.custom.consent = Some(Consent {
            host: "other.example.net".to_owned(),
            granted_at: "2026-09-23T10:00:00Z".to_owned(),
        });
        assert!(matches!(
            JobAi::plan(&remote),
            JobAi::ConsentRequired { .. }
        ));

        let granted = remote.granting_consent().expect("granted");
        let JobAi::Endpoint { spec, .. } = JobAi::plan(&granted) else {
            panic!("an endpoint");
        };
        assert!(spec.non_loopback_consent);
        spec_is_valid(spec);
    }

    /// Fail-open: an endpoint that is not a URL, or plain http off this computer, converts the book
    /// without AI and says why (UI_UX §4).
    #[test]
    fn an_unusable_endpoint_converts_without_ai_and_says_why() {
        let mut empty = on(Provider::Custom);
        empty.custom.endpoint = String::new();
        assert_eq!(
            JobAi::plan(&empty).view(None),
            Some(AiView {
                provider: Provider::Custom,
                unavailable: Some(AiUnavailable::NoEndpoint)
            })
        );

        let mut plain = on(Provider::Custom);
        plain.custom.endpoint = "http://llm.example.org/v1".to_owned();
        let plain = plain.granting_consent().expect("granted");
        assert_eq!(
            JobAi::plan(&plain),
            JobAi::Unusable {
                provider: Provider::Custom,
                why: AiUnavailable::PlainHttp
            }
        );
    }

    /// The spec the queue writes validates against the committed schema, consent and all.
    fn spec_is_valid(ai: AiSpec) {
        let mut spec = oc_core::jobspec::JobSpec::new("/in/a.pdf".into(), "/out/a.epub".into());
        spec.ai = Some(ai);
        spec.validate().expect("valid");
    }
}
