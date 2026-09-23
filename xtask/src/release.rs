//! `cargo xtask release …` — the release artefacts, their SHA-256s and the gates over them
//! (PHASE 15 details 3, 9; rows 15.6, 15.15, 15.20).
//!
//! Everything here is scriptable and none of it is interactive: the release job runs it, and a
//! maintainer can run the same commands against a bundle directory by hand.
//!
//! ```text
//! release manifest --os <linux|macos|windows> --bundle-dir <dir> --out <file.json>
//! release notes <manifest.json>...                     the release body's SHA-256 section
//! release verify-published --body <file> --assets <dir> every asset's hash is in the body
//! release size-check --os <os> --bundle-dir <dir>       installers within the D12 budget
//! ```

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The operating systems a release ships for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Os {
    Linux,
    Macos,
    Windows,
}

impl Os {
    pub fn parse(text: &str) -> Result<Self> {
        match text {
            "linux" => Ok(Self::Linux),
            "macos" => Ok(Self::Macos),
            "windows" => Ok(Self::Windows),
            other => bail!("unknown OS `{other}` (linux, macos, windows)"),
        }
    }

    /// The OS this xtask was built for.
    pub fn host() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else {
            Self::Linux
        }
    }

    /// The installers D12 promises on this OS; a release without one of them is not a release.
    pub fn required_installers(self) -> &'static [Installer] {
        match self {
            Self::Linux => &[Installer::AppImage],
            Self::Macos => &[Installer::Dmg],
            Self::Windows => &[Installer::Nsis, Installer::Msi],
        }
    }
}

/// What a file in the bundle directory is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Installer {
    AppImage,
    Dmg,
    Nsis,
    Msi,
    /// The archive the updater downloads (`.app.tar.gz`); on Linux and Windows the installer
    /// itself is the update payload.
    UpdaterArchive,
    /// An updater signature (`.sig`), published beside its payload and named in `latest.json`.
    Signature,
}

impl Installer {
    /// The Tauri bundler's output directory for this kind, and how its files end.
    fn locate(self) -> (&'static str, &'static str) {
        match self {
            Self::AppImage => ("appimage", ".AppImage"),
            Self::Dmg => ("dmg", ".dmg"),
            Self::Nsis => ("nsis", ".exe"),
            Self::Msi => ("msi", ".msi"),
            Self::UpdaterArchive => ("macos", ".app.tar.gz"),
            Self::Signature => ("", ".sig"),
        }
    }

    /// Whether this is something a user installs, which is what the size budget is about.
    pub fn is_installer(self) -> bool {
        !matches!(self, Self::UpdaterArchive | Self::Signature)
    }
}

/// One file a release publishes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub name: String,
    pub kind: Installer,
    pub sha256: String,
    pub bytes: u64,
    #[serde(skip)]
    pub path: PathBuf,
}

/// Everything one OS's release job produced (PHASE 15 Architecture).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseArtifacts {
    pub os: Os,
    pub files: Vec<Artifact>,
}

/// SHA-256 of a file, lowercase hex, streamed.
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file =
        std::fs::File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("cannot read {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Every artefact under a Tauri bundle directory (`target/release/bundle`) for `os`, hashed, in a
/// stable order. Fails, naming what is missing, when an installer D12 promises for `os` was not
/// produced (row 15.6 on Windows).
pub fn collect(bundle_dir: &Path, os: Os) -> Result<ReleaseArtifacts> {
    let mut kinds: Vec<Installer> = os.required_installers().to_vec();
    if os == Os::Macos {
        kinds.push(Installer::UpdaterArchive);
    }
    let mut files = Vec::new();
    for kind in kinds {
        let (dir, suffix) = kind.locate();
        let dir = bundle_dir.join(dir);
        let mut found = false;
        for path in sorted_files(&dir) {
            let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_owned) else {
                continue;
            };
            let (kind, wanted) = if name.ends_with(".sig") {
                (
                    Installer::Signature,
                    name.trim_end_matches(".sig").ends_with(suffix),
                )
            } else {
                (kind, name.ends_with(suffix))
            };
            if !wanted {
                continue;
            }
            found |= kind != Installer::Signature;
            files.push(Artifact {
                sha256: sha256_file(&path)?,
                bytes: std::fs::metadata(&path)?.len(),
                name,
                kind,
                path,
            });
        }
        if !found && os.required_installers().contains(&kind) {
            bail!(
                "{os:?}: no {kind:?} installer (*{suffix}) under {}; the release needs {:?}",
                dir.display(),
                os.required_installers()
            );
        }
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(ReleaseArtifacts { os, files })
}

fn sorted_files(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file())
                .collect()
        })
        .unwrap_or_default();
    paths.sort();
    paths
}

/// The release body's hash section: one `sha256  name` line per artefact, the format
/// `sha256sum --check` reads, so a user can verify a download with the tool they already have.
pub fn hashes_section(releases: &[ReleaseArtifacts]) -> String {
    let mut lines: BTreeMap<&str, &str> = BTreeMap::new();
    for release in releases {
        for file in &release.files {
            lines.insert(&file.name, &file.sha256);
        }
    }
    let mut out = String::from("## SHA-256\n\n```\n");
    for (name, sha) in lines {
        out.push_str(sha);
        out.push_str("  ");
        out.push_str(name);
        out.push('\n');
    }
    out.push_str("```\n");
    out
}

/// The assets whose SHA-256 is not published in `body` next to their name (row 15.20).
/// `assets` is `(name, sha256)`; an empty result is the only passing one.
pub fn unpublished(body: &str, assets: &[(String, String)]) -> Vec<String> {
    assets
        .iter()
        .filter(|(name, sha)| {
            !body.lines().any(|line| {
                let mut words = line.split_whitespace();
                words.next() == Some(sha.as_str()) && words.next() == Some(name.as_str())
            })
        })
        .map(|(name, _)| name.clone())
        .collect()
}

/// The installers larger than `budget` bytes (row 15.15). Updater archives and signatures do not
/// count: the budget is what a user downloads to install.
pub fn over_budget(release: &ReleaseArtifacts, budget: u64) -> Vec<(String, u64)> {
    release
        .files
        .iter()
        .filter(|f| f.kind.is_installer() && f.bytes > budget)
        .map(|f| (f.name.clone(), f.bytes))
        .collect()
}

/// The installer budget, from `thresholds.toml` (D17).
pub fn installer_budget() -> u64 {
    u64::try_from(oc_core::thresholds::T.release.max_installer_bytes).unwrap_or_default()
}

fn flag(args: &[String], name: &str) -> Result<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
        .with_context(|| format!("{name} <value> is required"))
}

pub fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("manifest") => {
            let os = Os::parse(&flag(args, "--os")?)?;
            let release = collect(Path::new(&flag(args, "--bundle-dir")?), os)?;
            let out = PathBuf::from(flag(args, "--out")?);
            std::fs::write(&out, serde_json::to_string_pretty(&release)? + "\n")
                .with_context(|| format!("cannot write {}", out.display()))?;
            for file in &release.files {
                println!("{}  {}", file.sha256, file.name);
            }
            Ok(())
        }
        Some("notes") => {
            let mut releases = Vec::new();
            for path in &args[1..] {
                let text =
                    std::fs::read_to_string(path).with_context(|| format!("cannot read {path}"))?;
                releases.push(serde_json::from_str::<ReleaseArtifacts>(&text)?);
            }
            print!("{}", hashes_section(&releases));
            Ok(())
        }
        Some("verify-published") => {
            let body_path = flag(args, "--body")?;
            let body = std::fs::read_to_string(&body_path)
                .with_context(|| format!("cannot read {body_path}"))?;
            let mut assets = Vec::new();
            for path in sorted_files(Path::new(&flag(args, "--assets")?)) {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .context("an asset name")?
                    .to_owned();
                assets.push((name, sha256_file(&path)?));
            }
            if assets.is_empty() {
                bail!("no assets to verify");
            }
            let missing = unpublished(&body, &assets);
            if !missing.is_empty() {
                bail!("the release body publishes no SHA-256 for: {missing:?}");
            }
            println!(
                "all {} assets have their SHA-256 in the release body",
                assets.len()
            );
            Ok(())
        }
        Some("size-check") => {
            let os = Os::parse(&flag(args, "--os")?)?;
            let release = collect(Path::new(&flag(args, "--bundle-dir")?), os)?;
            let budget = installer_budget();
            for file in release.files.iter().filter(|f| f.kind.is_installer()) {
                println!("{} bytes  {}  (budget {budget})", file.bytes, file.name);
            }
            let over = over_budget(&release, budget);
            if !over.is_empty() {
                bail!("installers over the {budget}-byte budget (D12, A15.6): {over:?}");
            }
            Ok(())
        }
        other => {
            bail!("release: unknown step {other:?} (manifest, notes, verify-published, size-check)")
        }
    }
}
