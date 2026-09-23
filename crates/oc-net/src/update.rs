//! The app's updater (PHASE 15 detail 5, SECURITY §9, D12): Tauri's update format, verified before
//! anything is installed, fetched through the one crate that may open a socket.
//!
//! **The format is Tauri's.** A release publishes a static `latest.json` — `version`, `notes`,
//! `pub_date` and, per platform, the payload's `url` and its `signature` — and every payload is
//! signed with the release's Ed25519 key in minisign's format, which `tauri signer sign` writes and
//! the bundler's `createUpdaterArtifacts` produces. The public key is the one `tauri.conf.json`
//! carries at `plugins.updater.pubkey`, base64-wrapped the way Tauri wraps it. So the release
//! tooling is Tauri's own, and so is the verifier: `minisign-verify`, the crate
//! `tauri-plugin-updater` verifies with.
//!
//! **The client is not the plugin.** `tauri-plugin-updater` downloads with `reqwest`, which
//! `deny.toml` bans from every crate (D13.9; CLAUDE.md: no HTTP client outside `oc-net`). Here the
//! manifest and the payload come through [`Fetch`], every hop checked against
//! [`UPDATE_HOST_ALLOWLIST`] before it is fetched — GitHub's release hosts, and not the model
//! hosts — with the same redirect bound as a model download (DECISIONS_LOG 2026-09-23, provisional).
//!
//! **Nothing unverified can be installed.** [`VerifiedUpdate`] has no public constructor: the only
//! way to get one is [`fetch_update`], which returns it after the signature over the exact bytes
//! downloaded has verified against the embedded key. A payload with one byte changed, a signature
//! from another key, or a key that is still the placeholder, ends in an [`UpdateError`] with the
//! payload never written anywhere (row 15.10). Signature checking cannot be turned off: there is no
//! option that skips it.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use serde::Deserialize;

use crate::allowlist;
use crate::download::{Fetch, Fetched};
use crate::NetError;

/// Every host an update check or download may connect to: the release page's host and the CDN
/// GitHub redirects release assets to. Deliberately not [`allowlist::HOST_ALLOWLIST`]: a model
/// host serves no update, and GitHub serves no model.
pub const UPDATE_HOST_ALLOWLIST: &[&str] = &[
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

/// What `tauri.conf.json` carries until the maintainer generates the release keypair; with it,
/// every update is refused before anything is fetched.
pub const PLACEHOLDER_PREFIX: &str = "TODO_";

/// Why an update was not offered or not accepted.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum UpdateError {
    /// No usable public key is built in, so no update can be verified — and none is fetched.
    #[error("this build has no updater key, so it cannot verify an update")]
    NoKey,
    #[error("the update manifest is not valid: {0}")]
    BadManifest(String),
    /// The manifest names no payload for this platform.
    #[error("the update manifest has nothing for {0}")]
    NoPlatform(String),
    #[error("`{0}` is not a version the updater can compare")]
    BadVersion(String),
    /// The payload is larger than any installer a release ships.
    #[error("the update is larger than {0} bytes")]
    TooLarge(u64),
    /// The signature does not verify against the built-in key over the bytes downloaded.
    #[error("the update's signature does not verify: {0}")]
    BadSignature(String),
    #[error(transparent)]
    Net(#[from] NetError),
}

/// The numbers an update check needs, read by the caller from `thresholds.toml`.
#[derive(Clone, Copy, Debug)]
pub struct UpdateConfig {
    pub max_redirects: u32,
    /// The largest `latest.json` read.
    pub manifest_max_bytes: u64,
    /// The largest payload read: anything bigger than an installer is not an update.
    pub payload_max_bytes: u64,
}

/// `latest.json`, as Tauri's static update format writes it.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct UpdateManifest {
    pub version: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub pub_date: Option<String>,
    pub platforms: BTreeMap<String, PlatformUpdate>,
}

/// One platform's payload.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct PlatformUpdate {
    pub url: String,
    /// The `.sig` file's content: minisign's signature text, base64-wrapped.
    pub signature: String,
}

/// An update whose signature has verified. The bytes are exactly the ones the signature covers.
#[derive(Debug)]
pub struct VerifiedUpdate {
    version: String,
    file_name: String,
    bytes: Vec<u8>,
}

impl VerifiedUpdate {
    pub fn version(&self) -> &str {
        &self.version
    }

    /// The payload's name, the last segment of its URL.
    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// The keys `latest.json` may list this build under, most specific first: Tauri writes
/// `<os>-<arch>` and, since 2.x, `<os>-<arch>-<installer>`.
pub fn platform_keys(installer: Option<&str>) -> Vec<String> {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    };
    let base = format!("{os}-{}", std::env::consts::ARCH);
    let mut keys = Vec::new();
    if let Some(installer) = installer {
        keys.push(format!("{base}-{installer}"));
    }
    keys.push(base);
    keys
}

/// `major.minor.patch` with an optional pre-release, compared as SemVer orders them: a
/// pre-release sorts before its release, and pre-release identifiers compare field by field.
fn parse_version(text: &str) -> Result<(Vec<u64>, Option<Vec<String>>), UpdateError> {
    let bad = || UpdateError::BadVersion(text.to_owned());
    let bare = text.strip_prefix('v').unwrap_or(text);
    let bare = bare.split('+').next().unwrap_or(bare);
    let (core, pre) = match bare.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (bare, None),
    };
    let numbers: Vec<u64> = core
        .split('.')
        .map(|part| part.parse::<u64>().map_err(|_| bad()))
        .collect::<Result<_, _>>()?;
    if numbers.len() != 3 {
        return Err(bad());
    }
    let pre = pre.map(|p| p.split('.').map(str::to_owned).collect());
    Ok((numbers, pre))
}

/// Whether `candidate` is a later version than `current`.
pub fn is_newer(candidate: &str, current: &str) -> Result<bool, UpdateError> {
    let (candidate_core, candidate_pre) = parse_version(candidate)?;
    let (current_core, current_pre) = parse_version(current)?;
    if candidate_core != current_core {
        return Ok(candidate_core > current_core);
    }
    Ok(match (candidate_pre, current_pre) {
        (None, None) => false,
        (None, Some(_)) => true,
        (Some(_), None) => false,
        (Some(a), Some(b)) => compare_pre(&a, &b) == std::cmp::Ordering::Greater,
    })
}

fn compare_pre(a: &[String], b: &[String]) -> std::cmp::Ordering {
    for (x, y) in a.iter().zip(b) {
        let order = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(x), Ok(y)) => x.cmp(&y),
            (Ok(_), Err(_)) => std::cmp::Ordering::Less,
            (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
            (Err(_), Err(_)) => x.cmp(y),
        };
        if order != std::cmp::Ordering::Equal {
            return order;
        }
    }
    a.len().cmp(&b.len())
}

fn unwrap_base64(text: &str) -> Result<String, UpdateError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(text.trim())
        .map_err(|e| UpdateError::BadSignature(format!("not base64: {e}")))?;
    String::from_utf8(bytes).map_err(|e| UpdateError::BadSignature(format!("not UTF-8: {e}")))
}

/// The built-in public key, decoded; `NoKey` for the placeholder or anything that is not a key.
pub fn public_key(pubkey_b64: &str) -> Result<minisign_verify::PublicKey, UpdateError> {
    if pubkey_b64.trim().is_empty() || pubkey_b64.starts_with(PLACEHOLDER_PREFIX) {
        return Err(UpdateError::NoKey);
    }
    let text = unwrap_base64(pubkey_b64).map_err(|_| UpdateError::NoKey)?;
    minisign_verify::PublicKey::decode(&text).map_err(|_| UpdateError::NoKey)
}

/// Verify `payload` against a Tauri `.sig` (base64-wrapped minisign signature) and the built-in
/// key. The signature's own trusted comment is covered too, as minisign requires.
pub fn verify(payload: &[u8], signature_b64: &str, pubkey_b64: &str) -> Result<(), UpdateError> {
    let key = public_key(pubkey_b64)?;
    let text = unwrap_base64(signature_b64)?;
    let signature = minisign_verify::Signature::decode(&text)
        .map_err(|e| UpdateError::BadSignature(e.to_string()))?;
    key.verify(payload, &signature, true)
        .map_err(|e| UpdateError::BadSignature(e.to_string()))
}

/// The host of `url` if it is an `https://` URL on [`UPDATE_HOST_ALLOWLIST`].
pub fn check_host(url: &str) -> Result<String, NetError> {
    let host = allowlist::host_of(url)?;
    if UPDATE_HOST_ALLOWLIST.contains(&host.as_str()) {
        Ok(host)
    } else {
        Err(NetError::HostNotAllowed { host })
    }
}

/// GET `url` through its redirects, every hop on the update allowlist, reading at most `max`
/// bytes; one byte more is [`UpdateError::TooLarge`].
fn get_bounded(
    fetch: &dyn Fetch,
    url: &str,
    config: &UpdateConfig,
    max: u64,
) -> Result<Vec<u8>, UpdateError> {
    let mut current = url.to_owned();
    let mut hops = 0;
    let body = loop {
        check_host(&current)?;
        match fetch.get(&current)? {
            Fetched::Body(body) => break body,
            Fetched::Redirect(location) => {
                hops += 1;
                if hops > config.max_redirects {
                    return Err(NetError::TooManyRedirects(config.max_redirects).into());
                }
                current = allowlist::resolve_location(&current, &location)?;
            }
        }
    };
    let mut bytes = Vec::new();
    body.take(max.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| NetError::Transport(e.to_string()))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max {
        return Err(UpdateError::TooLarge(max));
    }
    Ok(bytes)
}

/// Ask `endpoint` for `latest.json`, and when it offers a version later than `current` for one of
/// `platforms` (most specific first, [`platform_keys`]), download that payload and verify it
/// against `pubkey_b64`. `Ok(None)`: this build is up to date.
///
/// The key is checked before any socket is opened, so a build without a real key never asks.
pub fn fetch_update(
    fetch: &dyn Fetch,
    endpoint: &str,
    current: &str,
    platforms: &[String],
    pubkey_b64: &str,
    config: &UpdateConfig,
) -> Result<Option<VerifiedUpdate>, UpdateError> {
    public_key(pubkey_b64)?;
    let manifest = get_bounded(fetch, endpoint, config, config.manifest_max_bytes)?;
    let manifest: UpdateManifest =
        serde_json::from_slice(&manifest).map_err(|e| UpdateError::BadManifest(e.to_string()))?;
    if !is_newer(&manifest.version, current)? {
        return Ok(None);
    }
    let Some(entry) = platforms.iter().find_map(|key| manifest.platforms.get(key)) else {
        return Err(UpdateError::NoPlatform(platforms.join(" or ")));
    };
    let payload = get_bounded(fetch, &entry.url, config, config.payload_max_bytes)?;
    verify(&payload, &entry.signature, pubkey_b64)?;
    let file_name = entry
        .url
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty() && !name.contains(['\\', '?', '#']))
        .ok_or_else(|| UpdateError::BadManifest(format!("no file name in {}", entry.url)))?
        .to_owned();
    Ok(Some(VerifiedUpdate {
        version: manifest.version.trim_start_matches('v').to_owned(),
        file_name,
        bytes: payload,
    }))
}

/// Replace the running AppImage with a verified one (Linux, the updater's primary target, D12):
/// written beside it, made executable, synced, then renamed over it, so an interrupted update
/// leaves the old AppImage whole. The app restarts into the new one.
pub fn install_appimage(update: &VerifiedUpdate, appimage: &Path) -> Result<PathBuf, UpdateError> {
    let io = |e: std::io::Error| NetError::Io(format!("{}: {e}", appimage.display()));
    let dir = appimage
        .parent()
        .ok_or_else(|| NetError::Io(format!("{} has no directory", appimage.display())))?;
    let name = appimage
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| NetError::Io(format!("{} has no file name", appimage.display())))?;
    let staged = dir.join(format!(".{name}.update-{}", std::process::id()));
    let result = (|| {
        let mut file = std::fs::File::create(&staged).map_err(io)?;
        std::io::Write::write_all(&mut file, &update.bytes).map_err(io)?;
        file.sync_all().map_err(io)?;
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(appimage)
                .map_err(io)?
                .permissions()
                .mode();
            std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(mode)).map_err(io)?;
        }
        std::fs::rename(&staged, appimage).map_err(io)?;
        Ok(appimage.to_path_buf())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&staged);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_order_as_semver_does() {
        assert!(is_newer("1.0.1", "1.0.0").expect("versions"));
        assert!(is_newer("v1.1.0", "1.0.9").expect("versions"));
        assert!(is_newer("1.0.0", "1.0.0-rc.2").expect("versions"));
        assert!(is_newer("1.0.0-rc.10", "1.0.0-rc.2").expect("versions"));
        assert!(!is_newer("1.0.0", "1.0.0").expect("versions"));
        assert!(!is_newer("0.9.9", "1.0.0").expect("versions"));
        assert!(!is_newer("1.0.0-rc.1", "1.0.0").expect("versions"));
        assert!(matches!(
            is_newer("1.0", "1.0.0"),
            Err(UpdateError::BadVersion(_))
        ));
    }

    #[test]
    fn the_placeholder_key_verifies_nothing() {
        assert_eq!(
            verify(b"x", "c2ln", "TODO_UPDATER_PUBKEY"),
            Err(UpdateError::NoKey)
        );
        assert_eq!(public_key("").err(), Some(UpdateError::NoKey));
        assert_eq!(public_key("bm90IGEga2V5").err(), Some(UpdateError::NoKey));
    }

    #[test]
    fn this_build_looks_itself_up_most_specific_first() {
        let keys = platform_keys(Some("appimage"));
        assert_eq!(keys.len(), 2);
        assert!(keys[0].ends_with("-appimage"));
        assert!(keys[0].starts_with(&keys[1]));
        assert!(!keys[1].starts_with("macos"), "Tauri calls macOS darwin");
    }
}
