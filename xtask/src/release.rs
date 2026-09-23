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
//! release changelog --version <tag> --out <file>      the release notes, from docs/CHANGELOG.md
//! release hash-dir --dir <dir>                         a SHA-256 section for every file there
//! release latest-json --dir <dir> --version <v> --notes <file> --pub-date <rfc3339>
//!                     --base-url <url> --out <file>    Tauri's static updater manifest
//! release verify-latest --latest <file> --assets <dir> every payload against the updater key
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

/// The operating systems this release ships installers for, and which the release gates cover.
///
/// v1.0.0 is Windows and Linux (maintainer decision 2026-09-23; D12 amendment): there is no Apple
/// Developer ID, so there is no signed and notarized macOS build to ship. macOS comes in a later
/// 1.x — it joins this list, and its legs return to `release.yml`, together. [`Os::Macos`] stays
/// known to every tool here so that nothing else has to change when it does.
pub const SHIPPED: [Os; 2] = [Os::Linux, Os::Windows];

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

/// The installer budget on `os`, from `thresholds.toml` (D17). The Linux AppImage has its own,
/// because it carries WebKitGTK (maintainer decision 2026-09-23, D12 amendment); every other
/// installer keeps D12's.
pub fn installer_budget(os: Os) -> u64 {
    let t = &oc_core::thresholds::T.release;
    let bytes = match os {
        Os::Linux => t.max_linux_installer_bytes,
        Os::Macos | Os::Windows => t.max_installer_bytes,
    };
    u64::try_from(bytes).unwrap_or_default()
}

/// The release notes for `tag`: the body of `docs/CHANGELOG.md`'s `## [X.Y.Z]` section, up to the
/// next `## ` heading, trimmed. Refused when there is no such section, when it is empty, and when it
/// still holds a `TODO_` placeholder (the 1.0.0 draft's, for Phase 14's security claims).
pub fn release_notes(changelog: &str, tag: &str) -> Result<String> {
    let version = tag.trim_start_matches('v');
    let heading = format!("## [{version}]");
    let mut lines = changelog.lines();
    if !lines.any(|line| line.starts_with(&heading)) {
        bail!("docs/CHANGELOG.md has no `{heading}` section");
    }
    let body: Vec<&str> = lines.take_while(|line| !line.starts_with("## ")).collect();
    let notes = body.join("\n").trim().to_owned();
    if notes.is_empty() {
        bail!("docs/CHANGELOG.md's `{heading}` section is empty");
    }
    if let Some(line) = notes.lines().find(|line| line.contains("TODO_")) {
        bail!(
            "docs/CHANGELOG.md's `{heading}` section still has a TODO_ placeholder: {}",
            line.trim()
        );
    }
    Ok(notes)
}

/// Every file directly in `dir`, hashed, as `(name, sha256)` in name order.
pub fn hash_dir(dir: &Path) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    for path in sorted_files(dir) {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .context("an asset name")?
            .to_owned();
        out.push((name, sha256_file(&path)?));
    }
    Ok(out)
}

/// The `latest.json` platform keys an updater payload serves — Tauri's `<os>-<arch>` and the
/// installer-specific `<os>-<arch>-<installer>` — from its file name, or `None` for a file that is
/// not an update payload. The macOS archive's name must carry its architecture (the release job
/// renames `OpenConvert.app.tar.gz` to `OpenConvert_<version>_<arch>.app.tar.gz`).
pub fn platform_keys_for(name: &str) -> Option<Vec<String>> {
    let arch = if name.contains("aarch64") || name.contains("arm64") {
        "aarch64"
    } else if name.contains("x86_64") || name.contains("amd64") || name.contains("x64") {
        "x86_64"
    } else {
        return None;
    };
    let (os, installer) = if name.ends_with(".AppImage") {
        ("linux", "appimage")
    } else if name.ends_with("-setup.exe") {
        ("windows", "nsis")
    } else if name.ends_with(".app.tar.gz") {
        ("darwin", "app")
    } else {
        return None;
    };
    Some(vec![
        format!("{os}-{arch}"),
        format!("{os}-{arch}-{installer}"),
    ])
}

/// Tauri's static `latest.json` for `version`, from the payloads and `.sig` files in `dir`: each
/// payload's URL under `base_url` and its signature (the `.sig` file's content, as Tauri writes it).
pub fn latest_json(
    dir: &Path,
    version: &str,
    notes: &str,
    pub_date: &str,
    base_url: &str,
) -> Result<serde_json::Value> {
    let mut platforms = serde_json::Map::new();
    for path in sorted_files(dir) {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(keys) = platform_keys_for(name) else {
            continue;
        };
        let sig = dir.join(format!("{name}.sig"));
        let signature = std::fs::read_to_string(&sig)
            .with_context(|| format!("{name} has no signature beside it ({})", sig.display()))?;
        for key in keys {
            let entry = serde_json::json!({
                "signature": signature.trim(),
                "url": format!("{}/{name}", base_url.trim_end_matches('/')),
            });
            if platforms.insert(key.clone(), entry).is_some() {
                bail!("two update payloads for {key}");
            }
        }
    }
    if platforms.is_empty() {
        bail!("no update payloads in {}", dir.display());
    }
    Ok(serde_json::json!({
        "version": version.trim_start_matches('v'),
        "notes": notes,
        "pub_date": pub_date,
        "platforms": platforms,
    }))
}

/// Every payload `latest.json` names, found by its file name in `dir`, verified with the updater's
/// own verifier against `pubkey` (row 15.9 on the published release, with the release key).
pub fn verify_latest(latest: &serde_json::Value, dir: &Path, pubkey: &str) -> Result<usize> {
    let platforms = latest["platforms"]
        .as_object()
        .context("latest.json has no platforms")?;
    for (key, entry) in platforms {
        let url = entry["url"].as_str().context("an url")?;
        let name = url.rsplit('/').next().unwrap_or_default();
        let bytes = std::fs::read(dir.join(name))
            .with_context(|| format!("{key}: {name} is not among the assets"))?;
        let signature = entry["signature"].as_str().context("a signature")?;
        oc_net::update::verify(&bytes, signature, pubkey)
            .map_err(|e| anyhow::anyhow!("{key}: {name}: {e}"))?;
    }
    Ok(platforms.len())
}

/// The updater's public key as `tauri.conf.json` carries it.
pub fn configured_pubkey(workspace_root: &Path) -> Result<String> {
    let path = workspace_root.join("apps/desktop/src-tauri/tauri.conf.json");
    let config: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&path)
            .with_context(|| format!("cannot read {}", path.display()))?,
    )?;
    config["plugins"]["updater"]["pubkey"]
        .as_str()
        .map(str::to_owned)
        .context("tauri.conf.json has no plugins.updater.pubkey")
}

fn flag(args: &[String], name: &str) -> Result<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
        .with_context(|| format!("{name} <value> is required"))
}

pub fn run(workspace_root: &Path, args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("hash-dir") => {
            let dir = PathBuf::from(flag(args, "--dir")?);
            let mut out = String::from("## SHA-256\n\n```\n");
            for (name, sha) in hash_dir(&dir)? {
                out.push_str(&format!("{sha}  {name}\n"));
            }
            out.push_str("```\n");
            print!("{out}");
            Ok(())
        }
        Some("changelog") => {
            let tag = flag(args, "--version")?;
            let path = workspace_root.join("docs/CHANGELOG.md");
            let changelog = std::fs::read_to_string(&path)
                .with_context(|| format!("cannot read {}", path.display()))?;
            let notes = release_notes(&changelog, &tag)?;
            let out = PathBuf::from(flag(args, "--out")?);
            std::fs::write(&out, notes + "\n")
                .with_context(|| format!("cannot write {}", out.display()))?;
            Ok(())
        }
        Some("latest-json") => {
            let notes_path = flag(args, "--notes")?;
            let notes = std::fs::read_to_string(&notes_path)
                .with_context(|| format!("cannot read {notes_path}"))?;
            let latest = latest_json(
                Path::new(&flag(args, "--dir")?),
                &flag(args, "--version")?,
                notes.trim(),
                &flag(args, "--pub-date")?,
                &flag(args, "--base-url")?,
            )?;
            let out = PathBuf::from(flag(args, "--out")?);
            std::fs::write(&out, serde_json::to_string_pretty(&latest)? + "\n")
                .with_context(|| format!("cannot write {}", out.display()))?;
            Ok(())
        }
        Some("verify-latest") => {
            let latest_path = flag(args, "--latest")?;
            let latest: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(&latest_path)
                    .with_context(|| format!("cannot read {latest_path}"))?,
            )?;
            let pubkey = configured_pubkey(workspace_root)?;
            let verified = verify_latest(&latest, Path::new(&flag(args, "--assets")?), &pubkey)?;
            println!("latest.json: {verified} platform entries verify against the updater key");
            Ok(())
        }
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
            let budget = installer_budget(os);
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
