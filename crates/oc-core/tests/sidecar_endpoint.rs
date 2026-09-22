//! Row 9.11: with `--llm-endpoint` the engine spawns nothing (PHASE 9 detail 3, RT C2).

use std::path::PathBuf;

use oc_core::sidecar::endpoint::LlmEndpoint;
use oc_core::sidecar::server::{read_key_file, SidecarError};
use oc_core::sidecar::supervise;
use secrecy::ExposeSecret;

fn key_file(name: &str, text: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("oc-core-{}-{name}", std::process::id()));
    std::fs::write(&path, text).expect("a key file");
    path
}

/// Row 9.11.
#[test]
fn external_endpoint_spawns_nothing() {
    let path = key_file("key", "sk-local-0123\n");
    let chosen = LlmEndpoint::choose(
        Some("http://127.0.0.1:11434".to_owned()),
        Some(path),
        || panic!("an external endpoint must not start a server"),
    )
    .expect("the external endpoint");
    match chosen {
        LlmEndpoint::External { url, api_key } => {
            assert_eq!(url, "http://127.0.0.1:11434");
            assert_eq!(
                api_key.as_ref().map(|key| key.expose_secret()),
                Some("sk-local-0123"),
                "read once, trailing newline not part of the key"
            );
        }
        LlmEndpoint::Owned(_) => panic!("spawned a server"),
    }
    assert_eq!(supervise::live_children(), 0, "zero child processes");

    // Without an endpoint, and only then, the engine starts its own.
    let mut asked = false;
    let error = LlmEndpoint::choose(None, None, || {
        asked = true;
        Err(SidecarError::Exited)
    })
    .expect_err("the stand-in start fails");
    assert!(asked);
    assert!(matches!(error, SidecarError::Exited));
}

#[test]
fn an_unreadable_or_empty_key_file_is_an_error() {
    assert!(matches!(
        read_key_file(&std::env::temp_dir().join("oc-core-no-such-key-file")),
        Err(SidecarError::KeyFile { .. })
    ));
    let empty = key_file("empty", "  \n");
    assert!(matches!(
        read_key_file(&empty),
        Err(SidecarError::KeyFile { .. })
    ));
}
