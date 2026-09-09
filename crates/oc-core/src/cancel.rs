//! Cancellation: one flag, polled at stage boundaries and inside every per-page loop (D13.2).
//!
//! An `AtomicBool` behind an `Arc`, and not a channel, for a reason that decides the shape:
//! the UI sets it from a different thread while the loop is running (Phase 12), and the loop
//! must be able to ask "has this been cancelled?" without blocking, without allocating, and
//! without a `select!` in the middle of a page. A load-relaxed read of an atomic is free
//! enough to do between every page of a nine-hundred-page book.
//!
//! **One flag, later two causes.** Phase 14 gives per-stage deadlines the *same* flag rather
//! than a mechanism of their own (`AbortCause::Deadline`), so that cancel and deadline share
//! one abort path and one place where partial output is cleaned up. Nothing here needs to
//! change for that; the cause is what gets added.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A shared cancellation flag.
///
/// Cloning shares the flag rather than copying it — that is the whole point, and it is why
/// this is a wrapper rather than a bare `Arc<AtomicBool>`: `Clone` on the bare type is equally
/// shared but reads as though it might not be.
#[derive(Clone, Debug, Default)]
pub struct Cancel {
    flag: Arc<AtomicBool>,
}

impl Cancel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ask for the work to stop. Idempotent, and callable from any thread.
    pub fn cancel(&self) {
        // `Release` pairs with the `Acquire` below so that anything the cancelling thread did
        // first — writing a reason, closing a file — is visible to the thread that observes
        // the flag.
        self.flag.store(true, Ordering::Release);
    }

    /// Whether cancellation has been asked for.
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::Acquire)
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
