//! Cassettes: a model's answer to one question, recorded once and replayed offline (IMPLEMENTATION_
//! PLAN Appendix B, R9 §C.4).
//!
//! **Keyed by the cache key.** A cassette lives at `<root>/<task>/<key>.json`, where `<key>` is the
//! production cache key of its question, so there is one keying rule in the system and not two
//! (B.1); `<root>/<task>/index.json` maps a readable `<task>__<fixture>__v<N>` name to it.
//!
//! **At the provider seam, not the transport.** Appendix B speaks of a "cassette transport", but the
//! key carries the prompt version, which never travels on the wire — a transport sees a body with no
//! version in it and could not compute the key. So [`Replay`] is an `LlmProvider`: it answers a
//! request from files, holds no transport of any kind, and so cannot reach a model or a socket. The
//! test tiers are built on it (A8.3); the client and its wire format are tested against the stub.
//!
//! **Replay is total and exact** (B.3): the key is computed from the live prompt, grammar and
//! payload; a miss is an error naming the expected key and the nearest recording, never a
//! fallback; and a recording whose prompt or grammar hash disagrees with the current artifacts is
//! refused even when its key matched, which is what a hand-edited cassette looks like.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::cache::cache_key;
use crate::digest::{hex, sha256, unhex32};
use crate::prompt;
use crate::provider::{
    Constraint, LlmError, LlmProvider, LlmRequest, LlmResponse, ProviderCaps, ThinkingControl,
};

/// The cassette file format's version (Appendix B.2).
pub const CASSETTE_VERSION: u32 = 1;

/// The model id of a cassette the stub answered rather than a model: the seeds, until Phase 9.
pub const STUB_MODEL: &str = "stub";

/// One recorded question and answer, in Appendix B.2's format. Every field is load-bearing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cassette {
    pub cassette_version: u32,
    pub task: String,
    pub fixture: String,
    /// The task's prompt text when recorded: `Artifacts::prompt_sha256`.
    pub prompt_sha256: String,
    pub prompt_version: u32,
    pub grammar_sha256: String,
    pub model_id: String,
    pub params: Params,
    pub request: RecordedRequest,
    pub response: RecordedResponse,
    /// Informational. Cassettes expire when a version they are keyed to changes, never on a clock
    /// (B.3 rule 4).
    pub recorded_at: String,
    /// The pinned llama.cpp build: greedy decoding varies between builds. Absent for the stub.
    pub llama_cpp_build: Option<String>,
    /// The thread count: greedy decoding varies with it too. Absent for the stub.
    pub threads: Option<u32>,
    pub machine: String,
}

/// How the answer was decoded.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Params {
    pub temperature: f32,
    pub max_tokens: u32,
    pub enable_thinking: bool,
    /// The server's sampling and context settings, where a server was involved.
    pub top_p: Option<f32>,
    pub seed: Option<u64>,
    pub n_ctx: Option<u32>,
    pub n_parallel: Option<u32>,
}

/// What the model was shown: the prefix by hash (one shared file; a copy per cassette would make
/// every diff unreadable) and the user message verbatim, so a reviewer can read what it saw.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedRequest {
    pub system_sha256: String,
    pub user: String,
}

/// What the model said.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedResponse {
    pub content: String,
    /// Reasoning a server separated out of the answer. Recorded, because replaying a thinking
    /// answer as a clean one would hide from gate S the one thing it exists to see.
    pub reasoning: Option<String>,
    pub finish_reason: Option<String>,
    pub tokens_in: u32,
    pub tokens_out: u32,
    /// Whether the server reused the shared prefix's cache. Absent for the stub.
    pub cached_prefix: Option<bool>,
}

/// Where and how a recording was made.
#[derive(Clone, Debug, PartialEq)]
pub struct Recording<'a> {
    /// The payload's readable name, the `<fixture>` of `<task>__<fixture>__v<N>`.
    pub fixture: &'a str,
    pub model_id: &'a str,
    pub params: Params,
    pub recorded_at: String,
    pub llama_cpp_build: Option<String>,
    pub threads: Option<u32>,
    pub machine: String,
}

/// The cassette of `response` to `request`.
pub fn record(request: &LlmRequest, response: &LlmResponse, recording: Recording<'_>) -> Cassette {
    Cassette {
        cassette_version: CASSETTE_VERSION,
        task: request.purpose.as_str().to_owned(),
        fixture: recording.fixture.to_owned(),
        prompt_sha256: hex(&prompt::artifacts(request.purpose).prompt_sha256()),
        prompt_version: request.prompt_version,
        grammar_sha256: hex(&request.grammar_sha256()),
        model_id: recording.model_id.to_owned(),
        params: recording.params,
        request: RecordedRequest {
            system_sha256: hex(&sha256(request.system_prefix.as_bytes())),
            user: request.user.clone(),
        },
        response: RecordedResponse {
            content: response.text.clone(),
            reasoning: response.reasoning.clone(),
            finish_reason: response.finish_reason.clone(),
            tokens_in: response.tokens_in,
            tokens_out: response.tokens_out,
            cached_prefix: None,
        },
        recorded_at: recording.recorded_at,
        llama_cpp_build: recording.llama_cpp_build,
        threads: recording.threads,
        machine: recording.machine,
    }
}

impl Cassette {
    /// The readable name the index files it under (Appendix B.1).
    pub fn name(&self) -> String {
        format!("{}__{}__v{}", self.task, self.fixture, self.prompt_version)
    }

    /// The cache key of the recorded question, from what the cassette records.
    pub fn key(&self) -> Option<[u8; 32]> {
        let grammar = unhex32(&self.grammar_sha256)?;
        Some(cache_key(
            &self.model_id,
            self.prompt_version,
            &grammar,
            &self.request.user,
        ))
    }

    /// Write the cassette to `<root>/<task>/<key>.json` and file it in the task's `index.json`.
    ///
    /// Each file is written beside its final name and renamed, so a reader never meets half of one.
    /// A cassette the index no longer names is left for the version bump's commit to delete (B.4
    /// rule 3); `every_cassette_file_is_indexed` refuses to let one linger.
    pub fn write(&self, root: &Path) -> Result<PathBuf, LlmError> {
        let key = self
            .key()
            .ok_or_else(|| LlmError::Cassette("the grammar hash is not a hash".to_owned()))?;
        let directory = root.join(&self.task);
        std::fs::create_dir_all(&directory).map_err(io)?;

        let path = directory.join(format!("{}.json", hex(&key)));
        write_json(&path, self)?;

        let index_path = directory.join(INDEX);
        let mut index = read_index(&index_path)?;
        index.insert(self.name(), hex(&key));
        write_json(&index_path, &index)?;
        Ok(path)
    }
}

/// The file in each task directory that names its cassettes.
const INDEX: &str = "index.json";

/// The recording nearest a question that missed: its readable name, and the byte of the user
/// message at which the two first differ — which is where to look for what changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Nearest {
    pub name: String,
    pub diverges_at: usize,
}

/// Answers from cassettes, and from nothing else.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Replay {
    root: PathBuf,
    model_id: String,
}

impl Replay {
    /// Replay the cassettes under `root` recorded for `model_id`.
    pub fn new(root: impl AsRef<Path>, model_id: &str) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            model_id: model_id.to_owned(),
        }
    }

    fn nearest(&self, request: &LlmRequest) -> Option<Nearest> {
        let directory = self.root.join(request.purpose.as_str());
        let index = read_index(&directory.join(INDEX)).ok()?;
        let mut best: Option<Nearest> = None;
        for (name, key) in index {
            let Ok(text) = std::fs::read_to_string(directory.join(format!("{key}.json"))) else {
                continue;
            };
            let Ok(cassette) = serde_json::from_str::<Cassette>(&text) else {
                continue;
            };
            let diverges_at = common_prefix(&cassette.request.user, &request.user);
            if best
                .as_ref()
                .is_none_or(|nearest| diverges_at > nearest.diverges_at)
            {
                best = Some(Nearest { name, diverges_at });
            }
        }
        best
    }
}

impl LlmProvider for Replay {
    fn id(&self) -> &str {
        &self.model_id
    }

    fn capabilities(&self) -> ProviderCaps {
        ProviderCaps {
            constraint: Constraint::Gbnf,
        }
    }

    fn thinking_control(&self) -> ThinkingControl {
        ThinkingControl::None
    }

    fn complete(&self, request: &LlmRequest) -> Result<LlmResponse, LlmError> {
        let key = hex(&request.cache_key(&self.model_id));
        let path = self
            .root
            .join(request.purpose.as_str())
            .join(format!("{key}.json"));
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(LlmError::CassetteMiss {
                    key,
                    nearest: self.nearest(request),
                })
            }
            Err(error) => return Err(io(error)),
        };
        let cassette: Cassette = serde_json::from_str(&text)
            .map_err(|error| LlmError::Cassette(format!("{}: {error}", path.display())))?;

        let checks = [
            (
                cassette.cassette_version == CASSETTE_VERSION,
                "it is written in another cassette format",
            ),
            (
                cassette.prompt_sha256 == hex(&prompt::artifacts(request.purpose).prompt_sha256()),
                "its prompt_sha256 is not the current prompt's",
            ),
            (
                cassette.grammar_sha256 == hex(&request.grammar_sha256()),
                "its grammar_sha256 is not the current grammar's",
            ),
            (
                cassette.request.system_sha256 == hex(&sha256(request.system_prefix.as_bytes())),
                "its system prefix is not the current one",
            ),
            (
                cassette.request.user == request.user,
                "the question it records is not this question",
            ),
            (
                cassette.model_id == self.model_id,
                "it was recorded from another model",
            ),
        ];
        if let Some((_, reason)) = checks.iter().find(|(holds, _)| !holds) {
            return Err(LlmError::StaleCassette {
                key,
                reason: (*reason).to_owned(),
            });
        }

        Ok(LlmResponse {
            text: cassette.response.content,
            reasoning: cassette.response.reasoning,
            tokens_in: cassette.response.tokens_in,
            tokens_out: cassette.response.tokens_out,
            cached: true,
            cached_tokens: None,
            finish_reason: cassette.response.finish_reason,
        })
    }
}

/// How many leading bytes two texts share, at a character boundary.
fn common_prefix(a: &str, b: &str) -> usize {
    a.char_indices()
        .zip(b.chars())
        .find(|((_, left), right)| left != right)
        .map_or(a.len().min(b.len()), |((offset, _), _)| offset)
}

fn read_index(path: &Path) -> Result<BTreeMap<String, String>, LlmError> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|error| LlmError::Cassette(format!("{}: {error}", path.display()))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(error) => Err(io(error)),
    }
}

/// Pretty JSON with a final newline, written beside `path` and renamed onto it.
fn write_json(path: &Path, value: &impl Serialize) -> Result<(), LlmError> {
    let mut text = serde_json::to_string_pretty(value)
        .map_err(|error| LlmError::Cassette(error.to_string()))?;
    text.push('\n');
    let partial = path.with_extension(format!("json.partial-{}", std::process::id()));
    std::fs::write(&partial, text).map_err(io)?;
    std::fs::rename(&partial, path).map_err(io)
}

fn io(error: std::io::Error) -> LlmError {
    LlmError::Cassette(error.to_string())
}
