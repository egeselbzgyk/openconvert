//! What is listening at an endpoint (D10, PHASE 11 details 2 and 5).
//!
//! **Ollama is found, not configured**: `GET http://localhost:11434/api/tags` lists the models it
//! serves, and a reply that is a model list is an Ollama. It is on this machine, so finding it needs
//! no consent, and a machine without it answers nothing — which is "not detected", never an error.
//!
//! **The capability probe** asks once, before any question is sent, what kind of server an
//! endpoint is, by what it answers rather than by a version string: `llama-server` answers
//! `GET /props` with its generation settings; Ollama answers `GET /api/tags` with a model list; any
//! other OpenAI-compatible server answers `GET /v1/models`. The first that answers decides, and the
//! caller turns the answer into an adapter — GBNF for `llama-server`, a JSON Schema for Ollama, and
//! for anything else neither, because nothing short of a generation says whether it honours one.

use std::time::Duration;

use oc_ai::transport::{Transport, TransportError};
use serde::Deserialize;

use crate::transport::HttpTransport;
use crate::NetError;

/// Where Ollama listens unless told otherwise.
pub const OLLAMA_DEFAULT_URL: &str = "http://localhost:11434";

/// Ollama's model list.
pub const OLLAMA_TAGS: &str = "/api/tags";

/// `llama-server`'s properties: its generation settings, slots and build.
pub const LLAMA_PROPS: &str = "/props";

/// The OpenAI model list.
pub const OPENAI_MODELS: &str = "/v1/models";

/// The OpenAI API's version prefix, which a base URL is often written with.
const V1: &str = "/v1";

/// What Ollama says it serves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OllamaInfo {
    /// Model names as Ollama lists them (`qwen3:1.7b`), in its order.
    pub models: Vec<String>,
}

/// What the probe found at an endpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Server {
    /// `llama-server`: takes a GBNF grammar and `chat_template_kwargs`.
    LlamaServer,
    /// Ollama, and the models it serves.
    Ollama { models: Vec<String> },
    /// Some other OpenAI-compatible server, and the model ids it lists.
    OpenAiCompatible { models: Vec<String> },
}

/// A transport to Ollama's default endpoint, on this machine.
pub fn ollama_transport() -> Result<HttpTransport, NetError> {
    HttpTransport::new(OLLAMA_DEFAULT_URL, None)
}

/// Ollama behind `transport`, if that is what answers there.
pub fn detect_ollama(transport: &dyn Transport, timeout: Duration) -> Option<OllamaInfo> {
    let reply = transport.get(OLLAMA_TAGS, timeout).ok()?;
    let tags: Tags = serde_json::from_str(&reply).ok()?;
    Some(OllamaInfo {
        models: tags.models.into_iter().map(|model| model.name).collect(),
    })
}

/// What kind of server answers behind `transport`: asked once per session, before any question
/// (PHASE 11 detail 5). The error is the last thing that went wrong when nothing answered.
pub fn probe(transport: &dyn Transport, timeout: Duration) -> Result<Server, TransportError> {
    let props = transport.get(LLAMA_PROPS, timeout);
    if props.as_deref().is_ok_and(is_llama_props) {
        return Ok(Server::LlamaServer);
    }
    if let Some(info) = detect_ollama(transport, timeout) {
        return Ok(Server::Ollama {
            models: info.models,
        });
    }
    let models = transport.get(OPENAI_MODELS, timeout)?;
    let list: ModelList = serde_json::from_str(&models).map_err(|_| TransportError::Status {
        status: NOT_A_MODEL_LIST,
    })?;
    Ok(Server::OpenAiCompatible {
        models: list.data.into_iter().map(|model| model.id).collect(),
    })
}

/// The status reported when `/v1/models` answered 2xx with something that is not a model list: the
/// server is not an OpenAI-compatible one, which for this purpose is the same as not being there.
const NOT_A_MODEL_LIST: u16 = 502;

/// The server's root, which the probe's paths and the chat path are appended to: `url` without a
/// trailing slash or the `/v1` a base URL is often written with.
pub fn api_root(url: &str) -> String {
    let trimmed = url.trim_end_matches('/');
    trimmed.strip_suffix(V1).unwrap_or(trimmed).to_owned()
}

#[derive(Deserialize)]
struct Tags {
    models: Vec<Tag>,
}

#[derive(Deserialize)]
struct Tag {
    name: String,
}

/// Whether a `/props` reply is `llama-server`'s: an object carrying the generation settings it has
/// had since the endpoint was added.
fn is_llama_props(reply: &str) -> bool {
    serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(reply)
        .is_ok_and(|props| props.contains_key(LLAMA_PROPS_MARKER))
}

/// The key only `llama-server`'s `/props` carries.
const LLAMA_PROPS_MARKER: &str = "default_generation_settings";

#[derive(Deserialize)]
struct ModelList {
    data: Vec<ModelId>,
}

#[derive(Deserialize)]
struct ModelId {
    id: String,
}
