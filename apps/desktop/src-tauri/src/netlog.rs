//! Settings › Network log: `oc-net`'s audit log, newest first (PHASE 14 detail 12, SECURITY §8).
//!
//! The section is the design's own decision (it is visible by default, so "no network unless you
//! asked" can be checked without digging) and SECURITY §8's promise: every outbound connection
//! `oc-net` makes is written to a local, user-visible audit log — one JSON line
//! `{ts, host, purpose, bytes, outcome, loopback}` per connection in
//! `<data_dir>/openconvert/network-audit.log`, and the previous generation in `….log.1`. The engine
//! and this app write the same file (`AuditLog::default_location`): the engine for a conversion's
//! model calls and `provider` probes, the app for the downloads its model manager makes.
//!
//! It lists what was written down and nothing else: no row is invented, no count is estimated. A
//! line this reader does not understand is skipped rather than guessed at.

use std::path::Path;

use oc_net::audit::{AuditLog, ROTATED_SUFFIX};
use serde::Serialize;

/// What Settings › Network log shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum NetworkLog {
    /// No audit log could be read. Kept for a build whose data directory is unreadable.
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

/// The audit log both processes write, where they write it.
pub fn log() -> AuditLog {
    let rotate =
        u64::try_from(oc_core::thresholds::T.net.audit_log_rotate_bytes).unwrap_or(u64::MAX);
    AuditLog::default_location(rotate)
}

/// The network log: every recorded connection, newest first. No log yet is no connections yet.
pub fn read() -> NetworkLog {
    read_from(log().path())
}

/// [`read`] from an explicit file (and its rotated predecessor).
pub fn read_from(path: &Path) -> NetworkLog {
    let mut rotated = path.as_os_str().to_owned();
    rotated.push(ROTATED_SUFFIX);
    let mut entries: Vec<NetworkEntry> = [std::path::PathBuf::from(rotated), path.to_path_buf()]
        .iter()
        .filter_map(|file| std::fs::read_to_string(file).ok())
        .flat_map(|text| {
            text.lines()
                .filter_map(|line| serde_json::from_str::<Line>(line).ok())
                .map(NetworkEntry::from)
                .collect::<Vec<_>>()
        })
        .collect();
    entries.reverse();
    NetworkLog::Entries { entries }
}

/// One line as `oc-net` writes it.
#[derive(serde::Deserialize)]
struct Line {
    ts: String,
    host: String,
    purpose: String,
    bytes: u64,
    outcome: String,
}

impl From<Line> for NetworkEntry {
    fn from(line: Line) -> Self {
        Self {
            ts: line.ts,
            host: line.host,
            purpose: line.purpose,
            bytes: line.bytes,
            outcome: line.outcome,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The page's two shapes, as the webview reads them.
    #[test]
    fn the_network_log_serialises_in_the_shapes_the_page_reads() {
        assert_eq!(
            serde_json::to_value(NetworkLog::NotRecorded).expect("serialises"),
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

    /// PHASE 14 detail 12, as the page sees it: what `oc-net` wrote, newest first, across the
    /// rotation; no file is no connections; a line from another version is skipped, not guessed at.
    #[test]
    fn the_network_log_lists_the_audit_log_newest_first() {
        let dir = std::env::temp_dir().join(format!("oc-netlog-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let log = oc_net::audit::AuditLog::in_data_dir(&dir, u64::MAX);
        assert_eq!(
            read_from(log.path()),
            NetworkLog::Entries {
                entries: Vec::new()
            }
        );

        for (host, purpose) in [
            ("huggingface.co", oc_net::audit::Purpose::Download),
            ("127.0.0.1", oc_net::audit::Purpose::LlmRequest),
        ] {
            log.append(&oc_net::audit::Entry::now(
                &format!("https://{host}/x"),
                purpose,
                7,
                "ok",
            ))
            .expect("append");
        }
        std::fs::OpenOptions::new()
            .append(true)
            .open(log.path())
            .and_then(|mut file| std::io::Write::write_all(&mut file, b"not json\n"))
            .expect("a stray line");
        let NetworkLog::Entries { entries } = read_from(log.path()) else {
            panic!("entries");
        };
        let hosts: Vec<&str> = entries.iter().map(|entry| entry.host.as_str()).collect();
        assert_eq!(hosts, ["127.0.0.1", "huggingface.co"]);
        assert_eq!(entries[0].purpose, "llm-request");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
