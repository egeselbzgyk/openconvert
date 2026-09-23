//! Where `convert --ai` finds its model: somebody else's endpoint, or a server the engine starts
//! and stops (PHASE 9 detail 3, PHASE 10, RT C2, D8).
//!
//! **Never a failure of the conversion.** A server that cannot be started, a model that is not
//! installed, an endpoint that does not answer: each is a reason, the book is converted by the
//! deterministic path alone, and the report carries `W_LLM_UNAVAILABLE` saying why (RT D20). The
//! one refusal is a usage error: an endpoint that is not on this machine. Sending a book's text
//! off the machine needs consent the command line cannot give yet (D10; Phase 11 adds it).

use std::path::{Path, PathBuf};
use std::time::Duration;

use oc_ai::openai::{ClientConfig, OpenAiCompatible};
use oc_ai::provider::{Constraint, LlmProvider, ThinkingControl};
use oc_core::sidecar::llama::ServerSpec;
use oc_core::sidecar::server::{read_key_file, Health, OwnedServer};
use oc_core::thresholds::Thresholds;
use oc_net::transport::HttpTransport;

/// `--ai` and the flags that only mean something with it (§2.1, PHASE 10).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AiArgs {
    /// `--llm-endpoint <URL>`: somebody else's OpenAI-compatible server; the engine spawns
    /// nothing (RT C2).
    pub endpoint: Option<String>,
    /// `--llm-api-key-file <PATH>`: never an inline key.
    pub api_key_file: Option<PathBuf>,
    /// `--model-path <PATH>`: the GGUF the engine-owned sidecar loads.
    pub model_path: Option<PathBuf>,
    /// `--ai-all-tasks`: run every escalated task for every language, including those the
    /// evaluation has not enabled (`ai.task.<task>.languages`, PHASE 10 detail 7).
    pub all_tasks: bool,
}

/// The environment variable that names the `llama-server` to start, as the live tests use it.
pub const LLAMA_SERVER_ENV: &str = "OC_LLAMA_SERVER";

/// A provider the conversion can ask, and the server behind it when the engine owns one. The
/// server is killed when this is dropped.
pub struct Opened {
    pub provider: Box<dyn LlmProvider>,
    pub server: Option<OwnedServer>,
}

/// Why no provider could be opened.
#[derive(Debug, PartialEq, Eq)]
pub enum OpenError {
    /// A usage error: exit 2, nothing converted.
    Refused(String),
    /// Convert without the model, and say why (`W_LLM_UNAVAILABLE`'s `reason`).
    Unavailable(&'static str),
}

/// Open the provider `args` describe.
pub fn open(args: &AiArgs, registry_text: &str, t: &Thresholds) -> Result<Opened, OpenError> {
    let config = |model_id: String| ClientConfig {
        model_id,
        constraint: Constraint::Gbnf,
        thinking: ThinkingControl::ChatTemplateKwargs,
        temperature: t.llm.temperature as f32,
        timeout: Duration::from_secs(u64::try_from(t.llm.call_timeout_secs).unwrap_or_default()),
    };

    if let Some(url) = &args.endpoint {
        let host = host_of(url).ok_or_else(|| {
            OpenError::Refused(format!("`{url}` is not an http:// or https:// URL"))
        })?;
        if !is_loopback(&host) {
            return Err(OpenError::Refused(format!(
                "--llm-endpoint names `{host}`, which is not this machine; a book's text is not \
                 sent off the machine without consent the command line cannot give yet"
            )));
        }
        let key = match &args.api_key_file {
            Some(path) => Some(
                read_key_file(path)
                    .map_err(|_| OpenError::Unavailable("the API key file could not be read"))?,
            ),
            None => None,
        };
        let transport = HttpTransport::new(url, key)
            .map_err(|_| OpenError::Refused(format!("`{url}` is not a usable URL")))?;
        let model_id = args
            .model_path
            .as_deref()
            .and_then(Path::file_stem)
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("endpoint@{host}"));
        return Ok(Opened {
            provider: Box::new(OpenAiCompatible::new(transport, config(model_id))),
            server: None,
        });
    }

    // An engine-owned server: a program, a model, a port, a key.
    let program = llama_server().ok_or(OpenError::Unavailable("no llama-server was found"))?;
    let registry = oc_net::registry::ModelRegistry::parse(registry_text).ok();
    let (model, entry) = match &args.model_path {
        Some(path) => {
            let entry = registry.as_ref().and_then(|registry| {
                let name = path.file_name()?.to_string_lossy().into_owned();
                registry
                    .entries()
                    .iter()
                    .find(|entry| entry.file == name)
                    .cloned()
            });
            (path.clone(), entry)
        }
        None => {
            let registry = registry.ok_or(OpenError::Unavailable("no model is installed"))?;
            let entry = registry
                .get(registry.default_id())
                .cloned()
                .ok_or(OpenError::Unavailable("no model is installed"))?;
            let store = oc_net::store::ModelStore::new(crate::data_dir::models());
            let path = store
                .path_of(&entry)
                .map_err(|_| OpenError::Unavailable("no model is installed"))?;
            (path, Some(entry))
        }
    };
    if !model.is_file() {
        return Err(OpenError::Unavailable("the model file does not exist"));
    }

    let spec = ServerSpec {
        model: model.clone(),
        context: entry.as_ref().map_or(
            u32::try_from(t.llm.sidecar_context_tokens).unwrap_or_default(),
            |entry| entry.context,
        ),
        threads: std::thread::available_parallelism()
            .map(|threads| u32::try_from(threads.get()).unwrap_or(u32::MAX))
            .unwrap_or(1),
        cache_reuse: entry.as_ref().is_some_and(|entry| entry.cache_reuse),
        context_checkpoints: entry.as_ref().and_then(|entry| entry.context_checkpoints),
    };
    let port = oc_net::loopback::free_port()
        .map_err(|_| OpenError::Unavailable("no loopback port was free"))?;
    let server = OwnedServer::spawn(&program, &spec, port)
        .map_err(|_| OpenError::Unavailable("the model server could not be started"))?;
    let probe_timeout =
        Duration::from_millis(u64::try_from(t.llm.health_probe_timeout_millis).unwrap_or_default());
    server
        .wait_healthy(
            &|server: &OwnedServer| match HttpTransport::new(&server.base_url(), None)
                .ok()
                .map(|transport| transport.get("/health", probe_timeout))
            {
                Some(Ok(_)) => Health::Ready,
                _ => Health::NotYet,
            },
            Duration::from_secs(u64::try_from(t.llm.load_timeout_secs).unwrap_or_default()),
        )
        .map_err(|_| OpenError::Unavailable("the model server did not become ready"))?;
    let transport = HttpTransport::new(&server.base_url(), Some(server.api_key().clone()))
        .map_err(|_| OpenError::Unavailable("the model server's address is not usable"))?;
    let model_id = entry.map_or_else(
        || {
            model
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default()
        },
        |entry| entry.id.0,
    );
    Ok(Opened {
        provider: Box::new(OpenAiCompatible::new(transport, config(model_id))),
        server: Some(server),
    })
}

/// The `llama-server` to start: `OC_LLAMA_SERVER`, or the one bundled beside this program
/// (the desktop bundle's `externalBin`, D8).
fn llama_server() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(LLAMA_SERVER_ENV).map(PathBuf::from) {
        return path.is_file().then_some(path);
    }
    let name = format!("llama-server{}", std::env::consts::EXE_SUFFIX);
    let beside = std::env::current_exe().ok()?.parent()?.join(name);
    beside.is_file().then_some(beside)
}

/// The host of an `http://` or `https://` URL, lower-cased, without its port or brackets.
pub fn host_of(url: &str) -> Option<String> {
    let lower = url.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("http://")
        .or_else(|| lower.strip_prefix("https://"))?;
    let authority = rest.split(['/', '?', '#']).next()?;
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let host = match authority.strip_prefix('[') {
        Some(bracketed) => bracketed.split(']').next()?,
        None => authority.split(':').next()?,
    };
    (!host.is_empty()).then(|| host.to_owned())
}

/// Whether a host is this machine.
pub fn is_loopback(host: &str) -> bool {
    host == "localhost"
        || host == "::1"
        || host
            .parse::<std::net::Ipv4Addr>()
            .is_ok_and(|ip| ip.is_loopback())
}

#[test]
fn only_this_machine_is_loopback() {
    for (url, host, loopback) in [
        ("http://127.0.0.1:8080/v1", "127.0.0.1", true),
        ("http://localhost:11434", "localhost", true),
        ("http://[::1]:9000", "::1", true),
        ("https://api.example.com/v1", "api.example.com", false),
        (
            "http://127.0.0.1.example.com",
            "127.0.0.1.example.com",
            false,
        ),
        ("http://10.0.0.2:8080", "10.0.0.2", false),
    ] {
        assert_eq!(host_of(url).as_deref(), Some(host), "{url}");
        assert_eq!(is_loopback(host), loopback, "{url}");
    }
    assert_eq!(host_of("ftp://127.0.0.1"), None);
    assert_eq!(host_of("http://user@127.0.0.1"), None);
}
