//! Row 9.19: the store lists what is installed and removes it, and removing what is not there is
//! an answer, not a failure.

mod common;

use common::{Answer, Quiet, Server, TEMPLATE};
use oc_net::download::Downloader;
use oc_net::registry::ModelId;
use oc_net::store::{ModelStore, Removal};

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("oc-net-store-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// Row 9.19.
#[test]
fn model_remove_is_idempotent() {
    let dir = scratch("remove");
    let store = ModelStore::new(&dir);
    let id = ModelId("tiny".to_owned());

    // Nothing there: an answer, twice.
    assert_eq!(store.remove(&id), Ok(Removal::Absent));
    assert_eq!(store.remove(&id), Ok(Removal::Absent));
    // A store directory that does not exist at all is still just empty.
    let missing = ModelStore::new(dir.join("never-created"));
    assert_eq!(missing.remove(&id), Ok(Removal::Absent));
    assert!(missing.list().is_empty());

    // Install, list, remove, and the second remove is Absent again.
    let server = Server::start();
    let body = common::body();
    server.route(
        "huggingface.co",
        &common::resolve_path(),
        Answer::Body(body.clone()),
    );
    let downloader = Downloader::new(store.clone(), server.fetch(), common::config());
    let path = downloader
        .pull(&common::entry(&body, TEMPLATE), &Quiet)
        .expect("installed");

    let listed = store.list();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, id);
    assert_eq!(listed[0].path, path);
    assert_eq!(listed[0].size_bytes, body.len() as u64);

    assert_eq!(store.remove(&id), Ok(Removal::Removed));
    assert!(!dir.join("tiny").exists(), "the whole directory goes");
    assert!(store.list().is_empty());
    assert_eq!(store.remove(&id), Ok(Removal::Absent));

    // An id that is not a plain name is refused, not resolved against the filesystem.
    assert!(store.remove(&ModelId("../elsewhere".to_owned())).is_err());
}

/// A half-finished download is not an installed model.
#[test]
fn a_part_file_is_not_listed_as_installed() {
    let dir = scratch("part");
    std::fs::create_dir_all(dir.join("tiny")).expect("dir");
    std::fs::write(dir.join("tiny/tiny.gguf.part"), b"half").expect("part");
    std::fs::write(dir.join("tiny/LICENSE"), b"licence").expect("licence");
    let store = ModelStore::new(&dir);
    assert!(store.list().is_empty(), "{:?}", store.list());
    assert_eq!(
        store.remove(&ModelId("tiny".to_owned())),
        Ok(oc_net::store::Removal::Removed),
        "remove still clears the leftovers"
    );
}
