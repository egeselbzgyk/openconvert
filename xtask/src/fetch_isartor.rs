//! `cargo xtask fetch-isartor` — the Isartor suite for the crash-regression tier (PHASE 14 detail 9,
//! row 14.16), pinned by commit and per-file SHA-256 in `xtask/isartor.lock`, written to
//! `target/isartor/` and never committed.
//!
//! The download shells out to `curl`, as every other fetch here does: `oc-net` stays the only crate
//! that opens a socket (D13.9). A file whose digest differs is deleted and the task fails; a lock
//! that is not pinned yet makes the task fail before anything is fetched.

use std::path::{Component, Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const LOCK: &str = include_str!("../isartor.lock");
const SUPPORTED_SCHEMA_VERSION: u32 = 1;
/// The lock's named-slot convention for a value not filled yet (as in `models.toml`).
const PLACEHOLDER: &str = "TODO_";

#[derive(Deserialize)]
struct Lock {
    schema_version: u32,
    source: String,
    commit: String,
    files: Vec<LockedFile>,
}

#[derive(Deserialize)]
struct LockedFile {
    path: String,
    sha256: String,
}

/// Where the fetched suite lives.
pub fn target_dir(workspace_root: &Path) -> PathBuf {
    workspace_root.join("target/isartor")
}

pub fn run(workspace_root: &Path) -> Result<()> {
    let lock = lock()?;
    let root = target_dir(workspace_root);
    for file in &lock.files {
        let relative = safe_relative(&file.path)?;
        let target = root.join(&relative);
        if target.is_file() && sha256_of(&target)? == file.sha256 {
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        let url = lock
            .source
            .replace("{commit}", &lock.commit)
            .replace("{path}", &file.path.replace(' ', "%20"));
        let status = Command::new("curl")
            .args([
                "--location",
                "--fail",
                "--silent",
                "--show-error",
                "--output",
            ])
            .arg(&target)
            .arg(&url)
            .status()
            .context("cannot run curl")?;
        if !status.success() {
            bail!("curl failed fetching {url}");
        }
        let actual = sha256_of(&target)?;
        if actual != file.sha256 {
            let _ = std::fs::remove_file(&target);
            bail!(
                "{} has SHA-256 {actual}; xtask/isartor.lock pins {}",
                file.path,
                file.sha256
            );
        }
    }
    println!("{} Isartor files at {}", lock.files.len(), root.display());
    Ok(())
}

/// Read the lock, refusing one that is not pinned.
fn lock() -> Result<Lock> {
    let lock: Lock = toml::from_str(LOCK).context("xtask/isartor.lock is not valid TOML")?;
    if lock.schema_version != SUPPORTED_SCHEMA_VERSION {
        bail!(
            "isartor.lock declares schema_version {}; this xtask understands {SUPPORTED_SCHEMA_VERSION}",
            lock.schema_version
        );
    }
    if lock.commit.starts_with(PLACEHOLDER) || lock.files.is_empty() {
        bail!(
            "xtask/isartor.lock is not pinned yet (commit `{}`, {} files): fill it on a machine \
             that can reach the source, as its header says",
            lock.commit,
            lock.files.len()
        );
    }
    Ok(lock)
}

/// A lock path as a relative path that cannot leave `target/isartor/`.
fn safe_relative(path: &str) -> Result<PathBuf> {
    let candidate = PathBuf::from(path);
    if candidate
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        Ok(candidate)
    } else {
        bail!("xtask/isartor.lock names `{path}`, which is not a plain relative path")
    }
}

fn sha256_of(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[test]
fn an_unpinned_lock_refuses_before_fetching() {
    let error = lock()
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(
        error.contains("not pinned yet"),
        "the committed lock is a placeholder until it is filled: {error}"
    );
}

#[test]
fn a_lock_path_cannot_escape_the_target() {
    assert!(safe_relative("Isartor test files/PDFA-1b/6.1 File structure/a.pdf").is_ok());
    assert!(safe_relative("../outside.pdf").is_err());
    assert!(safe_relative("/etc/passwd").is_err());
}
