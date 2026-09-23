//! The pack registry: `packs.toml`, shipped with the app (D6, LICENSE_AND_DEPENDENCIES §6).
//!
//! A pack is an optional, post-install, SHA-256-pinned, licence-labelled download that is never
//! fetched during a conversion — the validation pack (a minimal Java runtime and `epubcheck.jar`),
//! later the OCR pack. It is pinned exactly as a model is (a 40-hex commit, a hash the download is
//! checked against while it streams) and installed by the same [`crate::download::Downloader`], as
//! an [`Artifact`].
//!
//! A pack the registry names but this version does not offer is a [`DeferredPack`] (a
//! `[[deferred]]` table): it carries no pins and nothing can download it, so the app can say when
//! it arrives instead of shipping an unpinned entry. The validation pack is one in 1.0 (maintainer
//! decision 2026-09-23; D6 amendment).

use crate::download::Artifact;
use crate::registry::{check_pins, ModelId, RegistryError};

/// The registry that ships with the app, compiled in like the model registry.
pub const BUNDLED: &str = include_str!("../../../packs.toml");

/// One `[[pack]]` table.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct PackEntry {
    pub id: ModelId,
    pub display_name: String,
    /// What the pack holds, in a few words, for its row.
    pub contents: String,
    /// SPDX. For the validation pack this has to name the Java runtime's licence as well as
    /// EPUBCheck's, verified for the vendor chosen (LICENSE_AND_DEPENDENCIES §6).
    pub license: String,
    pub notice_text: Option<String>,
    pub repo: String,
    pub revision: String,
    pub file: String,
    pub url_template: Option<String>,
    pub sha256: String,
    pub size_bytes: u64,
}

/// One `[[deferred]]` table: a pack this version names and does not offer. It has no pin fields
/// at all — a `sha256` or `revision` here is refused, so a half-filled pack cannot hide in it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeferredPack {
    pub id: ModelId,
    pub display_name: String,
    pub contents: String,
    /// Why it is not offered, and when it comes, in words a user reads.
    pub reason: String,
}

#[derive(serde::Deserialize)]
struct PackFile {
    #[serde(default, rename = "pack")]
    packs: Vec<PackEntry>,
    #[serde(default)]
    deferred: Vec<DeferredPack>,
}

/// The parsed pack registry.
#[derive(Clone, Debug)]
pub struct PackRegistry {
    entries: Vec<PackEntry>,
    deferred: Vec<DeferredPack>,
}

impl PackRegistry {
    /// Parse and check. Like the model registry, one unresolved entry refuses the whole file.
    pub fn parse(text: &str) -> Result<Self, RegistryError> {
        let file: PackFile =
            toml::from_str(text).map_err(|error| RegistryError::Parse(error.to_string()))?;
        for entry in &file.packs {
            check_pins(
                &entry.id.0,
                &[
                    ("license", entry.license.as_str()),
                    ("repo", entry.repo.as_str()),
                    ("file", entry.file.as_str()),
                ],
                &entry.revision,
                &entry.sha256,
            )?;
        }
        for later in &file.deferred {
            if file.packs.iter().any(|pack| pack.id == later.id) {
                return Err(RegistryError::Parse(format!(
                    "pack `{}` is both offered and deferred",
                    later.id.0
                )));
            }
        }
        Ok(Self {
            entries: file.packs,
            deferred: file.deferred,
        })
    }

    /// Every pack, in file order.
    pub fn entries(&self) -> &[PackEntry] {
        &self.entries
    }

    pub fn get(&self, id: &ModelId) -> Option<&PackEntry> {
        self.entries.iter().find(|entry| &entry.id == id)
    }

    /// Every pack this version names but does not offer, in file order.
    pub fn deferred(&self) -> &[DeferredPack] {
        &self.deferred
    }

    /// The deferred pack `id`, when this version does not offer it.
    pub fn get_deferred(&self, id: &ModelId) -> Option<&DeferredPack> {
        self.deferred.iter().find(|later| &later.id == id)
    }
}

impl From<&PackEntry> for Artifact {
    fn from(e: &PackEntry) -> Self {
        Self {
            id: e.id.clone(),
            display_name: e.display_name.clone(),
            license: e.license.clone(),
            notice_text: e.notice_text.clone(),
            repo: e.repo.clone(),
            revision: e.revision.clone(),
            file: e.file.clone(),
            url_template: e.url_template.clone(),
            sha256: e.sha256.clone(),
            size_bytes: e.size_bytes,
        }
    }
}
