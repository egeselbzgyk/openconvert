//! The engine restricting itself (PHASE 14 details 5 and 7, SECURITY §11 "now, low-cost
//! platform-native" tier).
//!
//! Three modules, one per mechanism, each a thin layer over a crate that owns the system call —
//! `rustix` for `setrlimit` and `prctl`, `landlock` for Landlock, `win32job` for job objects — so
//! that none of them needs an `unsafe` block of this project's (CLAUDE.md, ARCHITECTURE §5):
//!
//! - [`rlimit`]: `--max-memory` as `RLIMIT_AS`, applied before the PDF is opened;
//! - [`landlock`]: the filesystem (and, on ABI ≥ 4, TCP) restricted to the job's [`ScopeSet`]
//!   before the first PDF byte is read — Linux only, and never fatal;
//! - [`jobobject`]: on Windows, a job object that takes the engine's children with it.
//!
//! **Nothing here ever fails a conversion.** Each returns an outcome the report records: a
//! sandbox that could not be applied is a line in the report, not an error for the user.

pub mod jobobject;
pub mod landlock;
pub mod rlimit;

use std::path::PathBuf;

/// What a job may touch once it is restricted (PHASE 14 detail 7).
///
/// `read`: the input PDF, the model store, tessdata, and — when a child process will be started —
/// the roots its executable and libraries live under. `read_write`: the output directory and the
/// job's temporary directory. Nothing else on the filesystem is reachable afterwards.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScopeSet {
    pub read: Vec<PathBuf>,
    pub read_write: Vec<PathBuf>,
    /// TCP ports the job may connect to: the LLM endpoint's, when `--ai` has one. Empty is none.
    pub connect_ports: Vec<u16>,
}

/// Why a mechanism was not applied. Always recorded, never raised.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SandboxError {
    #[error("{mechanism} is not available on this platform")]
    Unsupported { mechanism: &'static str },
    #[error("{mechanism} could not be applied: {message}")]
    Failed {
        mechanism: &'static str,
        message: String,
    },
}
