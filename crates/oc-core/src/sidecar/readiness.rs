//! What a first-run screen shows about each model (PHASE 9 detail 7).
//!
//! `openconvert model list --json` prints one of these per registry entry, and the GUI (Phase 12)
//! renders exactly these fields. Every one of them is read from the registry or the model store:
//! nothing here is estimated at the UI layer.

use std::path::PathBuf;

use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelReadiness {
    pub id: String,
    pub display_name: String,
    /// `default`, `small`, `quality` or `experimental` (D9).
    pub tier: String,
    /// Whether this is the registry's `default`.
    pub is_default: bool,
    pub installed: bool,
    /// The download's size, from the registry.
    pub size_bytes: u64,
    /// `min_ram_bytes` from the registry.
    pub ram_estimate_bytes: u64,
    /// UI_UX §2's words: "fast", "moderate", "slower, higher quality", or "not yet measured".
    pub cpu_expectation: String,
    /// The licence's SPDX name.
    pub license: String,
    /// The `LICENSE` written beside the model, once it is installed.
    pub license_path: Option<PathBuf>,
    /// The registry's warning, for the experimental tier.
    pub warn: Option<String>,
}
