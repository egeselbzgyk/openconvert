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

/// The engine binary's name, before the triple suffix Tauri requires.
const ENGINE_NAME: &str = "openconvert";

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
    let staged = staged_dir.join(format!("{ENGINE_NAME}-{triple}{extension}"));
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
