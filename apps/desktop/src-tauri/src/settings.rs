//! The user's settings, kept by the Rust side in the app's config directory.
//!
//! Only what the UI lets a user choose in part A of Phase 12: the language, the document preset,
//! the two resource caps Advanced exposes (UI_UX §2.4), and whether the first-run card was
//! dismissed. The password field is deliberately absent — it is used for one job and never saved
//! (design decision 13).

use std::path::{Path, PathBuf};

use oc_core::jobspec::LimitsSpec;
use oc_model::document::PresetName;
use serde::{Deserialize, Serialize};

use crate::engine::UiError;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// `"en"`, `"de"`, `"tr"`, or `None` to follow the system.
    pub language: Option<String>,
    pub preset: PresetName,
    /// Overrides of `limits.max_pages` / `limits.max_memory_bytes`; `None` keeps the shipped value.
    pub max_pages: Option<u64>,
    pub max_memory_bytes: Option<u64>,
    /// "Not now" on the first-run card: it does not return to the main window (design decision 10).
    pub firstrun_dismissed: bool,
}

impl Settings {
    /// The limits a job spec carries, when the user changed any.
    pub fn limits(&self) -> Option<LimitsSpec> {
        (self.max_pages.is_some() || self.max_memory_bytes.is_some()).then_some(LimitsSpec {
            max_pages: self.max_pages,
            max_memory_bytes: self.max_memory_bytes,
            stage_deadline_secs: None,
        })
    }
}

/// Where the settings live.
pub fn settings_path(config_dir: &Path) -> PathBuf {
    config_dir.join("settings.json")
}

/// Read the settings; a missing or unreadable file is the defaults, never an error at start-up.
pub fn load(path: &Path) -> Settings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Write the settings atomically.
pub fn save(path: &Path, settings: &Settings) -> Result<(), UiError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let partial = path.with_extension("json.part");
    let text =
        serde_json::to_string_pretty(settings).map_err(|error| UiError::Io(error.to_string()))?;
    std::fs::write(&partial, text)?;
    std::fs::rename(&partial, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_a_missing_file_is_the_defaults() {
        let dir = std::env::temp_dir().join(format!("oc-desktop-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = settings_path(&dir);
        assert_eq!(load(&path), Settings::default());

        let chosen = Settings {
            language: Some("tr".to_owned()),
            preset: PresetName::Academic,
            max_pages: Some(5000),
            max_memory_bytes: None,
            firstrun_dismissed: true,
        };
        save(&path, &chosen).expect("saved");
        assert_eq!(load(&path), chosen);
        assert_eq!(
            chosen.limits().and_then(|limits| limits.max_pages),
            Some(5000)
        );
        assert_eq!(
            Settings::default().limits(),
            None,
            "untouched caps stay the shipped ones"
        );
    }
}
