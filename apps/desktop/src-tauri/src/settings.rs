//! The user's settings, kept by the Rust side in the app's config directory.
//!
//! What the UI lets a user choose: the language, the document preset, the resource caps Advanced
//! exposes (pages, memory, and the time each stage may take), where converted books are saved,
//! whether the first-run card was dismissed and the earlier conversions are shown, and AI
//! assistance — the switch, the provider, and each provider's configuration (UI_UX §2.4). The
//! password field is deliberately absent — it is used for one job and never saved (design decision
//! 13) — and so is any API key: only the path of the file that holds one is kept, and it is chosen
//! in a native dialog.
//!
//! **Three fields the webview cannot write.** The custom endpoint's key file, the consent given to
//! its host, and the library folder are set only by the Rust side — the file and the folder by the
//! native pickers, the consent by the consent dialog's Allow ([`Settings::granting_consent`]) — and
//! a save from the webview keeps them as they were ([`Settings::merged_from_webview`]). A consent
//! is to one host: changing the endpoint to another host withdraws it.

use std::path::{Path, PathBuf};

use oc_core::jobspec::LimitsSpec;
use oc_core::thresholds::T;
use oc_model::document::PresetName;
use serde::{Deserialize, Serialize};

use crate::engine::UiError;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// `"en"`, `"de"`, `"tr"`, or `None` to follow the system.
    pub language: Option<String>,
    pub preset: PresetName,
    /// Overrides of `limits.max_pages` / `limits.max_memory_bytes`; `None` keeps the shipped value.
    pub max_pages: Option<u64>,
    pub max_memory_bytes: Option<u64>,
    /// The time each stage may take, in seconds; `None` is the app's default,
    /// `desktop.default_stage_deadline_secs` — which every job carries, since the engine's own
    /// `limits.stage_deadline_secs` is too short for large, image-heavy books.
    pub stage_deadline_secs: Option<u64>,
    /// Save every book in the library folder rather than beside its PDF. On by default.
    pub save_to_library: bool,
    /// The library folder the user chose; `None` is the default (`OpenConvert` in Documents).
    /// Set by the native folder picker only.
    pub library_dir: Option<PathBuf>,
    /// "Previous conversions" on the main page is open.
    pub history_open: bool,
    /// "Not now" on the first-run card: it does not return to the main window (design decision 10).
    pub firstrun_dismissed: bool,
    /// AI assistance (UI_UX §2.4). Off by default, as `ai.enabled` is (D17).
    pub ai_enabled: bool,
    /// Which provider answers when AI assistance is on.
    pub provider: Provider,
    /// The Ollama model to ask, as Ollama names it; `None` asks the only one it serves.
    pub ollama_model: Option<String>,
    /// The custom endpoint, kept whichever provider is chosen.
    pub custom: CustomEndpoint,
}

/// Every field its type's default but the two that are on unless turned off: saving to the library
/// and showing the earlier conversions. A settings file without either key reads them as on.
impl Default for Settings {
    fn default() -> Self {
        Self {
            language: None,
            preset: PresetName::default(),
            max_pages: None,
            max_memory_bytes: None,
            stage_deadline_secs: None,
            save_to_library: true,
            library_dir: None,
            history_open: true,
            firstrun_dismissed: false,
            ai_enabled: false,
            provider: Provider::default(),
            ollama_model: None,
            custom: CustomEndpoint::default(),
        }
    }
}

/// Settings › Provider (UI_UX §2.4, D10).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    /// The app-owned `llama-server` and the installed default model.
    #[default]
    Builtin,
    /// Ollama on this computer (`localhost:11434`).
    Ollama,
    /// Any OpenAI-compatible base URL.
    Custom,
}

/// A custom endpoint: a base URL, the model it serves, the file holding its key, and the consent
/// its host needed when it is not this computer.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CustomEndpoint {
    pub endpoint: String,
    pub model: String,
    /// Set by the native file picker only.
    pub api_key_file: Option<PathBuf>,
    /// Set by the consent dialog's Allow only.
    pub consent: Option<Consent>,
}

/// The user allowed document text to be sent to `host` (D10, UI_UX §2.4) — kept per configuration
/// and written into every job as `ai.non_loopback_consent`; the engine remembers nothing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Consent {
    pub host: String,
    /// RFC 3339, UTC, to the second.
    pub granted_at: String,
}

/// A threshold in seconds as the job spec counts them; a negative one is a malformed file, which
/// `thresholds-lint` catches, not a runtime condition.
fn seconds(value: i64) -> u64 {
    u64::try_from(value).unwrap_or_default()
}

impl Settings {
    /// The limits every job spec carries: the page and memory caps when the user changed them, and
    /// always a stage deadline ([`Settings::stage_deadline`]).
    pub fn limits(&self) -> LimitsSpec {
        LimitsSpec {
            max_pages: self.max_pages,
            max_memory_bytes: self.max_memory_bytes,
            stage_deadline_secs: Some(self.stage_deadline()),
        }
    }

    /// The time each stage may take: the user's choice, brought inside what Settings offers
    /// (`desktop.min_stage_deadline_secs` … `desktop.max_stage_deadline_secs`), or the app's default.
    pub fn stage_deadline(&self) -> u64 {
        let shortest = seconds(T.desktop.min_stage_deadline_secs);
        let longest = seconds(T.desktop.max_stage_deadline_secs).max(shortest);
        self.stage_deadline_secs
            .unwrap_or_else(|| seconds(T.desktop.default_stage_deadline_secs))
            .clamp(shortest, longest)
    }
}

impl Settings {
    /// `next`, as the webview sent it, with the three fields only the Rust side sets kept as they
    /// were: the library folder, the key file, and the consent — which is dropped when the endpoint
    /// now names another host, since it was given to the old one.
    pub fn merged_from_webview(&self, mut next: Settings) -> Settings {
        next.library_dir = self.library_dir.clone();
        next.custom.api_key_file = self.custom.api_key_file.clone();
        next.custom.consent = self.custom.consent.clone().filter(|consent| {
            endpoint_host(&next.custom.endpoint).as_deref() == Some(&consent.host)
        });
        next
    }

    /// These settings with consent given, now, to the custom endpoint's own host — the consent
    /// dialog's Allow, which named that host. `None` when the endpoint is not a URL the engine will
    /// use, or is this computer, which needs no consent.
    pub fn granting_consent(&self) -> Option<Settings> {
        let host = endpoint_host(&self.custom.endpoint)?;
        if oc_net::consent::is_loopback(&host) {
            return None;
        }
        let record =
            oc_net::consent::ConsentRecord::grant(&host, oc_net::consent::ConsentScope::Run);
        let mut next = self.clone();
        next.custom.consent = Some(Consent {
            host: record.host.clone(),
            granted_at: record.granted_at_rfc3339(),
        });
        Some(next)
    }
}

/// The host an endpoint URL names, as the engine and `oc-net` read it.
pub fn endpoint_host(endpoint: &str) -> Option<String> {
    oc_net::consent::host_of(endpoint).ok()
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
    use oc_core::thresholds::T;

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
            ..Settings::default()
        };
        save(&path, &chosen).expect("saved");
        assert_eq!(load(&path), chosen);
        assert_eq!(chosen.limits().max_pages, Some(5000));
        let untouched = Settings::default().limits();
        assert_eq!(
            (untouched.max_pages, untouched.max_memory_bytes),
            (None, None),
            "untouched caps stay the shipped ones"
        );
    }

    fn secs(value: i64) -> u64 {
        u64::try_from(value).expect("positive")
    }

    /// The maintainer's decision (2026-09-26): every job the app queues carries a stage deadline —
    /// the app's own default, `desktop.default_stage_deadline_secs`, unless the user set one in
    /// Settings › Advanced — so a large book is not stopped at the engine's shorter default.
    #[test]
    fn the_job_spec_always_carries_a_stage_deadline() {
        let default = secs(T.desktop.default_stage_deadline_secs);
        assert_eq!(
            Settings::default().limits().stage_deadline_secs,
            Some(default),
            "the app's default, not the engine's"
        );
        assert!(default > secs(T.limits.stage_deadline_secs));

        let chosen = Settings {
            stage_deadline_secs: Some(secs(T.desktop.min_stage_deadline_secs) * 3),
            ..Settings::default()
        };
        assert_eq!(
            chosen.limits().stage_deadline_secs,
            chosen.stage_deadline_secs
        );

        // A value outside what Settings offers (a hand-edited file) is brought inside it.
        let short = Settings {
            stage_deadline_secs: Some(1),
            ..Settings::default()
        };
        assert_eq!(
            short.limits().stage_deadline_secs,
            Some(secs(T.desktop.min_stage_deadline_secs))
        );
        let long = Settings {
            stage_deadline_secs: Some(u64::MAX),
            ..Settings::default()
        };
        assert_eq!(
            long.limits().stage_deadline_secs,
            Some(secs(T.desktop.max_stage_deadline_secs))
        );
    }

    /// Books go to the OpenConvert folder unless the user says otherwise — also for a settings file
    /// written before the setting existed, which has no such key.
    #[test]
    fn books_are_saved_to_the_library_by_default() {
        assert!(Settings::default().save_to_library);
        assert!(Settings::default().history_open);
        let old: Settings =
            serde_json::from_str(r#"{"language":"de","preset":"novel"}"#).expect("old file");
        assert!(old.save_to_library, "an old file reads as on");
        assert_eq!(old.stage_deadline_secs, None);
        assert_eq!(old.library_dir, None);
        let json = serde_json::to_value(Settings::default()).expect("json");
        assert_eq!(json["saveToLibrary"], true, "camelCase for the webview");
        assert!(json["stageDeadlineSecs"].is_null());
    }

    /// The library folder is chosen in the native folder picker, which is the Rust side's: a save
    /// from the webview keeps it as it was.
    #[test]
    fn the_webview_cannot_write_the_library_folder() {
        let kept = Settings {
            library_dir: Some(PathBuf::from("/home/me/Books")),
            ..Settings::default()
        };
        let forged = Settings {
            library_dir: Some(PathBuf::from("/etc")),
            save_to_library: false,
            ..Settings::default()
        };
        let saved = kept.merged_from_webview(forged);
        assert_eq!(saved.library_dir, kept.library_dir);
        assert!(!saved.save_to_library, "the switch is the webview's");
    }

    /// The key file and the consent are the Rust side's to set: a save from the webview cannot add
    /// either, and moving the endpoint to another host withdraws the consent given to the old one.
    #[test]
    fn the_webview_cannot_write_a_key_file_or_a_consent() {
        let mut kept = Settings::default();
        kept.custom.endpoint = "https://llm.example.org/v1".to_owned();
        kept.custom.api_key_file = Some(PathBuf::from("/home/me/keys/llm.key"));
        kept = kept.granting_consent().expect("a host off this computer");
        let consent = kept.custom.consent.clone().expect("granted");
        assert_eq!(consent.host, "llm.example.org");
        assert!(consent.granted_at.ends_with('Z'), "{}", consent.granted_at);

        let mut forged = kept.clone();
        forged.custom.api_key_file = Some(PathBuf::from("/etc/shadow"));
        forged.custom.consent = Some(Consent {
            host: "evil.example".to_owned(),
            granted_at: "2020-01-01T00:00:00Z".to_owned(),
        });
        forged.custom.model = "qwen3-8b".to_owned();
        let saved = kept.merged_from_webview(forged);
        assert_eq!(
            saved.custom.model, "qwen3-8b",
            "ordinary fields are the webview's"
        );
        assert_eq!(saved.custom.api_key_file, kept.custom.api_key_file);
        assert_eq!(saved.custom.consent, Some(consent));

        let mut moved = saved.clone();
        moved.custom.endpoint = "https://other.example.net".to_owned();
        assert_eq!(
            saved.merged_from_webview(moved).custom.consent,
            None,
            "consent to one host is not consent to another"
        );

        let mut local = Settings::default();
        local.custom.endpoint = "http://127.0.0.1:1234/v1".to_owned();
        assert_eq!(
            local.granting_consent(),
            None,
            "this computer needs no consent"
        );
    }
}
