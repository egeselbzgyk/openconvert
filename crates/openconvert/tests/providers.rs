//! PHASE 11 in the engine: which provider `convert --ai` opens, what it needs consent for, and what
//! it never lets out. The library tests reach endpoints through an in-process [`Connector`], so a
//! host off this machine can be exercised without a network; the binary tests use a model server on
//! `127.0.0.1`.

mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::endpoint::{Endpoint, LLAMA_PROPS};
use oc_ai::provider::{Constraint, ProviderKind, ThinkingControl};
use oc_ai::transport::{Transport, TransportError};
use oc_core::exit::ExitCode;
use oc_core::thresholds::T;
use oc_net::consent::ConsentRecord;
use oc_net::NetError;
use openconvert::ai_endpoint::{open_with, AiArgs, Connector, OpenError, E_CONSENT_REQUIRED};
use secrecy::SecretString;

const REGISTRY: &str = include_str!("../../../models.toml");

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_openconvert"))
}

/// A scratch directory of its own per test, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("oc-providers-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("a scratch directory");
        Self(path)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// What a connection was asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Connected {
    base: String,
    keyed: bool,
    consent: Option<String>,
}

/// An in-process network: every base URL answers GETs from one route table, and every connection
/// is recorded — which is how "no bytes were sent" is a count of zero, not an absence of evidence.
#[derive(Clone, Default)]
struct Fake {
    routes: Arc<BTreeMap<String, String>>,
    connected: Arc<Mutex<Vec<Connected>>>,
    asked: Arc<Mutex<Vec<String>>>,
}

impl Fake {
    fn serving(routes: &[(&str, &str)]) -> Self {
        Self {
            routes: Arc::new(
                routes
                    .iter()
                    .map(|(path, body)| ((*path).to_owned(), (*body).to_owned()))
                    .collect(),
            ),
            ..Self::default()
        }
    }

    fn connected(&self) -> Vec<Connected> {
        self.connected.lock().expect("not poisoned").clone()
    }
}

impl Transport for Fake {
    fn post_json(&self, path: &str, _: &str, _: Duration) -> Result<String, TransportError> {
        self.asked
            .lock()
            .expect("not poisoned")
            .push(format!("POST {path}"));
        Err(TransportError::Status { status: 500 })
    }

    fn get(&self, path: &str, _: Duration) -> Result<String, TransportError> {
        self.asked
            .lock()
            .expect("not poisoned")
            .push(format!("GET {path}"));
        self.routes
            .get(path)
            .cloned()
            .ok_or(TransportError::Status { status: 404 })
    }
}

impl Connector for Fake {
    fn connect(
        &self,
        base: &str,
        api_key: Option<SecretString>,
        consent: Option<&ConsentRecord>,
    ) -> Result<Box<dyn Transport>, NetError> {
        oc_net::consent::authorize(base, consent)?;
        self.connected
            .lock()
            .expect("not poisoned")
            .push(Connected {
                base: base.to_owned(),
                keyed: api_key.is_some(),
                consent: consent.map(|consent| consent.host.clone()),
            });
        Ok(Box::new(self.clone()))
    }
}

fn args(endpoint: &str) -> AiArgs {
    AiArgs {
        endpoint: Some(endpoint.to_owned()),
        ..AiArgs::default()
    }
}

const TAGS: &str = r#"{"models":[{"name":"qwen3:1.7b"},{"name":"llama3.2:3b"}]}"#;

/// Row 11.5 and A11.2. A host that is not this machine, with no consent: exit 2, a `fatal` with
/// `E_CONSENT_REQUIRED` that names the host — and nothing was connected to, so no byte was sent.
/// Consent for some other host is no consent for this one.
#[test]
fn non_loopback_requires_consent() {
    let network = Fake::serving(&[("/props", LLAMA_PROPS)]);
    for allow in [None, Some("elsewhere.example.org")] {
        let refused = open_with(
            &AiArgs {
                allow_host: allow.map(str::to_owned),
                ..args("https://example.com/v1")
            },
            REGISTRY,
            &T,
            &network,
        )
        .err()
        .expect("refused");
        assert_eq!(
            refused,
            OpenError::ConsentRequired {
                host: "example.com".to_owned()
            }
        );
        assert_eq!(refused.exit_code(), ExitCode::Usage);
        let (code, message) = refused.fatal().expect("a fatal event");
        assert_eq!(code, E_CONSENT_REQUIRED);
        assert!(message.contains("example.com"), "{message}");
        assert!(
            message.contains("--llm-allow-host example.com"),
            "says how: {message}"
        );
    }
    assert!(network.connected().is_empty(), "no connection was made");

    // The binary: exit 2, the fatal event on the NDJSON channel, no book and no report.
    let scratch = Scratch::new("consent");
    let output = Command::new(binary())
        .arg("convert")
        .arg(common::fixture("f01_prose_single_column"))
        .arg("-o")
        .arg(scratch.join("remote.epub"))
        .args([
            "--ai",
            "--llm-endpoint",
            "https://example.com/v1",
            "--progress",
            "json",
        ])
        .env("XDG_DATA_HOME", scratch.join("data"))
        .output()
        .expect("the binary runs");
    assert_eq!(output.status.code(), Some(2));
    let fatal = String::from_utf8_lossy(&output.stderr)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|event| event["t"] == "fatal")
        .expect("a fatal event");
    assert_eq!(fatal["code"], E_CONSENT_REQUIRED);
    assert!(fatal["message"]
        .as_str()
        .is_some_and(|message| message.contains("example.com")));
    assert!(!scratch.join("remote.epub").exists());
    assert!(!scratch.join("remote.epub.report.json").exists());
}

/// Consent naming the host opens it — the connection carries the consent, and the record says
/// which host and when — and plain http off the machine is refused even with it.
#[test]
fn consent_naming_the_host_opens_it() {
    let network = Fake::serving(&[("/props", LLAMA_PROPS)]);
    let opened = open_with(
        &AiArgs {
            allow_host: Some("Books.Example.org".to_owned()),
            model: Some("qwen3-8b".to_owned()),
            ..args("https://books.example.org/v1")
        },
        REGISTRY,
        &T,
        &network,
    )
    .expect("consent was given");
    let consent = opened.consent.as_ref().expect("the consent is recorded");
    assert_eq!(consent.host, "books.example.org");
    assert_eq!(
        network.connected(),
        [Connected {
            base: "https://books.example.org".to_owned(),
            keyed: false,
            consent: Some("books.example.org".to_owned()),
        }]
    );

    let plain = open_with(
        &AiArgs {
            allow_host: Some("10.0.0.2".to_owned()),
            ..args("http://10.0.0.2:8080")
        },
        REGISTRY,
        &T,
        &network,
    )
    .err()
    .expect("refused");
    assert_eq!(plain.exit_code(), ExitCode::Usage);
    assert!(
        plain
            .fatal()
            .is_some_and(|(_, message)| message.contains("https")),
        "{plain:?}"
    );

    // Loopback needs none, and records none, whatever the command line says.
    let local = open_with(
        &AiArgs {
            allow_host: Some("127.0.0.1".to_owned()),
            ..args("http://127.0.0.1:8080")
        },
        REGISTRY,
        &T,
        &network,
    )
    .expect("loopback");
    assert!(local.consent.is_none());
}

/// The probe decides the adapter when the command line does not: `llama-server` is the sidecar
/// adapter (GBNF), Ollama is Ollama, anything else is a custom endpoint that constrains nothing.
#[test]
fn the_probe_decides_the_adapter() {
    let llama = open_with(
        &args("http://127.0.0.1:8080"),
        REGISTRY,
        &T,
        &Fake::serving(&[("/props", LLAMA_PROPS)]),
    )
    .expect("opened");
    assert_eq!(llama.kind, ProviderKind::LocalSidecar);
    assert_eq!(llama.provider.capabilities().constraint, Constraint::Gbnf);
    assert_eq!(
        llama.provider.thinking_control(),
        ThinkingControl::ChatTemplateKwargs
    );
    assert_eq!(llama.provider.id(), "endpoint@127.0.0.1");

    let ollama = open_with(
        &AiArgs {
            model: Some("qwen3:1.7b".to_owned()),
            ..args("http://localhost:11434")
        },
        REGISTRY,
        &T,
        &Fake::serving(&[("/api/tags", TAGS)]),
    )
    .expect("opened");
    assert_eq!(ollama.kind, ProviderKind::Ollama);
    assert_eq!(ollama.provider.id(), "qwen3:1.7b");
    assert_eq!(
        ollama.provider.thinking_control(),
        ThinkingControl::OllamaThink
    );

    let studio = open_with(
        &args("http://127.0.0.1:1234/v1"),
        REGISTRY,
        &T,
        &Fake::serving(&[("/v1/models", r#"{"data":[{"id":"qwen3-8b-instruct"}]}"#)]),
    )
    .expect("opened");
    assert_eq!(studio.kind, ProviderKind::OpenAiCompatible);
    assert_eq!(studio.provider.capabilities().constraint, Constraint::None);
    assert_eq!(
        studio.provider.id(),
        "qwen3-8b-instruct",
        "the one it lists"
    );
    assert_eq!(
        studio.provider.thinking_control(),
        ThinkingControl::NoThinkSuffix
    );
}

/// `--llm-provider ollama` with no endpoint is Ollama on `localhost:11434`; which model is asked is
/// never guessed: the one named, or the only one there is.
#[test]
fn ollama_is_found_on_localhost_and_its_model_is_never_guessed() {
    let ollama = || AiArgs {
        provider: Some(ProviderKind::Ollama),
        ..AiArgs::default()
    };
    let network = Fake::serving(&[("/api/tags", TAGS)]);
    let opened = open_with(
        &AiArgs {
            model: Some("llama3.2:3b".to_owned()),
            ..ollama()
        },
        REGISTRY,
        &T,
        &network,
    )
    .expect("opened");
    assert_eq!(opened.kind, ProviderKind::Ollama);
    assert_eq!(network.connected()[0].base, "http://localhost:11434");
    assert!(opened.consent.is_none(), "loopback: no consent");

    for (model, reason) in [
        (
            None,
            "Ollama serves more than one model; name one with --llm-model",
        ),
        (Some("mistral:7b"), "the model is not one Ollama serves"),
    ] {
        let unavailable = open_with(
            &AiArgs {
                model: model.map(str::to_owned),
                ..ollama()
            },
            REGISTRY,
            &T,
            &network,
        )
        .err()
        .expect("not opened");
        assert_eq!(unavailable, OpenError::Unavailable(reason));
        assert_eq!(unavailable.exit_code(), ExitCode::Ok, "never a failure");
    }

    let one = Fake::serving(&[("/api/tags", r#"{"models":[{"name":"qwen3:1.7b"}]}"#)]);
    let opened = open_with(&ollama(), REGISTRY, &T, &one).expect("the only model");
    assert_eq!(opened.provider.id(), "qwen3:1.7b");

    let nothing = Fake::serving(&[]);
    assert_eq!(
        open_with(&ollama(), REGISTRY, &T, &nothing).err(),
        Some(OpenError::Unavailable(
            "the endpoint did not answer the capability probe"
        ))
    );
}

/// Row 11.8. The key is read from a file, sent to the endpoint as a bearer token and nowhere else:
/// it is in no NDJSON event, no report field, no line on either channel, nothing the engine wrote,
/// and no argv — the endpoint is someone else's, so no process is started, and the command line has
/// no flag that would take the key inline. (The engine-owned sidecar's key goes in its environment,
/// never its argv: PHASE 9 rows 9.12 and 9.14.)
#[test]
fn api_key_is_never_logged_or_echoed() {
    const KEY: &str = "sk-oc-phase11-9f3a7c21d4e8b6";
    let endpoint = Endpoint::start(&[
        ("GET", "/props", 200, LLAMA_PROPS),
        ("POST", "/v1/chat/completions", 500, r#"{"error":"boom"}"#),
    ]);
    let scratch = Scratch::new("key");
    let key_file = scratch.join("llm.key");
    std::fs::write(&key_file, format!("{KEY}\n")).expect("the key file");

    let output = Command::new(binary())
        .arg("convert")
        .arg(common::fixture("f07_verse_and_quote"))
        .arg("-o")
        .arg(scratch.join("book.epub"))
        .args(["--modified", "2026-01-01T00:00:00Z", "--lang", "en"])
        .args(["--progress", "json", "--ai", "--ai-all-tasks"])
        .args(["--llm-endpoint", &endpoint.url(), "--llm-api-key-file"])
        .arg(&key_file)
        .env("XDG_DATA_HOME", scratch.join("data"))
        .env("RUST_LOG", "trace")
        .env("RUST_BACKTRACE", "1")
        .output()
        .expect("the binary runs");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    // It was used: every request carried it, as a bearer token.
    let received = endpoint.received();
    assert!(
        received.iter().any(|request| request.method == "POST"),
        "a question was sent"
    );
    for request in &received {
        assert!(
            request
                .headers
                .iter()
                .any(|header| header.eq_ignore_ascii_case(&format!("authorization: Bearer {KEY}"))),
            "{} {} without the key",
            request.method,
            request.path
        );
        assert!(!request.path.contains(KEY) && !request.body.contains(KEY));
    }

    // And it is nowhere else.
    let report = std::fs::read_to_string(scratch.join("book.epub.report.json")).expect("report");
    for (what, text) in [
        (
            "stdout",
            String::from_utf8_lossy(&output.stdout).into_owned(),
        ),
        (
            "stderr",
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ),
        ("the report", report),
    ] {
        assert!(!text.contains(KEY), "the key is in {what}");
    }
    // Nor in anything the engine wrote: the book, and the answer cache in the data directory.
    let mut written = vec![scratch.join("book.epub")];
    written.extend(files_under(&scratch.join("data")));
    for path in written {
        let bytes = std::fs::read(&path).expect("readable");
        assert!(
            !bytes
                .windows(KEY.len())
                .any(|window| window == KEY.as_bytes()),
            "the key is in {}",
            path.display()
        );
    }

    // No inline key: the flag does not exist, and refusing it does not echo what followed it.
    for flag in ["--llm-api-key", "--api-key"] {
        let inline = Command::new(binary())
            .arg("convert")
            .arg(common::fixture("f01_prose_single_column"))
            .args(["--ai", flag, KEY])
            .output()
            .expect("the binary runs");
        assert_eq!(inline.status.code(), Some(2), "{flag}");
        assert!(
            !String::from_utf8_lossy(&inline.stderr).contains(KEY),
            "{flag}"
        );
        assert!(
            !String::from_utf8_lossy(&inline.stdout).contains(KEY),
            "{flag}"
        );
    }
}

/// Every file under `root`, however deep.
fn files_under(root: &std::path::Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(&directory)
            .into_iter()
            .flatten()
            .flatten()
        {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files
}

/// The provider flags mean nothing without `--ai`, and an unknown provider is refused by name.
#[test]
fn provider_flags_need_ai_and_a_known_provider() {
    for extra in [
        &["--llm-allow-host", "example.com"][..],
        &["--llm-model", "qwen3:1.7b"],
        &["--llm-provider", "ollama"],
        &["--ai", "--llm-provider", "openai"],
        &["--ai", "--llm-provider", "openai-compatible"],
    ] {
        let output = Command::new(binary())
            .arg("convert")
            .arg(common::fixture("f01_prose_single_column"))
            .args(["-o", "/nonexistent/never-written.epub"])
            .args(extra)
            .output()
            .expect("the binary runs");
        assert_eq!(output.status.code(), Some(2), "{extra:?}");
    }
}

/// Row 11.7. With consent, the report says that text left the machine: the host, when consent was
/// given, and for how long — and a conversion that needed no consent has no such section.
#[test]
fn consent_is_recorded_in_report() {
    use oc_ai::session::{Clock, SystemClock};
    use openconvert::ai::AiContext;
    use openconvert::report::{report, to_json, ReportInput};

    let network = Fake::serving(&[("/props", LLAMA_PROPS)]);
    let opened = open_with(
        &AiArgs {
            allow_host: Some("books.example.org".to_owned()),
            all_tasks: true,
            ..args("https://books.example.org/v1")
        },
        REGISTRY,
        &T,
        &network,
    )
    .expect("consent was given");
    let record = opened.consent.clone().expect("a consent record");

    let backend = oc_pdf::pdfium::PdfiumBackend::bind().expect("PDFium");
    let bytes = std::fs::read(common::fixture("f07_verse_and_quote")).expect("the fixture");
    let clock = SystemClock::new();
    let context = AiContext {
        provider: opened.provider.as_ref(),
        cache: None,
        clock: &clock,
        started_ms: clock.now_ms(),
        all_tasks: true,
    };
    let options = openconvert::convert::ConvertOptions {
        filename: "f07_verse_and_quote.pdf".to_owned(),
        language: Some(oc_model::lang::LangTag::new("en")),
        preset: oc_model::document::PresetName::Auto,
        epub: common::epub_options(),
        ocr: openconvert::ocr::OcrOptions::off(),
    };
    let conversion = openconvert::convert::convert_bytes_with_ai(
        &backend,
        &bytes,
        None,
        &options,
        Some(&context),
        &T,
    )
    .expect("converted");
    assert!(
        network
            .asked
            .lock()
            .expect("not poisoned")
            .iter()
            .any(|asked| asked.starts_with("POST")),
        "a question went to the consented host"
    );

    let written = |consent: Option<&ConsentRecord>| -> serde_json::Value {
        let report = report(
            &conversion,
            ReportInput {
                filename: &options.filename,
                pdfium_version: "test",
                producer_family: conversion.producer_family,
                pages: 1,
                page_classes: conversion.page_classes.clone(),
                provider: Some(opened.kind),
                consent,
            },
        );
        serde_json::from_str(&to_json(&report).expect("serialises")).expect("JSON")
    };

    let json = written(Some(&record));
    assert_eq!(json["consent"]["host"], "books.example.org");
    assert_eq!(json["consent"]["granted_at"], record.granted_at_rfc3339());
    assert_eq!(json["consent"]["scope"], "run");
    assert!(json["consent"]["granted_at"]
        .as_str()
        .is_some_and(|stamp| stamp.ends_with('Z') && stamp.contains('T')));
    assert_eq!(json["ai"]["provider"], "local_sidecar");

    let json = written(None);
    assert!(json.get("consent").is_none(), "no consent, no section");
}

/// Row 11.9 and A11.4. An endpoint that answers its probe and then fails every question — a 500 —
/// and one that answers nothing at all: each book is the deterministic book byte for byte, exit 0,
/// and the report says why the model did not help. For every adapter.
#[test]
fn provider_failure_degrades_to_deterministic() {
    let scratch = Scratch::new("degrade");
    let convert = |name: &str, extra: &[&str]| {
        Command::new(binary())
            .arg("convert")
            .arg(common::fixture("f07_verse_and_quote"))
            .arg("-o")
            .arg(scratch.join(&format!("{name}.epub")))
            .args(["--modified", "2026-01-01T00:00:00Z", "--lang", "en"])
            .args(extra)
            .env("XDG_DATA_HOME", scratch.join("data"))
            .output()
            .expect("the binary runs")
    };
    let plain = convert("plain", &["--no-ai"]);
    assert!(plain.status.success());
    let deterministic = std::fs::read(scratch.join("plain.epub")).expect("the book");

    let failing = [
        (
            "llama",
            Endpoint::start(&[
                ("GET", "/props", 200, LLAMA_PROPS),
                ("POST", "/v1/chat/completions", 500, r#"{"error":"boom"}"#),
            ]),
            &[][..],
            "the endpoint refused the request",
        ),
        (
            "ollama",
            Endpoint::start(&[
                (
                    "GET",
                    "/api/tags",
                    200,
                    r#"{"models":[{"name":"qwen3:1.7b"}]}"#,
                ),
                ("POST", "/api/chat", 500, r#"{"error":"boom"}"#),
            ]),
            &[][..],
            "the endpoint refused the request",
        ),
        (
            "generic",
            Endpoint::start(&[
                (
                    "GET",
                    "/v1/models",
                    200,
                    r#"{"data":[{"id":"some-model"}]}"#,
                ),
                ("POST", "/v1/chat/completions", 500, r#"{"error":"boom"}"#),
            ]),
            &[][..],
            "the endpoint refused the request",
        ),
        (
            "silent",
            Endpoint::start(&[]),
            &["--llm-provider", "builtin"][..],
            "the endpoint did not answer the capability probe",
        ),
    ];
    for (name, endpoint, extra, reason) in &failing {
        let url = endpoint.url();
        let mut flags = vec!["--ai", "--ai-all-tasks", "--llm-endpoint", url.as_str()];
        flags.extend_from_slice(extra);
        let output = convert(name, &flags);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            std::fs::read(scratch.join(&format!("{name}.epub"))).expect("the book"),
            deterministic,
            "{name}: the deterministic book"
        );
        let report: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(scratch.join(&format!("{name}.epub.report.json")))
                .expect("the report"),
        )
        .expect("JSON");
        let banner = report["warnings"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|warning| warning["code"] == "W_LLM_UNAVAILABLE")
            .unwrap_or_else(|| panic!("{name}: no W_LLM_UNAVAILABLE"));
        assert_eq!(banner["args"]["reason"], *reason, "{name}");
    }
}

/// `openconvert provider …` as JSON on stdout: what the desktop's Provider settings read.
fn provider_cmd(args: &[&str]) -> (Option<i32>, serde_json::Value, String) {
    let output = Command::new(binary())
        .arg("provider")
        .args(args)
        .args(["--json", "--progress", "json"])
        .output()
        .expect("the binary runs");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let json = serde_json::from_str(&stdout).unwrap_or(serde_json::Value::Null);
    (
        output.status.code(),
        json,
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// `provider detect`: Ollama on `localhost:11434`, or nothing — never an error, whichever this
/// machine has.
#[test]
fn provider_detect_reports_ollama_or_nothing() {
    let (code, json, stderr) = provider_cmd(&["detect"]);
    assert_eq!(code, Some(0), "{stderr}");
    match &json["ollama"] {
        serde_json::Value::Null => {}
        found => {
            assert_eq!(found["url"], "http://localhost:11434");
            assert!(found["models"].is_array());
        }
    }
    assert!(json.get("ollama").is_some(), "{json}");
}

/// `provider check <URL>`: whether consent is needed, before anything is sent — nothing is.
#[test]
fn provider_check_says_whether_consent_is_needed() {
    for (url, host, requires_consent, usable) in [
        ("http://localhost:11434", "localhost", false, true),
        ("http://[::1]:8080/v1", "::1", false, true),
        ("https://llm.example.org/v1", "llm.example.org", true, true),
        ("http://192.168.1.20:11434", "192.168.1.20", true, false),
    ] {
        let (code, json, stderr) = provider_cmd(&["check", url]);
        assert_eq!(code, Some(0), "{url}: {stderr}");
        assert_eq!(json["host"], host, "{url}");
        assert_eq!(json["requires_consent"], requires_consent, "{url}");
        assert_eq!(json["usable"], usable, "{url}");
        assert_eq!(json["loopback"], !requires_consent, "{url}");
        if !usable {
            assert!(json["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("https")));
        }
    }
    let (code, _, stderr) = provider_cmd(&["check", "http://user@localhost"]);
    assert_eq!(code, Some(2), "not a URL the engine will use: {stderr}");
}

/// `provider probe <URL>`: what `convert --ai` would open there — the adapter, what it can
/// constrain, the model — under the same consent rule; an endpoint that does not answer is exit 1
/// with the reason.
#[test]
fn provider_probe_answers_what_convert_would_open() {
    let ollama = Endpoint::start(&[(
        "GET",
        "/api/tags",
        200,
        r#"{"models":[{"name":"qwen3:1.7b"},{"name":"llama3.2:3b"}]}"#,
    )]);
    let (code, json, stderr) = provider_cmd(&["probe", &ollama.url(), "--llm-model", "qwen3:1.7b"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(json["available"], true);
    assert_eq!(json["provider"], "ollama");
    assert_eq!(json["constraint"], "json_schema");
    assert_eq!(json["model"], "qwen3:1.7b");
    assert_eq!(
        json["models"],
        serde_json::json!(["qwen3:1.7b", "llama3.2:3b"])
    );
    assert!(json["consent"].is_null());
    assert!(
        ollama
            .received()
            .iter()
            .all(|request| request.method == "GET"),
        "a probe asks, it never sends a question"
    );

    let (code, json, _) = provider_cmd(&["probe", &ollama.url()]);
    assert_eq!(code, Some(1), "two models and none named");
    assert_eq!(json["available"], false);
    assert_eq!(
        json["reason"],
        "Ollama serves more than one model; name one with --llm-model"
    );

    let llama = Endpoint::start(&[("GET", "/props", 200, LLAMA_PROPS)]);
    let (code, json, _) = provider_cmd(&["probe", &llama.url()]);
    assert_eq!(code, Some(0));
    assert_eq!(json["provider"], "local_sidecar");
    assert_eq!(json["constraint"], "gbnf");

    let (code, json, stderr) = provider_cmd(&["probe", "https://llm.example.org/v1"]);
    assert_eq!(code, Some(2), "{json}");
    let fatal = stderr
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|event| event["t"] == "fatal")
        .expect("a fatal event");
    assert_eq!(fatal["code"], E_CONSENT_REQUIRED);

    let silent = Endpoint::start(&[]);
    let (code, json, _) = provider_cmd(&["probe", &silent.url()]);
    assert_eq!(code, Some(1));
    assert_eq!(
        json["reason"],
        "the endpoint did not answer the capability probe"
    );

    for wrong in [
        &["detect", "http://localhost:11434"][..],
        &["check"],
        &["probe"],
        &["detect", "--llm-model", "x"],
        &["frobnicate"],
    ] {
        let (code, _, _) = provider_cmd(wrong);
        assert_eq!(code, Some(2), "{wrong:?}");
    }
}

/// A11.1 against a **real** Ollama (nightly, `--features live-llm`): Ollama on `localhost:11434`
/// serving `OC_LIVE_OLLAMA_MODEL`, found by `--llm-provider ollama` with no endpoint given. Every
/// fixture that escalates a task comes back whole — exit 0, no `W_LLM_UNAVAILABLE`, the report
/// naming Ollama, at most `llm.max_calls_per_book` calls, and I-7 holding. That every request
/// overrode `num_ctx` and carried the schema in `format` is rows 11.2 and 11.3, over the same
/// adapter code.
///
/// With the feature on and the variable unset this fails: a live test that passes without a
/// model is not a live test.
#[cfg(feature = "live-llm")]
#[test]
fn ai_against_a_live_ollama_converts_every_book() {
    let model = std::env::var("OC_LIVE_OLLAMA_MODEL")
        .unwrap_or_else(|_| panic!("--features live-llm needs OC_LIVE_OLLAMA_MODEL"));
    let scratch = Scratch::new("live-ollama");
    for stem in [
        "f03_image_only",
        "f07_verse_and_quote",
        "f10_lists_and_table",
    ] {
        let output = Command::new(binary())
            .arg("convert")
            .arg(common::fixture(stem))
            .arg("-o")
            .arg(scratch.join(&format!("{stem}.epub")))
            .args(["--lang", "en", "--ai", "--ai-all-tasks"])
            .args(["--llm-provider", "ollama", "--llm-model", &model])
            .env("XDG_DATA_HOME", scratch.join("data"))
            .output()
            .expect("the binary runs");
        assert_eq!(
            output.status.code(),
            Some(0),
            "{stem}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(scratch.join(&format!("{stem}.epub.report.json")))
                .expect("the report"),
        )
        .expect("JSON");
        assert!(
            report["warnings"]
                .as_array()
                .into_iter()
                .flatten()
                .all(|warning| warning["code"] != "W_LLM_UNAVAILABLE"),
            "{stem}"
        );
        assert_eq!(report["ai"]["provider"], "ollama", "{stem}");
        let calls = report["ai"]["calls"].as_u64().unwrap_or(u64::MAX);
        assert!(
            calls <= u64::try_from(T.llm.max_calls_per_book).unwrap_or(0),
            "{stem}: {calls} calls"
        );
        assert_eq!(report["conservation"]["i7"]["holds"], true, "{stem}: I-7");
    }
}
