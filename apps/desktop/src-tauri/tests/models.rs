//! The model manager (Phase 12 detail 9, D9, UI_UX §2.4): rows made of exactly the
//! `ModelReadiness` fields, the licence accepted before the first download, download progress
//! streamed as it arrives, and a Cancel that stops the transfer and deletes the `.part`.
//!
//! Downloads go to `oc-testkit`'s stub model host on loopback, through `oc-net`'s real client and
//! the real allowlist: the registry's URLs are the real `https://huggingface.co/...` ones. A real
//! download from huggingface.co is unverified here (the sandbox's egress policy refuses the host).

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use oc_core::sidecar::readiness::ModelReadiness;
use oc_net::registry::ModelRegistry;
use oc_net::store::ModelStore;
use oc_testkit::download_stub::{self, Answer, Server};
use openconvert_desktop::engine::UiError;
use openconvert_desktop::models::{DownloadState, ModelManager, ModelRow, RowSink};

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
/// Long enough to wait for an event that is coming; a test that waits this long has failed.
const PATIENCE: Duration = Duration::from_secs(20);

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("oc-desktop-models-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("made");
    dir
}

/// A registry of one default model whose download is `body`, and one experimental model.
fn registry(body: &[u8]) -> ModelRegistry {
    ModelRegistry::parse(&format!(
        r#"schema_version = 1
default = "tiny"

[[model]]
id            = "tiny"
tier          = "default"
display_name  = "Tiny"
family        = "qwen3"
arch          = "dense"
license       = "Apache-2.0"
repo          = "org/tiny-GGUF"
revision      = "{COMMIT}"
file          = "tiny.gguf"
sha256        = "{}"
size_bytes    = {}
context       = 8192
parallel      = 1
min_ram_bytes = 3221225472
cpu_expectation = "moderate"
prompt_profile = "qwen3-chatml"
cache_reuse   = true

[[model]]
id            = "other"
tier          = "experimental"
display_name  = "Other"
family        = "qwen3"
arch          = "dense"
license       = "Apache-2.0"
repo          = "org/other-GGUF"
revision      = "{COMMIT}"
file          = "other.gguf"
sha256        = "{}"
size_bytes    = 10
context       = 8192
parallel      = 1
min_ram_bytes = 1610612736
prompt_profile = "qwen3-chatml"
cache_reuse   = false
warn          = "Experimental."
"#,
        download_stub::sha256_hex(body),
        body.len(),
        download_stub::sha256_hex(&[0; 10]),
    ))
    .expect("a valid registry")
}

const PATH: &str = "/org/tiny-GGUF/resolve/0123456789abcdef0123456789abcdef01234567/tiny.gguf";

/// Every row the manager announces, in order.
struct Recorder(Mutex<mpsc::Sender<ModelRow>>);
impl RowSink<ModelReadiness> for Recorder {
    fn changed(&self, row: &ModelRow) {
        let _ = self.0.lock().expect("sender").send(row.clone());
    }
}

fn manager(server: &Server, body: &[u8], store: &Path) -> (ModelManager, Receiver<ModelRow>) {
    let (tx, rx) = mpsc::channel();
    let port = server.port();
    let manager = ModelManager::new(
        Ok(registry(body)),
        ModelStore::new(store),
        Arc::new(move || download_stub::loopback_fetch(port)),
        None,
        Arc::new(Recorder(Mutex::new(tx))),
    );
    (manager, rx)
}

/// The next announced row that satisfies `wanted`.
fn next(rx: &Receiver<ModelRow>, wanted: impl Fn(&ModelRow) -> bool) -> ModelRow {
    loop {
        let row = rx.recv_timeout(PATIENCE).expect("a row in time");
        if wanted(&row) {
            return row;
        }
    }
}

fn files_under(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(
                files_under(&path)
                    .into_iter()
                    .map(|name| format!("{}/{name}", entry.file_name().to_string_lossy())),
            );
        } else {
            out.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    out.sort();
    out
}

fn downloading(row: &ModelRow) -> Option<(u64, u64)> {
    match row.download {
        DownloadState::Downloading { done, total } => Some((done, total)),
        _ => None,
    }
}

/// 12.12 — progress events arrive while the bytes do, and Cancel deletes the `.part`.
#[test]
fn model_download_progress_streams_and_cancels() {
    let server = Server::start();
    let store = scratch("cancel");
    let body = download_stub::body(4 << 20);
    server.route(
        "huggingface.co",
        PATH,
        Answer::Slow {
            body: body.clone(),
            chunk: 64 << 10,
            pause: Duration::from_millis(25),
        },
    );
    let (manager, rx) = manager(&server, &body, &store);
    manager.accept_license("tiny").expect("accepted");
    manager.pull("tiny").expect("started");

    // Progress streams: several rows, each with more bytes, none of them the whole file yet.
    let first = next(&rx, |row| {
        downloading(row).is_some_and(|(done, _)| done > 0)
    });
    let (done_1, total) = downloading(&first).expect("downloading");
    assert_eq!(total, body.len() as u64, "the registry's size");
    let later = next(&rx, |row| {
        downloading(row).is_some_and(|(done, _)| done > done_1)
    });
    let (done_2, _) = downloading(&later).expect("downloading");
    assert!(done_2 < total, "cancelled before the end");
    assert!(
        files_under(&store).contains(&"tiny/tiny.gguf.part".to_owned()),
        "{:?}",
        files_under(&store)
    );

    // Cancel: the transfer stops and nothing of it is left.
    manager.cancel("tiny").expect("cancelling");
    let ended = next(&rx, |row| row.download == DownloadState::Idle);
    assert!(!ended.readiness.installed);
    assert!(files_under(&store).is_empty(), "{:?}", files_under(&store));
    assert!(!manager.view().rows[0].readiness.installed);

    // A later download runs to the end and installs the verified file beside its licence.
    server.route("huggingface.co", PATH, Answer::Body(body.clone()));
    manager.pull("tiny").expect("started again");
    let installed = next(&rx, |row| row.readiness.installed);
    assert_eq!(installed.download, DownloadState::Idle);
    assert_eq!(
        files_under(&store),
        ["tiny/LICENSE", "tiny/NOTICE", "tiny/tiny.gguf"]
    );
    assert_eq!(
        installed.readiness.license_path,
        Some(store.join("tiny").join("LICENSE"))
    );
}

/// The licence is shown and accepted before the first download of a model; the manager refuses a
/// download nobody accepted, whatever the webview sends.
#[test]
fn a_model_is_downloaded_only_after_its_license_is_accepted() {
    let server = Server::start();
    let store = scratch("license");
    let body = download_stub::body(1 << 10);
    server.route("huggingface.co", PATH, Answer::Body(body.clone()));
    let (manager, rx) = manager(&server, &body, &store);

    let license = manager.license("tiny").expect("a licence");
    assert_eq!(license.license, "Apache-2.0");
    assert!(license.text.contains("Apache License"), "the full text");
    assert!(!manager.view().rows[0].license_accepted);

    assert!(matches!(
        manager.pull("tiny"),
        Err(UiError::LicenseNotAccepted(_))
    ));
    assert_eq!(server.requests(), 0, "nothing was fetched");

    manager.accept_license("tiny").expect("accepted");
    assert!(manager.view().rows[0].license_accepted);
    manager.pull("tiny").expect("started");
    next(&rx, |row| row.readiness.installed);

    // Delete frees the space and leaves the other rows alone.
    manager.remove("tiny").expect("removed");
    assert!(!manager.view().rows[0].readiness.installed);
    assert!(files_under(&store).is_empty());
}

/// A download that fails verification is a failed row the user can retry, with nothing left on
/// disk, and an unknown id is refused by name.
#[test]
fn a_failed_download_is_a_row_to_retry() {
    let server = Server::start();
    let store = scratch("failed");
    let body = download_stub::body(1 << 10);
    let mut tampered = body.clone();
    tampered[0] ^= 1;
    server.route("huggingface.co", PATH, Answer::Body(tampered));
    let (manager, rx) = manager(&server, &body, &store);
    manager.accept_license("tiny").expect("accepted");
    manager.pull("tiny").expect("started");

    let failed = next(&rx, |row| {
        matches!(row.download, DownloadState::Failed { .. })
    });
    assert!(!failed.readiness.installed);
    assert!(files_under(&store).is_empty(), "{:?}", files_under(&store));

    server.route("huggingface.co", PATH, Answer::Body(body));
    manager.pull("tiny").expect("retried");
    next(&rx, |row| row.readiness.installed);

    assert!(matches!(
        manager.pull("nope"),
        Err(UiError::UnknownModel(_))
    ));
}

/// Rows are the registry's `ModelReadiness`, in registry order: the default flagged, the
/// experimental warning kept, nothing added at the UI layer.
#[test]
fn model_rows_are_the_readiness_fields() {
    let server = Server::start();
    let store = scratch("rows");
    let body = download_stub::body(16);
    let (manager, _rx) = manager(&server, &body, &store);
    let view = manager.view();
    assert_eq!(view.unavailable, None);
    let ids: Vec<&str> = view
        .rows
        .iter()
        .map(|row| row.readiness.id.as_str())
        .collect();
    assert_eq!(ids, ["tiny", "other"]);
    let tiny = &view.rows[0].readiness;
    assert!(tiny.is_default);
    assert_eq!(tiny.size_bytes, 16);
    assert_eq!(tiny.ram_estimate_bytes, 3_221_225_472);
    assert_eq!(tiny.cpu_expectation, "moderate");
    assert_eq!(tiny.license, "Apache-2.0");
    assert_eq!(tiny.license_path, None);
    let other = &view.rows[1].readiness;
    assert_eq!(other.cpu_expectation, "not yet measured");
    assert_eq!(other.warn.as_deref(), Some("Experimental."));
}

/// The registry shipped today still has placeholder pins: the manager says models are not
/// available in this build, and why, instead of offering a download that cannot be verified.
#[test]
fn an_unpinned_registry_offers_no_download() {
    let (tx, _rx) = mpsc::channel();
    let manager = ModelManager::new(
        ModelRegistry::parse(oc_net::registry::BUNDLED).map_err(|error| error.to_string()),
        ModelStore::new(scratch("bundled")),
        Arc::new(|| download_stub::loopback_fetch(1)),
        None,
        Arc::new(Recorder(Mutex::new(tx))),
    );
    let view = manager.view();
    if ModelRegistry::parse(oc_net::registry::BUNDLED).is_err() {
        assert!(view.unavailable.is_some());
        assert!(view.rows.is_empty());
        assert!(matches!(
            manager.pull("qwen3-1.7b-q4_k_m"),
            Err(UiError::ModelsUnavailable(_))
        ));
    } else {
        assert_eq!(view.unavailable, None);
    }
}
