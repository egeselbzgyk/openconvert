//! The in-app updater (PHASE 15 detail 5, D12): asked by the user, verified before install.
//!
//! Compiled only with the `updater` feature, which is on by default and off in the Flatpak build,
//! where Flathub does the updating and a second update path in one binary would be a bug waiting to
//! happen (detail 4, row 15.8). The work is `oc_net::update`'s: the manifest and payload come
//! through `oc-net` from GitHub's release hosts only, and a payload whose signature does not verify
//! against the key built into `tauri.conf.json` never reaches [`install`].
//!
//! Nothing here runs by itself. A check is a network request, and the app makes none that the user
//! did not ask for (SECURITY §8), so a check happens when the user asks for one.

use std::path::PathBuf;
use std::sync::Mutex;

use oc_core::thresholds::T;
use oc_net::update::{self, UpdateConfig, UpdateError, VerifiedUpdate};
use serde::Serialize;

/// Where `tauri.conf.json` keeps the updater's settings: the same place `tauri-plugin-updater`
/// reads them from, so the release tooling (`tauri signer`, `createUpdaterArtifacts`) is Tauri's.
pub const CONFIG_KEY: &str = "updater";

/// The public key and endpoint this build was configured with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Setup {
    pub pubkey: String,
    pub endpoint: String,
}

impl Setup {
    /// Read from the `plugins.updater` object of the app's configuration. A build without one has
    /// no updater to offer.
    pub fn from_plugin_config(plugin: Option<&serde_json::Value>) -> Option<Self> {
        let plugin = plugin?;
        let pubkey = plugin.get("pubkey")?.as_str()?.to_owned();
        let endpoint = plugin
            .get("endpoints")?
            .as_array()?
            .first()?
            .as_str()?
            .to_owned();
        Some(Self { pubkey, endpoint })
    }
}

/// What a check found, for the webview.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Checked {
    UpToDate,
    /// Downloaded and verified; [`install`] will put it in place.
    Ready {
        version: String,
    },
    /// Why no update could be offered, as a code the UI localises.
    Failed {
        code: String,
    },
}

impl Checked {
    fn failed(error: &UpdateError) -> Self {
        let code = match error {
            UpdateError::NoKey => "no_key",
            UpdateError::BadSignature(_) => "bad_signature",
            UpdateError::TooLarge(_) => "too_large",
            UpdateError::NoPlatform(_) => "no_platform",
            UpdateError::BadManifest(_) | UpdateError::BadVersion(_) => "bad_manifest",
            UpdateError::Net(_) => "network",
        };
        Self::Failed {
            code: code.to_owned(),
        }
    }
}

/// The update a check verified, held until the user installs it.
#[derive(Default)]
pub struct Pending(pub Mutex<Option<VerifiedUpdate>>);

/// The limits a check runs under, from `thresholds.toml`.
pub fn config() -> UpdateConfig {
    UpdateConfig {
        max_redirects: u32::try_from(T.net.max_redirects).unwrap_or_default(),
        manifest_max_bytes: u64::try_from(T.net.update_manifest_max_bytes).unwrap_or_default(),
        payload_max_bytes: u64::try_from(payload_max_bytes()).unwrap_or_default(),
    }
}

/// The largest update payload this build downloads: its own installer's budget. On Linux the
/// payload is the AppImage, which carries WebKitGTK and has a budget of its own (maintainer
/// decision 2026-09-23); a cap at the other installers' budget would refuse every AppImage update.
fn payload_max_bytes() -> i64 {
    if cfg!(target_os = "linux") {
        T.release.max_linux_installer_bytes
    } else {
        T.release.max_installer_bytes
    }
}

/// The installer this build came from, for the manifest's most specific key.
fn installer() -> Option<&'static str> {
    if std::env::var_os("APPIMAGE").is_some() {
        Some("appimage")
    } else if cfg!(target_os = "windows") {
        Some("nsis")
    } else if cfg!(target_os = "macos") {
        Some("app")
    } else {
        None
    }
}

/// Check, download and verify. The key is checked before any request is made.
pub fn check(
    setup: &Setup,
    current: &str,
    fetch: &dyn oc_net::download::Fetch,
) -> (Checked, Option<VerifiedUpdate>) {
    match update::fetch_update(
        fetch,
        &setup.endpoint,
        current,
        &update::platform_keys(installer()),
        &setup.pubkey,
        &config(),
    ) {
        Ok(None) => (Checked::UpToDate, None),
        Ok(Some(verified)) => (
            Checked::Ready {
                version: verified.version().to_owned(),
            },
            Some(verified),
        ),
        Err(error) => (Checked::failed(&error), None),
    }
}

/// Put a verified update in place. Linux: the AppImage replaces itself, and the caller restarts.
/// **Windows and macOS are unverified here** (no machine of either): the NSIS installer is started
/// in passive mode for the app to exit into, and the macOS `.app.tar.gz` is unpacked and swapped in.
pub fn install(update: &VerifiedUpdate, scratch: &std::path::Path) -> Result<(), String> {
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        return update::install_appimage(update, &PathBuf::from(appimage))
            .map(|_| ())
            .map_err(|e| e.to_string());
    }
    install_platform(update, scratch)
}

#[cfg(target_os = "windows")]
fn install_platform(update: &VerifiedUpdate, scratch: &std::path::Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    std::fs::create_dir_all(scratch).map_err(|e| e.to_string())?;
    let installer = scratch.join(update.file_name());
    std::fs::write(&installer, update.bytes()).map_err(|e| e.to_string())?;
    std::process::Command::new(&installer)
        .args(["/P", "/R"])
        // As for every process the app starts (D13.2).
        .creation_flags(crate::engine::CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
fn install_platform(update: &VerifiedUpdate, scratch: &std::path::Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // …/OpenConvert.app/Contents/MacOS/OpenConvert
    let bundle = exe
        .ancestors()
        .nth(3)
        .filter(|p| p.extension().is_some_and(|e| e == "app"))
        .ok_or("the app is not running from an .app bundle")?
        .to_path_buf();
    let unpack = scratch.join("update");
    let _ = std::fs::remove_dir_all(&unpack);
    std::fs::create_dir_all(&unpack).map_err(|e| e.to_string())?;
    let archive = scratch.join(update.file_name());
    std::fs::write(&archive, update.bytes()).map_err(|e| e.to_string())?;
    let status = std::process::Command::new("tar")
        .arg("-xzf")
        .arg(&archive)
        .arg("-C")
        .arg(&unpack)
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("the update archive did not unpack".to_owned());
    }
    let fresh = std::fs::read_dir(&unpack)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .ok_or("the update archive holds no .app")?;
    let old = bundle.with_extension(format!("app.old-{}", std::process::id()));
    std::fs::rename(&bundle, &old).map_err(|e| e.to_string())?;
    if let Err(error) = std::fs::rename(&fresh, &bundle) {
        let _ = std::fs::rename(&old, &bundle);
        return Err(error.to_string());
    }
    let _ = std::fs::remove_dir_all(&old);
    Ok(())
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn install_platform(_update: &VerifiedUpdate, _scratch: &std::path::Path) -> Result<(), String> {
    Err("only the AppImage updates itself on Linux; this install updates through its package manager".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_updater_reads_the_key_and_endpoint_tauri_conf_carries() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("json");
        let setup = Setup::from_plugin_config(config["plugins"].get(CONFIG_KEY)).expect("a setup");
        assert!(setup
            .endpoint
            .starts_with("https://github.com/openconvert/openconvert/releases/"));
        assert!(setup.endpoint.ends_with("/latest.json"));
        // Until the maintainer generates the release keypair (RELEASE_CHECKLIST), a check refuses
        // before it asks anything.
        struct NoNetwork;
        impl oc_net::download::Fetch for NoNetwork {
            fn get(&self, url: &str) -> Result<oc_net::download::Fetched, oc_net::NetError> {
                panic!("a request was made to {url}")
            }
        }
        if setup.pubkey.starts_with(oc_net::update::PLACEHOLDER_PREFIX) {
            let (checked, verified) = check(&setup, "0.1.0", &NoNetwork);
            assert_eq!(
                checked,
                Checked::Failed {
                    code: "no_key".to_owned()
                }
            );
            assert!(verified.is_none());
        }
        assert_eq!(Setup::from_plugin_config(None), None);
    }

    /// An update may be as large as this OS's installer budget, and no larger: on Linux the
    /// AppImage's (it carries WebKitGTK; 112 953 848 bytes measured), elsewhere D12's.
    #[test]
    fn an_update_payload_may_be_as_large_as_this_os_installer() {
        let cap = config().payload_max_bytes;
        let budget = if cfg!(target_os = "linux") {
            T.release.max_linux_installer_bytes
        } else {
            T.release.max_installer_bytes
        };
        assert_eq!(i64::try_from(cap).expect("fits"), budget);
        if cfg!(target_os = "linux") {
            assert!(cap >= 112_953_848, "the measured AppImage must fit");
        }
    }
}
