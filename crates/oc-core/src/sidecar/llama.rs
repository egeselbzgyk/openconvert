//! The `llama-server` command line (PHASE 9 detail 2): verified flags only, and never a key.
//!
//! Every flag here is one V1 §3 verified against the pinned llama.cpp build; none is invented. What
//! varies between models is decided by the model's `models.toml` entry, never by its family name:
//! `--cache-reuse` only for an entry that says `cache_reuse = true` (RT A3 — recurrent layers cannot
//! be KV-shifted), `--ctx-checkpoints` only for an entry that names a count. That flag is spelled
//! as the pinned server's `--help` spells it; the design documents' `--context-checkpoints` is
//! refused as an unknown argument, and the server exits before it is ever healthy.
//!
//! The per-run key never appears in the argument list, which any user on the machine can read with
//! `ps`. `llama-server` reads it from [`API_KEY_ENV`] when `--api-key` is absent, and a process's
//! environment is readable only by its own user.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use secrecy::{ExposeSecret, SecretString};

use crate::thresholds::T;

/// Where `llama-server` reads its API key from when `--api-key` is not given.
pub const API_KEY_ENV: &str = "LLAMA_API_KEY";

/// The only interface the server binds to (D8).
pub const LOOPBACK: &str = "127.0.0.1";

/// One slot: one warm prefix for the whole book (D8, ARCHITECTURE §9.3). A spec constant, not a
/// tunable — more slots would split the prefix cache the design depends on.
const SLOTS: &str = "1";

/// Thinking off for every request by default; the client also asks per request (D10).
const THINKING_OFF: &str = r#"{"enable_thinking":false}"#;

/// Windows' `CREATE_NO_WINDOW`: without it a console flashes on every spawn (ARCHITECTURE §8.2).
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// What the server is started with, taken from the model's registry entry by the caller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerSpec {
    pub model: PathBuf,
    pub context: u32,
    pub threads: u32,
    /// `cache_reuse` in `models.toml`: whether KV shifting works for this architecture (RT A3).
    pub cache_reuse: bool,
    /// `context_checkpoints` in `models.toml`, for hybrid recurrent entries.
    pub context_checkpoints: Option<u32>,
}

/// The command that starts `program` for `spec`, listening on `127.0.0.1:port`, with `key` in the
/// environment. Its output goes nowhere: the server's health is asked over HTTP, never read from
/// its log.
pub fn command(program: &Path, spec: &ServerSpec, port: u16, key: &SecretString) -> Command {
    let mut command = super::orphan::command(program);
    command
        .arg("--host")
        .arg(LOOPBACK)
        .arg("--port")
        .arg(port.to_string())
        .arg("-np")
        .arg(SLOTS)
        .arg("-c")
        .arg(spec.context.to_string())
        .arg("-t")
        .arg(spec.threads.to_string())
        .arg("-m")
        .arg(&spec.model)
        .arg("--chat-template-kwargs")
        .arg(THINKING_OFF);
    if spec.cache_reuse {
        command
            .arg("--cache-reuse")
            .arg(T.llm.cache_reuse_min_chunk.to_string());
    }
    if let Some(checkpoints) = spec.context_checkpoints {
        command
            .arg("--ctx-checkpoints")
            .arg(checkpoints.to_string());
    }
    command
        .env(API_KEY_ENV, key.expose_secret())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}
