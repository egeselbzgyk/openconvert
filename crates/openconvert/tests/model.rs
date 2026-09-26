//! `openconvert model pull|list|remove` (IMPLEMENTATION_PLAN §2.1, PHASE 9 details 6–7): what a
//! first-run screen and a CLI user see of the model manager. Row 9.18 and the CLI half of 9.19 and
//! A9.2.
//!
//! These run the built binary against a registry written here, with synthetic pins, except
//! `model_list_works_on_the_shipped_registry`, which reads the `models.toml` compiled in. Nothing
//! here downloads a real model.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_openconvert"))
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("openconvert-model-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn registry(dir: &Path, template: &str) -> PathBuf {
    let text = format!(
        r#"schema_version = 1
default = "tiny-default"

[[model]]
id             = "tiny-default"
tier           = "default"
display_name   = "Tiny default"
family         = "qwen3"
arch           = "dense"
license        = "Apache-2.0"
notice_text    = "Tiny (c) nobody, Apache-2.0."
repo           = "org/tiny-GGUF"
revision       = "{COMMIT}"
file           = "tiny.gguf"
url_template   = "{template}"
sha256         = "{SHA}"
size_bytes     = 1000
context        = 8192
parallel       = 1
min_ram_bytes  = 3221225472
cpu_expectation = "moderate"
prompt_profile = "qwen3-chatml"
cache_reuse    = true

[[model]]
id             = "tiny-experimental"
tier           = "experimental"
display_name   = "Tiny experimental"
family         = "qwen3.5"
arch           = "hybrid-recurrent-moe"
license        = "Apache-2.0"
repo           = "org/tiny2-GGUF"
revision       = "{COMMIT}"
file           = "tiny2.gguf"
sha256         = "{SHA}"
size_bytes     = 2000
context        = 8192
parallel       = 1
min_ram_bytes  = 1610612736
cpu_expectation = "not yet measured on the reference machines"
prompt_profile = "qwen3-chatml"
cache_reuse    = false
context_checkpoints = 32
warn           = "Experimental."
"#
    );
    let path = dir.join("models.toml");
    std::fs::write(&path, text).expect("a registry");
    path
}

fn model(args: &[&str]) -> Output {
    Command::new(binary())
        .arg("model")
        .args(args)
        .output()
        .expect("the binary runs")
}

/// Row 9.18: the first-run fields, as the GUI will read them.
#[test]
fn model_list_json_shape() {
    let dir = scratch("list");
    let registry = registry(
        &dir,
        "https://huggingface.co/{repo}/resolve/{revision}/{file}",
    );
    let store = dir.join("store");
    // One model installed: its file and licence are there.
    std::fs::create_dir_all(store.join("tiny-default")).expect("store");
    std::fs::write(store.join("tiny-default/tiny.gguf"), [0u8; 1000]).expect("model");
    std::fs::write(store.join("tiny-default/LICENSE"), "Apache License").expect("licence");
    std::fs::write(store.join("tiny-default/NOTICE"), "Tiny").expect("notice");

    let output = model(&[
        "list",
        "--json",
        "--registry",
        &registry.to_string_lossy(),
        "--dir",
        &store.to_string_lossy(),
    ]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).expect("UTF-8");
    let value: serde_json::Value = serde_json::from_str(&text).expect("JSON on stdout");
    let license_path = value[0]["license_path"]
        .as_str()
        .expect("an installed licence");
    assert_eq!(Path::new(license_path), store.join("tiny-default/LICENSE"));
    insta::assert_json_snapshot!(value, {
        "[0].license_path" => "[store]/tiny-default/LICENSE",
    });
}

/// The CLI half of row 9.19: removing what is not there says so and exits 0.
#[test]
fn model_remove_absent_exits_zero_with_a_message() {
    let dir = scratch("remove");
    let registry = registry(
        &dir,
        "https://huggingface.co/{repo}/resolve/{revision}/{file}",
    );
    let store = dir.join("store");
    let args = [
        "remove",
        "tiny-default",
        "--registry",
        &registry.to_string_lossy(),
        "--dir",
        &store.to_string_lossy(),
    ];
    for _ in 0..2 {
        let output = model(&args);
        assert_eq!(output.status.code(), Some(0));
        assert!(String::from_utf8_lossy(&output.stderr).contains("not installed"));
    }
    std::fs::create_dir_all(store.join("tiny-default")).expect("store");
    std::fs::write(store.join("tiny-default/tiny.gguf"), [0u8; 1000]).expect("model");
    let output = model(&args);
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stderr).contains("removed"));
    assert!(!store.join("tiny-default").exists());
}

/// A9.2 at the CLI: a registry pointing off the allowlist is refused before any socket, exit 1.
#[test]
fn model_pull_refuses_a_host_off_the_allowlist() {
    let dir = scratch("pull");
    let registry = registry(&dir, "https://example.com/{repo}/resolve/{revision}/{file}");
    let store = dir.join("store");
    let output = model(&[
        "pull",
        "tiny-default",
        "--registry",
        &registry.to_string_lossy(),
        "--dir",
        &store.to_string_lossy(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("example.com") && stderr.contains("allowlist"),
        "{stderr}"
    );
    assert!(!store.join("tiny-default").exists());
}

/// A registry with a placeholder is a configuration error, exit 2, whatever the subcommand. The
/// shipped registry is taken with one pin put back to a placeholder, so the refusal is exercised on
/// the real file's shape and never reaches the network.
#[test]
fn an_unresolved_registry_is_a_usage_error() {
    let dir = scratch("unresolved");
    let registry = dir.join("models.toml");
    let shipped = std::fs::read_to_string(super_registry()).expect("the bundled registry");
    let sha = shipped
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once('=')?;
            let quoted = value.split('"').nth(1)?;
            (key.trim() == "sha256").then(|| quoted.to_owned())
        })
        .expect("a sha256 line");
    assert!(!sha.starts_with("TODO_"), "the shipped registry is pinned");
    std::fs::write(&registry, shipped.replacen(&sha, "TODO_SHA256", 1)).expect("copy");
    for sub in ["list", "pull"] {
        let mut args = vec![sub];
        if sub == "pull" {
            args.push("qwen3-1.7b-q4_k_m");
        }
        let registry = registry.to_string_lossy();
        let store = dir.join("store");
        let store = store.to_string_lossy();
        args.extend(["--registry", &registry, "--dir", &store]);
        let output = model(&args);
        assert_eq!(output.status.code(), Some(2), "{sub}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("placeholder"));
    }
}

/// The registry compiled into the engine loads: `model list` answers with every entry, the
/// default marked, nothing installed in an empty store. The snapshot is the shipped pins' sizes,
/// RAM estimates and licences as a first-run screen reads them.
#[test]
fn model_list_works_on_the_shipped_registry() {
    let dir = scratch("shipped");
    let store = dir.join("store");
    let output = model(&["list", "--json", "--dir", &store.to_string_lossy()]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON on stdout");
    let rows = value.as_array().expect("one row per entry");
    let defaults: Vec<&str> = rows
        .iter()
        .filter(|row| row["is_default"] == true)
        .filter_map(|row| row["id"].as_str())
        .collect();
    assert_eq!(defaults, ["tev1-4b-experimental.q4_k_m"]);
    assert!(rows.iter().all(|row| row["installed"] == false));
    insta::assert_json_snapshot!(value);

    let human = model(&["list", "--dir", &store.to_string_lossy()]);
    assert_eq!(human.status.code(), Some(0));
    assert!(
        String::from_utf8_lossy(&human.stdout).contains("tev1-4b-experimental.q4_k_m (default)")
    );
}

fn super_registry() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models.toml")
}

/// An id the registry does not have is a usage error that names it.
#[test]
fn an_unknown_model_id_is_a_usage_error() {
    let dir = scratch("unknown");
    let registry = registry(
        &dir,
        "https://huggingface.co/{repo}/resolve/{revision}/{file}",
    );
    let output = model(&[
        "pull",
        "no-such-model",
        "--registry",
        &registry.to_string_lossy(),
        "--dir",
        &dir.join("store").to_string_lossy(),
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("no-such-model"));
}

/// Maintainer decision 2026-09-23 (D6 amendment): the validation pack is deferred past v1.0.
/// Asking for it by name is refused as a usage error that says it is not offered in 1.0 and comes
/// later — never "unknown", and nothing is downloaded or written.
#[test]
fn the_validation_pack_is_refused_with_a_clear_message() {
    let dir = scratch("validation");
    let store = dir.join("store");
    let output = model(&[
        "pull",
        "validation",
        "--progress",
        "json",
        "--dir",
        &store.to_string_lossy(),
    ]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E_PACK_NOT_OFFERED"), "{stderr}");
    assert!(
        stderr.contains("does not offer the validation pack"),
        "{stderr}"
    );
    assert!(stderr.contains("later release"), "{stderr}");
    assert!(!store.join("validation").exists(), "nothing was installed");

    let human = model(&["pull", "validation", "--dir", &store.to_string_lossy()]);
    assert_eq!(human.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&human.stderr).contains("arrives in a later release"));
}
