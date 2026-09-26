//! The numbers the webview needs, from `thresholds.toml` (D17).
//!
//! The UI never carries a literal of its own for a deadline or a limit: "not responding after six
//! seconds" is `ipc.heartbeat_timeout_secs`, and the page limit Settings shows is
//! `limits.max_pages`. They cross the IPC boundary once, at start-up, in this shape.

use oc_core::thresholds::T;
use serde::Serialize;

/// What `ui_config` returns. Field names are the webview's (`src/lib/backend.ts`, `UiConfig`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiConfig {
    pub app_version: String,
    pub heartbeat_timeout_ms: u64,
    pub cancel_deadline_ms: u64,
    pub kill_after_ms: u64,
    pub copied_revert_ms: u64,
    pub supervisor_tick_ms: u64,
    pub max_pages: u64,
    pub max_memory_bytes: u64,
    /// The stage deadline every job carries unless the user set one, and the range Settings ›
    /// Advanced accepts (`desktop.*_stage_deadline_secs`).
    pub default_stage_deadline_secs: u64,
    pub min_stage_deadline_secs: u64,
    pub max_stage_deadline_secs: u64,
    /// `std::env::consts::OS`, for the one OS-specific thing the UI says: how to install Tesseract.
    pub os: String,
    /// How many of the four AI tasks this build enables for at least one language
    /// (`ai.task.<task>.languages`, PHASE 10 detail 7). Zero means turning AI assistance on changes
    /// no book, and Settings says so rather than let the switch promise what it does not do.
    pub ai_tasks_enabled: u64,
    /// This build has the in-app updater (PHASE 15 detail 5): every build but the Flatpak's, which
    /// Flathub updates and which is built without the `updater` feature. Settings shows the
    /// "Check for updates" row only when it is there.
    pub updater: bool,
}

/// Seconds as milliseconds, saturating; a negative threshold is a malformed file, not a runtime
/// condition, and `thresholds-lint` is what catches it.
fn ms(seconds: i64) -> u64 {
    u64::try_from(seconds)
        .unwrap_or_default()
        .saturating_mul(MS_PER_SECOND)
}

/// A unit conversion, not a tunable.
const MS_PER_SECOND: u64 = 1000;

impl UiConfig {
    pub fn from_thresholds(app_version: &str) -> Self {
        Self {
            app_version: app_version.to_owned(),
            heartbeat_timeout_ms: ms(T.ipc.heartbeat_timeout_secs),
            cancel_deadline_ms: ms(T.ipc.cancel_deadline_secs),
            kill_after_ms: ms(T.ipc.kill_after_secs),
            copied_revert_ms: ms(T.desktop.copied_revert_secs),
            supervisor_tick_ms: u64::try_from(T.desktop.supervisor_tick_ms).unwrap_or_default(),
            max_pages: u64::try_from(T.limits.max_pages).unwrap_or_default(),
            max_memory_bytes: u64::try_from(T.limits.max_memory_bytes).unwrap_or_default(),
            default_stage_deadline_secs: u64::try_from(T.desktop.default_stage_deadline_secs)
                .unwrap_or_default(),
            min_stage_deadline_secs: u64::try_from(T.desktop.min_stage_deadline_secs)
                .unwrap_or_default(),
            max_stage_deadline_secs: u64::try_from(T.desktop.max_stage_deadline_secs)
                .unwrap_or_default(),
            os: std::env::consts::OS.to_owned(),
            ai_tasks_enabled: [
                T.ai.task.metadata.languages,
                T.ai.task.heading_roles.languages,
                T.ai.task.book_structure.languages,
                T.ai.task.verse_quote.languages,
            ]
            .iter()
            .filter(|languages| !languages.is_empty())
            .count() as u64,
            updater: cfg!(feature = "updater"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The UI's updater row follows the build, not a setting: a build compiled without the
    /// `updater` feature (the Flatpak) has no command to call, and the row is not shown.
    #[test]
    fn the_ui_is_told_whether_this_build_has_an_updater() {
        let config = UiConfig::from_thresholds("0.1.0");
        assert_eq!(config.updater, cfg!(feature = "updater"));
        let json = serde_json::to_value(&config).expect("json");
        assert_eq!(
            json["updater"],
            serde_json::Value::Bool(cfg!(feature = "updater"))
        );
    }

    #[test]
    fn every_number_the_ui_shows_comes_from_thresholds() {
        let config = UiConfig::from_thresholds("0.1.0");
        assert_eq!(
            config.heartbeat_timeout_ms,
            u64::try_from(T.ipc.heartbeat_timeout_secs).expect("positive") * 1000
        );
        assert_eq!(
            config.max_pages,
            u64::try_from(T.limits.max_pages).expect("positive")
        );
        let json = serde_json::to_value(&config).expect("serialises");
        assert!(
            json["heartbeatTimeoutMs"].is_u64(),
            "camelCase for the webview: {json}"
        );
        assert!(json["supervisorTickMs"].is_u64());
        assert_eq!(
            json["defaultStageDeadlineSecs"], T.desktop.default_stage_deadline_secs,
            "the stage deadline Settings shows is the app's default"
        );
        assert_eq!(
            json["minStageDeadlineSecs"],
            T.desktop.min_stage_deadline_secs
        );
        assert_eq!(
            json["maxStageDeadlineSecs"],
            T.desktop.max_stage_deadline_secs
        );
        let enabled = [
            T.ai.task.metadata.languages.is_empty(),
            T.ai.task.heading_roles.languages.is_empty(),
            T.ai.task.book_structure.languages.is_empty(),
            T.ai.task.verse_quote.languages.is_empty(),
        ]
        .iter()
        .filter(|empty| !**empty)
        .count();
        assert_eq!(
            json["aiTasksEnabled"], enabled,
            "from the four language maps"
        );
    }
}
