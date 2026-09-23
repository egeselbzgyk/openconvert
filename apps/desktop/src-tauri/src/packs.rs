//! Optional packs (PHASE 12 detail 9, D6, LICENSE_AND_DEPENDENCIES §6).
//!
//! A pack is installed by the model manager's own mechanism — the same [`crate::models::Manager`]
//! over the pack registry (`packs.toml`, compiled in), the same `oc-net` downloader, the same
//! licence-first rule, progress and Cancel. The Packs screen renders the registry, never constants:
//! the validation pack's size and licence are whatever its entry pins.
//!
//! The registry that ships pins nothing yet. The validation pack's payload (a jlink'd Java runtime
//! and `epubcheck.jar`) is not built or published, and its runtime's licence is to be verified
//! per vendor before it ships (LICENSE_AND_DEPENDENCIES §6), so the screen says it is not
//! available in this version. The OCR pack is not in v1 at all: v1 uses the system's Tesseract
//! (D4), which the Packs screen offers to install instead.

use std::path::PathBuf;

use oc_net::download::Artifact;
use oc_net::packs::PackRegistry;
use oc_net::store::{self, ModelStore};
use serde::Serialize;

use crate::models::{Catalog, Manager, View};

/// What a pack row shows, read from the registry and the packs store only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PackReadiness {
    pub id: String,
    pub display_name: String,
    pub contents: String,
    pub installed: bool,
    /// The download's size, from the registry.
    pub size_bytes: u64,
    pub license: String,
    /// The `LICENSE` written beside the pack, once it is installed.
    pub license_path: Option<PathBuf>,
}

impl Catalog for PackRegistry {
    type Readiness = PackReadiness;

    fn artifacts(&self) -> Vec<Artifact> {
        self.entries().iter().map(Artifact::from).collect()
    }

    fn readiness(&self, store: &ModelStore) -> Vec<PackReadiness> {
        let installed = store.list();
        self.entries()
            .iter()
            .map(|entry| {
                let on_disk = installed.iter().find(|pack| pack.id == entry.id);
                PackReadiness {
                    id: entry.id.0.clone(),
                    display_name: entry.display_name.clone(),
                    contents: entry.contents.clone(),
                    installed: on_disk.is_some(),
                    size_bytes: entry.size_bytes,
                    license: entry.license.clone(),
                    license_path: on_disk.and_then(|pack| {
                        let path = pack.path.parent()?.join(store::LICENSE_FILE);
                        path.is_file().then_some(path)
                    }),
                }
            })
            .collect()
    }
}

/// The pack manager.
pub type PackManager = Manager<PackRegistry>;

/// The Packs screen's downloadable rows.
pub type PacksView = View<PackReadiness>;

/// The pack registry compiled into the app, or why it cannot be used.
pub fn bundled_registry() -> Result<PackRegistry, String> {
    PackRegistry::parse(oc_net::packs::BUNDLED).map_err(|error| error.to_string())
}

/// Where accepted pack licences are kept, beside the settings.
pub fn accepted_path(config_dir: &std::path::Path) -> PathBuf {
    config_dir.join("pack-licenses.json")
}
