//! Optional packs (PHASE 12 detail 9, LICENSE_AND_DEPENDENCIES §6): the validation pack installs
//! through the same manager and the same downloader as a model — licence first, progress streamed,
//! Cancel deleting the `.part` — and renders its registry, not constants.

use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use oc_net::packs::PackRegistry;
use oc_net::store::ModelStore;
use oc_testkit::download_stub::{self, Answer, Server};
use openconvert_desktop::engine::UiError;
use openconvert_desktop::models::{DownloadState, Row, RowSink};
use openconvert_desktop::packs::{PackManager, PackReadiness};

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
const PATIENCE: Duration = Duration::from_secs(20);

struct Recorder(Mutex<mpsc::Sender<Row<PackReadiness>>>);
impl RowSink<PackReadiness> for Recorder {
    fn changed(&self, row: &Row<PackReadiness>) {
        let _ = self.0.lock().expect("sender").send(row.clone());
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("oc-desktop-packs-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("made");
    dir
}

/// The validation pack is a pinned file like a model, and installs through the model manager's
/// mechanism into the packs store.
#[test]
fn the_validation_pack_installs_through_the_model_mechanism() {
    let server = Server::start();
    let store = scratch("install");
    let body = download_stub::body(2 << 20);
    let registry = PackRegistry::parse(&format!(
        r#"schema_version = 1

[[pack]]
id           = "validation"
display_name = "Validation pack"
contents     = "EPUBCheck with a minimal Java runtime"
license      = "Apache-2.0"
repo         = "org/packs"
revision     = "{COMMIT}"
file         = "validation.zip"
sha256       = "{}"
size_bytes   = {}
"#,
        download_stub::sha256_hex(&body),
        body.len()
    ))
    .expect("a pinned pack");
    let path = format!("/org/packs/resolve/{COMMIT}/validation.zip");
    server.route(
        "huggingface.co",
        &path,
        Answer::Slow {
            body: body.clone(),
            chunk: 64 << 10,
            pause: Duration::from_millis(10),
        },
    );
    let (tx, rx) = mpsc::channel();
    let port = server.port();
    let packs = PackManager::new(
        Ok(registry),
        ModelStore::new(&store),
        Arc::new(move || download_stub::loopback_fetch(port)),
        None,
        Arc::new(Recorder(Mutex::new(tx))),
    );

    let view = packs.view();
    let row = &view.rows[0];
    assert_eq!(row.readiness.id, "validation");
    assert_eq!(
        row.readiness.contents,
        "EPUBCheck with a minimal Java runtime"
    );
    assert_eq!(row.readiness.size_bytes, body.len() as u64);
    assert!(!row.readiness.installed);

    assert!(matches!(
        packs.pull("validation"),
        Err(UiError::LicenseNotAccepted(_))
    ));
    packs.accept_license("validation").expect("accepted");
    packs.pull("validation").expect("started");
    let mut saw_progress = false;
    let installed = loop {
        let row = rx.recv_timeout(PATIENCE).expect("a row in time");
        if let DownloadState::Downloading { done, total } = row.download {
            saw_progress |= done > 0 && done < total;
        }
        if row.readiness.installed {
            break row;
        }
    };
    assert!(saw_progress, "progress streamed");
    assert_eq!(
        installed.readiness.license_path,
        Some(store.join("validation").join("LICENSE"))
    );
    assert_eq!(
        packs.installed_path("validation"),
        Some(store.join("validation").join("validation.zip"))
    );
}

/// The pack registry that ships has nothing pinned: the Packs screen says the validation pack is
/// not available in this version, and no download can start.
#[test]
fn the_shipped_validation_pack_is_not_available_in_this_version() {
    let (tx, _rx) = mpsc::channel();
    let packs = PackManager::new(
        openconvert_desktop::packs::bundled_registry(),
        ModelStore::new(scratch("bundled")),
        Arc::new(|| download_stub::loopback_fetch(1)),
        None,
        Arc::new(Recorder(Mutex::new(tx))),
    );
    let view = packs.view();
    assert!(view.unavailable.is_some());
    assert!(matches!(
        packs.pull("validation"),
        Err(UiError::ModelsUnavailable(_))
    ));
}
