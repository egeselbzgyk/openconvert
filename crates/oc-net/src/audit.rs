//! The network audit log (PHASE 14 detail 12, SECURITY §8, D13.9).
//!
//! Every connection this crate opens appends one JSON line — `{ts, host, purpose, bytes, outcome}`,
//! plus whether the host is this machine — to `<data_dir>/openconvert/network-audit.log`, rotated
//! at a size the caller reads from `thresholds.toml` (`net.audit_log_rotate_bytes`). It prevents
//! nothing. It makes "no network unless you asked for it" something a user can check: a model
//! download is a line, and a conversion without `--ai` is none, because nothing on the conversion
//! path links this crate (D13.9).
//!
//! The log is installed once per process by the binary that decides where the data directory is
//! ([`install`]); a process that never installs one records nothing, which is what tests that do
//! not look at the log want. Writing it never fails a connection: an unwritable log is a lost line,
//! not a lost download.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};

use serde::Serialize;

/// The log's file name, inside `<data_dir>/openconvert/`.
pub const FILE_NAME: &str = "network-audit.log";
/// What the previous generation is renamed to when the log rotates.
pub const ROTATED_SUFFIX: &str = ".1";

/// Why a connection was opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Purpose {
    /// A model or pack download (`model pull`, the desktop's model manager).
    Download,
    /// A question to an LLM endpoint (`convert --ai`).
    LlmRequest,
    /// Asking an endpoint what it is, or whether it is there (`provider detect|probe`, a health
    /// check).
    LlmProbe,
}

/// One line of the log.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Entry {
    /// RFC 3339, UTC, to the second.
    pub ts: String,
    pub host: String,
    pub purpose: Purpose,
    /// Bytes received (and, for a request, sent).
    pub bytes: u64,
    /// `ok`, an HTTP status, or what went wrong.
    pub outcome: String,
    /// Whether the host is this machine.
    pub loopback: bool,
}

impl Entry {
    /// An entry stamped now, for `url`.
    pub fn now(url: &str, purpose: Purpose, bytes: u64, outcome: impl Into<String>) -> Self {
        let host = host_of(url);
        Self {
            ts: now_rfc3339(),
            loopback: crate::consent::is_loopback(&host),
            host,
            purpose,
            bytes,
            outcome: outcome.into(),
        }
    }
}

/// Where the lines go, and when the file is rotated.
#[derive(Debug)]
pub struct AuditLog {
    path: PathBuf,
    rotate_bytes: u64,
    /// One writer at a time: lines from concurrent downloads must not interleave.
    lock: Mutex<()>,
}

impl AuditLog {
    pub fn new(path: PathBuf, rotate_bytes: u64) -> Self {
        Self {
            path,
            rotate_bytes,
            lock: Mutex::new(()),
        }
    }

    /// `<data_dir>/openconvert/network-audit.log`.
    pub fn in_data_dir(data_dir: &Path, rotate_bytes: u64) -> Self {
        Self::new(data_dir.join("openconvert").join(FILE_NAME), rotate_bytes)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append `entry`, rotating first if the line would take the file past its size.
    pub fn append(&self, entry: &Entry) -> std::io::Result<()> {
        let _held = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let mut line = serde_json::to_string(entry).map_err(std::io::Error::other)?;
        line.push('\n');
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let current = std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
        let adding = u64::try_from(line.len()).unwrap_or(u64::MAX);
        if current > 0 && current.saturating_add(adding) > self.rotate_bytes {
            let mut rotated = self.path.clone().into_os_string();
            rotated.push(ROTATED_SUFFIX);
            std::fs::rename(&self.path, PathBuf::from(rotated))?;
        }
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?
            .write_all(line.as_bytes())
    }
}

static INSTALLED: OnceLock<AuditLog> = OnceLock::new();

/// Make `log` the one every connection in this process is recorded in. The first call wins.
pub fn install(log: AuditLog) {
    let _ = INSTALLED.set(log);
}

/// The installed log, if any.
pub fn installed() -> Option<&'static AuditLog> {
    INSTALLED.get()
}

/// Record one connection in the installed log. Never fails the connection.
pub fn record(entry: Entry) {
    if let Some(log) = INSTALLED.get() {
        if let Err(error) = log.append(&entry) {
            eprintln!(
                "openconvert: the network audit log {} could not be written: {error}",
                log.path.display()
            );
        }
    }
}

/// The host of `url`, lowercased, without port or credentials. Empty when there is none.
pub fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = if let Some(bracketed) = host_port.strip_prefix('[') {
        bracketed.split(']').next().unwrap_or_default()
    } else {
        host_port.split(':').next().unwrap_or_default()
    };
    host.to_ascii_lowercase()
}

fn now_rfc3339() -> String {
    use time::format_description::well_known::Rfc3339;
    let now = time::OffsetDateTime::now_utc();
    now.replace_nanosecond(0)
        .unwrap_or(now)
        .format(&Rfc3339)
        .unwrap_or_default()
}

#[test]
fn hosts_are_read_without_port_or_credentials() {
    assert_eq!(host_of("https://HuggingFace.co/a/b"), "huggingface.co");
    assert_eq!(host_of("http://127.0.0.1:8080/health"), "127.0.0.1");
    assert_eq!(host_of("https://user:pw@example.org:443/x"), "example.org");
    assert_eq!(host_of("http://[::1]:11434/api/tags"), "::1");
}

#[test]
fn the_log_rotates_at_its_size() {
    let dir = std::env::temp_dir().join(format!("oc-audit-rotate-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let entry = Entry::now("https://huggingface.co/x", Purpose::Download, 7, "ok");
    let line = serde_json::to_string(&entry).expect("json").len() as u64 + 1;
    // Room for two lines: the third rotates.
    let log = AuditLog::in_data_dir(&dir, 2 * line + 1);
    for _ in 0..3 {
        log.append(&entry).expect("append");
    }
    let current = std::fs::read_to_string(log.path()).expect("current");
    let mut rotated = log.path().as_os_str().to_owned();
    rotated.push(ROTATED_SUFFIX);
    let previous = std::fs::read_to_string(PathBuf::from(rotated)).expect("rotated");
    assert_eq!(current.lines().count(), 1);
    assert_eq!(previous.lines().count(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}
