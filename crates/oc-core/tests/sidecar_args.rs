//! Rows 9.12 and 9.14: the `llama-server` command line.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use oc_core::sidecar::llama::{command, ServerSpec, API_KEY_ENV};
use oc_core::thresholds::T;
use secrecy::{ExposeSecret, SecretString};

fn spec(cache_reuse: bool, context_checkpoints: Option<u32>) -> ServerSpec {
    ServerSpec {
        model: PathBuf::from("/models/qwen3-1.7b-q4_k_m/Qwen3-1.7B-Q4_K_M.gguf"),
        context: 8192,
        threads: 4,
        cache_reuse,
        context_checkpoints,
    }
}

fn args(command: &std::process::Command) -> Vec<String> {
    command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
}

/// The value after `flag`, if the flag is there.
fn value_of<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

/// Row 9.12.
#[test]
fn api_key_never_appears_in_argv() {
    let key = SecretString::from("k3y-0123456789abcdef-per-run".to_owned());
    let command = command(Path::new("llama-server"), &spec(true, None), 43127, &key);

    let args = args(&command);
    for arg in &args {
        assert!(
            !arg.contains(key.expose_secret()),
            "the key is on the command line: {args:?}"
        );
    }
    assert!(!args.iter().any(|arg| arg == "--api-key"), "{args:?}");

    let envs: Vec<(&OsStr, Option<&OsStr>)> = command.get_envs().collect();
    assert_eq!(
        envs.iter()
            .find(|(name, _)| *name == OsStr::new(API_KEY_ENV))
            .and_then(|(_, value)| *value),
        Some(OsStr::new(key.expose_secret())),
        "the key travels in {API_KEY_ENV}"
    );

    // The rest of detail 2's verified flags, and nothing that binds beyond loopback.
    assert_eq!(value_of(&args, "--host"), Some("127.0.0.1"));
    assert_eq!(value_of(&args, "--port"), Some("43127"));
    assert_eq!(value_of(&args, "-np"), Some("1"));
    assert_eq!(value_of(&args, "-c"), Some("8192"));
    assert_eq!(value_of(&args, "-t"), Some("4"));
    assert_eq!(
        value_of(&args, "-m"),
        Some("/models/qwen3-1.7b-q4_k_m/Qwen3-1.7B-Q4_K_M.gguf")
    );
    assert_eq!(
        value_of(&args, "--chat-template-kwargs"),
        Some(r#"{"enable_thinking":false}"#)
    );
}

/// Row 9.14.
#[test]
fn cache_reuse_flag_follows_registry() {
    let key = SecretString::from("k".to_owned());
    let program = Path::new("llama-server");

    let dense = args(&command(program, &spec(true, None), 1, &key));
    assert_eq!(
        value_of(&dense, "--cache-reuse"),
        Some(T.llm.cache_reuse_min_chunk.to_string().as_str())
    );
    assert!(!dense.iter().any(|arg| arg == "--ctx-checkpoints"));

    // A hybrid recurrent entry: no KV shifting, checkpoints instead (RT A3). The flag is the
    // pinned server's own spelling: `--context-checkpoints` is refused as an unknown argument,
    // and the server exits before it is ever healthy (2026-09-26, llama.cpp b10456).
    let hybrid = args(&command(program, &spec(false, Some(32)), 1, &key));
    assert!(
        !hybrid.iter().any(|arg| arg == "--cache-reuse"),
        "cache_reuse = false never gets --cache-reuse: {hybrid:?}"
    );
    assert_eq!(value_of(&hybrid, "--ctx-checkpoints"), Some("32"));
    assert!(!hybrid.iter().any(|arg| arg == "--context-checkpoints"));

    // And the registry, not the family name, decides: a dense entry that says false gets none.
    let dense_off = args(&command(program, &spec(false, None), 1, &key));
    assert!(!dense_off.iter().any(|arg| arg.starts_with("--cache-reuse")));
    assert!(!dense_off
        .iter()
        .any(|arg| arg.starts_with("--ctx-checkpoints")));
}
