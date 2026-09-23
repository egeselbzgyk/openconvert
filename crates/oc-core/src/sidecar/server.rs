//! `OwnedServer`: a `llama-server` the engine started and must stop (D8, PHASE 9 detail 3).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use secrecy::SecretString;

use super::llama::{self, ServerSpec, LOOPBACK};
use super::supervise;
use crate::thresholds::T;

/// A per-run key is 256 bits from the operating system's CSPRNG, written as hex. A format
/// constant, not a tunable.
const KEY_BYTES: usize = 32;

/// Why a sidecar could not be used.
#[derive(Debug, thiserror::Error)]
pub enum SidecarError {
    #[error("could not start {program}: {message}")]
    Spawn { program: PathBuf, message: String },
    #[error("the server exited before it became healthy")]
    Exited,
    #[error("the server was not healthy within {0:?}")]
    Unhealthy(Duration),
    #[error("could not read the API key file {path}: {message}")]
    KeyFile { path: PathBuf, message: String },
    #[error("no CSPRNG to mint a key with: {0}")]
    Random(String),
}

/// What a health probe saw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Health {
    Ready,
    NotYet,
}

/// A server the engine owns. Dropping it kills it.
#[derive(Debug)]
pub struct OwnedServer {
    pid: u32,
    port: u16,
    api_key: SecretString,
    idle_kill: Duration,
    last_activity: Instant,
    in_flight: u32,
}

impl OwnedServer {
    /// Start `program` for `spec` on `127.0.0.1:port` with a fresh CSPRNG key. The server is
    /// registered with the supervisor before this returns, so from here on the engine cannot end
    /// without taking it down.
    pub fn spawn(program: &Path, spec: &ServerSpec, port: u16) -> Result<Self, SidecarError> {
        let api_key = mint_key()?;
        supervise::install();
        let child = llama::command(program, spec, port, &api_key)
            .spawn()
            .map_err(|error| SidecarError::Spawn {
                program: program.to_owned(),
                message: error.to_string(),
            })?;
        let pid = supervise::register(child);
        Ok(Self {
            pid,
            port,
            api_key,
            idle_kill: Duration::from_secs(u64::try_from(T.llm.idle_kill_secs).unwrap_or_default()),
            last_activity: Instant::now(),
            in_flight: 0,
        })
    }

    /// Ask `probe` until it says `Ready`, the server exits, or `timeout` passes.
    pub fn wait_healthy(
        &self,
        probe: &dyn Fn(&OwnedServer) -> Health,
        timeout: Duration,
    ) -> Result<(), SidecarError> {
        let poll =
            Duration::from_millis(u64::try_from(T.llm.health_poll_millis).unwrap_or_default());
        let start = Instant::now();
        loop {
            if !self.is_running() {
                return Err(SidecarError::Exited);
            }
            if probe(self) == Health::Ready {
                return Ok(());
            }
            if start.elapsed() >= timeout {
                return Err(SidecarError::Unhealthy(timeout));
            }
            std::thread::sleep(poll);
        }
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn api_key(&self) -> &SecretString {
        &self.api_key
    }

    /// `http://127.0.0.1:<port>`.
    pub fn base_url(&self) -> String {
        format!("http://{LOOPBACK}:{}", self.port)
    }

    pub fn is_running(&self) -> bool {
        supervise::is_running(self.pid)
    }

    /// A call is about to be made.
    pub fn call_started(&mut self, now: Instant) {
        self.in_flight = self.in_flight.saturating_add(1);
        self.last_activity = now;
    }

    /// A call came back.
    pub fn call_finished(&mut self, now: Instant) {
        self.in_flight = self.in_flight.saturating_sub(1);
        self.last_activity = now;
    }

    /// Kill the server if no call is in flight and none has been for `llm.idle_kill_secs`.
    /// Returns whether it was killed. `now` is passed in, so the policy is testable with any clock.
    pub fn kill_if_idle(&mut self, now: Instant) -> bool {
        if self.in_flight > 0 || !self.is_running() {
            return false;
        }
        if now.saturating_duration_since(self.last_activity) < self.idle_kill {
            return false;
        }
        supervise::kill(self.pid);
        true
    }
}

impl Drop for OwnedServer {
    fn drop(&mut self) {
        supervise::kill(self.pid);
    }
}

/// 256 bits from the operating system's CSPRNG, as lowercase hex.
fn mint_key() -> Result<SecretString, SidecarError> {
    let mut bytes = [0u8; KEY_BYTES];
    getrandom::fill(&mut bytes).map_err(|error| SidecarError::Random(error.to_string()))?;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    Ok(SecretString::from(hex))
}

/// Read an API key file once (`--llm-api-key-file`). Surrounding whitespace is not part of the key.
pub fn read_key_file(path: &Path) -> Result<SecretString, SidecarError> {
    let error = |message: String| SidecarError::KeyFile {
        path: path.to_owned(),
        message,
    };
    let text = std::fs::read_to_string(path).map_err(|e| error(e.to_string()))?;
    let key = text.trim();
    if key.is_empty() {
        return Err(error("the file is empty".to_owned()));
    }
    Ok(SecretString::from(key.to_owned()))
}
