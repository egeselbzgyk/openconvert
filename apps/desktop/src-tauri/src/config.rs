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
    /// `std::env::consts::OS`, for the one OS-specific thing the UI says: how to install Tesseract.
    pub os: String,
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
            os: std::env::consts::OS.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
