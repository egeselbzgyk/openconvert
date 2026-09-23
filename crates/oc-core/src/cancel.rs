//! Cancellation: one flag, polled at stage boundaries and inside every per-page loop (D13.2).
//!
//! An `AtomicBool` behind an `Arc`, and not a channel, for a reason that decides the shape:
//! the UI sets it from a different thread while the loop is running (Phase 12), and the loop
//! must be able to ask "has this been cancelled?" without blocking, without allocating, and
//! without a `select!` in the middle of a page. A load-relaxed read of an atomic is free
//! enough to do between every page of a nine-hundred-page book.
//!
//! **One flag, two causes.** Per-stage deadlines set the *same* flag rather than having a
//! mechanism of their own ([`AbortCause::Deadline`], armed by `deadline::DeadlineGuard`), so
//! cancel and deadline share one abort path and one place where partial output is cleaned up.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

/// Why the work was asked to stop (PHASE 14 detail 6): one abort path, two causes.
///
/// A user's `{"t":"cancel"}` and a stage's deadline set the **same** flag; only the cause
/// differs, and it decides the ending — a cancel exits 3, a deadline is a failed conversion that
/// exits 1 with a report naming `stage_deadline_secs`. Everything between the flag and the ending
/// (the polls, the unwinding, the one place scratch files are removed) is shared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbortCause {
    Cancelled,
    Deadline(&'static str),
}

/// A shared cancellation flag.
///
/// Cloning shares the flag rather than copying it — that is the whole point, and it is why
/// this is a wrapper rather than a bare `Arc<AtomicBool>`: `Clone` on the bare type is equally
/// shared but reads as though it might not be.
#[derive(Clone, Debug, Default)]
pub struct Cancel {
    flag: Arc<AtomicBool>,
    /// The first cause to set the flag. Later ones do not overwrite it: a deadline that fires
    /// while a cancel is being honoured does not turn the user's cancel into a failure.
    cause: Arc<OnceLock<AbortCause>>,
    /// The stage deadline currently armed on this flag, if any (`deadline::DeadlineGuard`).
    armed: Arc<crate::deadline::Armed>,
}

impl Cancel {
    pub fn new() -> Self {
        Self::default()
    }

    /// A flag whose deadlines read `clock` instead of the process's monotonic clock.
    pub fn with_clock(clock: Arc<dyn crate::deadline::Clock>) -> Self {
        Self {
            armed: Arc::new(crate::deadline::Armed::new(clock)),
            ..Self::default()
        }
    }

    pub(crate) fn armed(&self) -> &crate::deadline::Armed {
        &self.armed
    }

    /// Ask for the work to stop. Idempotent, and callable from any thread.
    pub fn cancel(&self) {
        self.abort(AbortCause::Cancelled);
    }

    /// Ask for the work to stop, saying why. The first cause is kept.
    pub fn abort(&self, cause: AbortCause) {
        let _ = self.cause.set(cause);
        // `Release` pairs with the `Acquire` below so that anything the cancelling thread did
        // first — writing a reason, closing a file — is visible to the thread that observes
        // the flag.
        self.flag.store(true, Ordering::Release);
    }

    /// Whether cancellation has been asked for — or the armed stage deadline has passed, which
    /// this poll turns into the same flag with [`AbortCause::Deadline`].
    pub fn is_cancelled(&self) -> bool {
        if self.flag.load(Ordering::Acquire) {
            return true;
        }
        match self.armed.expired() {
            Some(stage) => {
                self.abort(AbortCause::Deadline(stage));
                true
            }
            None => false,
        }
    }

    /// Why, once the flag is set.
    pub fn cause(&self) -> Option<AbortCause> {
        if self.flag.load(Ordering::Acquire) {
            self.cause.get().copied()
        } else {
            None
        }
    }
}

/// How a run ended, from the loop's point of view.
///
/// Distinguished from an error because being cancelled is not a failure: it is the answer the
/// user asked for, and it exits 3 rather than 1 (§2.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Completed,
    Cancelled,
}

impl Outcome {
    /// The `done` event's status string (D13.2).
    pub fn status(self) -> &'static str {
        match self {
            Outcome::Completed => "ok",
            Outcome::Cancelled => "cancelled",
        }
    }
}

#[test]
fn cancel_is_shared_by_clones() {
    let first = Cancel::new();
    let second = first.clone();
    assert!(!first.is_cancelled());

    second.cancel();
    assert!(
        first.is_cancelled(),
        "a clone shares the flag, it does not copy it"
    );

    // Idempotent: asking twice is not an error and does not un-cancel.
    second.cancel();
    assert!(first.is_cancelled());
}

#[test]
fn cancel_crosses_threads() {
    let cancel = Cancel::new();
    let other = cancel.clone();
    std::thread::spawn(move || other.cancel())
        .join()
        .expect("the thread runs");
    assert!(cancel.is_cancelled());
}
