//! The per-OS data directory, and what lives in it: the model store and the LLM answer cache.
//!
//! One definition, because the model manager (`openconvert model`) and the conversion's AI step
//! (`convert --ai`) must agree on where a model is (PHASE 9 detail 6, ARCHITECTURE §9.4).

use std::path::PathBuf;

/// Where models live: `<data>/openconvert/models` — the model store's own default, so that the
/// desktop app's model manager, `openconvert model` and the AI step name one place.
pub fn models() -> PathBuf {
    oc_net::store::default_root()
}

/// The network audit log: `<data>/openconvert/network-audit.log` (PHASE 14 detail 12).
pub fn network_audit_log() -> oc_net::audit::AuditLog {
    let rotate =
        u64::try_from(oc_core::thresholds::T.net.audit_log_rotate_bytes).unwrap_or(u64::MAX);
    oc_net::audit::AuditLog::in_data_dir(&base(), rotate)
}

/// Where the LLM answer cache lives: `<data>/openconvert/cache/llm` (ARCHITECTURE §9.4).
pub fn llm_cache() -> PathBuf {
    base().join("openconvert").join("cache").join("llm")
}

#[cfg(target_os = "linux")]
fn base() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| home().map(|home| home.join(".local").join("share")))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(target_os = "macos")]
fn base() -> PathBuf {
    home()
        .map(|home| home.join("Library").join("Application Support"))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(windows)]
fn base() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn base() -> PathBuf {
    home().unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(unix)]
fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}
