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
