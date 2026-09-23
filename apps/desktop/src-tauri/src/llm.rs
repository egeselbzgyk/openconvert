//! The app-owned `llama-server` (PHASE 9 detail 3, D8, RT C2/A5.5).
//!
//! When AI assistance is on, the **app** owns one long-lived server and hands every engine its
//! address as an external endpoint, so the engine spawns nothing and a 1 GB model loads once per
//! batch rather than once per book. There is one code path for starting and stopping it, the one
//! the CLI engine uses for a server of its own: [`oc_core::sidecar::server::OwnedServer`].
//!
//! - **Where it listens.** `127.0.0.1` on a port the kernel has just reported free
//!   ([`oc_net::loopback::free_port`]), never llama-server's default 8080, with one slot (`-np 1`).
//! - **Its key.** A fresh CSPRNG key per start. The server reads it from `LLAMA_API_KEY`; an
//!   engine reads it from a file in the app's run directory that only this user can read (the
//!   job spec's `ai.api_key_file`). It is never on a command line, where `ps` would show it, and
//!   never in a job spec, which is kept on disk.
//! - **Whether it is ready.** `GET /health` through `oc-net` (the only crate that opens a socket),
//!   asked every `llm.health_poll_millis` until it answers or `llm.load_timeout_secs` pass.
//! - **When it stops.** When no job has used it for `llm.idle_kill_secs` ([`LlmHost::tick`], from
//!   the app's supervisor clock); when the app exits ([`LlmHost::shutdown`], and `Drop`); and, on a
//!   panic or a signal, through `oc_core::sidecar::supervise`, which owns the child from the moment
//!   it is spawned. Its key file goes with it.
//!
//! Turning AI assistance on — and so the first [`LlmHost::acquire`] — is Phase 10's engine side and
//! Phase 12's toggle; until then nothing calls it and no server is ever started.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use oc_core::sidecar::llama::ServerSpec;
use oc_core::sidecar::server::{Health, OwnedServer, SidecarError};
use oc_core::thresholds::T;
use oc_net::transport::HttpTransport;
use secrecy::ExposeSecret;

/// The key file's name in the run directory.
const KEY_FILE: &str = "llm.key";

/// Why the app's server could not be used.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error(transparent)]
    Sidecar(#[from] SidecarError),
    #[error("no free loopback port: {0}")]
    Port(std::io::Error),
    #[error("could not write the key file {path}: {error}")]
    KeyFile {
        path: PathBuf,
        error: std::io::Error,
    },
    /// A job still holds the server, and another model was asked for. With one conversion at a
    /// time (`desktop.max_concurrent_jobs`) this is a bug in the caller, not a state a user sees.
    #[error("the model server is in use by another job")]
    Busy,
}

/// What a job needs to use the server: the job spec's `ai.endpoint` and `ai.api_key_file`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lease {
    /// `http://127.0.0.1:<port>`.
    pub endpoint: String,
    pub api_key_file: PathBuf,
}

/// A running server and what it was started for.
struct Hosted {
    server: OwnedServer,
    spec: ServerSpec,
    key_file: PathBuf,
    leases: u32,
}

/// The app's model server: started on first use, shared by every job, stopped when idle.
pub struct LlmHost {
    program: PathBuf,
    run_dir: PathBuf,
    hosted: Option<Hosted>,
}

impl LlmHost {
    /// A host that starts `program` (the bundled `llama-server`) and keeps its key file in
    /// `run_dir`. Nothing is started until [`LlmHost::acquire`].
    pub fn new(program: PathBuf, run_dir: PathBuf) -> Self {
        Self {
            program,
            run_dir,
            hosted: None,
        }
    }

    /// The server for `spec`, started if none is running — or restarted, if the one running
    /// serves another model and no job holds it. The caller releases the lease with
    /// [`LlmHost::release`] when its job ends.
    pub fn acquire(&mut self, spec: &ServerSpec, now: Instant) -> Result<Lease, LlmError> {
        self.forget_if_exited();
        if let Some(hosted) = &self.hosted {
            if hosted.spec != *spec {
                if hosted.leases > 0 {
                    return Err(LlmError::Busy);
                }
                self.shutdown();
            }
        }
        if self.hosted.is_none() {
            self.hosted = Some(self.start(spec)?);
        }
        let hosted = self.hosted.as_mut().ok_or(LlmError::Busy)?;
        hosted.leases = hosted.leases.saturating_add(1);
        hosted.server.call_started(now);
        Ok(Lease {
            endpoint: hosted.server.base_url(),
            api_key_file: hosted.key_file.clone(),
        })
    }

    /// A job that held the server has ended. The idle clock starts from `now`.
    pub fn release(&mut self, now: Instant) {
        if let Some(hosted) = self.hosted.as_mut() {
            if hosted.leases > 0 {
                hosted.leases -= 1;
                hosted.server.call_finished(now);
            }
        }
    }

    /// The supervisor's clock: stop the server if no job has held it for `llm.idle_kill_secs`, or
    /// forget it if it has died. Returns whether a running server was stopped.
    pub fn tick(&mut self, now: Instant) -> bool {
        self.forget_if_exited();
        let killed = self
            .hosted
            .as_mut()
            .is_some_and(|hosted| hosted.server.kill_if_idle(now));
        if killed {
            self.shutdown();
        }
        killed
    }

    /// Stop the server, if one is running, and delete its key.
    pub fn shutdown(&mut self) {
        if let Some(hosted) = self.hosted.take() {
            drop(hosted.server);
            let _ = std::fs::remove_file(&hosted.key_file);
        }
    }

    /// The running server's process id.
    pub fn pid(&self) -> Option<u32> {
        self.hosted.as_ref().map(|hosted| hosted.server.pid())
    }

    fn forget_if_exited(&mut self) {
        if self
            .hosted
            .as_ref()
            .is_some_and(|hosted| !hosted.server.is_running())
        {
            self.shutdown();
        }
    }

    fn start(&self, spec: &ServerSpec) -> Result<Hosted, LlmError> {
        let port = oc_net::loopback::free_port().map_err(LlmError::Port)?;
        let server = OwnedServer::spawn(&self.program, spec, port)?;
        // From here on `server` is registered with the supervisor: an early return drops it, and
        // dropping it kills it.
        let key_file = self.run_dir.join(KEY_FILE);
        write_private(&key_file, server.api_key().expose_secret()).map_err(|error| {
            LlmError::KeyFile {
                path: key_file.clone(),
                error,
            }
        })?;
        let ready = server.wait_healthy(&probe, secs(T.llm.load_timeout_secs));
        if let Err(error) = ready {
            let _ = std::fs::remove_file(&key_file);
            return Err(error.into());
        }
        Ok(Hosted {
            server,
            spec: spec.clone(),
            key_file,
            leases: 0,
        })
    }
}

impl Drop for LlmHost {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// `GET /health`: ready once it answers 2xx. llama-server answers without a key, and 503 while it
/// loads the model.
fn probe(server: &OwnedServer) -> Health {
    let Ok(transport) = HttpTransport::new(&server.base_url(), None) else {
        return Health::NotYet;
    };
    let timeout =
        Duration::from_millis(u64::try_from(T.llm.health_probe_timeout_millis).unwrap_or_default());
    match transport.get("/health", timeout) {
        Ok(_) => Health::Ready,
        Err(_) => Health::NotYet,
    }
}

/// Write `text` to `path` so that only this user can read it: a fresh file, mode `0600` on Unix
/// (on Windows the app's data directory is the user's own). A key file left behind by a crash is
/// replaced, never appended to or reused.
fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Owner read and write, nobody else anything: an operating-system permission mask.
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()
}

fn secs(value: i64) -> Duration {
    Duration::from_secs(u64::try_from(value).unwrap_or_default())
}

/// Where the bundled server is: beside this executable, as Tauri's `externalBin` places it.
pub fn server_path() -> std::io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let dir = exe.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "the app's own directory is unknown",
        )
    })?;
    Ok(dir.join(format!("llama-server{}", std::env::consts::EXE_SUFFIX)))
}

/// The server command's settings for a model, from its registry entry: its context and its
/// architecture's cache flags (RT A3), and one thread per core the machine has.
pub fn spec_for(entry: &oc_net::registry::ModelEntry, model: PathBuf) -> ServerSpec {
    // A machine that cannot say how many cores it has gets one thread, not a guess.
    let cores = std::thread::available_parallelism().unwrap_or(std::num::NonZeroUsize::MIN);
    ServerSpec {
        model,
        context: entry.context,
        threads: u32::try_from(cores.get()).unwrap_or(u32::MAX),
        cache_reuse: entry.cache_reuse,
        context_checkpoints: entry.context_checkpoints,
    }
}
