//! `cargo xtask vendor-pdfium` — fetch the pinned PDFium binary for the host triple,
//! verify its SHA-256 against `xtask/pdfium.lock`, and unpack it to
//! `vendor/pdfium/<triple>/` (D3, IMPLEMENTATION_PLAN Phase 0 detail 1).
//!
//! The download and the extraction shell out to `curl` and `tar`, which ship with every
//! target we support (Windows 10+, macOS, every CI runner image). That keeps the
//! dependency firewall of D13.9 trivially true — `oc-net` remains the only crate in the
//! workspace that can open a socket, and no build-time tool needs an HTTP client either.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const LOCK: &str = include_str!("../pdfium.lock");

/// The lock-file schema this task understands.
const SUPPORTED_SCHEMA_VERSION: u32 = 1;

#[derive(Deserialize)]
struct Lock {
    schema_version: u32,
    repo: String,
    tag: String,
    build: u32,
    version: String,
    #[serde(rename = "asset")]
    assets: Vec<LockedAsset>,
}

#[derive(Deserialize)]
struct LockedAsset {
    triple: String,
    asset: String,
    sha256: String,
    size_bytes: u64,
}

pub fn run(workspace_root: &Path) -> Result<()> {
    let lock: Lock = toml::from_str(LOCK).context("xtask/pdfium.lock is not valid TOML")?;
    if lock.schema_version != SUPPORTED_SCHEMA_VERSION {
        bail!(
            "pdfium.lock declares schema_version {}; this xtask understands {}",
            lock.schema_version,
            SUPPORTED_SCHEMA_VERSION
        );
    }
    check_cargo_feature_matches(workspace_root, lock.build)?;

    let triple = host_triple();
    let entry = lock
        .assets
        .iter()
        .find(|a| a.triple == triple)
        .ok_or_else(|| {
            anyhow!(
                "no PDFium asset pinned for host triple {triple}; add one to xtask/pdfium.lock \
                 or set OC_PDFIUM_PATH to a library you supply yourself"
            )
        })?;

    let target_dir = workspace_root.join("vendor/pdfium").join(&triple);
    let stamp = target_dir.join("VENDOR_STAMP");
    let expected_stamp = format!("{} {} {}\n", lock.tag, entry.asset, entry.sha256);
    if std::fs::read_to_string(&stamp).ok().as_deref() == Some(expected_stamp.as_str()) {
        println!(
            "pdfium {} already vendored at {}",
            lock.version,
            target_dir.display()
        );
        return Ok(());
    }

    let cache_dir = workspace_root.join("target/vendor-cache");
    std::fs::create_dir_all(&cache_dir)?;
    let archive = cache_dir.join(&entry.asset);

    if !archive_is_intact(&archive, entry)? {
        // The tag contains a `/`, which must be percent-encoded in a download URL.
        let url = format!(
            "https://github.com/{}/releases/download/{}/{}",
            lock.repo,
            lock.tag.replace('/', "%2F"),
            entry.asset
        );
        println!("downloading {url}");
        run_tool(
            "curl",
            &[
                "--fail",
                "--location",
                "--silent",
                "--show-error",
                "--max-time",
                "300",
                "--output",
                &archive.to_string_lossy(),
                &url,
            ],
        )?;
        if !archive_is_intact(&archive, entry)? {
            bail!(
                "{} does not match the pinned SHA-256 after download; refusing to unpack it",
                entry.asset
            );
        }
    }

    let staging = cache_dir.join(format!("{}-unpacked", entry.triple));
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    std::fs::create_dir_all(&staging)?;
    run_tool(
        "tar",
        &[
            "-xzf",
            &archive.to_string_lossy(),
            "-C",
            &staging.to_string_lossy(),
        ],
    )?;

    let library = find_file(&staging, library_file_name())?
        .ok_or_else(|| anyhow!("{} contains no {}", entry.asset, library_file_name()))?;
    let version_file = find_file(&staging, "VERSION")?
        .ok_or_else(|| anyhow!("{} contains no VERSION file", entry.asset))?;
    let license = find_file(&staging, "LICENSE")?
        .ok_or_else(|| anyhow!("{} contains no LICENSE file", entry.asset))?;

    let found_build = parse_version_build(&std::fs::read_to_string(&version_file)?)?;
    if found_build != lock.build {
        bail!(
            "{} contains PDFium build {found_build}, but pdfium.lock pins {}",
            entry.asset,
            lock.build
        );
    }

    // Flatten: the archive nests the library under bin/ or lib/ depending on the platform,
    // and the loader only needs one predictable path.
    if target_dir.exists() {
        std::fs::remove_dir_all(&target_dir)?;
    }
    std::fs::create_dir_all(&target_dir)?;
    std::fs::copy(&library, target_dir.join(library_file_name()))?;
    std::fs::copy(&version_file, target_dir.join("VERSION"))?;
    std::fs::copy(&license, target_dir.join("LICENSE"))?;
    std::fs::write(&stamp, &expected_stamp)?;

    println!(
        "vendored pdfium {} ({}) for {triple} to {}",
        lock.version,
        lock.tag,
        target_dir.display()
    );
    Ok(())
}

/// The build number is pinned in three places (see the header of `pdfium.lock`). This one
/// is cheap to check here and would otherwise only surface as a runtime ABI error.
fn check_cargo_feature_matches(workspace_root: &Path, build: u32) -> Result<()> {
    let manifest = std::fs::read_to_string(workspace_root.join("Cargo.toml"))
        .context("cannot read the workspace Cargo.toml")?;
    let expected = format!("\"pdfium_{build}\"");
    if !manifest.contains(&expected) {
        bail!(
            "the workspace Cargo.toml does not select the {expected} feature of pdfium-render, \
             but pdfium.lock pins build {build}; the bindings and the library would disagree"
        );
    }
    Ok(())
}

fn archive_is_intact(archive: &Path, entry: &LockedAsset) -> Result<bool> {
    let Ok(bytes) = std::fs::read(archive) else {
        return Ok(false);
    };
    if bytes.len() as u64 != entry.size_bytes {
        return Ok(false);
    }
    let digest = Sha256::digest(&bytes);
    Ok(hex(&digest) == entry.sha256)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The per-OS shared-library file name PDFium ships under.
///
/// This mirrors `pdfium_render`'s `pdfium_platform_library_name()`, which is what `oc-pdf`
/// looks for at runtime; naming it here rather than depending on that crate keeps `xtask`
/// free of the PDF stack. If the two ever disagree, vendoring succeeds and test 0.7 fails
/// with a clear "library not found", which is a loud enough failure.
fn library_file_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "pdfium.dll"
    } else if cfg!(target_os = "macos") {
        "libpdfium.dylib"
    } else {
        "libpdfium.so"
    }
}

fn find_file(root: &Path, name: &str) -> Result<Option<PathBuf>> {
    for entry in std::fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            if let Some(found) = find_file(&path, name)? {
                return Ok(Some(found));
            }
        } else if path.file_name().is_some_and(|f| f == name) {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

fn parse_version_build(version_file: &str) -> Result<u32> {
    version_file
        .lines()
        .find_map(|line| line.strip_prefix("BUILD="))
        .ok_or_else(|| anyhow!("VERSION file has no BUILD= line"))?
        .trim()
        .parse()
        .context("VERSION file has a non-numeric BUILD=")
}

/// The host triple, captured by `xtask/build.rs` from Cargo's `TARGET`.
fn host_triple() -> String {
    env!("XTASK_HOST_TRIPLE").to_owned()
}

fn run_tool(tool: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(tool)
        .args(args)
        .status()
        .with_context(|| format!("cannot run `{tool}`; it is required by vendor-pdfium"))?;
    if !status.success() {
        bail!("`{tool}` exited with {status}");
    }
    Ok(())
}
