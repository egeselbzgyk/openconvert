//! Rows 9.1 and 9.2: the registry refuses anything the downloader could not pin.

use oc_net::registry::{ModelRegistry, RegistryError};

/// A registry entry with every field resolved. The hash and commit are synthetic: this file tests
/// the parser, not Hugging Face.
fn entry(revision: &str, sha256: &str) -> String {
    format!(
        r#"schema_version = 1
default = "m"

[[model]]
id            = "m"
tier          = "default"
display_name  = "M"
family        = "qwen3"
arch          = "dense"
license       = "Apache-2.0"
notice_text   = "M (c) someone, Apache-2.0."
repo          = "org/m-GGUF"
revision      = "{revision}"
file          = "m.gguf"
url_template  = "https://huggingface.co/{{repo}}/resolve/{{revision}}/{{file}}"
sha256        = "{sha256}"
size_bytes    = 10
context       = 8192
parallel      = 1
min_ram_bytes = 1
prompt_profile = "qwen3-chatml"
cache_reuse   = true
"#
    )
}

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

#[test]
fn registry_rejects_todo_placeholders() {
    let error = ModelRegistry::parse(&entry(COMMIT, "TODO_SHA256")).unwrap_err();
    assert_eq!(
        error,
        RegistryError::Unresolved {
            id: "m".to_owned(),
            field: "sha256"
        }
    );
    let error = ModelRegistry::parse(&entry("TODO_COMMIT_SHA", SHA)).unwrap_err();
    assert!(
        matches!(
            error,
            RegistryError::Unresolved {
                field: "revision",
                ..
            }
        ),
        "{error:?}"
    );
    assert!(ModelRegistry::parse(&entry(COMMIT, SHA)).is_ok());
}

#[test]
fn registry_rejects_non_commit_revision() {
    for revision in ["main", "v1.0", "0123456789abcdef", &COMMIT.to_uppercase()] {
        let error = ModelRegistry::parse(&entry(revision, SHA)).unwrap_err();
        assert_eq!(
            error,
            RegistryError::UnpinnedRevision {
                id: "m".to_owned(),
                revision: revision.to_owned()
            }
        );
    }
}

/// The registry that ships: every pin filled, so `ModelRegistry::load` accepts the file and the
/// engine compiled around it (`BUNDLED`) accepts the same text. D9 names the default.
#[test]
fn the_shipped_registry_loads_with_every_pin_filled() {
    use oc_net::download::resolve_url;
    use oc_net::registry::{is_commit, ModelId, BUNDLED};

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../models.toml");
    assert!(
        !BUNDLED.contains("TODO_"),
        "a placeholder is still in models.toml"
    );
    let loaded = ModelRegistry::load(&path).expect("the shipped models.toml loads");
    let bundled = ModelRegistry::parse(BUNDLED).expect("the compiled-in registry loads");
    assert_eq!(loaded.entries(), bundled.entries());

    assert_eq!(
        loaded.default_id(),
        &ModelId("tev1-4b-experimental.q4_k_m".to_owned())
    );
    let default = loaded
        .get(loaded.default_id())
        .expect("the default is an entry");
    assert_eq!(default.tier, "default");
    // The maintainer's pin (2026-09-26): the decision model the evaluation ran on, in the quant
    // it ran on.
    assert_eq!(default.repo, "prithivMLmods/Tev1-4B-experimental-GGUF");
    assert_eq!(default.file, "Tev1-4B-experimental.Q4_K_M.gguf");

    assert_eq!(loaded.entries().len(), 5);
    for entry in loaded.entries() {
        let id = &entry.id.0;
        assert!(is_commit(&entry.revision), "{id}: {}", entry.revision);
        assert!(
            entry.sha256.len() == 64
                && entry
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "{id}: {}",
            entry.sha256
        );
        assert!(entry.size_bytes > 0, "{id}: no size");
        assert!(
            entry.min_ram_bytes > entry.size_bytes,
            "{id}: the RAM estimate is below the file it loads"
        );
        let url = resolve_url(entry).expect("an allowlisted, pinned URL");
        assert_eq!(
            url,
            format!(
                "https://huggingface.co/{}/resolve/{}/{}",
                entry.repo, entry.revision, entry.file
            )
        );
    }
}

/// An id names the quantisation it downloads: `qwen3-0.6b-q8_0` is `Qwen3-0.6B-Q8_0.gguf`, and a
/// community quant says whose it is after that. A registry whose id and file disagree shows a user
/// one model and installs another.
#[test]
fn every_shipped_id_names_the_quantisation_it_downloads() {
    let registry = ModelRegistry::parse(oc_net::registry::BUNDLED).expect("the shipped registry");
    for entry in registry.entries() {
        let stem = entry
            .file
            .strip_suffix(".gguf")
            .expect("a GGUF")
            .to_ascii_lowercase();
        assert!(
            entry.id.0 == stem || entry.id.0.starts_with(&format!("{stem}-")),
            "`{}` downloads `{}`",
            entry.id.0,
            entry.file
        );
        let quant = stem.rsplit('-').next().expect("a quantisation suffix");
        assert!(
            entry.display_name.to_ascii_lowercase().contains(quant),
            "`{}` does not name {quant}",
            entry.display_name
        );
    }
}
