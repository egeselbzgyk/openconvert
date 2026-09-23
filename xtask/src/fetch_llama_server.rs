//! `cargo xtask fetch-llama-server` — fetch the pinned llama.cpp release for this host, verify it
//! against `xtask/llama.lock`, unpack it to `vendor/llama-server/<tag>/`, and print where
//! `llama-server` is (D8, IMPLEMENTATION_PLAN PHASE 9 detail 1).
//!
//! Like `fetch-epubcheck`, the download shells out to `curl`, so `oc-net` stays the only crate in the
//! workspace that opens a socket (D13.9). A release archive carries `llama-server` beside the shared
//! libraries it loads its backends from, so the whole archive is unpacked and the binary is found
//! by name rather than by a path V1 could not confirm. Staging it as a Tauri `externalBin` is
//! packaging, Phase 15.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const LOCK: &str = include_str!("../llama.lock");
const SUPPORTED_SCHEMA_VERSION: u32 = 1;
const PLACEHOLDER: &str = "TODO_";

#[derive(Debug, Deserialize)]
pub struct Lock {
    pub schema_version: u32,
    pub repo: String,
    pub tag: String,
    #[serde(rename = "asset")]
    pub assets: Vec<Asset>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Asset {
    pub target: String,
    pub name: String,
    pub sha256: String,
    pub size_bytes: u64,
}

pub fn lock() -> Result<Lock> {
    parse(LOCK)
}

pub fn parse(text: &str) -> Result<Lock> {
    let lock: Lock = toml::from_str(text).context("xtask/llama.lock is not valid TOML")?;
    if lock.schema_version != SUPPORTED_SCHEMA_VERSION {
        bail!(
            "llama.lock declares schema_version {}; this xtask understands {SUPPORTED_SCHEMA_VERSION}",
            lock.schema_version
        );
    }
    Ok(lock)
}

/// The asset for `target`, if its digest and size are filled in.
pub fn pinned<'a>(lock: &'a Lock, target: &str) -> Result<&'a Asset> {
    let Some(asset) = lock.assets.iter().find(|asset| asset.target == target) else {
        bail!("llama.lock pins no asset for {target}");
    };
    if asset.sha256.starts_with(PLACEHOLDER) || asset.size_bytes == 0 {
        bail!(
            "llama.lock has no digest for {} yet ({}); hash the release asset and fill it in \
             before fetching (docs/DECISIONS_LOG.md, 2026-09-23)",
            asset.name,
            asset.sha256
        );
    }
    if asset.sha256.len() != 64 || !asset.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("llama.lock's digest for {} is not a SHA-256", asset.name);
    }
    Ok(asset)
}

pub fn run(workspace_root: &Path) -> Result<()> {
    let lock = lock()?;
    let asset = pinned(&lock, env!("XTASK_HOST_TRIPLE"))?;
    let directory = workspace_root.join("vendor/llama-server").join(&lock.tag);
    if let Some(found) = find_server(&directory) {
        println!("{}", found.display());
        return Ok(());
    }
    std::fs::create_dir_all(&directory)
        .with_context(|| format!("cannot create {}", directory.display()))?;

    let archive = directory.join(&asset.name);
    let url = format!(
        "https://github.com/{}/releases/download/{}/{}",
        lock.repo, lock.tag, asset.name
    );
    let status = Command::new("curl")
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--output",
        ])
        .arg(&archive)
        .arg(&url)
        .status()
        .context("cannot run curl")?;
    if !status.success() {
        bail!("curl failed fetching {url}");
    }
    verify(&archive, asset)?;
    unpack(&archive, &directory)?;
    let _ = std::fs::remove_file(&archive);

    match find_server(&directory) {
        Some(found) => {
            println!("{}", found.display());
            Ok(())
        }
        None => bail!("{} has no llama-server in it", asset.name),
    }
}

fn verify(archive: &Path, asset: &Asset) -> Result<()> {
    let bytes =
        std::fs::read(archive).with_context(|| format!("cannot read {}", archive.display()))?;
    let digest = format!("{:x}", Sha256::digest(&bytes));
    if bytes.len() as u64 != asset.size_bytes || digest != asset.sha256 {
        let _ = std::fs::remove_file(archive);
        bail!(
            "{} is {} bytes hashing to {digest}; the lock pins {} bytes, {}",
            asset.name,
            bytes.len(),
            asset.size_bytes,
            asset.sha256
        );
    }
    Ok(())
}

/// A `.zip` in process (the Windows asset); a `.tar.gz` with the system `tar`, which reads gzip on
/// every Unix this runs on.
fn unpack(archive: &Path, into: &Path) -> Result<()> {
    let name = archive.to_string_lossy();
    if name.ends_with(".zip") {
        let file = std::fs::File::open(archive)?;
        let mut zip = ::zip::ZipArchive::new(file)?;
        zip.extract(into)
            .with_context(|| format!("cannot unpack {}", archive.display()))?;
        return Ok(());
    }
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(into)
        .status()
        .context("cannot run tar")?;
    if !status.success() {
        bail!("tar failed unpacking {}", archive.display());
    }
    Ok(())
}

/// The `llama-server` executable anywhere under `directory`.
pub fn find_server(directory: &Path) -> Option<PathBuf> {
    let wanted = format!("llama-server{}", std::env::consts::EXE_SUFFIX);
    let mut stack = vec![directory.to_owned()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).ok()?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().is_some_and(|name| name == wanted.as_str()) {
                return Some(path);
            }
        }
    }
    None
}

#[test]
fn the_lock_names_the_pinned_build_for_every_shipped_target() {
    let lock = lock().expect("the lock parses");
    assert_eq!(lock.repo, "ggml-org/llama.cpp");
    assert!(
        lock.tag.starts_with('b'),
        "a b<N> build tag, never a branch"
    );
    for target in [
        "x86_64-unknown-linux-gnu",
        "aarch64-apple-darwin",
        "x86_64-pc-windows-msvc",
    ] {
        let asset = lock
            .assets
            .iter()
            .find(|asset| asset.target == target)
            .unwrap_or_else(|| panic!("no asset for {target}"));
        assert!(
            asset.name.contains(&lock.tag),
            "{} is from {}",
            asset.name,
            lock.tag
        );
    }
}

#[test]
fn the_shipped_lock_pins_all_four_assets_by_sha256_and_size() {
    let lock = lock().expect("the lock parses");
    assert_eq!(lock.assets.len(), 4, "one CPU build per shipped target");
    for target in [
        "x86_64-unknown-linux-gnu",
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        "x86_64-pc-windows-msvc",
    ] {
        let asset = pinned(&lock, target).unwrap_or_else(|error| panic!("{target}: {error}"));
        // `verify` compares against a lower-case hex rendering, so an upper-case pin never matches.
        assert!(
            asset.sha256.len() == 64
                && asset
                    .sha256
                    .chars()
                    .all(|c| matches!(c, '0'..='9' | 'a'..='f')),
            "{} is pinned to {:?}, not a lower-case SHA-256",
            asset.name,
            asset.sha256
        );
        assert!(asset.size_bytes > 0, "{} has no size", asset.name);
    }
    let mut digests: Vec<&str> = lock.assets.iter().map(|a| a.sha256.as_str()).collect();
    digests.sort_unstable();
    digests.dedup();
    assert_eq!(digests.len(), 4, "a digest was copied onto two assets");
}

#[test]
fn an_unfilled_digest_is_refused_before_anything_is_fetched() {
    let lock = parse(
        r#"schema_version = 1
repo = "ggml-org/llama.cpp"
tag = "b1"
[[asset]]
target = "t"
name = "llama-b1-bin-x.tar.gz"
sha256 = "TODO_SHA256"
size_bytes = 0
"#,
    )
    .expect("parses");
    let error = pinned(&lock, "t").expect_err("a placeholder");
    assert!(error.to_string().contains("no digest"), "{error}");
    assert!(pinned(&lock, "elsewhere").is_err());

    let filled = parse(&format!(
        r#"schema_version = 1
repo = "ggml-org/llama.cpp"
tag = "b1"
[[asset]]
target = "t"
name = "llama-b1-bin-x.tar.gz"
sha256 = "{}"
size_bytes = 5
"#,
        "ab".repeat(32)
    ))
    .expect("parses");
    assert!(pinned(&filled, "t").is_ok());
}
