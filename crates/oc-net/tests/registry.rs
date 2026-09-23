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
