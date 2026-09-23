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

/// A bundle directory laid out the way the Tauri bundler writes one, with `files` in it.
fn fake_bundle(name: &str, files: &[(&str, &[u8])]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("oc-bundle-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (path, bytes) in files {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("dirs");
        std::fs::write(path, bytes).expect("write");
    }
    dir
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    format!("{:x}", sha2::Sha256::digest(bytes))
}

/// The logic under row 15.6: every installer D12 promises for an OS must be in the bundle, and each
/// one — and each updater signature beside it — gets its SHA-256 in the manifest.
#[test]
fn release_manifest_requires_every_declared_installer() {
    use xtask::release::{collect, Installer, Os};

    let only_nsis = fake_bundle(
        "nsis-only",
        &[("nsis/OpenConvert_1.0.0_x64-setup.exe", b"nsis")],
    );
    let error = collect(&only_nsis, Os::Windows).expect_err("no MSI");
    assert!(format!("{error:#}").contains("Msi"), "{error:#}");

    let both = fake_bundle(
        "windows",
        &[
            ("nsis/OpenConvert_1.0.0_x64-setup.exe", b"nsis"),
            ("nsis/OpenConvert_1.0.0_x64-setup.exe.sig", b"sig"),
            ("msi/OpenConvert_1.0.0_x64_en-US.msi", b"msi"),
            ("msi/unrelated.log", b"not an artefact"),
        ],
    );
    let release = collect(&both, Os::Windows).expect("both installers");
    let listed: Vec<(&str, Installer, &str)> = release
        .files
        .iter()
        .map(|f| (f.name.as_str(), f.kind, f.sha256.as_str()))
        .collect();
    assert_eq!(
        listed,
        [
            (
                "OpenConvert_1.0.0_x64-setup.exe",
                Installer::Nsis,
                sha256_hex(b"nsis").as_str()
            ),
            (
                "OpenConvert_1.0.0_x64-setup.exe.sig",
                Installer::Signature,
                sha256_hex(b"sig").as_str()
            ),
            (
                "OpenConvert_1.0.0_x64_en-US.msi",
                Installer::Msi,
                sha256_hex(b"msi").as_str()
            ),
        ]
    );
    let notes = xtask::release::hashes_section(&[release]);
    assert!(notes.contains(&format!(
        "{}  OpenConvert_1.0.0_x64_en-US.msi",
        sha256_hex(b"msi")
    )));
    let _ = std::fs::remove_dir_all(only_nsis);
    let _ = std::fs::remove_dir_all(both);
}

/// The logic under row 15.20: an asset is published only when its own SHA-256 sits next to its
/// own name in the release body; a hash of another file, or a name alone, does not count.
#[test]
fn a_release_body_missing_one_hash_is_refused() {
    use xtask::release::unpublished;

    let assets = vec![
        (
            "OpenConvert_1.0.0_amd64.AppImage".to_owned(),
            sha256_hex(b"a"),
        ),
        ("OpenConvert_1.0.0_aarch64.dmg".to_owned(), sha256_hex(b"b")),
        ("sbom.cdx.json".to_owned(), sha256_hex(b"c")),
    ];
    let body = format!(
        "## SHA-256\n\n```\n{}  OpenConvert_1.0.0_amd64.AppImage\n{}  OpenConvert_1.0.0_aarch64.dmg\n\
         {}  sbom.cdx.json\n```\n",
        sha256_hex(b"a"),
        sha256_hex(b"b"),
        sha256_hex(b"c")
    );
    assert!(unpublished(&body, &assets).is_empty());

    let wrong_hash = body.replace(&sha256_hex(b"b"), &sha256_hex(b"x"));
    assert_eq!(
        unpublished(&wrong_hash, &assets),
        ["OpenConvert_1.0.0_aarch64.dmg"]
    );
    let name_only = body.replace(
        &format!("{}  sbom.cdx.json", sha256_hex(b"c")),
        "sbom.cdx.json",
    );
    assert_eq!(unpublished(&name_only, &assets), ["sbom.cdx.json"]);
}

/// The logic under row 15.15: the budget is per installer, from `thresholds.toml`, and the macOS
/// updater archive and every signature are not installers.
#[test]
fn the_installer_budget_counts_installers_only() {
    use xtask::release::{collect, installer_budget, over_budget, Os};

    assert_eq!(installer_budget(), 45_000_000, "D12's upper estimate");
    let bundle = fake_bundle(
        "macos",
        &[
            ("dmg/OpenConvert_1.0.0_aarch64.dmg", &[0u8; 40]),
            ("macos/OpenConvert.app.tar.gz", &[0u8; 90]),
            ("macos/OpenConvert.app.tar.gz.sig", &[0u8; 90]),
        ],
    );
    let release = collect(&bundle, Os::Macos).expect("a dmg");
    assert!(over_budget(&release, 50).is_empty(), "only the dmg counts");
    assert_eq!(
        over_budget(&release, 30),
        [("OpenConvert_1.0.0_aarch64.dmg".to_owned(), 40)]
    );
    let _ = std::fs::remove_dir_all(bundle);
}

/// `OC_BUNDLE_DIR` and friends for the `release-artifacts` gates: set by `release.yml`, and a
/// failure rather than a pass when unset, because a gate that passes on nothing is not a gate.
#[cfg(feature = "release-artifacts")]
fn required_env(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(name).unwrap_or_else(|| {
        panic!("{name} is not set; the release-artifacts gates need the release job's outputs")
    }))
}

/// Row 15.6, on the Windows release job: NSIS `.exe` and `.msi` both produced, both hashed.
#[cfg(all(feature = "release-artifacts", windows))]
#[test]
fn windows_installers_are_produced_and_hashed() {
    use xtask::release::{collect, hashes_section, Installer, Os};

    let release = collect(&required_env("OC_BUNDLE_DIR"), Os::Windows).expect("both installers");
    for kind in [Installer::Nsis, Installer::Msi] {
        let file = release
            .files
            .iter()
            .find(|f| f.kind == kind)
            .unwrap_or_else(|| panic!("no {kind:?}"));
        assert_eq!(file.sha256.len(), 64);
        assert!(hashes_section(std::slice::from_ref(&release))
            .contains(&format!("{}  {}", file.sha256, file.name)));
    }
}

/// Row 15.15 on whichever OS the release job runs it: every installer ≤ the D12 budget.
#[cfg(feature = "release-artifacts")]
#[test]
fn installer_size_within_budget() {
    use xtask::release::{collect, installer_budget, over_budget, Os};

    let release = collect(&required_env("OC_BUNDLE_DIR"), Os::host()).expect("installers");
    let budget = installer_budget();
    for file in release.files.iter().filter(|f| f.kind.is_installer()) {
        println!("{} bytes  {}  (budget {budget})", file.bytes, file.name);
    }
    assert_eq!(over_budget(&release, budget), [], "over the D12 budget");
}

/// Row 15.20 on the published release: `OC_RELEASE_BODY` is the release notes as published,
/// `OC_RELEASE_ASSETS` a directory holding every asset downloaded back from the release.
#[cfg(feature = "release-artifacts")]
#[test]
fn release_artifacts_all_have_published_hashes() {
    use xtask::release::{sha256_file, unpublished};

    let body = read(&required_env("OC_RELEASE_BODY"));
    let dir = required_env("OC_RELEASE_ASSETS");
    let mut assets = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("the assets directory") {
        let path = entry.expect("an entry").path();
        let name = path.file_name().and_then(|n| n.to_str()).expect("a name");
        assets.push((name.to_owned(), sha256_file(&path).expect("hashed")));
    }
    assert!(!assets.is_empty(), "no assets were downloaded");
    assert_eq!(unpublished(&body, &assets), Vec::<String>::new());
}

/// The one AppImage under `OC_BUNDLE_DIR`.
#[cfg(all(feature = "release-artifacts", target_os = "linux"))]
fn the_appimage() -> PathBuf {
    let release =
        xtask::release::collect(&required_env("OC_BUNDLE_DIR"), xtask::release::Os::Linux)
            .expect("an AppImage");
    let found: Vec<PathBuf> = release
        .files
        .iter()
        .filter(|f| f.kind == xtask::release::Installer::AppImage)
        .map(|f| f.path.clone())
        .collect();
    assert_eq!(found.len(), 1, "exactly one AppImage: {found:?}");
    found[0].clone()
}

/// A scratch directory standing in for a user's machine: its own HOME and XDG directories, so the
/// app's settings, jobs and caches go nowhere near the runner's.
#[cfg(all(feature = "release-artifacts", target_os = "linux"))]
fn clean_home(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("oc-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("home")).expect("home");
    dir
}

/// Row 15.7: the AppImage itself — not the build tree — launches under `xvfb-run` on a machine with
/// no display, starts its bundled engine, and turns a PDF into an EPUB, exiting 0. It runs from a
/// directory that holds no `vendor/`, with `OC_PDFIUM_PATH` unset, so the engine can only have
/// found PDFium where the bundle put it.
#[cfg(all(feature = "release-artifacts", target_os = "linux"))]
#[test]
fn appimage_launches_and_converts_headless() {
    let appimage = the_appimage();
    let scratch = clean_home("appimage-smoke");
    let pdf = scratch.join("book.pdf");
    std::fs::copy(
        workspace_root().join("target/fixtures/f01_prose_single_column.pdf"),
        &pdf,
    )
    .expect("the f01 fixture (cargo run -p xtask -- fixtures)");

    let home = scratch.join("home");
    let output = std::process::Command::new("xvfb-run")
        .args(["-a"])
        .arg(&appimage)
        .arg("--smoke-convert")
        .arg(&pdf)
        .current_dir(&scratch)
        // No FUSE on CI runners or in containers: the AppImage runtime unpacks itself instead.
        .env("APPIMAGE_EXTRACT_AND_RUN", "1")
        .env("HOME", &home)
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env_remove("OC_PDFIUM_PATH")
        .output()
        .expect("xvfb-run runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "the AppImage exited {:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status
    );
    let epub = std::fs::read(scratch.join("book.epub")).expect("book.epub beside the PDF");
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(epub)).expect("a zip");
    let mut mimetype = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("mimetype").expect("a mimetype entry"),
        &mut mimetype,
    )
    .expect("utf-8");
    assert_eq!(mimetype, "application/epub+zip");
    let _ = std::fs::remove_dir_all(scratch);
}

/// The AppImage carries PHASE 15 detail 1's layout: both sidecars with their natives beside them in
/// `usr/bin`, the registry and thresholds and the natives' licences in the resource directory — and
/// no model. The bundled `llama-server` answers `--version`, which it can only do when every
/// library it links resolved from inside the bundle.
#[cfg(all(feature = "release-artifacts", target_os = "linux"))]
#[test]
fn appimage_carries_the_bundle_layout() {
    let appimage = the_appimage();
    let scratch = clean_home("appimage-layout");
    let status = std::process::Command::new(&appimage)
        .arg("--appimage-extract")
        .current_dir(&scratch)
        .stdout(std::process::Stdio::null())
        .status()
        .expect("the AppImage runs");
    assert!(status.success());
    let root = scratch.join("squashfs-root");
    for file in [
        "usr/bin/openconvert-engine",
        "usr/bin/llama-server",
        "usr/bin/libpdfium.so",
        "usr/bin/libllama-server-impl.so",
        "usr/bin/libggml-cpu-x64.so",
        "usr/lib/OpenConvert/models.toml",
        "usr/lib/OpenConvert/thresholds.toml",
        "usr/lib/OpenConvert/licenses/pdfium.LICENSE.txt",
        "usr/lib/OpenConvert/licenses/llama.cpp.LICENSE.txt",
    ] {
        assert!(root.join(file).exists(), "{file} is in the AppImage");
    }
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("a directory").flatten() {
            let path = entry.path();
            let name = path.to_string_lossy().to_string();
            let file = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            assert!(!name.ends_with(".gguf"), "a model is bundled: {name}");
            assert!(
                !file.starts_with("libggml-rpc") && !file.starts_with("ggml-rpc"),
                "the RPC backend is bundled: {name}"
            );
            if path.is_dir() && !path.is_symlink() {
                stack.push(path);
            }
        }
    }
    let version = std::process::Command::new(root.join("usr/bin/llama-server"))
        .arg("--version")
        .env_remove("LD_LIBRARY_PATH")
        .output()
        .expect("llama-server runs");
    let text = String::from_utf8_lossy(&version.stderr).to_string()
        + &String::from_utf8_lossy(&version.stdout);
    assert!(version.status.success(), "{text}");
    assert!(text.contains("build 10456"), "{text}");
    let _ = std::fs::remove_dir_all(scratch);
}

fn yaml(path: &Path) -> serde_yaml::Value {
    serde_yaml::from_str(&read(path)).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn yaml_strings(value: &serde_yaml::Value) -> Vec<String> {
    value
        .as_sequence()
        .map(|s| {
            s.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Row 15.8 (A15.8, detail 4): the Flatpak asks for no network, and builds the app without its
/// updater — the one crate feature the default build has and the Flatpak must not. Also: it is the
/// app the other builds are (same identifier), with the natives the locks pin.
#[test]
fn flatpak_manifest_has_no_network_finish_arg() {
    let root = workspace_root();
    let manifest = yaml(&root.join("packaging/linux/flatpak/io.openconvert.OpenConvert.yml"));

    let finish_args = yaml_strings(&manifest["finish-args"]);
    assert!(!finish_args.is_empty(), "the manifest has finish-args");
    for arg in &finish_args {
        assert!(
            !arg.starts_with("--share=network"),
            "the Flatpak asks for the network: {arg}"
        );
        // Nor anything that would let the sandbox reach outside itself some other way.
        assert!(!arg.starts_with("--filesystem=host") && !arg.starts_with("--filesystem=home"));
        assert!(!arg.contains("org.freedesktop.Flatpak"), "{arg}");
    }

    let config = json(&root.join("apps/desktop/src-tauri/tauri.conf.json"));
    assert_eq!(manifest["id"].as_str(), config["identifier"].as_str());

    // The desktop crate's default build has the updater; so the flag below is what removes it.
    let desktop: toml::Table =
        toml::from_str(&read(&root.join("apps/desktop/src-tauri/Cargo.toml"))).expect("toml");
    let default = desktop["features"]["default"].as_array().expect("defaults");
    assert!(default.iter().any(|f| f.as_str() == Some("updater")));

    let mut desktop_builds = 0;
    let mut pins = BTreeSet::new();
    for module in manifest["modules"].as_sequence().expect("modules") {
        for command in yaml_strings(&module["build-commands"]) {
            if command.contains("cargo") && command.contains("-p openconvert-desktop") {
                desktop_builds += 1;
                assert!(command.contains("--no-default-features"), "{command}");
                assert!(!command.contains("updater"), "{command}");
            }
        }
        for source in module["sources"].as_sequence().into_iter().flatten() {
            if let Some(sha) = source["sha256"].as_str() {
                pins.insert(sha.to_owned());
            }
        }
    }
    assert_eq!(
        desktop_builds, 1,
        "one build of the shell, without the updater"
    );

    let pdfium = xtask::vendor_pdfium::lock().expect("pdfium.lock");
    let pdfium_linux = pdfium
        .assets
        .iter()
        .find(|a| a.triple == "x86_64-unknown-linux-gnu")
        .expect("a Linux pin");
    let llama = xtask::fetch_llama_server::lock().expect("llama.lock");
    let llama_linux =
        xtask::fetch_llama_server::pinned(&llama, "x86_64-unknown-linux-gnu").expect("a Linux pin");
    assert_eq!(
        pins,
        BTreeSet::from([pdfium_linux.sha256.clone(), llama_linux.sha256.clone()]),
        "the Flatpak's natives are the ones the locks pin"
    );

    // The desktop entry and the AppStream data name the same app.
    let metainfo = read(&root.join("packaging/linux/io.openconvert.OpenConvert.metainfo.xml"));
    assert!(metainfo.contains("<id>io.openconvert.OpenConvert</id>"));
    assert!(metainfo.contains("<project_license>Apache-2.0</project_license>"));
    let entry = read(&root.join("packaging/linux/openconvert.desktop"));
    assert!(entry.contains("Icon=io.openconvert.OpenConvert"));
}

/// Two small inputs in the shape `cargo cyclonedx` and `npm sbom` write, rooted at `root`.
fn tiny_inputs(root: &str) -> (Vec<serde_json::Value>, serde_json::Value) {
    let engine = serde_json::json!({
        "bomFormat": "CycloneDX", "specVersion": "1.5", "version": 1,
        "serialNumber": "urn:uuid:749b3ca8-c7ae-40fa-b2ad-32f364d6f6cd",
        "metadata": {
            "timestamp": "2026-09-23T06:46:05Z",
            "component": {
                "type": "application", "name": "openconvert", "version": "0.1.0",
                "bom-ref": format!("path+file://{root}/crates/openconvert#0.1.0"),
                "components": [{ "type": "application", "name": "openconvert",
                                 "bom-ref": "bin-target-1" }],
            },
        },
        "components": [{
            "type": "library", "name": "serde", "version": "1.0.229",
            "bom-ref": "registry+https://github.com/rust-lang/crates.io-index#serde@1.0.229",
            "purl": "pkg:cargo/serde@1.0.229",
            "licenses": [{ "expression": "MIT OR Apache-2.0" }],
        }],
        "dependencies": [{
            "ref": format!("path+file://{root}/crates/openconvert#0.1.0"),
            "dependsOn": ["registry+https://github.com/rust-lang/crates.io-index#serde@1.0.229"],
        }],
    });
    let ui = serde_json::json!({
        "bomFormat": "CycloneDX", "specVersion": "1.5", "version": 1,
        "metadata": { "component": { "type": "library", "name": "ui", "version": "0.1.0",
                                     "bom-ref": "openconvert-ui@0.1.0" } },
        "components": [{ "type": "library", "name": "svelte", "version": "5.57.1",
                         "bom-ref": "svelte@5.57.1", "scope": "optional" }],
        "dependencies": [{ "ref": "openconvert-ui@0.1.0", "dependsOn": ["svelte@5.57.1"] }],
    });
    (vec![engine], ui)
}

/// The merge logic under row 15.11: one CycloneDX 1.6 document that validates offline against the
/// vendored schema, with no build-machine path left in it, no timestamp, and the same serial
/// number every time.
#[test]
fn the_merged_sbom_validates_offline_and_names_no_build_path() {
    use xtask::sbom::{merge, natives, validate, SCHEMA_DIR};

    let root = workspace_root();
    let build_root = "/home/builder/checkout";
    let (cargo, npm) = tiny_inputs(build_root);
    let natives = natives(&root).expect("the locks");
    let bom = merge(&cargo, &npm, &natives, "1.0.0", build_root);
    assert_eq!(bom["specVersion"], "1.6");
    let errors = validate(&bom, &root.join(SCHEMA_DIR)).expect("the schema compiles");
    assert!(errors.is_empty(), "{errors:#?}");

    let text = serde_json::to_string(&bom).expect("json");
    assert!(!text.contains(build_root), "a build path leaked");
    assert!(!text.contains("timestamp"));
    assert_eq!(bom, merge(&cargo, &npm, &natives, "1.0.0", build_root));
    let refs: Vec<&str> = bom["components"]
        .as_array()
        .expect("components")
        .iter()
        .filter_map(|c| c["bom-ref"].as_str())
        .collect();
    let mut sorted = refs.clone();
    sorted.sort_unstable();
    assert_eq!(refs, sorted, "components in a fixed order");
    assert!(refs.contains(&"path+file://./crates/openconvert#0.1.0"));
    assert!(refs.contains(&"svelte@5.57.1"));
}

/// The validator is not a formality: a document the schema forbids is reported, offline.
#[test]
fn the_sbom_schema_check_rejects_an_invalid_document() {
    use xtask::sbom::{validate, SCHEMA_DIR};

    let bad = serde_json::json!({
        "bomFormat": "CycloneDX", "specVersion": "1.6", "version": 1,
        "components": [{ "type": "banana", "name": "x",
                         "hashes": [{ "alg": "SHA-256", "content": "not hex" }] }],
    });
    let errors = validate(&bad, &workspace_root().join(SCHEMA_DIR)).expect("the schema compiles");
    assert!(errors.len() >= 2, "{errors:#?}");
}

/// Rows 15.12's natives, from the locks: PDFium and `llama-server` for every shipped triple with a
/// version, a SHA-256 and a licence; a pack once it is really pinned, and not while it is a
/// placeholder (the v1 state: no pack ships).
#[test]
fn a_pinned_pack_joins_the_sbom_and_a_placeholder_does_not() {
    use xtask::sbom::{natives, SHIPPED_TRIPLES};

    let root = workspace_root();
    let listed = natives(&root).expect("the locks");
    for name in ["pdfium", "llama-server"] {
        for triple in SHIPPED_TRIPLES {
            let native = listed
                .iter()
                .find(|n| n.name == name && n.target == triple)
                .unwrap_or_else(|| panic!("{name} for {triple}"));
            assert_eq!(native.sha256.len(), 64);
            assert!(!native.version.is_empty() && !native.license.is_empty());
            assert!(native.source_url.starts_with("https://github.com/"));
        }
    }
    assert!(
        !listed.iter().any(|n| n.target == "any"),
        "no pack is pinned in v1"
    );

    let scratch = std::env::temp_dir().join(format!("oc-sbom-packs-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("dir");
    std::fs::write(
        scratch.join("packs.toml"),
        "schema_version = 1\n[[pack]]\nid = \"ocr\"\nlicense = \"Apache-2.0\"\nrepo = \"o/ocr\"\n\
         revision = \"0123456789abcdef0123456789abcdef01234567\"\nfile = \"ocr.zip\"\n\
         sha256 = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n",
    )
    .expect("write");
    let with_pack = natives(&scratch).expect("natives");
    assert!(with_pack
        .iter()
        .any(|n| n.name == "ocr" && n.license == "Apache-2.0"));
    let _ = std::fs::remove_dir_all(scratch);
}

/// Row 15.11 on the release's own SBOM (`OC_SBOM`, written by `xtask sbom` in the release job):
/// valid CycloneDX 1.6.
#[cfg(feature = "release-artifacts")]
#[test]
fn sbom_is_valid_cyclonedx_1_6() {
    use xtask::sbom::{validate, SCHEMA_DIR};

    let bom = json(&required_env("OC_SBOM"));
    assert_eq!(bom["specVersion"], "1.6");
    assert_eq!(bom["bomFormat"], "CycloneDX");
    let errors = validate(&bom, &workspace_root().join(SCHEMA_DIR)).expect("the schema compiles");
    assert!(errors.is_empty(), "{errors:#?}");
    // The two crates that ship and the UI are all in it, with their dependencies.
    let components = bom["components"].as_array().expect("components");
    for name in [
        "openconvert",
        "openconvert-desktop",
        "ui",
        "tauri",
        "pdfium-render",
    ] {
        assert!(
            components.iter().any(|c| c["name"] == name),
            "{name} is in the SBOM"
        );
    }
}

/// Row 15.12 on the release's own SBOM: every vendored native, for every shipped target, with its
/// version, SHA-256 and licence.
#[cfg(feature = "release-artifacts")]
#[test]
fn sbom_lists_every_vendored_native() {
    use xtask::sbom::SHIPPED_TRIPLES;

    let bom = json(&required_env("OC_SBOM"));
    let components = bom["components"].as_array().expect("components");
    for name in ["pdfium", "llama-server"] {
        for triple in SHIPPED_TRIPLES {
            let native = components
                .iter()
                .find(|c| c["bom-ref"] == format!("native:{name}@{triple}"))
                .unwrap_or_else(|| panic!("{name} for {triple} is not in the SBOM"));
            assert!(native["version"].as_str().is_some_and(|v| !v.is_empty()));
            let sha = native["hashes"][0]["content"].as_str().expect("a hash");
            assert_eq!(native["hashes"][0]["alg"], "SHA-256");
            assert_eq!(sha.len(), 64, "{name} {triple}");
            assert!(native["licenses"][0]["license"]["id"].is_string());
        }
    }
}
