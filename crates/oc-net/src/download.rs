//! `openconvert model pull`: a pinned URL, the allowlist on every hop, SHA-256 while streaming,
//! an atomic rename, and the licence written beside the file (D9, IMPLEMENTATION_PLAN PHASE 9
//! detail 6).
//!
//! The order of a pull is the whole of its safety argument:
//!
//! 1. The URL is built from the registry entry and checked against the allowlist, and so is every
//!    redirect, *before* the fetch that would open a socket to it (A9.2).
//! 2. The body streams into `<file>.part`, hashed as it is written, and stops at the registry's
//!    size — a server cannot make the download longer than the file it is supposed to be.
//! 3. A hash that does not match deletes the `.part` and fails.
//! 4. `LICENSE` and `NOTICE` are written, and only then is the `.part` renamed to its final name,
//!    in the same directory, so the final name only ever holds a verified file (row 9.6).

use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::allowlist;
use crate::registry::{is_commit, ModelEntry};
use crate::store::{self, ModelStore};
use crate::verify::{self, CopyError};
use crate::NetError;

/// The URL a registry entry resolves to when it names no template of its own (§1.6).
pub const DEFAULT_URL_TEMPLATE: &str = "https://huggingface.co/{repo}/resolve/{revision}/{file}";

/// The only licence whose text is bundled. An entry under any other licence is refused rather
/// than installed without its terms beside it (LICENSE_AND_DEPENDENCIES §5).
const APACHE_2_0: &str = "Apache-2.0";
const APACHE_2_0_TEXT: &str = include_str!("../licenses/Apache-2.0.txt");

/// What one GET brought back.
pub enum Fetched {
    Body(Box<dyn Read + Send>),
    /// A 3xx, with its `Location`. The downloader follows it itself, so every hop meets the
    /// allowlist.
    Redirect(String),
}

/// One GET. The downloader decides *whether* to fetch a URL; this only fetches it.
pub trait Fetch: Send + Sync {
    fn get(&self, url: &str) -> Result<Fetched, NetError>;
}

/// [`Fetch`] with `ureq`, trusting the platform's roots and never following a redirect itself.
pub struct HttpFetch {
    agent: ureq::Agent,
}

impl HttpFetch {
    pub fn new(connect_timeout: Duration) -> Self {
        let tls = ureq::tls::TlsConfig::builder()
            .root_certs(ureq::tls::RootCerts::PlatformVerifier)
            .unversioned_rustls_crypto_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .build();
        let agent = ureq::Agent::config_builder()
            .max_redirects(0)
            .http_status_as_error(false)
            .timeout_connect(Some(connect_timeout))
            .tls_config(tls)
            .build()
            .new_agent();
        Self { agent }
    }
}

impl Fetch for HttpFetch {
    fn get(&self, url: &str) -> Result<Fetched, NetError> {
        let response = self
            .agent
            .get(url)
            .call()
            .map_err(|error| NetError::Transport(error.to_string()))?;
        let status = response.status();
        if status.is_redirection() {
            let location = response
                .headers()
                .get("location")
                .and_then(|value| value.to_str().ok())
                .ok_or(NetError::Status(status.as_u16()))?;
            return Ok(Fetched::Redirect(location.to_owned()));
        }
        if !status.is_success() {
            return Err(NetError::Status(status.as_u16()));
        }
        Ok(Fetched::Body(Box::new(response.into_body().into_reader())))
    }
}

/// Told how far a download has got.
pub trait DownloadProgress {
    fn bytes(&self, done: u64, total: u64);
}

/// The numbers a download needs, read by the caller from `thresholds.toml`.
#[derive(Clone, Copy, Debug)]
pub struct DownloadConfig {
    pub max_redirects: u32,
}

pub struct Downloader {
    store: ModelStore,
    fetch: Box<dyn Fetch>,
    config: DownloadConfig,
}

impl Downloader {
    pub fn new(store: ModelStore, fetch: Box<dyn Fetch>, config: DownloadConfig) -> Self {
        Self {
            store,
            fetch,
            config,
        }
    }

    pub fn store(&self) -> &ModelStore {
        &self.store
    }

    /// Download, verify and install one registry entry. Returns the model file's path.
    pub fn pull(&self, e: &ModelEntry, p: &dyn DownloadProgress) -> Result<PathBuf, NetError> {
        if e.license != APACHE_2_0 {
            return Err(NetError::UnknownLicense(e.license.clone()));
        }
        let url = resolve_url(e)?;
        let dir = self.store.dir_of(e)?;
        let target = self.store.path_of(e)?;
        let part = part_path(&target);

        let mut body = self.open(&url)?;
        std::fs::create_dir_all(&dir).map_err(io)?;
        let file = File::create(&part).map_err(io)?;
        let mut writer = BufWriter::new(file);
        let copied = verify::copy_hashed(&mut body, &mut writer, e.size_bytes, &mut |done| {
            p.bytes(done, e.size_bytes)
        })
        .map_err(|error| match error {
            CopyError::TooLarge => NetError::TooLarge {
                expected: e.size_bytes,
            },
            CopyError::Read(error) => NetError::Transport(error.to_string()),
            CopyError::Write(error) => io(error),
        })
        .and_then(|copied| {
            let file = writer
                .into_inner()
                .map_err(|error| io(error.into_error()))?;
            file.sync_all().map_err(io)?;
            Ok(copied)
        });
        let (_, actual) = match copied {
            Ok(copied) => copied,
            Err(error) => {
                let _ = std::fs::remove_file(&part);
                return Err(error);
            }
        };
        if actual != e.sha256 {
            let _ = std::fs::remove_file(&part);
            return Err(NetError::ShaMismatch {
                expected: e.sha256.clone(),
                actual,
            });
        }

        write_beside(&dir.join(store::LICENSE_FILE), APACHE_2_0_TEXT)?;
        write_beside(&dir.join(store::NOTICE_FILE), &notice(e, &url))?;
        std::fs::rename(&part, &target).map_err(io)?;
        Ok(target)
    }

    /// Follow `url` through its redirects to a body, checking every hop against the allowlist
    /// before fetching it.
    fn open(&self, url: &str) -> Result<Box<dyn Read + Send>, NetError> {
        let mut current = url.to_owned();
        let mut hops = 0;
        loop {
            allowlist::check(&current)?;
            match self.fetch.get(&current)? {
                Fetched::Body(body) => return Ok(body),
                Fetched::Redirect(location) => {
                    hops += 1;
                    if hops > self.config.max_redirects {
                        return Err(NetError::TooManyRedirects(self.config.max_redirects));
                    }
                    current = allowlist::resolve_location(&current, &location)?;
                }
            }
        }
    }
}

/// The URL a registry entry downloads from: its template with `{repo}`, `{revision}` and `{file}`
/// filled in. The revision must be a commit, and the result must be on the allowlist.
pub fn resolve_url(e: &ModelEntry) -> Result<String, NetError> {
    if !is_commit(&e.revision) {
        return Err(NetError::UnpinnedRevision(e.revision.clone()));
    }
    check_repo(&e.repo)?;
    let file = store::plain_name(&e.file)?;
    let url = e
        .url_template
        .as_deref()
        .unwrap_or(DEFAULT_URL_TEMPLATE)
        .replace("{repo}", &e.repo)
        .replace("{revision}", &e.revision)
        .replace("{file}", file);
    allowlist::check(&url)?;
    Ok(url)
}

/// `org/name`, each half a plain name.
fn check_repo(repo: &str) -> Result<(), NetError> {
    match repo.split_once('/') {
        Some((org, name)) => {
            store::plain_name(org)?;
            store::plain_name(name)?;
            Ok(())
        }
        None => Err(NetError::BadUrl(repo.to_owned())),
    }
}

fn part_path(target: &Path) -> PathBuf {
    let mut name = target.as_os_str().to_owned();
    name.push(store::PART_SUFFIX);
    PathBuf::from(name)
}

/// What `NOTICE` says: the entry's own notice, and where and what exactly was downloaded.
fn notice(e: &ModelEntry, url: &str) -> String {
    let own = e
        .notice_text
        .clone()
        .unwrap_or_else(|| format!("{}, {}.", e.display_name, e.license));
    format!(
        "{own}\n\nDownloaded by OpenConvert from {url}\nrepository {}\nrevision {}\nfile {}\nsha256 {}\nlicence {}\n",
        e.repo, e.revision, e.file, e.sha256, e.license
    )
}

fn write_beside(path: &Path, text: &str) -> Result<(), NetError> {
    let mut file = File::create(path).map_err(io)?;
    file.write_all(text.as_bytes()).map_err(io)?;
    file.sync_all().map_err(io)
}

fn io(error: std::io::Error) -> NetError {
    NetError::Io(error.to_string())
}
