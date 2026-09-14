//! `cargo xtask fetch-epubcheck` — fetch the pinned EPUBCheck release, verify its SHA-256
//! against `xtask/epubcheck.lock`, and unpack it to `vendor/epubcheck/` (D6).
//!
//! The download and the extraction shell out to `curl` and `tar`, exactly as `vendor-pdfium`
//! does and for the same reason: `oc-net` stays the only crate in the workspace that can open a
//! socket, and no build-time tool needs an HTTP client either (D13.9).
//!
//! The jar is never committed and never bundled into a shipped artefact. CI downloads it; in
//! the application it arrives with the optional validation pack.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const LOCK: &str = include_str!("../epubcheck.lock");

/// The lock-file schema this task understands.
const SUPPORTED_SCHEMA_VERSION: u32 = 1;

#[derive(Deserialize)]
struct Lock {
    schema_version: u32,
    repo: String,
    tag: String,
    version: String,
    asset: String,
    sha256: String,
    size_bytes: u64,
    jar: String,
}

/// Where the unpacked release lives.
pub fn vendor_dir(workspace_root: &Path) -> PathBuf {
    workspace_root.join("vendor/epubcheck")
}

/// The jar, once fetched.
pub fn jar_path(workspace_root: &Path) -> Result<PathBuf> {
    let lock = lock()?;
    Ok(vendor_dir(workspace_root).join(&lock.jar))
}

pub fn run(workspace_root: &Path) -> Result<()> {
    let lock = lock()?;
    let directory = vendor_dir(workspace_root);
    let jar = directory.join(&lock.jar);

    if jar.is_file() {
        println!("epubcheck {} already at {}", lock.version, jar.display());
        return Ok(());
    }

    std::fs::create_dir_all(&directory)
        .with_context(|| format!("cannot create {}", directory.display()))?;
    let archive = directory.join(&lock.asset);

    let url = format!(
        "https://github.com/{}/releases/download/{}/{}",
        lock.repo, lock.tag, lock.asset
    );
    println!("fetching {url}");
    let status = Command::new("curl")
        .arg("--location")
        .arg("--fail")
        .arg("--silent")
        .arg("--show-error")
        .arg("--output")
        .arg(&archive)
        .arg(&url)
        .status()
        .context("cannot run curl")?;
    if !status.success() {
        bail!("curl failed fetching {url}");
    }

    verify(&archive, &lock)?;

    let status = Command::new("tar")
        .arg("-xf")
        .arg(&archive)
        .arg("-C")
        .arg(&directory)
        .status()
        .context("cannot run tar")?;
    if !status.success() {
        bail!("tar failed unpacking {}", archive.display());
    }
    // The archive is large and every byte of it is already on disk, unpacked.
    let _ = std::fs::remove_file(&archive);

    if !jar.is_file() {
        bail!(
            "{} is not in the archive; the lock's `jar` path is wrong",
            lock.jar
        );
    }
    println!("epubcheck {} at {}", lock.version, jar.display());
    Ok(())
}

/// Read and check the lock file.
fn lock() -> Result<Lock> {
    let lock: Lock = toml::from_str(LOCK).context("xtask/epubcheck.lock is not valid TOML")?;
    if lock.schema_version != SUPPORTED_SCHEMA_VERSION {
        bail!(
            "epubcheck.lock declares schema_version {}; this xtask understands \
             {SUPPORTED_SCHEMA_VERSION}",
            lock.schema_version
        );
    }
    Ok(lock)
}

/// The digest and the size, both, before anything is unpacked.
///
/// A release asset that is re-uploaded — which does happen — would otherwise silently change
/// what the gate means. The downloaded file is deleted on a mismatch so that a second run
/// cannot pick up a half-verified archive.
fn verify(archive: &Path, lock: &Lock) -> Result<()> {
    let bytes =
        std::fs::read(archive).with_context(|| format!("cannot read {}", archive.display()))?;

    if bytes.len() as u64 != lock.size_bytes {
        let _ = std::fs::remove_file(archive);
        bail!(
            "{} is {} bytes; the lock pins {}",
            lock.asset,
            bytes.len(),
            lock.size_bytes
        );
    }

    let digest = format!("{:x}", Sha256::digest(&bytes));
    if digest != lock.sha256 {
        let _ = std::fs::remove_file(archive);
        bail!(
            "{} hashes to {digest}; the lock pins {}",
            lock.asset,
            lock.sha256
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// The lock is the whole of the supply-chain claim, so it has to parse and it has to carry a
/// digest — a lock with an empty hash would make `fetch-epubcheck` accept anything the network
/// handed it.
#[test]
fn the_lock_pins_a_digest_and_a_size() {
    let lock = lock().expect("the lock parses");
    assert_eq!(lock.sha256.len(), 64, "a SHA-256 is 64 hex characters");
    assert!(lock.sha256.chars().all(|ch| ch.is_ascii_hexdigit()));
    assert!(lock.size_bytes > 0);
    assert!(lock.jar.ends_with("epubcheck.jar"));
    assert!(lock.asset.contains(&lock.version));
}
