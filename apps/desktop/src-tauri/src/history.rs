//! The conversion history: every conversion that finished, kept across restarts, so the main page
//! can list the books of earlier sessions ("Previous conversions").
//!
//! One JSON file in the app's data directory (`history.json`), newest first, written atomically —
//! a temporary file, then a rename. A missing file is an empty history; a file that cannot be read
//! is set aside as `history.json.bak` and the history starts empty, so a damaged file never stops
//! the app. It holds at most `desktop.history_max_entries` entries, and one per output file:
//! converting to a path again replaces that path's entry (a password typed after a failure, a
//! "Fix and rebuild").
//!
//! An entry is what the queue reported of the job ([`Finished`]) and what the engine's report says
//! of the book: its title, authors and page count, whether it validated, and what stopped it. A
//! cancelled job is not a conversion and is not recorded, nor is one that waited for consent and
//! never ran (D10).
//!
//! The webview names entries by id only. The paths the app opens or reveals for one are the ones
//! recorded here, and only an EPUB on disk is ever handed to the OS to open
//! ([`crate::fs_scope::openable_epub`]).

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use oc_core::exit::ExitCode;
use oc_core::thresholds::T;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::engine::UiError;
use crate::jobqueue::{Finished, JobState};

/// The history's file in the app's data directory.
pub const FILE_NAME: &str = "history.json";

/// What the file says it is; a file that says anything else is not read as a history.
const SCHEMA: &str = "openconvert.history/1";

/// The report status of a book that was written but did not validate (PIPELINE §13).
const REPORT_INVALID: &str = "invalid";

/// How a recorded conversion ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// The book was written and validated.
    Complete,
    /// The book was written; validation found errors it could not repair. It can still be opened.
    Invalid,
    /// No book was written.
    Failed,
}

/// One recorded conversion. Field names are the webview's (`src/lib/backend.ts`, `HistoryEntry`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// `<session>-<job>`: unique across restarts.
    pub id: String,
    /// The app run that recorded it.
    pub session: String,
    pub input: PathBuf,
    /// The PDF's file name, for a row whose book has no title.
    pub input_name: String,
    pub output: PathBuf,
    pub report: PathBuf,
    /// RFC 3339, UTC, to the second. `None` for a job whose engine never started.
    #[serde(default)]
    pub started_at: Option<String>,
    pub finished_at: String,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    pub status: Status,
    /// The engine's exit code; `None` when it never ran or a signal ended it.
    #[serde(default)]
    pub exit_code: Option<i32>,
    /// What stopped a failed conversion: the engine's `fatal` code (`E_PDF`, `E_LIMIT_EXCEEDED`…),
    /// or `E_START` when the engine could not be started.
    #[serde(default)]
    pub error_code: Option<String>,
    /// The cap that fired, by its `thresholds.toml` name (`stage_deadline_secs`…), when one did.
    #[serde(default)]
    pub error_cap: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub pages: Option<u64>,
}

/// An entry as the main page lists it: the entry, and whether its book is still where it was.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    #[serde(flatten)]
    pub entry: Entry,
    pub output_exists: bool,
}

/// The file.
#[derive(Serialize, Deserialize)]
struct Stored {
    schema: String,
    entries: Vec<Entry>,
}

/// The code a failure is recorded with when the engine could not be started at all.
const START_FAILED: &str = "E_START";

/// `time` as RFC 3339, UTC, to the second (whole seconds by way of the Unix timestamp).
fn rfc3339(time: SystemTime) -> String {
    let utc = time::OffsetDateTime::from(time);
    time::OffsetDateTime::from_unix_timestamp(utc.unix_timestamp())
        .unwrap_or(utc)
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

/// The report the engine wrote for the run that started at `started`, or `None` — no report, one
/// that is not JSON, or one older than the run (a previous conversion's to the same path).
fn fresh_report(path: &Path, started: Option<SystemTime>) -> Option<Value> {
    let modified = std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()?;
    if started.is_some_and(|started| modified < started) {
        return None;
    }
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn text(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

impl Entry {
    /// The entry for a finished job, in session `session`, with the engine's `report` of it —
    /// `None` for a job that is not recorded: one the user cancelled, and one that never ran
    /// because its endpoint's host has no consent.
    pub fn from_finished(job: &Finished, session: &str, report: Option<&Value>) -> Option<Self> {
        if job.cancelled {
            return None;
        }
        let failure = report.map(|report| &report["failure"]);
        let (status, exit_code, error_code) = match &job.state {
            JobState::Exited { code: Some(code) } if *code == ExitCode::Ok.code() => {
                let invalid =
                    report.and_then(|report| report["status"].as_str()) == Some(REPORT_INVALID);
                let status = if invalid {
                    Status::Invalid
                } else {
                    Status::Complete
                };
                (status, Some(*code), None)
            }
            JobState::Exited { code: Some(code) } if *code == ExitCode::Cancelled.code() => {
                return None
            }
            JobState::Exited { code } => (
                Status::Failed,
                *code,
                failure.and_then(|failure| text(&failure["code"])),
            ),
            JobState::FailedToStart {
                error: UiError::ConsentRequired { .. },
            } => return None,
            JobState::FailedToStart { .. } => (Status::Failed, None, Some(START_FAILED.to_owned())),
            JobState::Queued { .. }
            | JobState::Preparing
            | JobState::Running
            | JobState::Cancelling
            | JobState::CancelledBeforeStart => return None,
        };
        let document = report.map(|report| &report["document"]);
        Some(Self {
            id: format!("{session}-{}", job.id),
            session: session.to_owned(),
            input_name: job
                .input
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            input: job.input.clone(),
            output: job.output.clone(),
            report: job.report.clone(),
            started_at: job.started.map(rfc3339),
            finished_at: rfc3339(job.ended),
            duration_ms: job
                .started
                .and_then(|started| job.ended.duration_since(started).ok())
                .map(|took| u64::try_from(took.as_millis()).unwrap_or(u64::MAX)),
            status,
            exit_code,
            error_code,
            error_cap: failure.and_then(|failure| text(&failure["cap"])),
            title: document.and_then(|document| text(&document["title"])),
            authors: document
                .and_then(|document| document["authors"].as_array())
                .map(|authors| authors.iter().filter_map(text).collect())
                .unwrap_or_default(),
            pages: report.and_then(|report| report["input"]["pages"].as_u64()),
        })
    }
}

/// The history, as the app holds it: the entries, the file they are kept in, and this run's
/// session.
#[derive(Debug)]
pub struct History {
    path: PathBuf,
    max: usize,
    session: String,
    entries: Vec<Entry>,
}

/// A name for this run of the app: when it started, and its process.
pub fn new_session() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis())
        .unwrap_or_default();
    format!("{millis}-{}", std::process::id())
}

impl History {
    /// The history kept at `path`, for session `session`, capped at `desktop.history_max_entries`.
    ///
    /// A missing file is an empty history. A file that is not one is renamed to `<path>.bak` —
    /// kept, not deleted — and the history starts empty.
    pub fn open(path: PathBuf, session: String) -> Self {
        let max = usize::try_from(T.desktop.history_max_entries).unwrap_or(usize::MAX);
        Self::open_capped(path, session, max)
    }

    /// [`History::open`] with a cap of `max` entries.
    pub fn open_capped(path: PathBuf, session: String, max: usize) -> Self {
        let entries = match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Stored>(&text) {
                Ok(stored) if stored.schema == SCHEMA => stored.entries,
                _ => {
                    let _ = std::fs::rename(&path, backup_of(&path));
                    Vec::new()
                }
            },
            Err(_) => Vec::new(),
        };
        let mut history = Self {
            path,
            max,
            session,
            entries,
        };
        history.entries.truncate(history.max);
        history
    }

    /// Every entry, newest first.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The entries of earlier runs of the app, newest first: "Previous conversions". This run's
    /// conversions are its queue's rows.
    pub fn earlier(&self) -> Vec<Row> {
        self.entries
            .iter()
            .filter(|entry| entry.session != self.session)
            .map(|entry| Row {
                output_exists: entry.output.is_file(),
                entry: entry.clone(),
            })
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    /// Record a finished job, reading the report its engine wrote. Returns whether it was
    /// recorded ([`Entry::from_finished`]).
    pub fn record_finished(&mut self, job: &Finished) -> Result<bool, UiError> {
        // Only an engine that ran wrote a report; any other there is an earlier run's.
        let report = match job.state {
            JobState::Exited { .. } => fresh_report(&job.report, job.started),
            _ => None,
        };
        match Entry::from_finished(job, &self.session, report.as_ref()) {
            Some(entry) => self.record(entry).map(|()| true),
            None => Ok(false),
        }
    }

    /// Put `entry` first, in place of any entry for the same output file, drop what is past the
    /// cap, and save.
    pub fn record(&mut self, entry: Entry) -> Result<(), UiError> {
        self.entries
            .retain(|known| known.output != entry.output && known.id != entry.id);
        self.entries.insert(0, entry);
        self.entries.truncate(self.max);
        self.save()
    }

    /// Forget entry `id`; its book is not touched. Returns whether there was one.
    pub fn remove(&mut self, id: &str) -> Result<bool, UiError> {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.id != id);
        if self.entries.len() == before {
            return Ok(false);
        }
        self.save().map(|()| true)
    }

    /// Forget every entry; no book is touched.
    pub fn clear(&mut self) -> Result<(), UiError> {
        self.entries.clear();
        self.save()
    }

    /// Write the file atomically: a temporary beside it, then a rename over it.
    fn save(&self) -> Result<(), UiError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let stored = Stored {
            schema: SCHEMA.to_owned(),
            entries: self.entries.clone(),
        };
        let text = serde_json::to_string_pretty(&stored)
            .map_err(|error| UiError::Io(error.to_string()))?;
        let partial = suffixed(&self.path, ".part");
        std::fs::write(&partial, text)?;
        std::fs::rename(&partial, &self.path)?;
        Ok(())
    }
}

/// `path` with `suffix` added to its file name.
fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

/// Where a file that could not be read as a history is kept.
pub fn backup_of(path: &Path) -> PathBuf {
    suffixed(path, ".bak")
}

/// Where the history lives.
pub fn history_path(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE_NAME)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("oc-desktop-history-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("made");
        dir
    }

    fn finished(dir: &Path, name: &str, state: JobState) -> Finished {
        let started = SystemTime::now() - Duration::from_secs(90);
        let output = dir.join(format!("{name}.epub"));
        Finished {
            id: format!("job-{name}"),
            input: dir.join(format!("{name}.pdf")),
            report: suffixed(&output, ".report.json"),
            output,
            started: Some(started),
            ended: started + Duration::from_millis(83_250),
            cancelled: false,
            state,
        }
    }

    fn ok_report() -> Value {
        serde_json::json!({
            "status": "ok",
            "input": {"filename": "novel.pdf", "pages": 214},
            "document": {"title": "A Short Novel", "authors": ["O. Convert"]},
        })
    }

    /// A finished job becomes an entry with what the report says of the book, its times and how
    /// it ended.
    #[test]
    fn an_entry_is_made_from_the_finished_job_and_its_report() {
        let dir = scratch("entry");
        let job = finished(&dir, "novel", JobState::Exited { code: Some(0) });
        let entry = Entry::from_finished(&job, "s1", Some(&ok_report())).expect("recorded");
        assert_eq!(entry.id, "s1-job-novel");
        assert_eq!(entry.input_name, "novel.pdf");
        assert_eq!(entry.status, Status::Complete);
        assert_eq!(entry.title.as_deref(), Some("A Short Novel"));
        assert_eq!(entry.authors, ["O. Convert"]);
        assert_eq!(entry.pages, Some(214));
        assert_eq!(entry.duration_ms, Some(83_250));
        assert_eq!(entry.exit_code, Some(0));
        let started = entry.started_at.expect("started");
        assert!(
            started.ends_with('Z') && !started.contains('.'),
            "{started}"
        );

        let mut invalid = ok_report();
        invalid["status"] = "invalid".into();
        let entry = Entry::from_finished(&job, "s1", Some(&invalid)).expect("recorded");
        assert_eq!(
            entry.status,
            Status::Invalid,
            "written, but did not validate"
        );

        let failed = finished(&dir, "big", JobState::Exited { code: Some(1) });
        let report = serde_json::json!({
            "status": "failed",
            "input": {"filename": "big.pdf"},
            "failure": {"code": "E_LIMIT_EXCEEDED", "message": "…", "cap": "stage_deadline_secs"},
        });
        let entry = Entry::from_finished(&failed, "s1", Some(&report)).expect("recorded");
        assert_eq!(entry.status, Status::Failed);
        assert_eq!(entry.error_code.as_deref(), Some("E_LIMIT_EXCEEDED"));
        assert_eq!(entry.error_cap.as_deref(), Some("stage_deadline_secs"));
        assert_eq!(entry.title, None);

        let unstarted = Finished {
            started: None,
            ..finished(
                &dir,
                "gone",
                JobState::FailedToStart {
                    error: UiError::Io("no such file".to_owned()),
                },
            )
        };
        let entry = Entry::from_finished(&unstarted, "s1", None).expect("recorded");
        assert_eq!(entry.error_code.as_deref(), Some("E_START"));
        assert_eq!((entry.started_at, entry.duration_ms), (None, None));
    }

    /// Cancelled is not a conversion; a job that waited for consent never ran.
    #[test]
    fn cancelled_and_unconsented_jobs_are_not_recorded() {
        let dir = scratch("skipped");
        let cancelled = Finished {
            cancelled: true,
            ..finished(&dir, "a", JobState::Exited { code: None })
        };
        assert_eq!(Entry::from_finished(&cancelled, "s", None), None);
        let exit_cancelled = finished(
            &dir,
            "b",
            JobState::Exited {
                code: Some(ExitCode::Cancelled.code()),
            },
        );
        assert_eq!(Entry::from_finished(&exit_cancelled, "s", None), None);
        let never = finished(&dir, "c", JobState::CancelledBeforeStart);
        assert_eq!(Entry::from_finished(&never, "s", None), None);
        let consent = finished(
            &dir,
            "d",
            JobState::FailedToStart {
                error: UiError::ConsentRequired {
                    host: "llm.example.org".to_owned(),
                },
            },
        );
        assert_eq!(Entry::from_finished(&consent, "s", None), None);
    }

    /// The file round-trips, newest first; a missing file is empty; writes leave no temporary.
    #[test]
    fn the_history_is_kept_across_restarts() {
        let dir = scratch("restart");
        let path = history_path(&dir);
        let mut first = History::open(path.clone(), "s1".to_owned());
        assert!(first.entries().is_empty(), "no file yet: empty");

        for name in ["one", "two"] {
            let job = finished(&dir, name, JobState::Exited { code: Some(0) });
            std::fs::write(&job.report, ok_report().to_string()).expect("the engine's report");
            assert!(first.record_finished(&job).expect("saved"));
        }
        assert!(
            !suffixed(&path, ".part").exists(),
            "no temporary left behind"
        );
        assert_eq!(first.entries()[0].id, "s1-job-two", "newest first");
        assert_eq!(first.entries()[0].pages, Some(214), "read from its report");
        assert!(
            first.earlier().is_empty(),
            "this run's are the queue's rows"
        );

        let second = History::open(path, "s2".to_owned());
        assert_eq!(second.entries(), first.entries());
        let earlier = second.earlier();
        assert_eq!(earlier.len(), 2, "a restart lists them as earlier");
        assert!(!earlier[0].output_exists, "its book was never written here");
        std::fs::write(dir.join("one.epub"), "x").expect("the book");
        assert!(second.earlier()[1].output_exists);
        let json = serde_json::to_value(&second.earlier()[0]).expect("json");
        assert_eq!(json["inputName"], "two.pdf", "camelCase for the webview");
        assert_eq!(json["outputExists"], false);
    }

    /// A report older than the run — a previous conversion's, to the same path — says nothing
    /// about this one.
    #[test]
    fn a_stale_report_is_not_read() {
        let dir = scratch("stale");
        let job = finished(&dir, "book", JobState::Exited { code: Some(1) });
        std::fs::write(&job.report, ok_report().to_string()).expect("an old report");
        let later = Finished {
            started: Some(SystemTime::now() + Duration::from_secs(3600)),
            ..job
        };
        let mut history = History::open(history_path(&dir), "s".to_owned());
        history.record_finished(&later).expect("saved");
        assert_eq!(history.entries()[0].pages, None);
    }

    /// A file that is not a history is set aside, not lost, and the app carries on with none.
    #[test]
    fn a_damaged_file_is_kept_as_bak_and_the_history_starts_empty() {
        let dir = scratch("damaged");
        let path = history_path(&dir);
        std::fs::write(&path, "{ not json").expect("damaged");
        let mut history = History::open(path.clone(), "s".to_owned());
        assert!(history.entries().is_empty());
        assert_eq!(
            std::fs::read_to_string(backup_of(&path)).expect("kept"),
            "{ not json"
        );
        let job = finished(&dir, "a", JobState::Exited { code: Some(0) });
        history.record_finished(&job).expect("saved");
        assert_eq!(
            History::open(path.clone(), "t".to_owned()).entries().len(),
            1
        );

        std::fs::write(&path, r#"{"schema":"something/9","entries":[]}"#).expect("foreign");
        assert!(History::open(path.clone(), "t".to_owned())
            .entries()
            .is_empty());
        assert!(backup_of(&path).exists());
    }

    /// At most `max` entries, the oldest dropped; one entry per book file; remove and clear.
    #[test]
    fn the_history_is_capped_and_keeps_one_entry_per_book() {
        let dir = scratch("capped");
        let mut history = History::open_capped(history_path(&dir), "s".to_owned(), 3);
        for name in ["a", "b", "c", "d"] {
            history
                .record_finished(&finished(&dir, name, JobState::Exited { code: Some(0) }))
                .expect("saved");
        }
        let ids: Vec<&str> = history.entries().iter().map(|e| e.id.as_str()).collect();
        assert_eq!(
            ids,
            ["s-job-d", "s-job-c", "s-job-b"],
            "the oldest is dropped"
        );

        // A password typed after a failure writes the same file: its entry replaces the failure's.
        let again = Finished {
            id: "job-b2".to_owned(),
            ..finished(&dir, "b", JobState::Exited { code: Some(0) })
        };
        history.record_finished(&again).expect("saved");
        let ids: Vec<&str> = history.entries().iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["s-job-b2", "s-job-d", "s-job-c"]);

        assert!(history.remove("s-job-d").expect("saved"));
        assert!(!history.remove("s-job-d").expect("nothing to do"));
        assert!(history.get("s-job-d").is_none());
        let reopened = History::open(history_path(&dir), "s".to_owned());
        assert_eq!(reopened.entries().len(), 2, "the removal was saved");

        history.clear().expect("saved");
        assert!(History::open(history_path(&dir), "s".to_owned())
            .entries()
            .is_empty());
    }

    #[test]
    fn the_cap_is_the_threshold() {
        let history = History::open(history_path(&scratch("cap")), new_session());
        assert_eq!(
            history.max,
            usize::try_from(T.desktop.history_max_entries).expect("positive")
        );
    }
}
