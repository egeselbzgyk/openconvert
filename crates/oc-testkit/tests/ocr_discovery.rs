//! Rows 13.1–13.4 and A13.7: finding the user's Tesseract, and refusing the wrong one.
//!
//! These live in `oc-testkit` rather than in `oc-core/tests` as the plan's file list has it: they
//! need the fake engine, which is here, and `oc-core` cannot take `oc-testkit` as a dev-dependency
//! without putting `oc-net` into the graph `oc_core_has_no_net_dependency` walks. Unix only — the
//! fake is a shell script (see `oc_testkit::fake_tesseract`).
#![cfg(unix)]

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use oc_core::ocr::discover::{
    discover_with, DiscoverySource, OcrUnavailable, Probe, Version, WellKnown,
};
use oc_testkit::fake_tesseract::{FakeConfig, FakeTesseract};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("oc-ocr-discovery-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

fn probe(path_var: Option<OsString>, well_known: Vec<WellKnown>) -> Probe {
    Probe {
        config: None,
        path_var,
        well_known,
        deadline: Duration::from_secs(10),
    }
}

/// Row 13.1. A stub on a temporary `PATH` is found, and the report says where from.
#[test]
fn discovery_finds_tesseract_on_path() {
    let dir = scratch("path");
    let empty = dir.join("empty");
    std::fs::create_dir_all(&empty).expect("an empty directory");
    let fake = FakeTesseract::install(&dir.join("bin"), &FakeConfig::default());

    // The first directory has no tesseract; the relative one is skipped whatever it holds.
    let path_var = std::env::join_paths([
        empty.clone(),
        PathBuf::from("relative/bin"),
        dir.join("bin"),
    ])
    .expect("a PATH");
    let info = discover_with(&probe(Some(path_var), Vec::new())).expect("found on PATH");

    assert_eq!(info.source, DiscoverySource::EnvPath);
    assert_eq!(info.path, fake.path);
    assert_eq!(
        info.version,
        Version {
            major: 5,
            minor: 3,
            patch: 4
        }
    );
    assert_eq!(
        info.langs.iter().map(String::as_str).collect::<Vec<_>>(),
        ["deu", "eng", "osd", "tur"]
    );
    assert_eq!(info.capability(), "ocr:tesseract-5.3.4");
    assert!(fake.ocr_calls().is_empty(), "discovery never runs OCR");
}

/// Row 13.2. With an empty `PATH`, the well-known list is searched in order and the search stops at
/// the first hit: the second entry is found and the third is never run.
#[test]
fn discovery_falls_back_to_well_known_dirs() {
    let dir = scratch("well-known");
    let second = FakeTesseract::install(&dir.join("second"), &FakeConfig::default());
    let third = FakeTesseract::install(
        &dir.join("third"),
        &FakeConfig {
            banner: "tesseract 5.9.9".to_owned(),
            ..FakeConfig::default()
        },
    );
    let list = vec![
        WellKnown {
            label: "first",
            path: dir.join("first/tesseract"),
        },
        WellKnown {
            label: "second",
            path: second.path.clone(),
        },
        WellKnown {
            label: "third",
            path: third.path.clone(),
        },
    ];

    for path_var in [None, Some(OsString::new())] {
        let info = discover_with(&probe(path_var, list.clone())).expect("found");
        assert_eq!(info.source, DiscoverySource::WellKnown("second"));
        assert_eq!(info.path, second.path);
    }
    assert!(
        third.invocations().is_empty(),
        "the search stops at the first hit: {:?}",
        third.invocations()
    );

    // An explicit path is the only candidate: nothing else is searched, and a missing one is
    // not-found rather than a fall-back to `PATH`.
    let explicit = Probe {
        config: Some(dir.join("nowhere/tesseract")),
        ..probe(None, list.clone())
    };
    assert_eq!(discover_with(&explicit), Err(OcrUnavailable::NotFound));
    let explicit = Probe {
        config: Some(third.path.clone()),
        ..probe(None, list)
    };
    let info = discover_with(&explicit).expect("the configured one");
    assert_eq!(info.source, DiscoverySource::ConfigPath);
    assert_eq!(info.version.to_string(), "5.9.9");

    // Nothing anywhere is not-found.
    assert_eq!(
        discover_with(&probe(None, Vec::new())),
        Err(OcrUnavailable::NotFound)
    );
}

/// Row 13.3 and A13.7. A 4.x binary is refused as too old, and nothing but `--version` is ever
/// asked of it.
#[test]
fn discovery_rejects_version_below_5() {
    let dir = scratch("old");
    let fake = FakeTesseract::install(
        &dir,
        &FakeConfig {
            banner: "tesseract 4.1.1".to_owned(),
            ..FakeConfig::default()
        },
    );
    let path_var = std::env::join_paths([dir.clone()]).expect("a PATH");
    let result = discover_with(&probe(Some(path_var), Vec::new()));
    assert_eq!(
        result,
        Err(OcrUnavailable::TooOld(Version {
            major: 4,
            minor: 1,
            patch: 1
        }))
    );
    assert_eq!(
        fake.invocations(),
        [vec!["--version".to_owned()]],
        "a refused binary is invoked for its version and for nothing else"
    );
    assert!(result
        .expect_err("too old")
        .to_string()
        .contains("4.1.1 was found"));

    // The UB-Mannheim banner form is read, not refused as unreadable (VD-g).
    let windows_style = FakeTesseract::install(
        &dir.join("ub"),
        &FakeConfig {
            banner: "tesseract v5.5.3.20260724".to_owned(),
            ..FakeConfig::default()
        },
    );
    let path_var = std::env::join_paths([dir.join("ub")]).expect("a PATH");
    let info = discover_with(&probe(Some(path_var), Vec::new())).expect("5.5.3 is usable");
    assert_eq!(info.version.to_string(), "5.5.3");
    assert_eq!(windows_style.ocr_calls().len(), 0);
}

/// Row 13.4. A group- or world-writable candidate is refused before it is ever run.
#[test]
fn discovery_rejects_writable_binary() {
    let dir = scratch("writable");
    for (name, mode) in [("world", 0o777), ("group", 0o775), ("other", 0o757)] {
        let fake = FakeTesseract::install(
            &dir.join(name),
            &FakeConfig {
                mode,
                ..FakeConfig::default()
            },
        );
        let path_var = std::env::join_paths([dir.join(name)]).expect("a PATH");
        assert_eq!(
            discover_with(&probe(Some(path_var), Vec::new())),
            Err(OcrUnavailable::Untrusted(fake.path.clone())),
            "mode {mode:o}"
        );
        assert!(
            fake.invocations().is_empty(),
            "an untrusted binary is never executed, not even for its version"
        );
    }

    // A misnamed explicit path is refused the same way.
    let misnamed = dir.join("misnamed");
    std::fs::create_dir_all(&misnamed).expect("dir");
    let fake = FakeTesseract::install(&misnamed, &FakeConfig::default());
    let renamed = misnamed.join("not-tesseract");
    std::fs::rename(&fake.path, &renamed).expect("rename");
    let explicit = Probe {
        config: Some(renamed.clone()),
        ..probe(None, Vec::new())
    };
    assert_eq!(
        discover_with(&explicit),
        Err(OcrUnavailable::Untrusted(renamed))
    );
}
