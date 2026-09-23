//! Finding the user's Tesseract (D4, PHASE 13 detail 1).
//!
//! **The order is fixed and short:** `--ocr-path` / `ocr.tesseract_path`, then `PATH`, then a
//! per-OS list of well-known install locations. No recursive search, ever — a converter that walks
//! the disk looking for executables to run is doing something no user asked it to.
//!
//! **A candidate has to earn its execution.** It must be an absolute path to a file named
//! `tesseract` (`tesseract.exe` on Windows), and on Unix it must not be group- or world-writable: a
//! binary any other user on the machine can replace is an execution primitive, and refusing it
//! costs nothing. Its version is read from the first line of `tesseract --version`, and **major ≥ 5
//! is required** — 4.x is refused rather than used, because the TSV column set and the LSTM
//! defaults differ, and nothing else about it is invoked. The installed languages come from
//! `tesseract --list-langs`.
//!
//! **Once per process.** [`discover`] caches its answer; the `hello` event reports it as
//! `ocr:tesseract-<version>`, or leaves it out.
//!
//! The Windows locations are the UB-Mannheim installer's two install modes (VD-g, closed
//! 2026-09-23 — `docs/DECISIONS_LOG.md`): all users under `%ProgramFiles%\Tesseract-OCR`, one user
//! under `%LOCALAPPDATA%\Programs\Tesseract-OCR`. The installer does not put itself on `PATH`, which
//! is why the well-known list is not a nicety on Windows but the usual way it is found, and its
//! builds print their version as `tesseract v5.5.3.20260724`, which [`Version::parse_banner`]
//! reads.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::Duration;

use super::Os;
use crate::sidecar::tesseract::{run_captured, RunError};
use crate::thresholds::T;

/// The oldest major version whose TSV and LSTM defaults this adapter was written against.
pub const MIN_MAJOR: u32 = 5;

/// A Tesseract version, as its banner states it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl Version {
    /// Read `tesseract --version`'s first line: `tesseract 5.3.4` on Linux and macOS,
    /// `tesseract v5.3.0.20221214` from the UB-Mannheim Windows builds, `tesseract 4.1.1` from an
    /// old distribution. A build date or a pre-release suffix after the third component is ignored.
    pub fn parse_banner(line: &str) -> Option<Version> {
        let mut words = line.split_whitespace();
        if words.next()? != "tesseract" {
            return None;
        }
        let number = words.next()?;
        let number = number.strip_prefix('v').unwrap_or(number);
        let mut parts = number.split(['.', '-']);
        let mut component = || -> Option<u32> {
            let digits: String = parts
                .next()?
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            digits.parse().ok()
        };
        let major = component()?;
        let minor = component().unwrap_or_default();
        let patch = component().unwrap_or_default();
        Some(Version {
            major,
            minor,
            patch,
        })
    }
}

/// Where the Tesseract in use was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiscoverySource {
    /// `--ocr-path`, or `ocr.tesseract_path` in the job spec.
    ConfigPath,
    /// A directory on `PATH`.
    EnvPath,
    /// One of the per-OS well-known locations, by its label.
    WellKnown(&'static str),
}

/// A usable Tesseract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TesseractInfo {
    pub path: PathBuf,
    pub version: Version,
    /// Traineddata names, e.g. `deu`, `eng`, `osd`.
    pub langs: BTreeSet<String>,
    pub source: DiscoverySource,
}

impl TesseractInfo {
    /// The `hello` event's capability string: `ocr:tesseract-5.3.4`.
    pub fn capability(&self) -> String {
        format!("ocr:tesseract-{}", self.version)
    }
}

/// Why there is no usable Tesseract. Every variant converts the book as if no engine existed (D4,
/// A13.7): pages that needed OCR become page images and `W_OCR_ENGINE_MISSING` says why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OcrUnavailable {
    NotFound,
    /// Major version below [`MIN_MAJOR`]. Refused, never invoked beyond `--version`.
    TooOld(Version),
    /// Not an absolute path, not named `tesseract`, or writable by others.
    Untrusted(PathBuf),
    /// Found, but it could not be run or did not say what it is. Carried as text so that the
    /// per-process cache can hand the same answer to every caller.
    Unrunnable(String),
}

impl std::fmt::Display for OcrUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OcrUnavailable::NotFound => write!(f, "no tesseract was found"),
            OcrUnavailable::TooOld(version) => write!(
                f,
                "tesseract {version} was found, and version {MIN_MAJOR} or newer is required"
            ),
            OcrUnavailable::Untrusted(path) => write!(
                f,
                "{} was not used: it must be an absolute path to a file named tesseract that \
                 other users cannot write to",
                path.display()
            ),
            OcrUnavailable::Unrunnable(message) => {
                write!(f, "tesseract could not be run: {message}")
            }
        }
    }
}

/// One well-known install location: a label for the report, and the binary's full path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WellKnown {
    pub label: &'static str,
    pub path: PathBuf,
}

/// The well-known locations for `os`, in the order they are tried. `env` reads an environment
/// variable, so the Windows list can be built — and tested — on any machine.
pub fn well_known(os: Os, env: &dyn Fn(&str) -> Option<OsString>) -> Vec<WellKnown> {
    let fixed = |label: &'static str, path: &str| WellKnown {
        label,
        path: PathBuf::from(path),
    };
    match os {
        Os::MacOs => vec![
            fixed("homebrew", "/opt/homebrew/bin/tesseract"),
            fixed("usr-local", "/usr/local/bin/tesseract"),
        ],
        Os::Linux => vec![
            fixed("usr-bin", "/usr/bin/tesseract"),
            fixed("usr-local", "/usr/local/bin/tesseract"),
            fixed("snap", "/snap/bin/tesseract"),
        ],
        Os::Windows => {
            let under = |label: &'static str, variable: &str, rest: &[&str]| {
                env(variable).map(|base| {
                    let mut path = PathBuf::from(base);
                    for part in rest {
                        path.push(part);
                    }
                    WellKnown { label, path }
                })
            };
            [
                under(
                    "program-files",
                    "ProgramFiles",
                    &["Tesseract-OCR", "tesseract.exe"],
                ),
                under(
                    "local-app-data",
                    "LOCALAPPDATA",
                    &["Programs", "Tesseract-OCR", "tesseract.exe"],
                ),
            ]
            .into_iter()
            .flatten()
            .collect()
        }
    }
}

/// Everything discovery reads from its environment, so that the whole search is testable with a
/// temporary `PATH` and a temporary well-known list.
#[derive(Clone, Debug, Default)]
pub struct Probe {
    /// `--ocr-path` / `ocr.tesseract_path`. When set, it is the only candidate.
    pub config: Option<PathBuf>,
    /// The value of `PATH`.
    pub path_var: Option<OsString>,
    pub well_known: Vec<WellKnown>,
    /// How long `--version` and `--list-langs` may take before they are treated as a hang.
    pub deadline: Duration,
}

impl Probe {
    /// The probe for this process: its `PATH` and this platform's well-known list.
    pub fn from_env(config: Option<&Path>) -> Probe {
        Probe {
            config: config.map(Path::to_path_buf),
            path_var: std::env::var_os("PATH"),
            well_known: well_known(Os::current(), &|name| std::env::var_os(name)),
            deadline: region_deadline(),
        }
    }
}

/// `ocr.region_deadline_secs`, the hang detector every Tesseract call runs under.
pub fn region_deadline() -> Duration {
    Duration::from_secs(u64::try_from(T.ocr.region_deadline_secs).unwrap_or_default())
}

/// The file name a Tesseract binary has on this platform.
pub fn binary_name() -> &'static str {
    if cfg!(windows) {
        "tesseract.exe"
    } else {
        "tesseract"
    }
}

type Cache = BTreeMap<Option<PathBuf>, Result<TesseractInfo, OcrUnavailable>>;

/// Discover Tesseract for this process, once per configured path.
pub fn discover(cfg: Option<&Path>) -> Result<TesseractInfo, OcrUnavailable> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    let key = cfg.map(Path::to_path_buf);
    if let Some(known) = cache
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&key)
    {
        return known.clone();
    }
    let found = discover_with(&Probe::from_env(cfg));
    cache
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(key, found.clone());
    found
}

/// Discover Tesseract under `probe`, uncached: the first candidate found is the answer, whether or
/// not it is usable. A refused binary on `PATH` is not a reason to go looking for another one.
pub fn discover_with(probe: &Probe) -> Result<TesseractInfo, OcrUnavailable> {
    let (path, source) = candidate(probe).ok_or(OcrUnavailable::NotFound)?;
    trusted(&path)?;
    let version = version_of(&path, probe.deadline)?;
    if version.major < MIN_MAJOR {
        return Err(OcrUnavailable::TooOld(version));
    }
    let langs = langs_of(&path, probe.deadline)?;
    Ok(TesseractInfo {
        path,
        version,
        langs,
        source,
    })
}

/// The first candidate in the fixed order, if any.
fn candidate(probe: &Probe) -> Option<(PathBuf, DiscoverySource)> {
    if let Some(config) = &probe.config {
        return config
            .is_file()
            .then(|| (config.clone(), DiscoverySource::ConfigPath));
    }
    if let Some(path_var) = &probe.path_var {
        for dir in std::env::split_paths(path_var) {
            // A relative `PATH` entry names a different directory from every working directory,
            // and "the candidate must be an absolute path" is how that ambiguity is refused.
            if !dir.is_absolute() {
                continue;
            }
            let path = dir.join(binary_name());
            if path.is_file() {
                return Some((path, DiscoverySource::EnvPath));
            }
        }
    }
    probe
        .well_known
        .iter()
        .find(|known| known.path.is_file())
        .map(|known| (known.path.clone(), DiscoverySource::WellKnown(known.label)))
}

/// Refuse a candidate that is relative, misnamed, or writable by anyone but its owner.
fn trusted(path: &Path) -> Result<(), OcrUnavailable> {
    let untrusted = || OcrUnavailable::Untrusted(path.to_path_buf());
    if !path.is_absolute() {
        return Err(untrusted());
    }
    let named = path
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case(binary_name()));
    if !named {
        return Err(untrusted());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        /// Group-write and other-write: `chmod g+w` or `o+w`.
        const WRITABLE_BY_OTHERS: u32 = 0o022;
        let mode = std::fs::metadata(path)
            .map_err(|error| OcrUnavailable::Unrunnable(error.to_string()))?
            .permissions()
            .mode();
        if mode & WRITABLE_BY_OTHERS != 0 {
            return Err(untrusted());
        }
    }
    Ok(())
}

fn version_of(path: &Path, deadline: Duration) -> Result<Version, OcrUnavailable> {
    let mut command = std::process::Command::new(path);
    command.arg("--version");
    let output = run_captured(command, deadline).map_err(unrunnable)?;
    // Tesseract 5 prints its banner on stdout; 3.x and some 4.x builds printed it on stderr.
    let banner = first_line(&output.stdout).or_else(|| first_line(&output.stderr));
    banner
        .as_deref()
        .and_then(Version::parse_banner)
        .ok_or_else(|| {
            OcrUnavailable::Unrunnable(format!(
                "`--version` printed no version banner: {:?}",
                banner.unwrap_or_default()
            ))
        })
}

fn langs_of(path: &Path, deadline: Duration) -> Result<BTreeSet<String>, OcrUnavailable> {
    let mut command = std::process::Command::new(path);
    command.arg("--list-langs");
    let output = run_captured(command, deadline).map_err(unrunnable)?;
    if !output.success() {
        return Err(OcrUnavailable::Unrunnable(
            "`--list-langs` failed".to_owned(),
        ));
    }
    // The first line is `List of available languages in "…" (N):`; every line after it is one
    // traineddata name.
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .skip(1)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect())
}

fn first_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

fn unrunnable(error: RunError) -> OcrUnavailable {
    OcrUnavailable::Unrunnable(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// VD-g: the banners the three platforms print, including the UB-Mannheim form with a `v` and
    /// a build date, which a parser written against `tesseract 5.3.4` alone would have refused.
    #[test]
    fn every_platforms_version_banner_parses() {
        let parsed = |line: &str| Version::parse_banner(line).map(|v| v.to_string());
        assert_eq!(parsed("tesseract 5.3.4").as_deref(), Some("5.3.4"));
        assert_eq!(
            parsed("tesseract v5.3.0.20221214").as_deref(),
            Some("5.3.0")
        );
        assert_eq!(
            parsed("tesseract v5.5.3.20260724").as_deref(),
            Some("5.5.3")
        );
        assert_eq!(parsed("tesseract 4.1.1").as_deref(), Some("4.1.1"));
        assert_eq!(
            parsed("tesseract 5.0.0-alpha-20201231").as_deref(),
            Some("5.0.0")
        );
        assert_eq!(parsed("tesseract 5").as_deref(), Some("5.0.0"));
        assert_eq!(parsed("leptonica-1.82.0"), None);
        assert_eq!(parsed("tesseract"), None);
        assert_eq!(parsed(""), None);
    }

    /// The per-OS lists are the ones PHASE 13 detail 1 and VD-g name, in that order.
    #[test]
    fn the_well_known_lists_are_the_documented_ones() {
        let none = |_: &str| None;
        let paths = |os: Os, env: &dyn Fn(&str) -> Option<OsString>| {
            well_known(os, env)
                .into_iter()
                .map(|known| known.path)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            paths(Os::MacOs, &none),
            [
                PathBuf::from("/opt/homebrew/bin/tesseract"),
                PathBuf::from("/usr/local/bin/tesseract")
            ]
        );
        assert_eq!(
            paths(Os::Linux, &none),
            [
                PathBuf::from("/usr/bin/tesseract"),
                PathBuf::from("/usr/local/bin/tesseract"),
                PathBuf::from("/snap/bin/tesseract")
            ]
        );
        let windows_env = |name: &str| match name {
            "ProgramFiles" => Some(OsString::from("C:/Program Files")),
            "LOCALAPPDATA" => Some(OsString::from("C:/Users/u/AppData/Local")),
            _ => None,
        };
        let windows = paths(Os::Windows, &windows_env);
        assert_eq!(windows.len(), 2);
        assert!(windows[0].ends_with("Tesseract-OCR/tesseract.exe"));
        assert!(windows[0].starts_with("C:/Program Files"));
        assert!(windows[1].ends_with("Programs/Tesseract-OCR/tesseract.exe"));
        assert!(windows[1].starts_with("C:/Users/u/AppData/Local"));
        // With neither variable set there is nowhere to look, and nothing is guessed.
        assert!(paths(Os::Windows, &none).is_empty());
    }
}
