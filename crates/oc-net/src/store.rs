//! Where models live on disk: `<root>/<id>/<file>`, with `LICENSE` and `NOTICE` beside it.

use std::path::{Path, PathBuf};

use crate::registry::ModelEntry;
use crate::NetError;

/// The suffix a download streams under until it is verified.
pub const PART_SUFFIX: &str = ".part";
pub const LICENSE_FILE: &str = "LICENSE";
pub const NOTICE_FILE: &str = "NOTICE";

/// The model store.
#[derive(Clone, Debug)]
pub struct ModelStore {
    root: PathBuf,
}

impl ModelStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The directory an entry lives in. The id and file name are checked to be plain names, so
    /// no registry entry can place a file outside the store.
    pub fn dir_of(&self, entry: &ModelEntry) -> Result<PathBuf, NetError> {
        Ok(self.root.join(plain_name(&entry.id.0)?))
    }

    /// Where an entry's model file lives once it is verified.
    pub fn path_of(&self, entry: &ModelEntry) -> Result<PathBuf, NetError> {
        Ok(self.dir_of(entry)?.join(plain_name(&entry.file)?))
    }
}

/// `name` if it is one path component made of letters, digits, `.`, `_` and `-`, and not `.`
/// or `..`.
pub fn plain_name(name: &str) -> Result<&str, NetError> {
    let ok = !name.is_empty()
        && name != "."
        && name != ".."
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if ok {
        Ok(name)
    } else {
        Err(NetError::BadUrl(name.to_owned()))
    }
}

/// A model that is on disk and verified.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledModel {
    pub id: crate::registry::ModelId,
    pub path: PathBuf,
    pub size_bytes: u64,
}

/// What `remove` did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Removal {
    Removed,
    Absent,
}

impl ModelStore {
    /// Every verified model on disk, by id. A directory holding only a `.part`, or nothing but
    /// licence files, is not an installed model. Sorted, so the listing is the same on every run.
    pub fn list(&self) -> Vec<InstalledModel> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return out;
        };
        for entry in entries.flatten() {
            let dir = entry.path();
            let Some(id) = dir.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !dir.is_dir() || plain_name(id).is_err() {
                continue;
            }
            if let Some((path, size_bytes)) = model_file(&dir) {
                out.push(InstalledModel {
                    id: crate::registry::ModelId(id.to_owned()),
                    path,
                    size_bytes,
                });
            }
        }
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }

    /// Delete a model's directory — the model, its licence files, and any `.part` left by an
    /// interrupted download. Removing what is not there is `Absent`, not an error.
    pub fn remove(&self, id: &crate::registry::ModelId) -> Result<Removal, NetError> {
        let dir = self.root.join(plain_name(&id.0)?);
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => Ok(Removal::Removed),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Removal::Absent),
            Err(error) => Err(NetError::Io(error.to_string())),
        }
    }
}

/// The one verified model file in `dir`, and its size.
fn model_file(dir: &Path) -> Option<(PathBuf, u64)> {
    let mut found = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        let name = path.file_name()?.to_str()?.to_owned();
        if name == LICENSE_FILE || name == NOTICE_FILE || name.ends_with(PART_SUFFIX) {
            continue;
        }
        let metadata = entry.metadata().ok()?;
        if !metadata.is_file() || found.is_some() {
            return None;
        }
        found = Some((path, metadata.len()));
    }
    found
}

/// Where models live unless told otherwise: `openconvert/models` in the per-OS data directory.
/// `openconvert model` (without `--dir`) and the desktop app's model manager use the same place, so
/// a model is downloaded once whichever of them fetched it.
pub fn default_root() -> PathBuf {
    data_dir().join("openconvert").join("models")
}

#[cfg(target_os = "linux")]
fn data_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| home().map(|home| home.join(".local").join("share")))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(target_os = "macos")]
fn data_dir() -> PathBuf {
    home()
        .map(|home| home.join("Library").join("Application Support"))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(windows)]
fn data_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn data_dir() -> PathBuf {
    home().unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(unix)]
fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}
