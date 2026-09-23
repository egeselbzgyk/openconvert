//! `openconvert model pull|list|remove` (IMPLEMENTATION_PLAN §2.1, PHASE 9 details 6–7).
//!
//! The only command that may open a socket, and only `pull` does: a conversion never downloads
//! anything (D9). The registry is compiled into the binary, so the hash a download is checked
//! against ships with the engine and is never fetched from the host that serves the weights
//! (SECURITY §7). `--registry` replaces it, for tests and for a maintainer filling it in.

use std::cell::{Cell, RefCell};
use std::io::Write;
use std::time::Duration;

use oc_core::events::EventSink;
use oc_core::exit::ExitCode;
use oc_core::sidecar::readiness::ModelReadiness;
use oc_core::thresholds::T;
use oc_net::download::{DownloadConfig, DownloadProgress, Downloader, HttpFetch};
use oc_net::registry::{ModelId, ModelRegistry};
use oc_net::store::{self, ModelStore, Removal};

use crate::cli::{ModelAction, ModelArgs, Progress};

const E_REGISTRY: &str = "E_MODEL_REGISTRY";
const E_UNKNOWN_MODEL: &str = "E_MODEL_UNKNOWN";
const E_DOWNLOAD: &str = "E_MODEL_DOWNLOAD";
const E_STORE: &str = "E_MODEL_STORE";

/// Progress is reported at most once per percent, so a gigabyte does not become a million events
/// (§2.3's coalescing). A unit, not a tunable.
const PERCENT: u64 = 100;

pub fn run<W: Write>(
    args: &ModelArgs,
    events: &mut EventSink<W>,
    stdout: &mut dyn Write,
) -> ExitCode {
    let store = ModelStore::new(args.dir.clone().unwrap_or_else(store::default_root));
    let human = args.progress != Progress::Json;
    match args.action {
        // Removing needs no registry: whatever is on disk can always be deleted.
        ModelAction::Remove => remove(
            args.id.as_deref().unwrap_or_default(),
            &store,
            events,
            human,
        ),
        ModelAction::List => match registry(args, events) {
            Some(registry) => list(&registry, &store, args.json, stdout),
            None => ExitCode::Usage,
        },
        ModelAction::Pull => match registry(args, events) {
            Some(registry) => pull(
                &registry,
                args.id.as_deref().unwrap_or_default(),
                store,
                events,
                human,
            ),
            None => ExitCode::Usage,
        },
    }
}

fn registry<W: Write>(args: &ModelArgs, events: &mut EventSink<W>) -> Option<ModelRegistry> {
    let loaded = match &args.registry {
        Some(path) => ModelRegistry::load(path),
        None => ModelRegistry::parse(oc_net::registry::BUNDLED),
    };
    match loaded {
        Ok(registry) => Some(registry),
        Err(error) => {
            events.fatal(E_REGISTRY, &error.to_string());
            None
        }
    }
}

fn list(
    registry: &ModelRegistry,
    store: &ModelStore,
    json: bool,
    stdout: &mut dyn Write,
) -> ExitCode {
    let rows = readiness(registry, store);
    let written = if json {
        serde_json::to_string_pretty(&rows)
            .map_err(std::io::Error::other)
            .and_then(|text| writeln!(stdout, "{text}"))
    } else {
        rows.iter().try_for_each(|row| {
            writeln!(
                stdout,
                "{}{}  {}  {}  {} bytes  {}",
                row.id,
                if row.is_default { " (default)" } else { "" },
                row.tier,
                if row.installed {
                    "installed"
                } else {
                    "not installed"
                },
                row.size_bytes,
                row.license
            )
        })
    };
    match written {
        Ok(()) => ExitCode::Ok,
        Err(_) => ExitCode::Failed,
    }
}

/// One row per registry entry, in registry order.
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

fn pull<W: Write>(
    registry: &ModelRegistry,
    id: &str,
    store: ModelStore,
    events: &mut EventSink<W>,
    human: bool,
) -> ExitCode {
    let Some(entry) = registry.get(&ModelId(id.to_owned())) else {
        events.fatal(
            E_UNKNOWN_MODEL,
            &format!("`{id}` is not in the model registry"),
        );
        return ExitCode::Usage;
    };
    let config = DownloadConfig {
        max_redirects: u32::try_from(T.net.max_redirects).unwrap_or_default(),
    };
    let connect =
        Duration::from_secs(u64::try_from(T.net.connect_timeout_secs).unwrap_or_default());
    let downloader = Downloader::new(store, Box::new(HttpFetch::new(connect)), config);
    let pulled = {
        let progress = LiveProgress {
            events: RefCell::new(&mut *events),
            last_percent: Cell::new(None),
        };
        downloader.pull(entry, &progress)
    };
    match pulled {
        Ok(path) => {
            events.done_with("ok", path.to_str());
            if human {
                eprintln!("installed {id} at {}", path.display());
            }
            ExitCode::Ok
        }
        Err(error) => {
            events.fatal(E_DOWNLOAD, &error.to_string());
            ExitCode::Failed
        }
    }
}

fn remove<W: Write>(
    id: &str,
    store: &ModelStore,
    events: &mut EventSink<W>,
    human: bool,
) -> ExitCode {
    let said = match store.remove(&ModelId(id.to_owned())) {
        Ok(Removal::Removed) => format!("removed {id}"),
        Ok(Removal::Absent) => format!("{id} is not installed; nothing to remove"),
        Err(error) => {
            events.fatal(E_STORE, &error.to_string());
            return ExitCode::Failed;
        }
    };
    events.done("ok");
    if human {
        eprintln!("{said}");
    }
    ExitCode::Ok
}

/// `progress` events for a download, at most one per percent (§2.3's coalescing), as they happen.
struct LiveProgress<'a, W: Write> {
    events: RefCell<&'a mut EventSink<W>>,
    last_percent: Cell<Option<u64>>,
}

impl<W: Write> DownloadProgress for LiveProgress<'_, W> {
    fn bytes(&self, done: u64, total: u64) {
        let percent = done.saturating_mul(PERCENT) / total.max(1);
        if self.last_percent.get() == Some(percent) {
            return;
        }
        self.last_percent.set(Some(percent));
        self.events.borrow_mut().emit(
            "progress",
            serde_json::json!({ "stage": "download", "done": done, "total": total, "unit": "bytes" }),
        );
    }
}
