//! `cargo xtask stage-sidecars` — copy the built engine to where Tauri expects a sidecar.
//!
//! Tauri's `externalBin` looks for `<name>-<target-triple>` beside the configured path, so
//! the binary is copied under that name rather than symlinked or referenced in place.
//!
//! A build stamp is written next to it (RT A5.7). The app refuses to start when the staged
//! engine's `hello.engine_version` differs from its own, and the stamp is what makes that
//! failure diagnosable rather than merely loud: it records which build was staged, when, and
//! from where.

use std::path::Path;

use anyhow::{bail, Context, Result};

/// Where Tauri looks for the sidecar, relative to `apps/desktop/src-tauri`.
const SIDECAR_DIR: &str = "apps/desktop/src-tauri/bin";

/// The engine binary cargo builds.
const ENGINE_NAME: &str = "openconvert";

/// The name the engine is staged under, before the triple suffix Tauri requires: the
/// `externalBin` entry in `tauri.conf.json` and `openconvert_desktop::engine::SIDECAR_NAME`.
///
/// Deliberately not [`ENGINE_NAME`]. `tauri-build` copies every `externalBin` into
/// `target/<profile>/` without its triple, and a sidecar called `openconvert` replaced the engine
/// cargo had just built there with whatever was last staged (PHASE 15 carry-over; the test is
/// `no_sidecar_shares_a_name_with_a_workspace_binary`).
pub const SIDECAR_NAME: &str = "openconvert-engine";

/// Records what was staged, so a mismatch report can say which build is on disk.
const STAMP_NAME: &str = "STAGE_STAMP";

pub fn run(workspace_root: &Path, release: bool) -> Result<()> {
    let profile = if release { "release" } else { "debug" };
    let triple = env!("XTASK_HOST_TRIPLE");
    let extension = std::env::consts::EXE_SUFFIX;

    let built = workspace_root
        .join("target")
        .join(profile)
        .join(format!("{ENGINE_NAME}{extension}"));
    if !built.is_file() {
        bail!(
            "no engine at {}; run `cargo build -p {ENGINE_NAME}{}` first",
            built.display(),
            if release { " --release" } else { "" }
        );
    }

    let staged_dir = workspace_root.join(SIDECAR_DIR);
    std::fs::create_dir_all(&staged_dir)
        .with_context(|| format!("cannot create {}", staged_dir.display()))?;
    // What an older checkout staged under the engine's own name. Left in place it is inert, but
    // it is exactly the stale build this naming exists to keep away, so it goes.
    let legacy = staged_dir.join(format!("{ENGINE_NAME}-{triple}{extension}"));
    if legacy.is_file() {
        std::fs::remove_file(&legacy)
            .with_context(|| format!("cannot remove {}", legacy.display()))?;
    }
    let staged = staged_dir.join(format!("{SIDECAR_NAME}-{triple}{extension}"));
    std::fs::copy(&built, &staged)
        .with_context(|| format!("cannot copy {} to {}", built.display(), staged.display()))?;

    // The version is asked of the binary rather than read from a manifest, so the stamp
    // describes what was actually staged.
    let version = std::process::Command::new(&staged)
        .arg("--version")
        .output()
        .with_context(|| format!("cannot run {}", staged.display()))?;
    let version = String::from_utf8_lossy(&version.stdout).trim().to_owned();

    let stamp = format!(
        "{version}\ntriple = {triple}\nprofile = {profile}\nsource = {}\n",
        built.display().to_string().replace('\\', "/")
    );
    std::fs::write(staged_dir.join(STAMP_NAME), &stamp)
        .with_context(|| format!("cannot write {}", staged_dir.join(STAMP_NAME).display()))?;

    println!("staged {version} for {triple} at {}", staged.display());
    Ok(())
}
