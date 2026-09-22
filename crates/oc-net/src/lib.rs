#![forbid(unsafe_code)]
//! The only crate permitted to open sockets (D13.9): model and pack downloads with a
//! host allowlist and SHA-256 verification, plus the BYO endpoint transport.
//!
//! Nothing on the conversion path depends on this crate. `openconvert model pull` and the desktop
//! app's model manager do, and a conversion only ever reads a model file that is already on disk
//! (D9: "downloads never happen during a conversion").

pub mod allowlist;
pub mod download;
pub mod registry;
pub mod store;
pub mod verify;

/// Why a download, or a change to the model store, did not happen.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NetError {
    /// The host is not one `HOST_ALLOWLIST` names. Raised before any socket is opened.
    #[error("`{host}` is not on the download allowlist")]
    HostNotAllowed { host: String },
    /// Not an `https://` URL with a plain host, or a name that would escape the store.
    #[error("`{0}` is not a URL or name the downloader will use")]
    BadUrl(String),
    #[error("revision `{0}` is not a 40-hex commit")]
    UnpinnedRevision(String),
    #[error("SHA-256 mismatch: the registry says {expected}, the download is {actual}")]
    ShaMismatch { expected: String, actual: String },
    #[error("the download is larger than the registry's {expected} bytes")]
    TooLarge { expected: u64 },
    #[error("more than {0} redirects")]
    TooManyRedirects(u32),
    #[error("the server answered HTTP {0}")]
    Status(u16),
    #[error("the transfer failed: {0}")]
    Transport(String),
    #[error("{0}")]
    Io(String),
    /// The downloader writes the licence text beside every model, and has none for this one.
    #[error("no licence text is bundled for `{0}`")]
    UnknownLicense(String),
}
