//! A stand-in for `tesseract`, for the tests that need a *process* (PHASE 13 rows 13.1–13.4, 13.10,
//! 13.18, 13.19): discovery, the argument vector, the deadline, and who kills the child.
//!
//! It is a POSIX shell script, so these tests are Unix-only. That is a real limit rather than a
//! convenience: Windows discovery and invocation are therefore unverified until a Windows runner
//! exists, and `docs/TEST_MATRIX.md` says so. Everything that does not need a process — the TSV
//! parser, the merge, the pipeline's routing — is tested with an in-process engine instead and runs
//! everywhere.
//!
//! The script answers `--version` with a configurable banner and `--list-langs` in Tesseract's own
//! format; any other invocation is an OCR call, which prints a canned TSV after an optional sleep.
//! Every invocation appends its argument vector to a log, so a test can assert both what was asked
//! and that something was *never* asked — "a 4.x binary is never invoked beyond `--version`".

use std::path::{Path, PathBuf};

/// How the fake behaves.
#[derive(Clone, Debug)]
pub struct FakeConfig {
    /// The first line of `--version`, e.g. `tesseract 5.3.4`.
    pub banner: String,
    /// What `--list-langs` lists.
    pub langs: Vec<String>,
    /// What an OCR call prints on stdout.
    pub tsv: String,
    /// Seconds an OCR call sleeps first. The sleep is `exec`ed, so the sleeping process *is* the
    /// fake, with the fake's pid — killing that pid leaves nothing behind.
    pub sleep_secs: u32,
    /// An OCR call's exit status.
    pub exit_code: i32,
    /// The file mode the script is installed with.
    pub mode: u32,
}

impl Default for FakeConfig {
    fn default() -> Self {
        Self {
            banner: "tesseract 5.3.4".to_owned(),
            langs: ["deu", "eng", "osd", "tur"].map(str::to_owned).to_vec(),
            tsv: String::new(),
            sleep_secs: 0,
            exit_code: 0,
            mode: 0o755,
        }
    }
}

/// An installed fake.
#[derive(Clone, Debug)]
pub struct FakeTesseract {
    /// The script, named `tesseract`.
    pub path: PathBuf,
    log: PathBuf,
    pid_file: PathBuf,
}

/// The separator between logged arguments: ASCII unit separator, which no argument contains.
const UNIT: char = '\u{1f}';

impl FakeTesseract {
    /// Write a fake named `tesseract` into `dir` (created if missing).
    pub fn install(dir: &Path, config: &FakeConfig) -> FakeTesseract {
        std::fs::create_dir_all(dir).expect("the fake's directory");
        let path = dir.join("tesseract");
        let log = dir.join("tesseract.log");
        let pid_file = dir.join("tesseract.pid");
        let tsv = dir.join("tesseract.tsv");
        std::fs::write(&tsv, &config.tsv).expect("the canned TSV");
        let _ = std::fs::remove_file(&log);
        let _ = std::fs::remove_file(&pid_file);

        let langs: String = config
            .langs
            .iter()
            .map(|lang| format!("{lang}\\n"))
            .collect();
        let script = format!(
            r#"#!/bin/sh
for a in "$@"; do printf '%s\037' "$a" >> '{log}'; done
printf '\n' >> '{log}'
case "$1" in
  --version) printf '%s\n leptonica-1.82.0\n' '{banner}'; exit 0 ;;
  --list-langs) printf 'List of available languages in "/fake/tessdata/" ({count}):\n{langs}'; exit 0 ;;
esac
echo $$ > '{pid}'
if [ {sleep} -gt 0 ]; then exec sleep {sleep}; fi
cat '{tsv}'
exit {code}
"#,
            log = log.display(),
            banner = config.banner,
            count = config.langs.len(),
            langs = langs,
            pid = pid_file.display(),
            sleep = config.sleep_secs,
            tsv = tsv.display(),
            code = config.exit_code,
        );
        std::fs::write(&path, script).expect("the fake script");
        set_mode(&path, config.mode);
        FakeTesseract {
            path,
            log,
            pid_file,
        }
    }

    /// Every invocation so far, each as its argument vector.
    pub fn invocations(&self) -> Vec<Vec<String>> {
        std::fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(|line| {
                line.split(UNIT)
                    .filter(|arg| !arg.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .collect()
    }

    /// The OCR calls so far: invocations that were neither `--version` nor `--list-langs`.
    pub fn ocr_calls(&self) -> Vec<Vec<String>> {
        self.invocations()
            .into_iter()
            .filter(|argv| {
                argv.first()
                    .is_some_and(|first| first != "--version" && first != "--list-langs")
            })
            .collect()
    }

    /// The pid of the last OCR call, once it has started.
    pub fn last_pid(&self) -> Option<u32> {
        std::fs::read_to_string(&self.pid_file)
            .ok()
            .and_then(|text| text.trim().parse().ok())
    }
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("chmod");
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) {}

/// A TSV with one word per `(text, conf)` pair, laid out left to right on one line of a raster, in
/// Tesseract's exact format — what the fake prints for an OCR call.
pub fn tsv_line(words: &[(&str, f32)]) -> String {
    let mut out = String::from(
        "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n",
    );
    out.push_str("1\t1\t0\t0\t0\t0\t0\t0\t2000\t200\t-1\t\n");
    for (index, (text, conf)) in words.iter().enumerate() {
        let left = 100 + index * 220;
        out.push_str(&format!(
            "5\t1\t1\t1\t1\t{}\t{left}\t100\t200\t40\t{conf}\t{text}\n",
            index + 1
        ));
    }
    out
}
