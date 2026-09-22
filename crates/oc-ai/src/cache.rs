//! The content-addressed decision cache (D13.8, ARCHITECTURE §9.4).
//!
//! **One keying rule in the system.** The cache key names a cached decision *and* a test cassette
//! (Appendix B.1), so a cassette is addressed by exactly the value production uses and there is no
//! second rule to drift:
//!
//! ```text
//! key  = sha256( len(model_id) ‖ model_id ‖ prompt_version ‖ grammar_sha256 ‖ user message )
//! path = <root>/<key[0..2]>/<key>.json
//! ```
//!
//! ARCHITECTURE writes `‖` as bare concatenation, which is ambiguous — model `ab` with input `c`
//! and model `a` with input `bc` are the same bytes. So the one variable-width field that is not
//! last carries its length, as a little-endian `u64`; `prompt_version` is a little-endian `u32`
//! and the grammar hash is 32 raw bytes, both fixed width. What varies per book is the user
//! message, and the system prefix is named by `prompt_version`, which the pinned manifest
//! (`prompts/v1.sha256`) keeps honest.
//!
//! **A damaged entry is an error, never a miss.** A miss would quietly spend a call the book already
//! paid for; a file that does not parse, or that holds another key's answer, is something to tell
//! the user about.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::digest::{hex, sha256};
use crate::provider::LlmRequest;

/// The cache key of a question put to a model (D13.8).
pub fn cache_key(
    model_id: &str,
    prompt_version: u32,
    grammar_sha256: &[u8; 32],
    user: &str,
) -> [u8; 32] {
    let model_length = u64::try_from(model_id.len()).unwrap_or(u64::MAX);
    let mut layout = Vec::with_capacity(
        std::mem::size_of::<u64>()
            + model_id.len()
            + std::mem::size_of::<u32>()
            + grammar_sha256.len()
            + user.len(),
    );
    layout.extend_from_slice(&model_length.to_le_bytes());
    layout.extend_from_slice(model_id.as_bytes());
    layout.extend_from_slice(&prompt_version.to_le_bytes());
    layout.extend_from_slice(grammar_sha256);
    layout.extend_from_slice(user.as_bytes());
    sha256(&layout)
}

impl LlmRequest {
    /// This request's cache key, for a model.
    pub fn cache_key(&self, model_id: &str) -> [u8; 32] {
        cache_key(
            model_id,
            self.prompt_version,
            &self.grammar_sha256(),
            &self.user,
        )
    }
}

/// One cached answer, and enough about the question to rebuild its trace on a hit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedAnswer {
    /// The key it is filed under, in hex: checked on every read.
    pub key: String,
    pub model_id: String,
    pub prompt_version: u32,
    pub grammar_sha256: String,
    /// The hash of the user message — the input half of the `LlmTrace`.
    pub input_sha256: String,
    /// What the model said, verbatim, before any gate.
    pub output: String,
    pub tokens_in: u32,
    pub tokens_out: u32,
}

impl CachedAnswer {
    /// The entry for `model`'s answer `output` to `request`.
    pub fn new(
        model_id: &str,
        request: &LlmRequest,
        output: &str,
        tokens_in: u32,
        tokens_out: u32,
    ) -> Self {
        Self {
            key: hex(&request.cache_key(model_id)),
            model_id: model_id.to_owned(),
            prompt_version: request.prompt_version,
            grammar_sha256: hex(&request.grammar_sha256()),
            input_sha256: hex(&sha256(request.user.as_bytes())),
            output: output.to_owned(),
            tokens_in,
            tokens_out,
        }
    }
}

/// Why the cache could not answer.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CacheError {
    #[error("the cache entry {path} is damaged: {reason}")]
    Damaged { path: PathBuf, reason: String },
    #[error("the cache entry {path} could not be written: {reason}")]
    Write { path: PathBuf, reason: String },
    #[error("`{0}` is not a cache key")]
    BadKey(String),
}

/// The file cache: one JSON file per key under `root` (the user data directory's `cache/llm`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileCache {
    root: PathBuf,
}

/// Hex digits in a key: 32 bytes, two each.
const KEY_HEX_LEN: usize = 64;

/// How many leading hex digits name the directory a key is filed in.
const SHARD_HEX_LEN: usize = 2;

impl FileCache {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    /// Where a key's entry lives.
    pub fn path(&self, key: &[u8; 32]) -> PathBuf {
        self.path_of(&hex(key))
    }

    fn path_of(&self, key: &str) -> PathBuf {
        self.root
            .join(&key[..SHARD_HEX_LEN])
            .join(format!("{key}.json"))
    }

    /// The entry for a key: `Ok(None)` when there is none, an error when the file is damaged or
    /// holds another key's answer.
    pub fn get(&self, key: &[u8; 32]) -> Result<Option<CachedAnswer>, CacheError> {
        let path = self.path(key);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(CacheError::Damaged {
                    path,
                    reason: error.to_string(),
                })
            }
        };
        let answer: CachedAnswer =
            serde_json::from_str(&text).map_err(|error| CacheError::Damaged {
                path: path.clone(),
                reason: error.to_string(),
            })?;
        if answer.key != hex(key) {
            return Err(CacheError::Damaged {
                path,
                reason: format!("it holds the answer for key {}", answer.key),
            });
        }
        Ok(Some(answer))
    }

    /// File an entry under its own key, atomically: written beside its final name and renamed, so
    /// a reader never meets half a file.
    pub fn put(&self, answer: &CachedAnswer) -> Result<(), CacheError> {
        if answer.key.len() != KEY_HEX_LEN
            || !answer
                .key
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
        {
            return Err(CacheError::BadKey(answer.key.clone()));
        }
        let path = self.path_of(&answer.key);
        let write_error = |reason: String| CacheError::Write {
            path: path.clone(),
            reason,
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| write_error(error.to_string()))?;
        }
        let text =
            serde_json::to_string_pretty(answer).map_err(|error| write_error(error.to_string()))?;
        let partial = path.with_extension(format!("json.partial-{}", std::process::id()));
        std::fs::write(&partial, text).map_err(|error| write_error(error.to_string()))?;
        std::fs::rename(&partial, &path).map_err(|error| write_error(error.to_string()))
    }
}
