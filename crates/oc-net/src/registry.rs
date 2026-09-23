//! The model registry: `models.toml`, shipped with the app (D9, IMPLEMENTATION_PLAN §1.6).
//!
//! There is no remote registry in v1. A hash fetched from the host that serves the weights would
//! make the integrity check circular (SECURITY §7), so every entry the downloader may act on is
//! pinned here: a 40-hex commit, never a branch (RT B14), and a SHA-256 the download is checked
//! against while it streams.

use std::path::Path;

/// The registry that ships with the engine and the app (D9: "no remote registry in v1"), compiled
/// in so the hash a download is checked against never comes from the host that serves the weights.
pub const BUNDLED: &str = include_str!("../../../models.toml");

/// A registry entry's id, e.g. `qwen3-1.7b-q4_k_m`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize, serde::Serialize)]
#[serde(transparent)]
pub struct ModelId(pub String);

/// Why a registry file was refused.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RegistryError {
    #[error("cannot read the registry: {0}")]
    Io(String),
    #[error("the registry is not valid: {0}")]
    Parse(String),
    #[error("model `{id}` still has a placeholder in `{field}`")]
    Unresolved { id: String, field: &'static str },
    #[error("model `{id}` is pinned to `{revision}`, which is not a 40-hex commit")]
    UnpinnedRevision { id: String, revision: String },
}

/// Marks a field nobody has filled in yet (IMPLEMENTATION_PLAN §1.6).
const PLACEHOLDER: &str = "TODO_";

/// A commit SHA is 40 lowercase hex characters (RT B14). A format constant, like the id
/// lengths in `oc-model`: changing it is not a calibration.
const COMMIT_HEX_LEN: usize = 40;

/// One `[[model]]` table.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct ModelEntry {
    pub id: ModelId,
    pub tier: String,
    pub display_name: String,
    pub family: String,
    pub arch: String,
    pub license: String,
    pub license_url: Option<String>,
    pub notice_text: Option<String>,
    pub repo: String,
    pub revision: String,
    pub file: String,
    pub url_template: Option<String>,
    pub sha256: String,
    pub size_bytes: u64,
    pub context: u32,
    pub parallel: u32,
    pub min_ram_bytes: u64,
    /// What the first-run screen says about speed on a CPU, in UI_UX §2's words: "fast",
    /// "moderate", "slower, higher quality" — or that it has not been measured.
    pub cpu_expectation: Option<String>,
    pub prompt_profile: String,
    pub cache_reuse: bool,
    pub context_checkpoints: Option<u32>,
    pub warn: Option<String>,
}

#[derive(serde::Deserialize)]
struct RegistryFile {
    default: ModelId,
    #[serde(rename = "model")]
    models: Vec<ModelEntry>,
}

/// The parsed registry.
#[derive(Clone, Debug)]
pub struct ModelRegistry {
    default: ModelId,
    entries: Vec<ModelEntry>,
}

impl ModelRegistry {
    pub fn load(path: &Path) -> Result<Self, RegistryError> {
        let text =
            std::fs::read_to_string(path).map_err(|error| RegistryError::Io(error.to_string()))?;
        Self::parse(&text)
    }

    /// Parse and check a registry. Every entry must be resolved and pinned: a registry the
    /// downloader could only half act on is refused whole, so the refusal happens when the app
    /// starts rather than when a user presses "download".
    pub fn parse(text: &str) -> Result<Self, RegistryError> {
        let file: RegistryFile =
            toml::from_str(text).map_err(|error| RegistryError::Parse(error.to_string()))?;
        for entry in &file.models {
            check(entry)?;
        }
        Ok(Self {
            default: file.default,
            entries: file.models,
        })
    }

    pub fn default_id(&self) -> &ModelId {
        &self.default
    }

    pub fn get(&self, id: &ModelId) -> Option<&ModelEntry> {
        self.entries.iter().find(|entry| &entry.id == id)
    }

    /// Every entry, in file order.
    pub fn entries(&self) -> &[ModelEntry] {
        &self.entries
    }
}

fn check(entry: &ModelEntry) -> Result<(), RegistryError> {
    check_pins(&entry.id.0, &[], &entry.revision, &entry.sha256)
}

/// A registry entry the downloader may act on: no `fields` still a placeholder, the revision and
/// hash filled in, and the revision a full commit. Shared by the model and the pack registries.
pub(crate) fn check_pins(
    id: &str,
    fields: &[(&'static str, &str)],
    revision: &str,
    sha256: &str,
) -> Result<(), RegistryError> {
    let unresolved = |field| RegistryError::Unresolved {
        id: id.to_owned(),
        field,
    };
    for (field, value) in fields {
        if value.starts_with(PLACEHOLDER) {
            return Err(unresolved(field));
        }
    }
    if revision.starts_with(PLACEHOLDER) {
        return Err(unresolved("revision"));
    }
    if sha256.starts_with(PLACEHOLDER) {
        return Err(unresolved("sha256"));
    }
    if !is_commit(revision) {
        return Err(RegistryError::UnpinnedRevision {
            id: id.to_owned(),
            revision: revision.to_owned(),
        });
    }
    Ok(())
}

/// Whether `revision` is a full commit SHA rather than a branch, a tag or an abbreviation.
pub fn is_commit(revision: &str) -> bool {
    revision.len() == COMMIT_HEX_LEN
        && revision
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
