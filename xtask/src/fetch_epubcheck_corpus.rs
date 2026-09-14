//! `cargo xtask fetch-epubcheck-corpus` — fetch EPUBCheck's own public test corpus (D6,
//! RT A6.2).
//!
//! The corpus is what makes Tier 1's coverage a measured number rather than an assertion. It is
//! BSD-3-Clause, like EPUBCheck itself, and it is never committed: it is several thousand files
//! of other people's test fixtures, and vendoring them would put this repository's licence
//! surface and its size both somewhere they do not need to be.
//!
//! Pinned by digest, like everything else fetched here. GitHub's source archives are generated
//! rather than uploaded and have been regenerated at least once in the platform's history, so a
//! mismatch here is more likely to mean "GitHub rebuilt the tarball" than "someone tampered
//! with it" — the task says so, and moving the pin is still a reviewed commit.

use std::io::Read;
use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const LOCK: &str = include_str!("../epubcheck-corpus.lock");

const SUPPORTED_SCHEMA_VERSION: u32 = 1;

#[derive(Deserialize)]
struct Lock {
    schema_version: u32,
    repo: String,
    tag: String,
    sha256: String,
    /// The path inside the archive that holds the test resources.
    resources: String,
}

/// Unpack the entries under `prefix` from a zip, preserving their paths.
///
/// Only that subtree: the source archive is six megabytes of build files and the corpus is the
/// part of it that measures anything.
fn extract(archive: &Path, into: &Path, prefix: &str) -> Result<()> {
    let file = std::fs::File::open(archive)
        .with_context(|| format!("cannot read {}", archive.display()))?;
    let mut zip = ::zip::ZipArchive::new(file).context("the source archive is not a zip")?;

    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let Some(path) = entry.enclosed_name() else {
            // An entry whose name escapes the destination is a zip-slip attempt and has no
            // business in a source archive. Skipped rather than fatal: the corpus is what
            // matters and one refused entry is not a reason to have none of it.
            continue;
        };
        let path = path.to_string_lossy().replace('\\', "/");
        if !path.starts_with(prefix) || entry.is_dir() {
            continue;
        }

        let target = into.join(&path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        std::fs::write(&target, bytes)?;
    }
    Ok(())
}

pub fn run(workspace_root: &Path) -> Result<()> {
    let lock: Lock =
        toml::from_str(LOCK).context("xtask/epubcheck-corpus.lock is not valid TOML")?;
    if lock.schema_version != SUPPORTED_SCHEMA_VERSION {
        bail!(
            "epubcheck-corpus.lock declares schema_version {}; this xtask understands \
             {SUPPORTED_SCHEMA_VERSION}",
            lock.schema_version
        );
    }

    let target = crate::epubcheck_parity::corpus_dir(workspace_root);
    if target.is_dir() && std::fs::read_dir(&target)?.next().is_some() {
        println!("corpus already at {}", target.display());
        return Ok(());
    }

    let staging = workspace_root.join("vendor/.epubcheck-corpus-staging");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;
    let archive = staging.join("source.zip");

    let url = format!(
        "https://github.com/{}/archive/refs/tags/{}.zip",
        lock.repo, lock.tag
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

    let bytes = std::fs::read(&archive)?;
    let digest = format!("{:x}", Sha256::digest(&bytes));
    if digest != lock.sha256 {
        let _ = std::fs::remove_dir_all(&staging);
        bail!(
            "the source archive hashes to {digest}; the lock pins {}. GitHub generates these \
             archives rather than storing them, so this most likely means it rebuilt the file — \
             check the contents, then move the pin in a reviewed commit",
            lock.sha256
        );
    }

    // Unpacked in-process rather than through `tar`. GitHub's source archives carry entries
    // whose names the platform `tar` on Windows refuses — "empty or unreadable filename" — and
    // it then fails the whole extraction. The `zip` crate is already in the workspace and reads
    // them without complaint.
    extract(&archive, &staging, &lock.resources)?;

    let resources = staging.join(&lock.resources);
    if !resources.is_dir() {
        bail!("{} is not in the archive", lock.resources);
    }
    std::fs::create_dir_all(target.parent().unwrap_or(workspace_root))?;
    let _ = std::fs::remove_dir_all(&target);
    std::fs::rename(&resources, &target)
        .with_context(|| format!("cannot move the corpus to {}", target.display()))?;
    let _ = std::fs::remove_dir_all(&staging);

    println!("corpus at {}", target.display());
    Ok(())
}
