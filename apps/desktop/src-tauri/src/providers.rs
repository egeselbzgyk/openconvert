//! Settings › Provider, asked of the engine (PHASE 11's hand-off, UI_UX §2.4, D10).
//!
//! The webview opens no socket (`connect-src 'none'`), and neither does this module: every question
//! about a provider is the engine's `openconvert provider …`, the same code `convert --ai` opens a
//! provider with, so the settings page and a conversion cannot disagree.
//!
//! - **`detect`** — is Ollama running on this computer, and which models does it serve.
//! - **`check <URL>`** — does the endpoint need consent, and would the engine use it at all. It sends
//!   nothing; it is how the page decides to show the consent dialog that names the host.
//! - **`probe`** — "Test connection": what `convert --ai` would open with the saved settings. It
//!   asks the endpoint what it is and sends no document text; a host nobody consented to is refused
//!   before anything is sent (`E_CONSENT_REQUIRED`).
//!
//! **Arguments.** A conversion's engine gets exactly one argument (RT B15); these are not
//! conversions. The command is still built here, never by the webview: the subcommand and flags are
//! fixed, and the one value the webview supplies — a URL — must be an `http://` or `https://` URL
//! with no control character, so it can never be read as a flag.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::Value;

use crate::engine::{UiError, CACHE_VAR, PASSWORD_VAR};
use crate::settings::{endpoint_host, Provider, Settings};

/// The `fatal` code of a host off this computer that nothing consented to (PHASE 11).
const E_CONSENT_REQUIRED: &str = "E_CONSENT_REQUIRED";

/// Asks the engine about providers.
#[derive(Clone, Debug)]
pub struct ProviderCli {
    program: PathBuf,
}

impl ProviderCli {
    pub fn new(program: PathBuf) -> Self {
        Self { program }
    }

    /// `provider detect --json`: `{"ollama": null}` or `{"ollama": {"url", "models"}}`.
    pub fn detect(&self) -> Result<Value, UiError> {
        self.run(&detect_args())
    }

    /// `provider check <URL> --json`: `{url, host, loopback, requires_consent, usable, reason}`.
    pub fn check(&self, url: &str) -> Result<Value, UiError> {
        self.run(&check_args(url)?)
    }

    /// `provider probe …` for the saved settings: `{available: true, provider, model, …}` or
    /// `{available: false, reason}`. A host nobody consented to is [`UiError::ConsentRequired`].
    pub fn probe(&self, settings: &Settings) -> Result<Value, UiError> {
        self.run(&probe_args(settings)?)
    }

    fn run(&self, args: &[String]) -> Result<Value, UiError> {
        let mut command = Command::new(&self.program);
        command
            .args(args)
            // Events as NDJSON on stderr, so a refusal is read by its code, never by its prose.
            .args(["--progress", "json"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // Nothing a conversion was given reaches a question about providers.
            .env_remove(PASSWORD_VAR)
            .env_remove(CACHE_VAR);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(crate::engine::CREATE_NO_WINDOW);
        }
        let output = command
            .output()
            .map_err(|error| UiError::Provider(format!("{}: {error}", self.program.display())))?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        if let Some(host) = consent_refusal(&stderr) {
            return Err(UiError::ConsentRequired { host });
        }
        // `probe` answers `{available: false}` with exit 1: an answer, not a failure.
        serde_json::from_slice(&output.stdout).map_err(|_| {
            UiError::Provider(
                stderr
                    .lines()
                    .find(|line| !line.trim().is_empty())
                    .unwrap_or("no answer")
                    .to_owned(),
            )
        })
    }
}

fn detect_args() -> Vec<String> {
    ["provider", "detect", "--json"].map(str::to_owned).to_vec()
}

fn check_args(url: &str) -> Result<Vec<String>, UiError> {
    let url = endpoint(url)?;
    Ok(vec![
        "provider".to_owned(),
        "check".to_owned(),
        url,
        "--json".to_owned(),
    ])
}

/// What "Test connection" asks: the provider the settings choose, as a conversion would open it.
fn probe_args(settings: &Settings) -> Result<Vec<String>, UiError> {
    let mut args = vec!["provider".to_owned(), "probe".to_owned()];
    match settings.provider {
        // The app's own server exists only while a job holds it; there is nothing to test.
        Provider::Builtin => {
            return Err(UiError::Provider(
                "the built-in model is tested by converting a book".to_owned(),
            ))
        }
        Provider::Ollama => {
            args.push(oc_net::detect::OLLAMA_DEFAULT_URL.to_owned());
            args.push("--llm-provider".to_owned());
            args.push("ollama".to_owned());
            if let Some(model) = &settings.ollama_model {
                args.push("--llm-model".to_owned());
                args.push(model.clone());
            }
        }
        Provider::Custom => {
            let custom = &settings.custom;
            let url = endpoint(&custom.endpoint)?;
            let host = endpoint_host(&url);
            args.push(url);
            let model = custom.model.trim();
            if !model.is_empty() {
                args.push("--llm-model".to_owned());
                args.push(model.to_owned());
            }
            // Consent travels only to the host it names (D10).
            if let Some(consent) = custom
                .consent
                .as_ref()
                .filter(|consent| host.as_deref() == Some(consent.host.as_str()))
            {
                args.push("--llm-allow-host".to_owned());
                args.push(consent.host.clone());
            }
            if let Some(file) = &custom.api_key_file {
                args.push("--llm-api-key-file".to_owned());
                args.push(file.to_string_lossy().into_owned());
            }
        }
    }
    args.push("--json".to_owned());
    Ok(args)
}

/// `url` as one argument: an `http://` or `https://` URL and nothing a command line could read as
/// anything else.
fn endpoint(url: &str) -> Result<String, UiError> {
    let url = url.trim();
    let scheme = url
        .get(..8)
        .is_some_and(|head| head.eq_ignore_ascii_case("https://"))
        || url
            .get(..7)
            .is_some_and(|head| head.eq_ignore_ascii_case("http://"));
    if !scheme || url.chars().any(char::is_control) {
        return Err(UiError::Provider(format!("{url:?} is not an http(s) URL")));
    }
    Ok(url.to_owned())
}

/// The host of an `E_CONSENT_REQUIRED` fatal on the engine's stderr, if there is one. The engine
/// names it in backticks (`ai_endpoint::OpenError::fatal`).
fn consent_refusal(stderr: &str) -> Option<String> {
    stderr.lines().find_map(|line| {
        let event: Value = serde_json::from_str(line).ok()?;
        (event["t"] == "fatal" && event["code"] == E_CONSENT_REQUIRED).then(|| {
            event["message"]
                .as_str()
                .and_then(|message| message.split('`').nth(1))
                .unwrap_or_default()
                .to_owned()
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Consent;

    /// The webview supplies a URL and nothing else, and a URL can never become a flag.
    #[test]
    fn a_provider_question_names_its_url_as_one_argument_and_refuses_anything_else() {
        assert_eq!(
            check_args("https://llm.example.org/v1").expect("a URL"),
            ["provider", "check", "https://llm.example.org/v1", "--json"]
        );
        for bad in [
            "--llm-allow-host=evil",
            "-rf",
            "ftp://x.example",
            "",
            "https://a\nb",
        ] {
            assert!(check_args(bad).is_err(), "{bad:?}");
        }
        assert_eq!(
            detect_args(),
            ["provider", "detect", "--json"],
            "detect takes nothing from the webview"
        );
    }

    /// "Test connection" asks what a conversion would open: the endpoint, the model, the key file,
    /// and the consent — to the host it names only.
    #[test]
    fn test_connection_probes_what_a_conversion_would_open() {
        let mut custom = Settings {
            provider: Provider::Custom,
            ..Settings::default()
        };
        custom.custom.endpoint = "https://llm.example.org/v1".to_owned();
        custom.custom.model = "qwen3-8b".to_owned();
        custom.custom.api_key_file = Some(PathBuf::from("/keys/llm.key"));
        assert_eq!(
            probe_args(&custom).expect("args"),
            [
                "provider",
                "probe",
                "https://llm.example.org/v1",
                "--llm-model",
                "qwen3-8b",
                "--llm-api-key-file",
                "/keys/llm.key",
                "--json"
            ],
            "no consent recorded: none is claimed"
        );
        custom.custom.consent = Some(Consent {
            host: "llm.example.org".to_owned(),
            granted_at: "2026-09-23T10:00:00Z".to_owned(),
        });
        let args = probe_args(&custom).expect("args");
        let at = args
            .iter()
            .position(|arg| arg == "--llm-allow-host")
            .expect("consent travels");
        assert_eq!(args[at + 1], "llm.example.org");

        let ollama = Settings {
            provider: Provider::Ollama,
            ollama_model: Some("qwen3:1.7b".to_owned()),
            ..Settings::default()
        };
        assert_eq!(
            probe_args(&ollama).expect("args"),
            [
                "provider",
                "probe",
                "http://localhost:11434",
                "--llm-provider",
                "ollama",
                "--llm-model",
                "qwen3:1.7b",
                "--json"
            ]
        );
        assert!(
            probe_args(&Settings::default()).is_err(),
            "built-in: nothing to probe"
        );
    }

    #[test]
    fn a_consent_refusal_names_its_host() {
        let stderr = concat!(
            r#"{"v":1,"t":"hello","seq":0}"#,
            "\n",
            r#"{"v":1,"t":"fatal","seq":1,"code":"E_CONSENT_REQUIRED","message":"`llm.example.org` is not this computer: with --ai, text from the book would be sent there."}"#,
            "\n"
        );
        assert_eq!(consent_refusal(stderr).as_deref(), Some("llm.example.org"));
        assert_eq!(consent_refusal("error: something else"), None);
    }
}
