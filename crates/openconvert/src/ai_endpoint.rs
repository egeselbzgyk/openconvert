//! Where `convert --ai` finds its model: a server the engine starts and stops, the desktop app's
//! own, a local server the user runs — Ollama, LM Studio, `llama-server` — or, with consent, one on
//! another machine (PHASE 9 detail 3, PHASE 10, PHASE 11, RT C2, D8, D10).
//!
//! **Never a failure of the conversion.** A server that cannot be started, a model that is not
//! installed, an endpoint that does not answer its capability probe: each is a reason, the book is
//! converted by the deterministic path alone, and the report carries `W_LLM_UNAVAILABLE` saying why
//! (RT D20). The refusals are usage errors, exit 2, before anything is sent: a URL the engine will
//! not interpret, plain `http://` off this machine, and — `E_CONSENT_REQUIRED` — a host off this
//! machine the command line has not consented to by name (D10).
//!
//! **One probe, once.** An endpoint is asked what it is before any question — `llama-server`,
//! Ollama, or another OpenAI-compatible server ([`oc_net::detect::probe`]) — and that answer picks
//! the adapter unless `--llm-provider` names one. A model is never guessed: the one `--llm-model`
//! names, or the only one the server lists.

use std::path::{Path, PathBuf};
use std::time::Duration;

use oc_ai::provider::local_sidecar::local_sidecar;
use oc_ai::provider::ollama::{Ollama, OllamaConfig};
use oc_ai::provider::openai_compatible::custom_endpoint;
use oc_ai::provider::{LlmProvider, ProviderCaps, ProviderKind};
use oc_ai::transport::Transport;
use oc_core::exit::ExitCode;
use oc_core::sidecar::llama::ServerSpec;
use oc_core::sidecar::server::{read_key_file, Health, OwnedServer};
use oc_core::thresholds::Thresholds;
use oc_net::consent::{self, ConsentRecord, ConsentScope};
use oc_net::detect::{self, Server};
use oc_net::transport::HttpTransport;
use oc_net::NetError;
use secrecy::SecretString;

/// `--ai` and the flags that only mean something with it (§2.1, PHASE 10, PHASE 11). The same
/// fields as the job spec's `ai` object (§2.2), which is what the desktop app writes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AiArgs {
    /// `--llm-endpoint <URL>`: somebody else's server; the engine spawns nothing (RT C2).
    pub endpoint: Option<String>,
    /// `--llm-api-key-file <PATH>`: never an inline key.
    pub api_key_file: Option<PathBuf>,
    /// `--model-path <PATH>`: the GGUF the engine-owned sidecar loads.
    pub model_path: Option<PathBuf>,
    /// `--ai-all-tasks`: run every escalated task for every language, including those the
    /// evaluation has not enabled (`ai.task.<task>.languages`, PHASE 10 detail 7).
    pub all_tasks: bool,
    /// `--llm-provider <builtin|ollama|openai-compatible>`: the adapter, when the capability probe
    /// should not decide. `ollama` without an endpoint is Ollama on `localhost:11434`.
    pub provider: Option<ProviderKind>,
    /// `--llm-model <NAME>` (the job spec's `model_id`): the model as the endpoint names it — or,
    /// for the engine-owned sidecar, a registry id.
    pub model: Option<String>,
    /// `--llm-allow-host <HOST>`: consent to sending the books' text to that host, which must be
    /// the endpoint's (D10). The job spec's `non_loopback_consent: true` is the same consent,
    /// given to whatever host its endpoint names ([`AiArgs::consenting_to_the_endpoint`]).
    pub allow_host: Option<String>,
}

impl AiArgs {
    /// These arguments with consent given to the endpoint's own host — how the job spec's
    /// `non_loopback_consent: true` reads, the desktop app's consent dialog having named the host
    /// to the user (UI_UX §2.4). An endpoint that is not a URL gets no consent.
    pub fn consenting_to_the_endpoint(self) -> Self {
        let allow_host = self
            .endpoint
            .as_deref()
            .and_then(|url| consent::host_of(url).ok());
        Self { allow_host, ..self }
    }
}

/// The `fatal` code of a host off this machine that nothing consented to (D10).
pub const E_CONSENT_REQUIRED: &str = "E_CONSENT_REQUIRED";

/// The `fatal` code of every other refusal.
const E_USAGE: &str = "E_USAGE";

/// The environment variable that names the `llama-server` to start, as the live tests use it.
pub const LLAMA_SERVER_ENV: &str = "OC_LLAMA_SERVER";

/// A provider the conversion can ask, and what the report says about it. A server the engine
/// started is killed when this is dropped.
pub struct Opened {
    pub provider: Box<dyn LlmProvider>,
    pub server: Option<OwnedServer>,
    /// Which adapter answers.
    pub kind: ProviderKind,
    /// The consent the endpoint needed, when it was off this machine: the report prints it.
    pub consent: Option<ConsentRecord>,
    /// The models the endpoint listed when it was probed (none for `llama-server`, which serves
    /// the one it loaded, and for a server the engine started).
    pub models: Vec<String>,
    /// The TCP port the conversion connects to: the only one Landlock lets it reach (PHASE 14).
    pub port: Option<u16>,
}

/// The port an endpoint URL names, or its scheme's default.
fn port_of(url: &str) -> Option<u16> {
    const HTTP: u16 = 80;
    const HTTPS: u16 = 443;
    let (scheme, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let after_host = match authority.strip_prefix('[') {
        Some(bracketed) => bracketed.split_once(']')?.1,
        None => authority.rsplit_once(':').map_or("", |(_, port)| port),
    };
    let explicit = after_host.trim_start_matches(':');
    if !explicit.is_empty() {
        return explicit.parse().ok();
    }
    match scheme.to_ascii_lowercase().as_str() {
        "http" => Some(HTTP),
        "https" => Some(HTTPS),
        _ => None,
    }
}

/// Why no provider was opened.
#[derive(Debug, PartialEq, Eq)]
pub enum OpenError {
    /// The endpoint is off this machine and nothing consented to its host: exit 2,
    /// `E_CONSENT_REQUIRED`, and nothing was sent.
    ConsentRequired { host: String },
    /// Any other usage error: exit 2, nothing converted.
    Refused(String),
    /// Convert without the model, and say why (`W_LLM_UNAVAILABLE`'s `reason`).
    Unavailable(&'static str),
}

impl OpenError {
    /// What the process exits with: a refusal is a usage error, an unavailable model is not an
    /// error at all.
    pub fn exit_code(&self) -> ExitCode {
        match self {
            OpenError::ConsentRequired { .. } | OpenError::Refused(_) => ExitCode::Usage,
            OpenError::Unavailable(_) => ExitCode::Ok,
        }
    }

    /// The `fatal` event's code and message, for a refusal.
    pub fn fatal(&self) -> Option<(&'static str, String)> {
        match self {
            OpenError::ConsentRequired { host } => Some((
                E_CONSENT_REQUIRED,
                format!(
                    "`{host}` is not this computer: with --ai, text from the book would be sent \
                     there. Nothing was sent. To consent to that, pass --llm-allow-host {host}"
                ),
            )),
            OpenError::Refused(message) => Some((E_USAGE, message.clone())),
            OpenError::Unavailable(_) => None,
        }
    }
}

/// How an endpoint is reached: `oc-net`'s HTTP transport in the engine, an in-process double in
/// the tests. Whatever it is, it gets the consent and must refuse a host the consent does not name
/// ([`consent::authorize`]).
pub trait Connector {
    fn connect(
        &self,
        base: &str,
        api_key: Option<SecretString>,
        consent: Option<&ConsentRecord>,
    ) -> Result<Box<dyn Transport>, NetError>;
}

/// The network, through [`HttpTransport`].
pub struct Network;

impl Connector for Network {
    fn connect(
        &self,
        base: &str,
        api_key: Option<SecretString>,
        consent: Option<&ConsentRecord>,
    ) -> Result<Box<dyn Transport>, NetError> {
        let transport = match consent {
            Some(consent) => HttpTransport::with_consent(base, api_key, consent)?,
            None => HttpTransport::new(base, api_key)?,
        };
        Ok(Box::new(transport))
    }
}

/// Open the provider `args` describe, over the network.
pub fn open(args: &AiArgs, registry_text: &str, t: &Thresholds) -> Result<Opened, OpenError> {
    open_with(args, registry_text, t, &Network)
}

/// Open the provider `args` describe, reaching endpoints through `connector`.
pub fn open_with(
    args: &AiArgs,
    registry_text: &str,
    t: &Thresholds,
    connector: &dyn Connector,
) -> Result<Opened, OpenError> {
    let endpoint = match (&args.endpoint, args.provider) {
        (Some(url), _) => url.clone(),
        (None, Some(ProviderKind::Ollama)) => detect::OLLAMA_DEFAULT_URL.to_owned(),
        (None, Some(ProviderKind::OpenAiCompatible)) => {
            return Err(OpenError::Refused(
                "--llm-provider openai-compatible needs --llm-endpoint".to_owned(),
            ))
        }
        (None, Some(ProviderKind::LocalSidecar) | None) => {
            return spawn_sidecar(args, registry_text, t)
        }
    };
    open_endpoint(args, &endpoint, t, connector)
}

/// An endpoint somebody else runs: consent, key, probe, adapter — in that order, and nothing is
/// sent before the consent is settled.
fn open_endpoint(
    args: &AiArgs,
    url: &str,
    t: &Thresholds,
    connector: &dyn Connector,
) -> Result<Opened, OpenError> {
    let host = consent::host_of(url).map_err(|_| {
        OpenError::Refused(format!(
            "`{url}` is not an endpoint the engine will use: an http:// or https:// URL with a \
             plain host, and no user name, query or escape in it"
        ))
    })?;
    let granted = if consent::is_loopback(&host) {
        None
    } else {
        match &args.allow_host {
            Some(allowed) if allowed.eq_ignore_ascii_case(&host) => {
                Some(ConsentRecord::grant(&host, ConsentScope::Run))
            }
            _ => return Err(OpenError::ConsentRequired { host }),
        }
    };
    consent::authorize(url, granted.as_ref()).map_err(refusal)?;

    let key = match &args.api_key_file {
        Some(path) => Some(
            read_key_file(path)
                .map_err(|_| OpenError::Unavailable("the API key file could not be read"))?,
        ),
        None => None,
    };
    let transport = connector
        .connect(&detect::api_root(url), key, granted.as_ref())
        .map_err(refusal)?;

    let probe_timeout = millis(t.llm.provider_probe_timeout_millis);
    let server = detect::probe(transport.as_ref(), probe_timeout)
        .map_err(|_| OpenError::Unavailable("the endpoint did not answer the capability probe"))?;
    let listed = match &server {
        Server::LlamaServer => Vec::new(),
        Server::Ollama { models } | Server::OpenAiCompatible { models } => models.clone(),
    };
    let kind = args.provider.unwrap_or(match &server {
        Server::LlamaServer => ProviderKind::LocalSidecar,
        Server::Ollama { .. } => ProviderKind::Ollama,
        Server::OpenAiCompatible { .. } => ProviderKind::OpenAiCompatible,
    });

    let temperature = t.llm.temperature as f32;
    let timeout = secs(t.llm.call_timeout_secs);
    let provider: Box<dyn LlmProvider> = match kind {
        ProviderKind::LocalSidecar => {
            let model_id = args
                .model
                .clone()
                .or_else(|| stem(args.model_path.as_deref()))
                .unwrap_or_else(|| format!("endpoint@{host}"));
            Box::new(local_sidecar(transport, model_id, temperature, timeout))
        }
        ProviderKind::Ollama => {
            let models = match &server {
                Server::Ollama { models } => models.clone(),
                _ => detect::detect_ollama(transport.as_ref(), probe_timeout)
                    .map(|info| info.models)
                    .unwrap_or_default(),
            };
            let model = ollama_model(args.model.as_deref(), &models)?;
            Box::new(Ollama::new(
                transport,
                OllamaConfig {
                    model,
                    temperature,
                    timeout,
                    num_ctx: u32::try_from(t.llm.ollama_num_ctx).unwrap_or(u32::MAX),
                    keep_alive_secs: u32::try_from(t.llm.ollama_keep_alive_secs)
                        .unwrap_or_default(),
                    template_overhead_tokens: u32::try_from(t.llm.ollama_template_overhead_tokens)
                        .unwrap_or(u32::MAX),
                },
            ))
        }
        ProviderKind::OpenAiCompatible => {
            let caps = match &server {
                Server::LlamaServer => ProviderCaps::grammar(),
                Server::Ollama { .. } | Server::OpenAiCompatible { .. } => ProviderCaps::neither(),
            };
            let model = match (&args.model, listed.as_slice()) {
                (Some(model), _) => model.clone(),
                (None, [only]) => only.clone(),
                (None, _) if server == Server::LlamaServer => format!("endpoint@{host}"),
                (None, []) => {
                    return Err(OpenError::Unavailable(
                        "the endpoint lists no model; name one with --llm-model",
                    ))
                }
                (None, _) => {
                    return Err(OpenError::Unavailable(
                        "the endpoint serves more than one model; name one with --llm-model",
                    ))
                }
            };
            Box::new(custom_endpoint(
                transport,
                model,
                caps,
                temperature,
                timeout,
            ))
        }
    };
    Ok(Opened {
        provider,
        server: None,
        kind,
        consent: granted,
        models: listed,
        port: port_of(url),
    })
}

/// The Ollama model to ask: the one named, if Ollama serves it — `llama3` is served as
/// `llama3:latest` — or the only one it serves. Never a guess among several.
fn ollama_model(named: Option<&str>, served: &[String]) -> Result<String, OpenError> {
    match named {
        Some(name) => served
            .iter()
            .find(|model| {
                *model == name
                    || model
                        .strip_suffix(OLLAMA_DEFAULT_TAG)
                        .is_some_and(|base| base == name)
            })
            .cloned()
            .ok_or(OpenError::Unavailable("the model is not one Ollama serves")),
        None => match served {
            [] => Err(OpenError::Unavailable("Ollama serves no model")),
            [only] => Ok(only.clone()),
            _ => Err(OpenError::Unavailable(
                "Ollama serves more than one model; name one with --llm-model",
            )),
        },
    }
}

/// The tag Ollama gives a model pulled without one.
const OLLAMA_DEFAULT_TAG: &str = ":latest";

/// A network refusal as the command line reports it.
fn refusal(error: NetError) -> OpenError {
    match error {
        NetError::ConsentRequired { host } => OpenError::ConsentRequired { host },
        other => OpenError::Refused(other.to_string()),
    }
}

/// An engine-owned server: a program, a model, a port, a key.
fn spawn_sidecar(args: &AiArgs, registry_text: &str, t: &Thresholds) -> Result<Opened, OpenError> {
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
            let id = args
                .model
                .clone()
                .map_or_else(|| registry.default_id().clone(), oc_net::registry::ModelId);
            let entry = registry
                .get(&id)
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
    let probe_timeout = millis(t.llm.health_probe_timeout_millis);
    server
        .wait_healthy(
            &|server: &OwnedServer| match HttpTransport::new(&server.base_url(), None)
                .ok()
                .map(|transport| transport.get("/health", probe_timeout))
            {
                Some(Ok(_)) => Health::Ready,
                _ => Health::NotYet,
            },
            secs(t.llm.load_timeout_secs),
        )
        .map_err(|_| OpenError::Unavailable("the model server did not become ready"))?;
    let transport = HttpTransport::new(&server.base_url(), Some(server.api_key().clone()))
        .map_err(|_| OpenError::Unavailable("the model server's address is not usable"))?;
    let model_id = entry.map_or_else(
        || stem(Some(model.as_path())).unwrap_or_default(),
        |entry| entry.id.0,
    );
    Ok(Opened {
        provider: Box::new(local_sidecar(
            transport,
            model_id,
            t.llm.temperature as f32,
            secs(t.llm.call_timeout_secs),
        )),
        port: Some(server.port()),
        server: Some(server),
        kind: ProviderKind::LocalSidecar,
        consent: None,
        models: Vec::new(),
    })
}

/// A file's stem, as a model id.
fn stem(path: Option<&Path>) -> Option<String> {
    path.and_then(Path::file_stem)
        .map(|stem| stem.to_string_lossy().into_owned())
}

fn secs(value: i64) -> Duration {
    Duration::from_secs(u64::try_from(value).unwrap_or_default())
}

fn millis(value: i64) -> Duration {
    Duration::from_millis(u64::try_from(value).unwrap_or_default())
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

#[test]
fn the_job_specs_consent_names_the_endpoints_own_host() {
    let args = AiArgs {
        endpoint: Some("https://LLM.example.org:8443/v1".to_owned()),
        ..AiArgs::default()
    }
    .consenting_to_the_endpoint();
    assert_eq!(args.allow_host.as_deref(), Some("llm.example.org"));
    let args = AiArgs {
        endpoint: Some("http://user@evil.example".to_owned()),
        allow_host: Some("evil.example".to_owned()),
        ..AiArgs::default()
    }
    .consenting_to_the_endpoint();
    assert_eq!(args.allow_host, None, "no consent to what is not a URL");
}
