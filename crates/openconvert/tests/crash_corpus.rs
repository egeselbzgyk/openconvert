//! PHASE 14 rows 14.16, 14.17 and 14.18: files that once crashed a PDF parser, or were built to,
//! as permanent regressions (SECURITY §12). The assertion is weak on quality and strong on
//! behaviour: every file terminates within the deadline with exit 0 or 1 and a written report —
//! never a hang, never 101, never a partial file at the output path.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn crash_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus/fixtures/crash")
}

fn manifest() -> serde_json::Value {
    let text = std::fs::read_to_string(crash_dir().join("manifest.json"))
        .expect("corpus/fixtures/crash/manifest.json; run `python -m oc_eval.mutate.crash`");
    serde_json::from_str(&text).expect("the manifest is JSON")
}

/// A run that has not ended by now is a hang. Far inside the engine's own stage deadline, because
/// every file here is small: a slow one is a bug worth seeing.
const HANG: Duration = Duration::from_secs(120);

/// Convert `input` in its own directory and hold it to the contract. `Err` says what broke it.
fn terminates_cleanly(input: &Path, name: &str) -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("oc-crash-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let output = dir.join("out.epub");
    let mut child = Command::new(env!("CARGO_BIN_EXE_openconvert"))
        .arg("convert")
        .arg(input)
        .arg("-o")
        .arg(&output)
        .args(["--ocr", "never"])
        .env("HOME", &dir)
        .env("XDG_DATA_HOME", dir.join("data"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| error.to_string())?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break status;
        }
        if started.elapsed() > HANG {
            let _ = child.kill();
            return Err(format!("{name}: still running after {HANG:?}"));
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let code = status.code();
    if !matches!(code, Some(0 | 1)) {
        return Err(format!("{name}: exit {status:?}, not 0 or 1"));
    }
    let report = dir.join("out.epub.report.json");
    let text = std::fs::read_to_string(&report)
        .map_err(|error| format!("{name}: exit {code:?} and no report: {error}"))?;
    let report: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| format!("{name}: the report: {error}"))?;
    match code {
        Some(1) => {
            if output.exists() {
                return Err(format!("{name}: exit 1 with a book at the output path"));
            }
            if report["status"] != "failed" {
                return Err(format!(
                    "{name}: exit 1 with a report saying {}",
                    report["status"]
                ));
            }
        }
        _ => {
            if !output.is_file() {
                return Err(format!("{name}: exit 0 and no book"));
            }
        }
    }
    let leftovers = std::fs::read_dir(&dir)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .any(|entry| entry.file_name().to_string_lossy().contains(".oc-tmp-"));
    if leftovers {
        return Err(format!("{name}: a temporary was left behind"));
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// Run every file on a few threads; collect every broken contract rather than stopping at one.
fn all_terminate(files: &[(PathBuf, String)]) -> Vec<String> {
    const THREADS: usize = 4;
    let next = std::sync::atomic::AtomicUsize::new(0);
    let failures = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..THREADS {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let Some((path, name)) = files.get(index) else {
                    break;
                };
                if let Err(failure) = terminates_cleanly(path, name) {
                    failures
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .push(failure);
                }
            });
        }
    });
    failures
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Row 14.18: every file under `corpus/fixtures/crash/` is in the manifest with its sha256, and
/// every manifest entry is a file that is there. A crash fixture nobody keyed is one a later
/// change can alter or drop without anyone noticing.
#[test]
fn crash_fixtures_are_manifest_keyed() {
    let manifest = manifest();
    let entries = manifest["files"].as_array().expect("files");
    let keyed: std::collections::BTreeMap<String, String> = entries
        .iter()
        .map(|entry| {
            (
                entry["name"].as_str().expect("name").to_owned(),
                entry["sha256"].as_str().expect("sha256").to_owned(),
            )
        })
        .collect();

    let mut on_disk = std::collections::BTreeMap::new();
    let mut stack = vec![crash_dir()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)
            .expect("the crash corpus")
            .filter_map(Result::ok)
        {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let name = path
                .strip_prefix(crash_dir())
                .expect("under the corpus")
                .to_string_lossy()
                .replace('\\', "/");
            if name == "manifest.json" {
                continue;
            }
            let bytes = std::fs::read(&path).expect("a crash fixture");
            on_disk.insert(name, openconvert::convert::sha256_hex(&bytes));
        }
    }
    assert!(
        on_disk.len() >= 20,
        "the mutated set is there: {} files",
        on_disk.len()
    );
    for (name, sha256) in &on_disk {
        assert_eq!(
            keyed.get(name),
            Some(sha256),
            "{name} is not keyed in the manifest with its sha256 ({sha256})"
        );
    }
    for name in keyed.keys() {
        assert!(
            on_disk.contains_key(name),
            "the manifest keys {name}, which is not there"
        );
    }
}

/// Row 14.17 (A14.6): every mutated file — xref cycles and chains past the cap, flipped xref
/// bytes, truncated and lying streams, nested object streams, page trees that contain themselves —
/// terminates cleanly.
#[test]
fn mutated_crash_corpus_terminates_cleanly() {
    let files: Vec<(PathBuf, String)> = manifest()["files"]
        .as_array()
        .expect("files")
        .iter()
        .filter(|entry| entry["set"] == "mutated" || entry["set"] == "fuzz")
        .map(|entry| {
            let name = entry["name"].as_str().expect("name");
            (crash_dir().join(name), name.replace(['/', '.'], "_"))
        })
        .collect();
    assert!(files.len() >= 20, "{} files", files.len());
    let failures = all_terminate(&files);
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Row 14.16 (A14.6): the Isartor suite — PDF/A-1b violations, catalogued — fetched by
/// `cargo run -p xtask -- fetch-isartor` into `target/isartor/`, terminates cleanly file by file.
#[cfg(feature = "isartor")]
#[test]
fn isartor_corpus_terminates_cleanly() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/isartor");
    let mut files = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
        {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "pdf") {
                let name = format!("isartor_{}", files.len());
                files.push((path, name));
            }
        }
    }
    assert!(
        !files.is_empty(),
        "no Isartor files in {}; run `cargo run -p xtask -- fetch-isartor`",
        root.display()
    );
    let failures = all_terminate(&files);
    assert!(failures.is_empty(), "{failures:#?}");
}
