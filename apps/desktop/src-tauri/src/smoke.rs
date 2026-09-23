//! `--smoke-convert <pdf>`: the installed app converts one book without a person at the window,
//! and exits with the outcome (PHASE 15 rows 15.7 and 15.19).
//!
//! It is the app, not a second program: the same startup (the version handshake with the bundled
//! engine), the same queue, the same supervisor and the same engine a drop would reach, only with
//! the drop made from the command line and the exit taken when the row finishes. What it proves is
//! what a release smoke test has to: that the installed bundle launches — the web view's libraries
//! load, the window opens, under `xvfb-run` on a runner with no display — that it finds its own
//! engine and that engine finds its own PDFium, and that a PDF becomes an EPUB. Nothing about it
//! reaches further than a drop does: the book is written where a drop would write it, beside the
//! PDF, never over an existing file.

use std::ffi::OsString;
use std::path::PathBuf;

use crate::jobqueue::{JobState, JobView};

/// The flag. Nothing else on the app's command line means anything.
pub const FLAG: &str = "--smoke-convert";

/// The exit code for "the smoke conversion could not start", as for the engine's usage errors
/// (§2.4).
pub const EXIT_NOT_STARTED: i32 = 2;

/// The PDF to convert, when the app was started with `--smoke-convert <pdf>`.
///
/// `Err` for the flag without a path: the app should say so and exit rather than open a window
/// and wait for a drop nobody will make.
pub fn requested(args: impl IntoIterator<Item = OsString>) -> Result<Option<PathBuf>, String> {
    let mut args = args.into_iter().skip(1);
    while let Some(arg) = args.next() {
        if arg == FLAG {
            return match args.next() {
                Some(pdf) => Ok(Some(PathBuf::from(pdf))),
                None => Err(format!("{FLAG} needs the path of a PDF")),
            };
        }
    }
    Ok(None)
}

/// The process exit code once the smoke job has finished, or `None` while it has not: the engine's
/// own code for a run it finished (0 is a valid EPUB), and [`EXIT_NOT_STARTED`] for a job that
/// never ran.
pub fn finished(view: &JobView) -> Option<i32> {
    match &view.state {
        JobState::Exited { code } => Some(code.unwrap_or(EXIT_NOT_STARTED)),
        JobState::FailedToStart { .. } | JobState::CancelledBeforeStart => Some(EXIT_NOT_STARTED),
        JobState::Queued { .. }
        | JobState::Preparing
        | JobState::Running
        | JobState::Cancelling => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn only_the_smoke_flag_asks_for_a_smoke_conversion() {
        assert_eq!(requested(args(&["app"])), Ok(None));
        assert_eq!(requested(args(&["app", "book.pdf"])), Ok(None));
        assert_eq!(
            requested(args(&["app", FLAG, "/tmp/book.pdf"])),
            Ok(Some(PathBuf::from("/tmp/book.pdf")))
        );
        assert!(requested(args(&["app", FLAG])).is_err());
    }

    #[test]
    fn a_smoke_job_ends_with_the_engines_exit_code() {
        let view = |state| JobView {
            id: "job-1".to_owned(),
            input: PathBuf::from("book.pdf"),
            output: PathBuf::from("book.epub"),
            renamed: false,
            unlocked: false,
            rebuild: false,
            ai: None,
            state,
        };
        assert_eq!(finished(&view(JobState::Running)), None);
        assert_eq!(finished(&view(JobState::Queued { position: 1 })), None);
        assert_eq!(finished(&view(JobState::Exited { code: Some(0) })), Some(0));
        assert_eq!(finished(&view(JobState::Exited { code: Some(1) })), Some(1));
        assert_eq!(
            finished(&view(JobState::Exited { code: None })),
            Some(EXIT_NOT_STARTED)
        );
        assert_eq!(
            finished(&view(JobState::FailedToStart {
                error: crate::engine::UiError::Io("x".to_owned())
            })),
            Some(EXIT_NOT_STARTED)
        );
    }
}
