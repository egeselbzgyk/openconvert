//! Settings › Network log: what the app reads for it, and why it reads nothing yet.
//!
//! The section is the design's own decision (it is visible by default, so "no network unless you
//! asked" can be checked without digging) and SECURITY §8's promise: every outbound connection
//! `oc-net` makes is written to a local, user-visible audit log. That log — `oc-net/src/audit.rs`,
//! one line `{ts, host, purpose, bytes, outcome}` per connection in `<data_dir>/network-audit.log` —
//! is **PHASE 14 detail 12**, and does not exist in this build.
//!
//! **This is the hook for it, not a stand-in.** [`read`] answers [`NetworkLog::NotRecorded`], always,
//! and the page says in words that this build does not record its connections yet and which ones it
//! makes. It never lists a connection it did not see written down: no row is invented, no count is
//! estimated. Phase 14 replaces [`read`]'s body with a reader of the audit log and fills
//! [`NetworkLog::Entries`]; the command, the type and the page are already wired to it.

use serde::Serialize;

/// What Settings › Network log shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum NetworkLog {
    /// This build keeps no audit log (PHASE 14 detail 12 adds it).
    NotRecorded,
    /// The audit log's lines, newest first.
    Entries { entries: Vec<NetworkEntry> },
}

/// One connection, as PHASE 14 detail 12 specifies the audit log's line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NetworkEntry {
    /// RFC 3339, UTC.
    pub ts: String,
    pub host: String,
    /// Why the connection was made: a model or pack download, a provider question, a conversion's
    /// model calls.
    pub purpose: String,
    pub bytes: u64,
    pub outcome: String,
}

/// The network log. The hook PHASE 14 detail 12 fills: until `oc-net` writes an audit log there is
/// nothing to read.
pub fn read() -> NetworkLog {
    NetworkLog::NotRecorded
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The page's two shapes, as the webview reads them — and this build's answer, which is the
    /// honest one: nothing recorded, rather than a list of connections nobody wrote down.
    #[test]
    fn the_network_log_says_it_is_not_recorded_until_there_is_an_audit_log() {
        assert_eq!(read(), NetworkLog::NotRecorded);
        assert_eq!(
            serde_json::to_value(read()).expect("serialises"),
            serde_json::json!({ "state": "not_recorded" })
        );
        let entry = NetworkLog::Entries {
            entries: vec![NetworkEntry {
                ts: "2026-09-22T10:14:00Z".to_owned(),
                host: "huggingface.co".to_owned(),
                purpose: "model download".to_owned(),
                bytes: 1_181_116_006,
                outcome: "ok".to_owned(),
            }],
        };
        let json = serde_json::to_value(entry).expect("serialises");
        assert_eq!(json["state"], "entries");
        assert_eq!(json["entries"][0]["host"], "huggingface.co");
    }
}
