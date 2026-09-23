//! The diagnostic bundle: what "Report a problem" and "Export diagnostic bundle" write (SECURITY
//! §10, UI_UX §2.3, result.html §4).
//!
//! **Nothing is ever sent.** The bundle is written to a place the user picks in the native save
//! dialog, and the app then shows what is inside it before the user decides whether to share it.
//! It holds the job's report and event log, the versions, and a line about the system; it never
//! holds the PDF, the EPUB, the settings file or any key — and says so on the review screen.

use std::io::Write;
use std::path::Path;

use serde::Serialize;

use crate::engine::{Hello, UiError};

/// One file in the bundle, as the review screen lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BundleEntry {
    pub name: String,
    pub bytes: u64,
}

/// What was written, and where.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Bundle {
    pub path: String,
    pub bytes: u64,
    pub entries: Vec<BundleEntry>,
}

/// The files of a bundle. `report` and `events` are the job's, when there is a job.
pub fn contents(
    report: Option<Vec<u8>>,
    events: Option<Vec<String>>,
    app_version: &str,
    hello: Option<&Hello>,
) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    if let Some(report) = report {
        files.push(("report.json".to_owned(), report));
    }
    if let Some(events) = events {
        let mut log = events.join("\n");
        log.push('\n');
        files.push(("events.ndjson".to_owned(), log.into_bytes()));
    }
    files.push((
        "versions.txt".to_owned(),
        versions(app_version, hello).into_bytes(),
    ));
    files.push(("system.txt".to_owned(), system().into_bytes()));
    files
}

/// The app's and the engine's versions, as the handshake saw them.
fn versions(app_version: &str, hello: Option<&Hello>) -> String {
    match hello {
        Some(hello) => format!(
            "app {app_version}\nengine {}\nprotocol {}\nir {}\npdfium {}\n",
            hello.engine_version, hello.protocol, hello.ir_version, hello.pdfium_version
        ),
        None => format!("app {app_version}\nengine: the start-up handshake did not succeed\n"),
    }
}

/// The operating system, the architecture and the core count — nothing that names the machine
/// or its user.
fn system() -> String {
    let cores = std::thread::available_parallelism()
        .map(|cores| cores.get().to_string())
        .unwrap_or_else(|_| "unknown".to_owned());
    format!(
        "os {}\narch {}\ncores {cores}\n",
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

/// Write the bundle to `path` as a zip.
pub fn write(path: &Path, files: &[(String, Vec<u8>)]) -> Result<Bundle, UiError> {
    let partial = path.with_extension("zip.part");
    {
        let file = std::fs::File::create(&partial)?;
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in files {
            zip.start_file(name.as_str(), options)
                .map_err(|error| UiError::Io(error.to_string()))?;
            zip.write_all(bytes)?;
        }
        zip.finish()
            .map_err(|error| UiError::Io(error.to_string()))?;
    }
    std::fs::rename(&partial, path)?;
    Ok(Bundle {
        path: path.to_string_lossy().into_owned(),
        bytes: std::fs::metadata(path)?.len(),
        entries: files
            .iter()
            .map(|(name, bytes)| BundleEntry {
                name: name.clone(),
                bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            })
            .collect(),
    })
}

/// `openconvert-diagnostics-YYYY-MM-DD.zip`, today.
pub fn default_name() -> String {
    let today = time::OffsetDateTime::now_utc().date();
    format!(
        "openconvert-diagnostics-{:04}-{:02}-{:02}.zip",
        today.year(),
        u8::from(today.month()),
        today.day()
    )
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;

    #[test]
    fn the_bundle_holds_the_report_log_versions_and_system_and_nothing_else() {
        let dir = std::env::temp_dir().join(format!("oc-desktop-diag-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("made");
        let hello = Hello {
            engine_version: "0.1.0".to_owned(),
            ir_version: 1,
            protocol: 1,
            pdfium_version: "151.0.7881.0".to_owned(),
        };
        let files = contents(
            Some(b"{\"schema\":\"openconvert.report/1\"}".to_vec()),
            Some(vec![r#"{"v":1,"t":"hello"}"#.to_owned()]),
            "0.1.0",
            Some(&hello),
        );
        let path = dir.join("bundle.zip");
        let bundle = write(&path, &files).expect("written");

        let names: Vec<&str> = bundle.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            ["report.json", "events.ndjson", "versions.txt", "system.txt"]
        );
        assert!(bundle.bytes > 0);
        assert!(!dir.join("bundle.zip.part").exists(), "written atomically");

        let mut archive =
            zip::ZipArchive::new(std::fs::File::open(&path).expect("opens")).expect("a zip");
        let mut versions = String::new();
        archive
            .by_name("versions.txt")
            .expect("present")
            .read_to_string(&mut versions)
            .expect("reads");
        assert!(versions.contains("engine 0.1.0") && versions.contains("pdfium 151.0.7881.0"));
        let mut system = String::new();
        archive
            .by_name("system.txt")
            .expect("present")
            .read_to_string(&mut system)
            .expect("reads");
        assert!(system.starts_with("os "), "{system}");

        // Without a job: versions and system only.
        let bare = contents(None, None, "0.1.0", None);
        assert_eq!(bare.len(), 2);
        assert!(default_name().starts_with("openconvert-diagnostics-"));
    }
}
