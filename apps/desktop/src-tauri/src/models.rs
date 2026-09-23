//! The model manager (PHASE 12 detail 9, D9, UI_UX §2.4 and §3).
//!
//! The models screen renders exactly Phase 9's `ModelReadiness` — size, RAM estimate, CPU
//! expectation, licence, licence path — read from the registry compiled into the app and the model
//! store on disk; nothing is estimated here. On top of those fields a row carries only what the
//! manager itself knows: whether the user has accepted the licence, and how far a download is.
//!
//! - **One download mechanism.** `oc_net::download::Downloader`, the one `openconvert model pull`
//!   uses: the allowlist on every hop, SHA-256 while streaming, `LICENSE` and `NOTICE` beside the
//!   file, an atomic rename. The store is the CLI's default one (`oc_net::store::default_root`), so a
//!   model is fetched once whichever of the two fetched it.
//! - **The licence first.** The full text is shown and accepted before the first download of a
//!   model, and the acceptance is kept in the app's local state (LICENSE_AND_DEPENDENCIES §6). The
//!   manager refuses a download nobody accepted, whatever the webview asks.
//! - **Progress streams** as bytes arrive, at most once per percent, on a thread of its own.
//! - **Cancel** stops the transfer between two chunks and deletes the `.part` (row 12.12). A
//!   transfer stalled inside one read notices the cancel when that read returns; the row says
//!   "cancelling" until then.
//! - **Downloads never happen during a conversion** (D9): nothing on the conversion path calls
//!   this, and the engine is only ever handed a model that is already on disk.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use oc_core::sidecar::readiness::ModelReadiness;
use oc_core::thresholds::T;
use oc_net::download::{DownloadConfig, DownloadProgress, Downloader, Fetch};
use oc_net::registry::{ModelEntry, ModelId, ModelRegistry};
use oc_net::store::{self, ModelStore};
use oc_net::NetError;
use serde::{Deserialize, Serialize};

use crate::engine::UiError;

/// Progress is announced at most once per percent: a gigabyte is not a million events. A unit,
/// not a tunable.
const PERCENT: u64 = 100;

/// Makes the fetcher a download uses: `HttpFetch` in the app, a loopback stub in the tests.
pub type FetchFactory = Arc<dyn Fn() -> Box<dyn Fetch> + Send + Sync>;

/// Told whenever a row changes: a download's progress, its end, a licence accepted, a delete.
pub trait ModelSink: Send + Sync {
    fn changed(&self, row: &ModelRow);
}

/// Where a model's download is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DownloadState {
    /// Nothing in flight: the row is installed or not, as `installed` says.
    Idle,
    Downloading {
        done: u64,
        total: u64,
    },
    /// Cancel was pressed; the transfer stops at its next chunk.
    Cancelling {
        done: u64,
        total: u64,
    },
    /// The last download did not install. Retry starts it again from the first byte.
    Failed {
        kind: FailKind,
        detail: String,
    },
}

/// Why a download failed, for the UI to say in the user's language.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailKind {
    /// The host could not be reached, answered with an error, or the connection dropped.
    Unreachable,
    /// The bytes were not the ones the registry pins.
    Verification,
    /// The file could not be written.
    Disk,
    /// The downloader refused the entry itself: a host off the allowlist, an unpinned revision, a
    /// licence with no bundled text.
    Refused,
}

/// One row of the models screen: Phase 9's readiness, and what the manager knows on top.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelRow {
    #[serde(flatten)]
    pub readiness: ModelReadiness,
    pub license_accepted: bool,
    pub download: DownloadState,
}

/// The models screen.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelsView {
    /// Why no model can be downloaded in this build, when none can: the registry it ships still
    /// has placeholder pins. `None` when the registry is usable.
    pub unavailable: Option<String>,
    pub rows: Vec<ModelRow>,
}

/// A model's licence, as it is shown before the download.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LicenseView {
    pub id: String,
    /// The SPDX name.
    pub license: String,
    /// The full text: the same text the downloader writes beside the model.
    pub text: String,
}

/// A download in flight.
struct Active {
    cancel: Arc<AtomicBool>,
    state: DownloadState,
}

struct Inner {
    registry: Result<ModelRegistry, String>,
    store: ModelStore,
    fetch: FetchFactory,
    /// Model id → the SPDX licence the user accepted for it.
    accepted: Mutex<BTreeMap<String, String>>,
    accepted_path: Option<PathBuf>,
    downloads: Mutex<BTreeMap<String, Active>>,
    sink: Arc<dyn ModelSink>,
}

/// The app's model manager. Cheap to share: every method takes `&self`.
pub struct ModelManager {
    inner: Arc<Inner>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl ModelManager {
    /// A manager over `registry` (or why there is none) and `store`, keeping accepted licences in
    /// `accepted_path` when one is given.
    pub fn new(
        registry: Result<ModelRegistry, String>,
        store: ModelStore,
        fetch: FetchFactory,
        accepted_path: Option<PathBuf>,
        sink: Arc<dyn ModelSink>,
    ) -> Self {
        let accepted = accepted_path
            .as_deref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str::<Accepted>(&text).ok())
            .map(|file| file.models)
            .unwrap_or_default();
        Self {
            inner: Arc::new(Inner {
                registry,
                store,
                fetch,
                accepted: Mutex::new(accepted),
                accepted_path,
                downloads: Mutex::new(BTreeMap::new()),
                sink,
            }),
        }
    }

    /// Every row, in registry order.
    pub fn view(&self) -> ModelsView {
        match &self.inner.registry {
            Ok(registry) => ModelsView {
                unavailable: None,
                rows: self.inner.rows(registry),
            },
            Err(why) => ModelsView {
                unavailable: Some(why.clone()),
                rows: Vec::new(),
            },
        }
    }

    /// The licence of model `id`, in full.
    pub fn license(&self, id: &str) -> Result<LicenseView, UiError> {
        let entry = self.inner.entry(id)?;
        let text = oc_net::download::license_text(&entry.license)
            .ok_or_else(|| UiError::Io(format!("no licence text for {}", entry.license)))?;
        Ok(LicenseView {
            id: id.to_owned(),
            license: entry.license.clone(),
            text: text.to_owned(),
        })
    }

    /// The user accepted model `id`'s licence, having been shown it.
    pub fn accept_license(&self, id: &str) -> Result<(), UiError> {
        let entry = self.inner.entry(id)?;
        let snapshot = {
            let mut accepted = lock(&self.inner.accepted);
            accepted.insert(id.to_owned(), entry.license.clone());
            accepted.clone()
        };
        if let Some(path) = &self.inner.accepted_path {
            save_accepted(path, snapshot)?;
        }
        self.inner.announce(id);
        Ok(())
    }

    /// Start downloading model `id` on a thread of its own. Refused unless its licence was
    /// accepted, and while it is already downloading.
    pub fn pull(&self, id: &str) -> Result<(), UiError> {
        let entry = self.inner.entry(id)?.clone();
        if !self.inner.accepted(&entry) {
            return Err(UiError::LicenseNotAccepted(id.to_owned()));
        }
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut downloads = lock(&self.inner.downloads);
            if downloads.get(id).is_some_and(Active::in_flight) {
                return Err(UiError::ModelBusy(id.to_owned()));
            }
            downloads.insert(
                id.to_owned(),
                Active {
                    cancel: cancel.clone(),
                    state: DownloadState::Downloading {
                        done: 0,
                        total: entry.size_bytes,
                    },
                },
            );
        }
        self.inner.announce(id);

        let inner = self.inner.clone();
        std::thread::spawn(move || {
            let config = DownloadConfig {
                max_redirects: u32::try_from(T.net.max_redirects).unwrap_or_default(),
            };
            let downloader = Downloader::new(inner.store.clone(), (inner.fetch)(), config);
            let progress = Progress {
                inner: &inner,
                id: &entry.id.0,
                cancel: &cancel,
                last_percent: Mutex::new(None),
            };
            let outcome = downloader.pull(&entry, &progress);
            inner.finish(&entry.id.0, outcome);
        });
        Ok(())
    }

    /// Stop model `id`'s download. Doing nothing when none is in flight.
    pub fn cancel(&self, id: &str) -> Result<(), UiError> {
        let cancelled = {
            let mut downloads = lock(&self.inner.downloads);
            match downloads.get_mut(id) {
                Some(active) => match active.state {
                    DownloadState::Downloading { done, total } => {
                        active.cancel.store(true, Ordering::SeqCst);
                        active.state = DownloadState::Cancelling { done, total };
                        true
                    }
                    _ => false,
                },
                None => false,
            }
        };
        if cancelled {
            self.inner.announce(id);
        }
        Ok(())
    }

    /// Delete model `id` from the store: the file, its licence files, any `.part`. Other models
    /// are not touched; deleting one that is not there is not an error.
    pub fn remove(&self, id: &str) -> Result<(), UiError> {
        self.inner.entry(id)?;
        if lock(&self.inner.downloads)
            .get(id)
            .is_some_and(Active::in_flight)
        {
            return Err(UiError::ModelBusy(id.to_owned()));
        }
        self.inner
            .store
            .remove(&ModelId(id.to_owned()))
            .map_err(|error| UiError::Io(error.to_string()))?;
        lock(&self.inner.downloads).remove(id);
        self.inner.announce(id);
        Ok(())
    }

    /// Where an installed model's file is, for the model server.
    pub fn installed_path(&self, id: &str) -> Option<PathBuf> {
        let entry = self.inner.entry(id).ok()?;
        let path = self.inner.store.path_of(entry).ok()?;
        path.is_file().then_some(path)
    }
}

impl Active {
    fn in_flight(&self) -> bool {
        matches!(
            self.state,
            DownloadState::Downloading { .. } | DownloadState::Cancelling { .. }
        )
    }
}

impl Inner {
    fn registry(&self) -> Result<&ModelRegistry, UiError> {
        self.registry
            .as_ref()
            .map_err(|why| UiError::ModelsUnavailable(why.clone()))
    }

    fn entry(&self, id: &str) -> Result<&ModelEntry, UiError> {
        self.registry()?
            .get(&ModelId(id.to_owned()))
            .ok_or_else(|| UiError::UnknownModel(id.to_owned()))
    }

    /// Accepted, and for the licence the registry names today.
    fn accepted(&self, entry: &ModelEntry) -> bool {
        lock(&self.accepted).get(&entry.id.0) == Some(&entry.license)
    }

    fn rows(&self, registry: &ModelRegistry) -> Vec<ModelRow> {
        let downloads = lock(&self.downloads);
        readiness(registry, &self.store)
            .into_iter()
            .zip(registry.entries())
            .map(|(readiness, entry)| ModelRow {
                license_accepted: self.accepted(entry),
                download: downloads
                    .get(&readiness.id)
                    .map_or(DownloadState::Idle, |active| active.state.clone()),
                readiness,
            })
            .collect()
    }

    /// Tell the sink about row `id` as it is now.
    fn announce(&self, id: &str) {
        let Ok(registry) = self.registry() else {
            return;
        };
        if let Some(row) = self
            .rows(registry)
            .into_iter()
            .find(|row| row.readiness.id == id)
        {
            self.sink.changed(&row);
        }
    }

    /// A download ended: installed, cancelled, or failed.
    fn finish(&self, id: &str, outcome: Result<PathBuf, NetError>) {
        {
            let mut downloads = lock(&self.downloads);
            match outcome {
                Ok(_) | Err(NetError::Cancelled) => {
                    downloads.remove(id);
                }
                Err(error) => {
                    if let Some(active) = downloads.get_mut(id) {
                        active.state = DownloadState::Failed {
                            kind: fail_kind(&error),
                            detail: error.to_string(),
                        };
                    }
                }
            }
        }
        self.announce(id);
    }
}

fn fail_kind(error: &NetError) -> FailKind {
    match error {
        NetError::ShaMismatch { .. } | NetError::TooLarge { .. } => FailKind::Verification,
        NetError::Io(_) => FailKind::Disk,
        NetError::HostNotAllowed { .. }
        | NetError::BadUrl(_)
        | NetError::UnpinnedRevision(_)
        | NetError::UnknownLicense(_) => FailKind::Refused,
        NetError::TooManyRedirects(_)
        | NetError::Status(_)
        | NetError::Transport(_)
        | NetError::Cancelled => FailKind::Unreachable,
    }
}

/// Streams a download's progress into its row, and carries Cancel back to the downloader.
struct Progress<'a> {
    inner: &'a Inner,
    id: &'a str,
    cancel: &'a AtomicBool,
    last_percent: Mutex<Option<u64>>,
}

impl DownloadProgress for Progress<'_> {
    fn bytes(&self, done: u64, total: u64) {
        let percent = done.saturating_mul(PERCENT) / total.max(1);
        {
            let mut last = lock(&self.last_percent);
            if *last == Some(percent) {
                return;
            }
            *last = Some(percent);
        }
        {
            let mut downloads = lock(&self.inner.downloads);
            match downloads.get_mut(self.id) {
                Some(active) if matches!(active.state, DownloadState::Downloading { .. }) => {
                    active.state = DownloadState::Downloading { done, total };
                }
                _ => return,
            }
        }
        self.inner.announce(self.id);
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
}

/// One `ModelReadiness` per registry entry, in registry order — the same rows, read the same way,
/// as `openconvert model list --json` prints (`crates/openconvert/src/cmd_model.rs`; the
/// engine-integration test `the_app_and_the_cli_list_the_same_models` holds the two equal).
pub fn readiness(registry: &ModelRegistry, store: &ModelStore) -> Vec<ModelReadiness> {
    let installed = store.list();
    registry
        .entries()
        .iter()
        .map(|entry| {
            let on_disk = installed.iter().find(|model| model.id == entry.id);
            ModelReadiness {
                id: entry.id.0.clone(),
                display_name: entry.display_name.clone(),
                tier: entry.tier.clone(),
                is_default: registry.default_id() == &entry.id,
                installed: on_disk.is_some(),
                size_bytes: entry.size_bytes,
                ram_estimate_bytes: entry.min_ram_bytes,
                cpu_expectation: entry
                    .cpu_expectation
                    .clone()
                    .unwrap_or_else(|| "not yet measured".to_owned()),
                license: entry.license.clone(),
                license_path: on_disk.and_then(|model| {
                    let path = model.path.parent()?.join(store::LICENSE_FILE);
                    path.is_file().then_some(path)
                }),
                warn: entry.warn.clone(),
            }
        })
        .collect()
}

/// The registry compiled into the app, or why it cannot be used.
pub fn bundled_registry() -> Result<ModelRegistry, String> {
    ModelRegistry::parse(oc_net::registry::BUNDLED).map_err(|error| error.to_string())
}

/// The real fetcher: `oc-net`'s client, with the connect timeout from `thresholds.toml`.
pub fn http_fetch() -> FetchFactory {
    Arc::new(|| {
        let connect =
            Duration::from_secs(u64::try_from(T.net.connect_timeout_secs).unwrap_or_default());
        Box::new(oc_net::download::HttpFetch::new(connect))
    })
}

/// The accepted-licences file.
#[derive(Default, Serialize, Deserialize)]
struct Accepted {
    models: BTreeMap<String, String>,
}

fn save_accepted(path: &std::path::Path, models: BTreeMap<String, String>) -> Result<(), UiError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(&Accepted { models })
        .map_err(|error| UiError::Io(error.to_string()))?;
    let partial = path.with_extension("json.part");
    std::fs::write(&partial, text)?;
    std::fs::rename(&partial, path)?;
    Ok(())
}

/// Where accepted licences are kept, beside the settings.
pub fn accepted_path(config_dir: &std::path::Path) -> PathBuf {
    config_dir.join("licenses.json")
}
