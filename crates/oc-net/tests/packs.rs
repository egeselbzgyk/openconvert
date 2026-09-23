//! Optional packs (LICENSE_AND_DEPENDENCIES §6, PHASE 12 detail 9): pinned like a model, and
//! installed by the same downloader — "one download mechanism, three payloads, all SHA-256
//! pinned".

mod common;

use common::{Answer, Quiet, Server, COMMIT};
use oc_net::download::{Artifact, Downloader};
use oc_net::packs::PackRegistry;
use oc_net::registry::RegistryError;
use oc_net::store::ModelStore;

fn pack_toml(sha256: &str, size: usize, license: &str) -> String {
    format!(
        r#"schema_version = 1

[[pack]]
id           = "validation"
display_name = "Validation pack"
contents     = "EPUBCheck and a minimal Java runtime"
license      = "{license}"
repo         = "org/packs"
revision     = "{COMMIT}"
file         = "validation.zip"
sha256       = "{sha256}"
size_bytes   = {size}
"#
    )
}

/// A pack entry is pinned the way a model entry is: a placeholder anywhere refuses the file.
#[test]
fn pack_registry_rejects_placeholders() {
    assert!(matches!(
        PackRegistry::parse(&pack_toml("TODO_SHA256", 0, "Apache-2.0")),
        Err(RegistryError::Unresolved {
            field: "sha256",
            ..
        })
    ));
    assert!(matches!(
        PackRegistry::parse(&pack_toml(&"a".repeat(64), 1, "TODO_LICENSE")),
        Err(RegistryError::Unresolved {
            field: "license",
            ..
        })
    ));
    let text = pack_toml(&"a".repeat(64), 1, "Apache-2.0").replace(COMMIT, "main");
    assert!(matches!(
        PackRegistry::parse(&text),
        Err(RegistryError::UnpinnedRevision { .. })
    ));

    // A deferred pack carries no pins at all, and a pack cannot be offered and deferred at once.
    let deferred = "[[deferred]]\nid = \"validation\"\ndisplay_name = \"V\"\ncontents = \"C\"\nreason = \"later\"\n";
    assert!(matches!(
        PackRegistry::parse(&format!("{deferred}sha256 = \"TODO_SHA256\"\n")),
        Err(RegistryError::Parse(_))
    ));
    assert!(matches!(
        PackRegistry::parse(&format!(
            "{}\n{deferred}",
            pack_toml(&"a".repeat(64), 1, "Apache-2.0")
        )),
        Err(RegistryError::Parse(_))
    ));
}

/// Maintainer decision 2026-09-23 (D6 amendment): the validation pack is deferred past v1.0. The
/// registry that ships parses, offers no pack, and names the validation pack as arriving later —
/// with no pin to fill, so the release gate (row 15.18) has nothing in `packs.toml` to refuse.
#[test]
fn the_validation_pack_is_deferred_past_1_0() {
    let registry = PackRegistry::parse(oc_net::packs::BUNDLED).expect("the shipped registry");
    assert!(registry.entries().is_empty(), "1.0 offers no pack");
    let id = oc_net::registry::ModelId("validation".to_owned());
    assert!(registry.get(&id).is_none());
    let later = registry.get_deferred(&id).expect("named as deferred");
    assert_eq!(later.display_name, "Validation pack");
    assert!(later.reason.contains("later release"), "{}", later.reason);
    assert!(!oc_net::packs::BUNDLED.contains("TODO_"));
}

/// A pack installs through the model downloader: verified while it streams, `LICENSE` and `NOTICE`
/// beside it, in its own directory of the packs store.
#[test]
fn a_pack_installs_through_the_model_downloader() {
    let server = Server::start();
    let dir = std::env::temp_dir().join(format!("oc-net-packs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let body = common::body();
    let registry = PackRegistry::parse(&pack_toml(
        &common::sha256_hex(&body),
        body.len(),
        "Apache-2.0",
    ))
    .expect("a pinned pack");
    server.route(
        "huggingface.co",
        &format!("/org/packs/resolve/{COMMIT}/validation.zip"),
        Answer::Body(body.clone()),
    );

    let downloader = Downloader::new(ModelStore::new(&dir), server.fetch(), common::config());
    let pack = registry.entries().first().expect("one pack");
    let path = downloader
        .pull_artifact(&Artifact::from(pack), &Quiet)
        .expect("installed");
    assert_eq!(path, dir.join("validation").join("validation.zip"));
    assert_eq!(std::fs::read(&path).expect("the pack"), body);
    assert!(dir.join("validation").join("LICENSE").is_file());
    let notice = std::fs::read_to_string(dir.join("validation").join("NOTICE")).expect("NOTICE");
    assert!(notice.contains(COMMIT), "{notice}");
}
