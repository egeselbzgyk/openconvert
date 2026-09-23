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

/// Every `<key>` in a property list, in document order.
fn plist_keys(text: &str) -> Vec<String> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(text);
    let mut keys = Vec::new();
    let mut in_key = false;
    let mut saw_plist_dict = false;
    loop {
        match reader.read_event().expect("well-formed XML") {
            Event::Start(tag) if tag.local_name().as_ref() == "key" => in_key = true,
            Event::End(tag) if tag.local_name().as_ref() == "key" => in_key = false,
            Event::Start(tag) if tag.local_name().as_ref() == "dict" => saw_plist_dict = true,
            Event::Text(text) if in_key => {
                let text: &str = text.as_ref();
                keys.push(text.trim().to_owned());
            }
            Event::Eof => break,
            _ => {}
        }
    }
    assert!(saw_plist_dict, "the plist's root is a dictionary");
    keys
}

/// Row 15.5 (SECURITY §5): the outer app's entitlements never weaken library validation or allow
/// JIT, nor anything else that would let unsigned or injected code into the process tree that
/// parses untrusted PDFs. And the macOS bundle config applies exactly this file.
#[test]
fn entitlements_do_not_disable_library_validation() {
    let root = workspace_root();
    let path = root.join("packaging/macos/entitlements.plist");
    let keys = plist_keys(&read(&path));
    for banned in [
        "com.apple.security.cs.disable-library-validation",
        "com.apple.security.cs.allow-jit",
        "com.apple.security.cs.allow-unsigned-executable-memory",
        "com.apple.security.cs.allow-dyld-environment-variables",
        "com.apple.security.cs.disable-executable-page-protection",
        "com.apple.security.get-task-allow",
    ] {
        assert!(
            !keys.iter().any(|k| k == banned),
            "{banned} is granted: {keys:?}"
        );
    }

    let config = merged_config(&root, "macos");
    let mac = &config["bundle"]["macOS"];
    assert_eq!(
        mac["entitlements"].as_str(),
        Some("../../../packaging/macos/entitlements.plist")
    );
    assert_ne!(mac["hardenedRuntime"], serde_json::Value::Bool(false));
    // The file the config names is the file this test read.
    let named = root
        .join("apps/desktop/src-tauri")
        .join(mac["entitlements"].as_str().expect("a path"));
    assert_eq!(
        std::fs::canonicalize(named).expect("exists"),
        std::fs::canonicalize(&path).expect("exists")
    );
}

/// A minimal 64-bit Mach-O header — enough for `file` to call it one — of `filetype` 2
/// (`MH_EXECUTE`) or 6 (`MH_DYLIB`).
#[cfg(unix)]
fn fake_macho(filetype: u8) -> Vec<u8> {
    let mut bytes = vec![
        0xcf, 0xfa, 0xed, 0xfe, 0x07, 0x00, 0x00, 0x01, 0x03, 0, 0, 0,
    ];
    bytes.extend([filetype, 0, 0, 0]);
    bytes.extend([0u8; 16]);
    bytes
}

/// PHASE 15 detail 2 and the Failure-modes paragraph: `sign_nested.sh` finds every Mach-O by
/// looking (a dylib without the executable bit included), signs libraries before executables and
/// the app last, each with the Hardened Runtime, a timestamp and `--force`, and gives entitlements
/// to the app alone. Run on Linux with `codesign` replaced by a recorder; the real signature and
/// its verification are rows 15.1–15.4, on macOS in `release.yml`.
#[cfg(unix)]
#[test]
fn sign_nested_signs_every_macho_inside_out() {
    use std::os::unix::fs::PermissionsExt;

    let root = workspace_root();
    let scratch = std::env::temp_dir().join(format!("oc-sign-nested-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    let app = scratch.join("OpenConvert.app");
    let macos = app.join("Contents/MacOS");
    let deep = app.join("Contents/Frameworks/Helper.framework/Versions/A");
    std::fs::create_dir_all(&macos).expect("dirs");
    std::fs::create_dir_all(&deep).expect("dirs");
    std::fs::create_dir_all(app.join("Contents/Resources")).expect("dirs");
    std::fs::write(
        app.join("Contents/Info.plist"),
        "<plist><dict>\n<key>CFBundleExecutable</key>\n<string>OpenConvert</string>\n</dict></plist>\n",
    )
    .expect("plist");
    let write = |path: PathBuf, bytes: Vec<u8>, mode: u32| {
        std::fs::write(&path, bytes).expect("write");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).expect("mode");
    };
    write(macos.join("OpenConvert"), fake_macho(2), 0o755);
    write(macos.join("openconvert-engine"), fake_macho(2), 0o755);
    write(macos.join("llama-server"), fake_macho(2), 0o755);
    // Not executable, as PDFium ships it: a `-perm +111` search would miss it.
    write(macos.join("libpdfium.dylib"), fake_macho(6), 0o644);
    write(macos.join("libllama.0.1.0.dylib"), fake_macho(6), 0o755);
    write(deep.join("Helper"), fake_macho(6), 0o755);
    std::os::unix::fs::symlink("libllama.0.1.0.dylib", macos.join("libllama.0.dylib"))
        .expect("link");
    write(
        app.join("Contents/Resources/models.toml"),
        b"x = 1\n".to_vec(),
        0o644,
    );

    let log = scratch.join("codesign.log");
    let stub = scratch.join("codesign");
    write(
        stub.clone(),
        format!("#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\n", log.display()).into_bytes(),
        0o755,
    );

    let status = std::process::Command::new("sh")
        .arg(root.join("packaging/macos/sign_nested.sh"))
        .arg(&app)
        .env(
            "APPLE_SIGNING_IDENTITY",
            "Developer ID Application: Test (TEAMID1234)",
        )
        .env("CODESIGN", &stub)
        .status()
        .expect("sh runs");
    assert!(status.success());

    let calls: Vec<String> = read(&log).lines().map(str::to_owned).collect();
    let signed: Vec<String> = calls
        .iter()
        .map(|c| c.rsplit(' ').next().expect("a path").to_owned())
        .collect();
    let name = |p: &str| {
        Path::new(p)
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_owned)
    };
    let names: Vec<Option<String>> = signed.iter().map(|s| name(s)).collect();
    assert_eq!(
        names,
        [
            Some("Helper".to_owned()),
            Some("libllama.0.1.0.dylib".to_owned()),
            Some("libpdfium.dylib".to_owned()),
            Some("llama-server".to_owned()),
            Some("openconvert-engine".to_owned()),
            Some("OpenConvert.app".to_owned()),
        ],
        "libraries deepest first, then executables, then the app: {calls:#?}"
    );
    for call in &calls {
        assert!(
            call.starts_with("--force --options runtime --timestamp"),
            "{call}"
        );
        assert!(call.contains("--sign Developer ID Application: Test (TEAMID1234)"));
        assert_eq!(
            call.contains("--entitlements"),
            call.ends_with("OpenConvert.app"),
            "entitlements on the app alone: {call}"
        );
    }
    let _ = std::fs::remove_dir_all(&scratch);
}
