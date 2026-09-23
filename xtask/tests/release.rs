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
            ("../../../NOTICE", "NOTICE"),
            (
                "../../../licenses/third-party-rust.txt",
                "licenses/third-party-rust.txt",
            ),
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
        "usr/lib/OpenConvert/licenses/third-party-rust.txt",
        "usr/lib/OpenConvert/NOTICE",
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

/// An EPUB-shaped zip whose `OEBPS/ch1.xhtml` is `chapter`.
fn tiny_epub(chapter: &str) -> Vec<u8> {
    use std::io::Write;
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut out);
        let stored = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file("mimetype", stored).expect("entry");
        zip.write_all(b"application/epub+zip").expect("write");
        zip.start_file("OEBPS/ch1.xhtml", stored).expect("entry");
        zip.write_all(chapter.as_bytes()).expect("write");
        zip.finish().expect("finish");
    }
    out.into_inner()
}

/// The comparison under row 15.13: all three tables must agree, and a disagreement names the
/// fixture, the two operating systems, and the first differing zip entry with the byte offset in
/// it — not only "hashes differ".
#[test]
fn repro_check_names_the_first_differing_zip_entry() {
    use std::collections::BTreeMap;
    use xtask::release::Os;
    use xtask::repro::{first_difference, repro_check, ReproMismatch, ZipDifference};

    let same = tiny_epub("<p>one two three</p>");
    let other = tiny_epub("<p>one two thre\u{0065}\u{0301}</p>");
    assert_eq!(
        first_difference(&same, &other).expect("zips"),
        Some(ZipDifference {
            entry: "OEBPS/ch1.xhtml".to_owned(),
            offset: 16,
        })
    );
    assert_eq!(first_difference(&same, &same).expect("zips"), None);

    let scratch = std::env::temp_dir().join(format!("oc-repro-{}", std::process::id()));
    let mut epubs = BTreeMap::new();
    let mut tables = BTreeMap::new();
    for os in [Os::Linux, Os::Macos, Os::Windows] {
        let dir = scratch.join(format!("{os:?}"));
        std::fs::create_dir_all(&dir).expect("dir");
        let bytes = if os == Os::Windows { &other } else { &same };
        std::fs::write(dir.join("f01.epub"), bytes).expect("write");
        epubs.insert(os, dir);
        tables.insert(
            os,
            BTreeMap::from([
                ("f01".to_owned(), format!("sha256:{}", sha256_hex(bytes))),
                ("h11_pixel_bomb".to_owned(), "exit:1".to_owned()),
            ]),
        );
    }
    let error = repro_check(&tables, &epubs).expect_err("windows differs");
    let text = error.to_string();
    assert!(
        matches!(&error, ReproMismatch::Differs { fixture, right: Os::Windows, difference: Some(d), .. }
            if fixture == "f01" && d.entry == "OEBPS/ch1.xhtml" && d.offset == 16),
        "{text}"
    );
    assert!(
        text.contains("first difference in `OEBPS/ch1.xhtml` at byte 16"),
        "{text}"
    );

    // Agreement passes; two tables are not three; a fixture missing on one OS is a failure.
    let mut agree = tables.clone();
    let linux = agree[&Os::Linux].clone();
    agree.insert(Os::Windows, linux);
    assert_eq!(repro_check(&agree, &epubs), Ok(()));
    let mut two = agree.clone();
    two.remove(&Os::Macos);
    assert_eq!(repro_check(&two, &epubs), Err(ReproMismatch::MissingOs(2)));
    let mut short = agree.clone();
    short
        .get_mut(&Os::Macos)
        .expect("macos")
        .remove("h11_pixel_bomb");
    assert!(matches!(
        repro_check(&short, &epubs),
        Err(ReproMismatch::DifferentCorpus { .. })
    ));
    let _ = std::fs::remove_dir_all(scratch);
}

/// Row 15.13 in the release job: the three OS jobs' tables (and EPUBs) in `OC_REPRO_DIR` as
/// `<os>.json` and `<os>/`, byte-identical across ubuntu, macOS and Windows (D13.8).
#[cfg(feature = "release-artifacts")]
#[test]
fn reproducible_no_ai_output_across_os() {
    use std::collections::BTreeMap;
    use xtask::release::Os;
    use xtask::repro::{repro_check, HashTable};

    let dir = required_env("OC_REPRO_DIR");
    let mut tables = BTreeMap::new();
    let mut epubs = BTreeMap::new();
    for os in ["linux", "macos", "windows"] {
        let table: HashTable =
            serde_json::from_str(&read(&dir.join(format!("{os}.json")))).expect("a hash table");
        epubs.insert(Os::parse(os).expect("os"), dir.join(os));
        tables.insert(table.os, table.fixtures);
    }
    if let Err(error) = repro_check(&tables, &epubs) {
        panic!("{error} — a release blocker (D13.8)");
    }
}

/// A scratch copy of everything `bump-rules-check` reads, with a baseline recorded as if `tag` had
/// just been released from it.
fn rehearsal(name: &str, tag: &str) -> PathBuf {
    use xtask::versions::{
        baseline_text, copy_tree, declared_versions, digests, guarded_paths, Baseline, BASELINE,
    };
    let root = workspace_root();
    let copy = std::env::temp_dir().join(format!("oc-bump-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&copy);
    for path in guarded_paths() {
        if path != BASELINE {
            copy_tree(&root, &copy, path).expect("copied");
        }
    }
    let versions = declared_versions(&copy).expect("versions");
    let baseline = Baseline {
        released: tag.to_owned(),
        digests: digests(&copy, &versions).expect("digests"),
        versions,
    };
    let path = copy.join(BASELINE);
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("dir");
    std::fs::write(path, baseline_text(&baseline).expect("toml")).expect("written");
    copy
}

/// `bump-rules-check` on a rehearsal copy, as the release job runs it.
fn bump_check(copy: &Path) -> Result<(), Vec<xtask::versions::BumpViolation>> {
    use xtask::versions::{
        bump_rules_check, declared_versions, digests, job_spec_digest, Baseline, BASELINE,
    };
    let prev: Baseline = toml::from_str(&read(&copy.join(BASELINE))).expect("a baseline");
    let declared = declared_versions(copy).expect("versions");
    let now = digests(copy, &declared).expect("digests");
    let old = job_spec_digest(copy, prev.versions.job_spec_schema);
    bump_rules_check(&prev, &now, &declared, old.as_deref())
}

fn edit(copy: &Path, path: &str, from: &str, to: &str) {
    let file = copy.join(path);
    let text = read(&file);
    assert!(text.contains(from), "{path} contains {from:?}");
    std::fs::write(&file, text.replacen(from, to, 1)).expect("edited");
}

/// Row 15.16 (A15.7), rehearsed as the plan's Expected-behaviour paragraph asks: after a release,
/// an IR field added without bumping `ir_version` fails the release; so does a new `Reason`
/// variant; a comment, a doc line or a new test does not; bumping passes; going back fails.
#[test]
fn ir_version_bump_is_enforced() {
    use xtask::versions::BumpViolation;

    let copy = rehearsal("ir", "v1.0.0");
    assert_eq!(
        bump_check(&copy),
        Ok(()),
        "a release is consistent with itself"
    );

    edit(
        &copy,
        "crates/oc-model/src/doc.rs",
        "    pub level: u8,\n",
        "    // A comment is not layout.\n    /// Nor is a doc line.\n    pub level: u8,\n",
    );
    std::fs::write(
        copy.join("crates/oc-model/src/rehearsal_test.rs"),
        "#[cfg(test)]\nmod t { #[test] fn x() {} }\n#[test]\nfn y() {}\n",
    )
    .expect("a test file");
    assert_eq!(
        bump_check(&copy),
        Ok(()),
        "comments, docs and tests owe no bump"
    );

    edit(
        &copy,
        "crates/oc-model/src/doc.rs",
        "    pub numbering: Option<String>,\n",
        "    pub numbering: Option<String>,\n    pub rehearsal: bool,\n",
    );
    let violations = bump_check(&copy).expect_err("a field was added");
    assert!(
        matches!(
            violations.as_slice(),
            [BumpViolation::NotBumped {
                version: "ir_version",
                value: 1,
                ..
            }]
        ),
        "{violations:?}"
    );
    assert!(violations[0].to_string().contains("since v1.0.0"));

    edit(
        &copy,
        "crates/oc-model/src/lib.rs",
        "pub const IR_VERSION: u32 = 1;",
        "pub const IR_VERSION: u32 = 2;",
    );
    assert_eq!(bump_check(&copy), Ok(()), "bumped");
    edit(
        &copy,
        "crates/oc-model/src/lib.rs",
        "pub const IR_VERSION: u32 = 2;",
        "pub const IR_VERSION: u32 = 0;",
    );
    assert!(matches!(
        bump_check(&copy).expect_err("backwards").as_slice(),
        [BumpViolation::Backwards {
            version: "ir_version",
            ..
        }]
    ));

    // A new `Reason` variant is an IR change too (the plan's table names it).
    let copy = rehearsal("reason", "v1.0.0");
    edit(
        &copy,
        "crates/oc-model/src/ledger.rs",
        "    SoftHyphen,\n",
        "    SoftHyphen,\n    Rehearsal,\n",
    );
    assert!(matches!(
        bump_check(&copy)
            .expect_err("a variant was added")
            .as_slice(),
        [BumpViolation::NotBumped {
            version: "ir_version",
            ..
        }]
    ));
    let _ = std::fs::remove_dir_all(copy);
}

/// Row 15.17 (A15.7): a changed event — a field in a payload, a new event type — without a
/// `protocol` bump fails the release; the bump passes; an edited comment in `events.rs` does not.
#[test]
fn protocol_bump_is_enforced() {
    use xtask::versions::BumpViolation;

    let copy = rehearsal("protocol", "v1.0.0");
    edit(
        &copy,
        "crates/oc-core/src/events.rs",
        "//! One UTF-8 JSON object per line.",
        "//! One UTF-8 JSON object per line, reworded.",
    );
    assert_eq!(bump_check(&copy), Ok(()));

    edit(
        &copy,
        "crates/oc-core/src/events.rs",
        "self.emit(\"heartbeat\", serde_json::json!({}));",
        "self.emit(\"heartbeat\", serde_json::json!({ \"alive\": true }));",
    );
    let violations = bump_check(&copy).expect_err("a payload changed");
    assert!(
        matches!(
            violations.as_slice(),
            [BumpViolation::NotBumped {
                version: "protocol",
                ..
            }]
        ),
        "{violations:?}"
    );
    edit(
        &copy,
        "crates/oc-core/src/events.rs",
        "pub const PROTOCOL_VERSION: u32 = 1;",
        "pub const PROTOCOL_VERSION: u32 = 2;",
    );
    assert_eq!(bump_check(&copy), Ok(()), "bumped");

    // An event emitted by name outside `events.rs` is part of the protocol as well.
    let copy = rehearsal("emit", "v1.0.0");
    edit(
        &copy,
        "crates/openconvert/src/cmd_convert.rs",
        "\"phase\": \"started\",",
        "\"phase\": \"started\", \"rehearsal\": 1,",
    );
    assert!(matches!(
        bump_check(&copy)
            .expect_err("the job event changed")
            .as_slice(),
        [BumpViolation::NotBumped {
            version: "protocol",
            ..
        }]
    ));
    let _ = std::fs::remove_dir_all(copy);
}

/// The two other rules of `docs/VERSIONING.md` that are mechanical: a prompt edit needs a
/// `prompt_version` bump, and a job-spec change is a new schema file, never an edit of the old one.
#[test]
fn prompt_and_job_spec_changes_follow_their_rules() {
    use xtask::versions::BumpViolation;

    let copy = rehearsal("prompt", "v1.0.0");
    edit(
        &copy,
        "crates/oc-ai/prompts/metadata/v1/system.md",
        "",
        "Rehearsal. ",
    );
    assert!(matches!(
        bump_check(&copy).expect_err("a prompt changed").as_slice(),
        [BumpViolation::NotBumped {
            version: "prompt_version",
            ..
        }]
    ));

    let copy = rehearsal("jobspec", "v1.0.0");
    edit(
        &copy,
        "schemas/job-spec.v1.json",
        "\"type\": \"object\"",
        "\"type\":\"object\"",
    );
    assert!(matches!(
        bump_check(&copy).expect_err("v1 was edited").as_slice(),
        [BumpViolation::JobSpecMutated { version: 1, .. }]
    ));
    // The right way: v1 untouched, a new v2, oc-core pointed at it.
    let copy = rehearsal("jobspec2", "v1.0.0");
    std::fs::copy(
        copy.join("schemas/job-spec.v1.json"),
        copy.join("schemas/job-spec.v2.json"),
    )
    .expect("a new file");
    edit(
        &copy,
        "crates/oc-core/src/jobspec.rs",
        "schemas/job-spec.v1.json",
        "schemas/job-spec.v2.json",
    );
    assert_eq!(bump_check(&copy), Ok(()));
    let _ = std::fs::remove_dir_all(copy);
}

/// The committed baseline is the tree's own: it parses, and before the first release it is
/// marked unreleased (so nothing owes a bump yet) and records the versions the tree declares.
#[test]
fn the_committed_baseline_describes_this_tree() {
    use xtask::versions::{declared_versions, Baseline, BASELINE, UNRELEASED};

    let root = workspace_root();
    let baseline: Baseline = toml::from_str(&read(&root.join(BASELINE))).expect("a baseline");
    let declared = declared_versions(&root).expect("versions");
    if baseline.released == UNRELEASED {
        assert_eq!(baseline.versions.ir_version, declared.ir_version);
        assert_eq!(baseline.versions.protocol, declared.protocol);
    }
    assert_eq!(declared.ir_version, oc_model::IR_VERSION);
    assert_eq!(declared.protocol, oc_core::events::PROTOCOL_VERSION);
}

/// Row 15.18: on a release tag, no `TODO_` in `models.toml`, `packs.toml`, `thresholds.toml` or the
/// app's configuration (the updater key), and no threshold whose `review_by` has passed. Checked on
/// a scratch tree built to fail each way, and clean once everything is filled.
///
/// The repository itself fails this gate today, correctly: the model pins could not be fetched
/// (huggingface.co is refused here), the validation pack is unbuilt, and the updater keypair is the
/// maintainer's to generate. `cargo run -p xtask -- ci-lint --release-branch` lists them; PROGRESS.md
/// carries them as release blockers.
#[test]
fn no_todo_placeholders_on_a_release_tag() {
    use xtask::ci_lint::{release_placeholders, RELEASE_PLACEHOLDER_FILES};

    let scratch = std::env::temp_dir().join(format!("oc-release-tag-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    let write = |path: &str, text: &str| {
        let path = scratch.join(path);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("dir");
        std::fs::write(path, text).expect("write");
    };
    let threshold = |review_by: &str| {
        format!(
            "[demo.entry]\nvalue = 1\nsource = \"provisional\"\nevidence = \"x\"\nowner = \
             \"maintainer\"\nreview_by = \"{review_by}\"\n"
        )
    };
    write("models.toml", "sha256 = \"ab\"\n");
    write("packs.toml", "schema_version = 1\n");
    write("thresholds.toml", &threshold("2027-06-30"));
    write(
        "apps/desktop/src-tauri/tauri.conf.json",
        "{\"pubkey\": \"RWS\"}\n",
    );
    assert_eq!(
        release_placeholders(&scratch, "2026-09-23").expect("readable"),
        Vec::<String>::new(),
        "a filled tree is releasable"
    );

    write("models.toml", "sha256 = \"TODO_SHA256\"\n");
    write("packs.toml", "license = \"TODO_LICENSE\"\n");
    write(
        "thresholds.toml",
        &(threshold("2026-01-01") + "note = \"TODO_X\"\n"),
    );
    write(
        "apps/desktop/src-tauri/tauri.conf.json",
        "{\"pubkey\": \"TODO_UPDATER_PUBKEY\"}\n",
    );
    let findings = release_placeholders(&scratch, "2026-09-23").expect("readable");
    for file in RELEASE_PLACEHOLDER_FILES {
        assert!(
            findings.iter().any(|f| f.starts_with(&format!("{file}:"))),
            "{file} is checked: {findings:#?}"
        );
    }
    assert!(
        findings
            .iter()
            .any(|f| f.starts_with("thresholds.toml:0: demo.entry")),
        "an expired review_by is a finding: {findings:#?}"
    );
    let _ = std::fs::remove_dir_all(scratch);
}

/// The release job's `latest.json` is what the app's updater reads and verifies (rows 15.9/15.10
/// through the release tooling): built from the payloads and the `.sig` files `tauri signer`
/// writes, parsed by `oc_net::update`, every entry verifying against the key — and a flipped byte
/// in one payload failing `verify-latest`.
#[test]
fn the_release_latest_json_is_what_the_updater_verifies() {
    use base64::Engine as _;
    use xtask::release::{latest_json, verify_latest};

    let pair = minisign::KeyPair::generate_unencrypted_keypair().expect("a keypair");
    let pubkey = base64::engine::general_purpose::STANDARD
        .encode(pair.pk.to_box().expect("a key box").to_string());
    let dir = std::env::temp_dir().join(format!("oc-latest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let payloads = [
        "OpenConvert_1.0.0_amd64.AppImage",
        "OpenConvert_1.0.0_x64-setup.exe",
        "OpenConvert_1.0.0_aarch64.app.tar.gz",
        "OpenConvert_1.0.0_x86_64.app.tar.gz",
    ];
    for (i, name) in payloads.iter().enumerate() {
        let bytes = vec![u8::try_from(i).expect("small"); 512];
        std::fs::write(dir.join(name), &bytes).expect("payload");
        let signature = minisign::sign(
            Some(&pair.pk),
            &pair.sk,
            std::io::Cursor::new(&bytes),
            Some(&format!("file:{name}")),
            Some("signature from tauri secret key"),
        )
        .expect("signed")
        .to_string();
        std::fs::write(
            dir.join(format!("{name}.sig")),
            base64::engine::general_purpose::STANDARD.encode(signature),
        )
        .expect("sig");
    }
    std::fs::write(
        dir.join("OpenConvert_1.0.0_x64_en-US.msi"),
        b"not an update",
    )
    .expect("msi");

    let latest = latest_json(
        &dir,
        "v1.0.0",
        "Notes.",
        "2026-09-23T00:00:00Z",
        "https://github.com/openconvert/openconvert/releases/download/v1.0.0",
    )
    .expect("a manifest");
    let manifest: oc_net::update::UpdateManifest =
        serde_json::from_value(latest.clone()).expect("the updater reads it");
    assert_eq!(manifest.version, "1.0.0");
    let keys: Vec<&str> = manifest.platforms.keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        [
            "darwin-aarch64",
            "darwin-aarch64-app",
            "darwin-x86_64",
            "darwin-x86_64-app",
            "linux-x86_64",
            "linux-x86_64-appimage",
            "windows-x86_64",
            "windows-x86_64-nsis",
        ]
    );
    assert_eq!(verify_latest(&latest, &dir, &pubkey).expect("verifies"), 8);

    let mut tampered = std::fs::read(dir.join(payloads[1])).expect("read");
    tampered[100] ^= 0x01;
    std::fs::write(dir.join(payloads[1]), tampered).expect("write");
    let error = verify_latest(&latest, &dir, &pubkey).expect_err("tampered");
    assert!(format!("{error:#}").contains("windows-x86_64"), "{error:#}");
    assert!(
        verify_latest(&latest, &dir, "TODO_UPDATER_PUBKEY").is_err(),
        "the placeholder key verifies nothing"
    );
    let _ = std::fs::remove_dir_all(dir);
}

fn release_workflow() -> serde_yaml::Value {
    yaml(&workspace_root().join(".github/workflows/release.yml"))
}

/// Every step of every job, as (job, step) pairs.
fn workflow_steps(workflow: &serde_yaml::Value) -> Vec<(String, serde_yaml::Value)> {
    let mut out = Vec::new();
    for (job, body) in workflow["jobs"].as_mapping().expect("jobs") {
        let job = job.as_str().expect("a job name").to_owned();
        for step in body["steps"].as_sequence().into_iter().flatten() {
            out.push((job.clone(), step.clone()));
        }
    }
    out
}

/// Row 15.14 (D1): the release needs no Python. No step of `release.yml` sets up, installs or runs
/// Python, `pip`, `uv` or the `eval/` harness; and every job that runs in a container — the gates,
/// the Linux build, the Linux reproducibility leg, the SBOM, the comparison and the publication — is
/// a Debian image without Python that asserts so before anything else. (That the job then succeeds
/// is the release run's to show — unverified here.)
#[test]
fn release_job_needs_no_python() {
    let workflow = release_workflow();
    let assertion = "command -v python3 || command -v python";
    for (job, step) in workflow_steps(&workflow) {
        let uses = step["uses"].as_str().unwrap_or_default();
        assert!(
            !uses.contains("setup-python") && !uses.contains("setup-uv"),
            "{job}: {uses}"
        );
        // The assertion line itself names Python, to say it is absent.
        let run: String = step["run"]
            .as_str()
            .unwrap_or_default()
            .lines()
            .filter(|line| !line.contains(assertion))
            .collect::<Vec<_>>()
            .join("\n");
        for word in ["python", "pip ", "pip3", "uv ", "uv sync", "eval/", ".venv"] {
            assert!(
                !run.to_lowercase().contains(word),
                "{job} runs {word:?}: {run}"
            );
        }
    }

    let jobs = workflow["jobs"].as_mapping().expect("jobs");
    let mut containers = 0;
    for (name, body) in jobs {
        let name = name.as_str().expect("a name");
        // A matrix job names its container per leg (`${{ matrix.container }}`).
        let container = body["strategy"]["matrix"]["include"]
            .as_sequence()
            .and_then(|legs| legs.iter().find_map(|l| l["container"].as_str()))
            .or_else(|| body["container"].as_str())
            .map(str::to_owned);
        let Some(container) = container else {
            continue;
        };
        containers += 1;
        assert!(
            container.contains("bookworm"),
            "{name} runs on {container}, not a Debian image this project knows has no Python"
        );
        let steps = body["steps"].as_sequence().expect("steps");
        let asserts_first = steps
            .iter()
            .take(1)
            .any(|s| s["run"].as_str().is_some_and(|r| r.contains(assertion)));
        assert!(
            asserts_first,
            "{name}'s first step asserts there is no Python"
        );
    }
    assert!(
        containers >= 6,
        "gates, build (Linux), repro (Linux), repro-compare, sbom, publish"
    );
}

/// Every CI-gate row of the plan's table is a step of the release workflow, named with its row
/// number and test name, so a red release reads as the rule it broke; and the release is signed
/// and checked with the tools this phase built.
#[test]
fn every_release_gate_row_is_a_named_release_step() {
    let workflow = release_workflow();
    let names: Vec<String> = workflow_steps(&workflow)
        .iter()
        .filter_map(|(_, s)| s["name"].as_str().map(str::to_owned))
        .collect();
    for row in [
        "row 15.1 every_nested_macho_is_signed_with_one_team_id",
        "row 15.2 codesign_verify_deep_strict_passes",
        "row 15.3 spctl_assess_accepts_the_bundle",
        "row 15.4 notarization_ticket_is_stapled",
        "row 15.6 windows_installers_are_produced_and_hashed",
        "row 15.7 appimage_launches_and_converts_headless",
        "row 15.9",
        "rows 15.11 sbom_is_valid_cyclonedx_1_6, 15.12 sbom_lists_every_vendored_native",
        "row 15.13 reproducible_no_ai_output_across_os",
        "row 15.14 release_job_needs_no_python",
        "row 15.15 installer_size_within_budget",
        "rows 15.16/15.17 bump rules",
        "row 15.18 no_todo_placeholders_on_a_release_tag",
        "row 15.20 release_artifacts_all_have_published_hashes",
    ] {
        assert!(
            names.iter().any(|n| n.starts_with(row)),
            "no release step named {row:?}"
        );
    }
    let text = read(&workspace_root().join(".github/workflows/release.yml"));
    for tool in [
        "packaging/macos/sign_nested.sh",
        "packaging/macos/notarize.sh",
        "xtask --locked -- sbom",
        "xtask --locked -- repro hash",
        "release latest-json",
        "release changelog",
        "release verify-latest",
        "bump-rules-check --tag",
        "ci-lint --release-branch",
        "flatpak-builder-lint",
        "--draft",
    ] {
        assert!(text.contains(tool), "release.yml does not use {tool}");
    }
    // Windows is unsigned in v1 and the release says so (D12), rather than half-signing it.
    assert!(!text.contains("signtool"));
    assert!(text.contains("not code-signed"));
}

/// PHASE 15 part B: the Rust side's licence notices ship in every bundle and are regenerated from
/// `Cargo.lock`, so a dependency added without its notice fails here, on every platform.
#[test]
fn the_rust_notices_are_up_to_date() {
    use xtask::notices::{collect, metadata, render, NOTICES_FILE};

    let root = workspace_root();
    let expected = render(&collect(&root, &metadata(&root).expect("metadata")).expect("notices"));
    let committed = std::fs::read_to_string(root.join(NOTICES_FILE)).unwrap_or_default();
    assert!(
        committed == expected,
        "{NOTICES_FILE} does not match Cargo.lock; run `cargo run -p xtask -- notices`"
    );
}

/// Every crate that ships is in the notices with at least one licence text; nothing that does not
/// ship is (a dev-dependency, the tooling, OpenConvert's own crates).
#[test]
fn every_shipped_crate_and_only_those_has_a_licence_text() {
    use xtask::notices::{chosen_license, collect, metadata};

    let root = workspace_root();
    let crates = collect(&root, &metadata(&root).expect("metadata")).expect("notices");
    let names: BTreeSet<&str> = crates.iter().map(|c| c.name.as_str()).collect();
    for shipped in [
        "serde",
        "tauri",
        "pdfium-render",
        "quick-xml",
        "zip",
        "ureq",
        "minisign-verify",
    ] {
        assert!(names.contains(shipped), "{shipped} ships");
    }
    for not_shipped in [
        "insta",
        "proptest",
        "minisign",
        "typst",
        "jsonschema",
        "oc-core",
        "openconvert",
    ] {
        assert!(!names.contains(not_shipped), "{not_shipped} does not ship");
    }
    for notice in &crates {
        assert!(
            !notice.texts.is_empty() && notice.texts.iter().all(|t| !t.trim().is_empty()),
            "{} {} has no licence text",
            notice.name,
            notice.version
        );
    }

    assert_eq!(chosen_license("MIT OR Apache-2.0"), Some("MIT"));
    assert_eq!(chosen_license("MIT/Apache-2.0"), Some("MIT"));
    assert_eq!(chosen_license("Zlib OR Apache-2.0 OR MIT"), Some("MIT"));
    assert_eq!(
        chosen_license("(Apache-2.0 OR MIT) AND BSD-3-Clause"),
        Some("MIT")
    );
    assert_eq!(chosen_license("MPL-2.0"), Some("MPL-2.0"));
    assert_eq!(chosen_license("Unlicense"), None);
}

/// PHASE 15 part B: the release notes are the `## [X.Y.Z]` section of `docs/CHANGELOG.md`, read by
/// `xtask release changelog`, which refuses a missing or empty section and one that still carries a
/// `TODO_` placeholder — the 1.0.0 draft has one for Phase 14's security claims, so the release job
/// cannot publish the draft as it stands.
#[test]
fn release_notes_come_from_the_changelog_and_refuse_a_placeholder() {
    use xtask::release::release_notes;

    let changelog = "# Changelog\n\nIntro.\n\n## [1.1.0] — 2027-01-01\n\nNewer.\n\n\
                     ## [1.0.0] — unreleased\n\nOpenConvert 1.0.\n\n### Security\n\nTODO_PHASE14: claims.\n\n\
                     ## Phase 0 — Repo\n\nHistory.\n";
    let error = release_notes(changelog, "v1.0.0").expect_err("a placeholder is left");
    assert!(format!("{error:#}").contains("TODO_"), "{error:#}");
    assert_eq!(
        release_notes(changelog, "1.1.0").expect("a section"),
        "Newer."
    );
    let filled = changelog.replace("TODO_PHASE14: claims.", "Landlock on Linux 5.13 and later.");
    let notes = release_notes(&filled, "v1.0.0").expect("filled");
    assert!(
        notes.starts_with("OpenConvert 1.0.")
            && notes.ends_with("Landlock on Linux 5.13 and later.")
    );
    assert!(
        !notes.contains("History"),
        "the section ends at the next ## heading"
    );
    assert!(release_notes(changelog, "2.0.0").is_err(), "no section");
    assert!(
        release_notes("## [3.0.0]\n\n## Phase 0\n", "3.0.0").is_err(),
        "an empty section"
    );

    // The committed changelog has the 1.0.0 draft; until Phase 14's claims replace the placeholder,
    // it is not releasable.
    let committed = read(&workspace_root().join("docs/CHANGELOG.md"));
    assert!(
        committed.contains("\n## [1.0.0]"),
        "the 1.0.0 draft is in docs/CHANGELOG.md"
    );
}
