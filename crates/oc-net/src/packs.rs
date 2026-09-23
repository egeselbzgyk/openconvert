//! The pack registry: `packs.toml`, shipped with the app (D6, LICENSE_AND_DEPENDENCIES §6).
//!
//! A pack is an optional, post-install, SHA-256-pinned, licence-labelled download that is never
//! fetched during a conversion — the validation pack (a minimal Java runtime and `epubcheck.jar`),
//! later the OCR pack. It is pinned exactly as a model is (a 40-hex commit, a hash the download is
//! checked against while it streams) and installed by the same [`crate::download::Downloader`], as
//! an [`Artifact`].

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

#[derive(serde::Deserialize)]
struct PackFile {
    #[serde(default, rename = "pack")]
    packs: Vec<PackEntry>,
}

/// The parsed pack registry.
#[derive(Clone, Debug)]
pub struct PackRegistry {
    entries: Vec<PackEntry>,
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
        Ok(Self {
            entries: file.packs,
        })
    }

    /// Every pack, in file order.
    pub fn entries(&self) -> &[PackEntry] {
        &self.entries
    }

    pub fn get(&self, id: &ModelId) -> Option<&PackEntry> {
        self.entries.iter().find(|entry| &entry.id == id)
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
