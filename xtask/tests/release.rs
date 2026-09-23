//! PHASE 15 — packaging and release, asserted against the repository itself.
//!
//! One test binary for the whole phase: every `xtask` test executable links Typst and the whole
//! pipeline, so a file per row would cost the disk a few hundred megabytes each.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .expect("xtask has a parent directory")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

fn json(path: &Path) -> serde_json::Value {
    serde_json::from_str(&read(path)).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Every binary target the workspace builds, by the file name cargo gives it in
/// `target/<profile>/`: explicit `[[bin]]` tables, the implicit `src/main.rs` one, and
/// `src/bin/*.rs`.
fn workspace_binaries(root: &Path) -> BTreeSet<String> {
    let manifest: toml::Table = toml::from_str(&read(&root.join("Cargo.toml"))).expect("toml");
    let members = manifest["workspace"]["members"]
        .as_array()
        .expect("an explicit member list");
    let mut bins = BTreeSet::new();
    for member in members {
        let dir = root.join(member.as_str().expect("a path"));
        let package: toml::Table =
            toml::from_str(&read(&dir.join("Cargo.toml"))).expect("member toml");
        let name = package["package"]["name"].as_str().expect("a name");
        let explicit = package.get("bin").and_then(|b| b.as_array());
        for bin in explicit.into_iter().flatten() {
            bins.insert(bin["name"].as_str().expect("a bin name").to_owned());
        }
        if explicit.is_none() && dir.join("src/main.rs").is_file() {
            bins.insert(name.to_owned());
        }
        if let Ok(entries) = std::fs::read_dir(dir.join("src/bin")) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "rs") {
                    let stem = path.file_stem().and_then(|s| s.to_str()).expect("stem");
                    bins.insert(stem.to_owned());
                }
            }
        }
    }
    bins
}

/// The Tauri configuration, and every per-OS file Tauri merges over it.
fn tauri_configs(root: &Path) -> Vec<(String, serde_json::Value)> {
    let dir = root.join("apps/desktop/src-tauri");
    [
        "tauri.conf.json",
        "tauri.linux.conf.json",
        "tauri.macos.conf.json",
        "tauri.windows.conf.json",
    ]
    .into_iter()
    .filter(|name| dir.join(name).is_file())
    .map(|name| (name.to_owned(), json(&dir.join(name))))
    .collect()
}

/// Carry-over from PHASE 12: `tauri-build` copies every `externalBin` into `target/<profile>/`
/// under its name without the triple, beside everything cargo builds. A sidecar called
/// `openconvert` therefore overwrote the engine the workspace tests run with whatever build was
/// last staged, and cargo, seeing its own fingerprint fresh, never put the new one back. A
/// sidecar whose name no workspace binary has cannot do that, whatever order things are built in.
#[test]
fn no_sidecar_shares_a_name_with_a_workspace_binary() {
    let root = workspace_root();
    let bins = workspace_binaries(&root);
    assert!(
        bins.contains("openconvert"),
        "the engine binary is found: {bins:?}"
    );

    let mut sidecars = 0;
    for (file, config) in tauri_configs(&root) {
        let external = config["bundle"]["externalBin"].as_array();
        for entry in external.into_iter().flatten() {
            sidecars += 1;
            let path = entry.as_str().expect("a path");
            let name = Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .expect("a file name");
            assert!(
                !bins.contains(name),
                "{file}: the sidecar `{path}` is copied to target/<profile>/{name}, which is \
                 where cargo puts the workspace binary of the same name"
            );
        }
    }
    assert!(sidecars > 0, "the app bundles its engine as an externalBin");
}

/// RFC 7396, which is how Tauri merges `tauri.<os>.conf.json` over `tauri.conf.json`.
fn merge_patch(target: &mut serde_json::Value, patch: &serde_json::Value) {
    match (target, patch) {
        (serde_json::Value::Object(target), serde_json::Value::Object(patch)) => {
            for (key, value) in patch {
                if value.is_null() {
                    target.remove(key);
                } else {
                    merge_patch(
                        target.entry(key.clone()).or_insert(serde_json::Value::Null),
                        value,
                    );
                }
            }
        }
        (target, patch) => *target = patch.clone(),
    }
}

/// The configuration Tauri builds with on `os`.
fn merged_config(root: &Path, os: &str) -> serde_json::Value {
    let dir = root.join("apps/desktop/src-tauri");
    let mut config = json(&dir.join("tauri.conf.json"));
    let overlay = dir.join(format!("tauri.{os}.conf.json"));
    assert!(overlay.is_file(), "{} exists", overlay.display());
    merge_patch(&mut config, &json(&overlay));
    config
}

fn string_list(value: &serde_json::Value) -> Vec<&str> {
    value
        .as_array()
        .map(|a| a.iter().filter_map(serde_json::Value::as_str).collect())
        .unwrap_or_default()
}

/// PHASE 15 detail 1: one bundle shape on all three operating systems — the engine and
/// `llama-server` as `externalBin`, the native libraries beside them, `models.toml`,
/// `thresholds.toml` and the natives' licences as resources — and the installers D12 names.
/// Nothing that is a download (a model, a pack) is in any of it.
#[test]
fn the_bundle_layout_is_the_same_on_every_os() {
    let root = workspace_root();
    let native = format!(
        "{}/",
        Path::new("bin")
            .join(xtask::stage_sidecars::NATIVE_DIR)
            .display()
    );
    for (os, targets) in [
        ("linux", vec!["appimage"]),
        ("macos", vec!["app", "dmg"]),
        ("windows", vec!["nsis", "msi"]),
    ] {
        let config = merged_config(&root, os);
        let bundle = &config["bundle"];
        assert_eq!(string_list(&bundle["targets"]), targets, "{os}: installers");
        assert_eq!(
            string_list(&bundle["externalBin"]),
            [
                format!("bin/{}", xtask::stage_sidecars::SIDECAR_NAME),
                format!("bin/{}", xtask::stage_sidecars::SERVER_NAME),
            ],
            "{os}: the engine and the model server are the sidecars"
        );
        let resources = bundle["resources"].as_object().expect("a resource map");
        for (source, target) in [
            ("../../../models.toml", "models.toml"),
            ("../../../thresholds.toml", "thresholds.toml"),
            ("bin/licenses/", "licenses/"),
        ] {
            assert_eq!(
                resources.get(source).and_then(|v| v.as_str()),
                Some(target),
                "{os}: {source} is a resource"
            );
        }
        // Beside the sidecars, wherever the platform puts them.
        let beside = match os {
            "linux" => bundle["linux"]["appimage"]["files"]["/usr/bin/"].as_str(),
            "macos" => bundle["macOS"]["files"]["MacOS/"].as_str(),
            _ => resources
                .get(native.as_str())
                .and_then(|v| v.as_str())
                .map(|root| {
                    assert_eq!(root, "", "{os}: the resource root is the install directory");
                    native.as_str()
                }),
        };
        assert_eq!(
            beside,
            Some(native.as_str()),
            "{os}: natives beside the binaries"
        );

        let text = serde_json::to_string(bundle).expect("json");
        for download in [".gguf", "tessdata", "epubcheck", "jre"] {
            assert!(
                !text.contains(download),
                "{os}: {download} is a download, not bundled"
            );
        }
    }
}
