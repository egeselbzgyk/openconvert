//! Per-stage deadlines on the cancel flag, and the one place scratch files are removed
//! (PHASE 14 detail 6, D13.2).
//!
//! A deadline does not get a mechanism of its own. [`DeadlineGuard::arm`] records "stage `s`
//! must be done by `t`" on the job's [`Cancel`]; every poll of that flag — at each stage boundary
//! and inside every per-page loop, the same polls a user's cancel is noticed by — also asks the
//! clock, and once `t` has passed sets the flag with [`AbortCause::Deadline`]. So the cancel tests
//! and the deadline tests exercise one path, and there is exactly one [`Scratch::clean_up`].
//!
//! What a poll cannot catch is a single native call that never returns (a PDFium page that takes
//! an hour). That is what the supervisor's kill is for, five seconds after it asked (D13.2); the
//! caps in `limits` exist so that such a call has nothing unbounded to do.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use crate::cancel::{AbortCause, Cancel};
use crate::limits::CapViolation;

/// Where the time comes from. Injected, so a deadline is a test and not a wait.
pub trait Clock: Send + Sync + std::fmt::Debug {
    /// Milliseconds since some fixed point; only differences are read.
    fn now_ms(&self) -> u64;
}

/// The process's monotonic clock.
#[derive(Debug)]
pub struct SystemClock {
    origin: std::time::Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            origin: std::time::Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        u64::try_from(self.origin.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

/// A clock that moves only when told to. For tests, here and downstream.
#[derive(Debug, Default)]
pub struct ManualClock {
    now: AtomicU64,
}

impl ManualClock {
    pub fn new(start_ms: u64) -> Self {
        Self {
            now: AtomicU64::new(start_ms),
        }
    }

    pub fn advance(&self, ms: u64) {
        self.now.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> u64 {
        self.now.load(Ordering::SeqCst)
    }
}

/// `at_ms` when nothing is armed.
const DISARMED: u64 = u64::MAX;

/// The deadline half of a [`Cancel`]: shared by its clones, read on every poll.
#[derive(Debug)]
pub(crate) struct Armed {
    clock: Arc<dyn Clock>,
    at_ms: AtomicU64,
    stage: Mutex<&'static str>,
    /// Which arming is current, so a stale guard's drop cannot disarm its successor.
    generation: AtomicU64,
}

impl Armed {
    pub(crate) fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            clock,
            at_ms: AtomicU64::new(DISARMED),
            stage: Mutex::new(""),
            generation: AtomicU64::new(0),
        }
    }

    /// The stage whose deadline has passed, if one has.
    pub(crate) fn expired(&self) -> Option<&'static str> {
        let at = self.at_ms.load(Ordering::Acquire);
        if at == DISARMED || self.clock.now_ms() < at {
            return None;
        }
        Some(*self.stage.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

impl Default for Armed {
    fn default() -> Self {
        Self::new(Arc::new(SystemClock::new()))
    }
}

/// "Stage `stage` must end within `limit`", armed on a job's [`Cancel`] for as long as the guard
/// lives. Dropping it disarms; arming the next stage replaces it.
#[derive(Debug)]
pub struct DeadlineGuard {
    stage: &'static str,
    limit: Duration,
    cancel: Cancel,
    generation: u64,
}

impl DeadlineGuard {
    pub fn arm(stage: &'static str, limit: Duration, cancel: &Cancel) -> Self {
        let armed = cancel.armed();
        let limit_ms = u64::try_from(limit.as_millis()).unwrap_or(u64::MAX);
        let generation = armed.generation.fetch_add(1, Ordering::AcqRel) + 1;
        *armed.stage.lock().unwrap_or_else(PoisonError::into_inner) = stage;
        armed.at_ms.store(
            armed.clock.now_ms().saturating_add(limit_ms),
            Ordering::Release,
        );
        Self {
            stage,
            limit,
            cancel: cancel.clone(),
            generation,
        }
    }

    /// The refusal a report prints for this deadline.
    pub fn violation(&self) -> CapViolation {
        CapViolation::Deadline {
            stage: self.stage,
            limit: self.limit,
        }
    }
}

impl Drop for DeadlineGuard {
    fn drop(&mut self) {
        let armed = self.cancel.armed();
        if armed.generation.load(Ordering::Acquire) == self.generation {
            armed.at_ms.store(DISARMED, Ordering::Release);
        }
    }
}

impl AbortCause {
    /// The cap a deadline abort reports, with the limit it ran past. `None` for a cancel, which
    /// is the user's answer and not a failure.
    pub fn violation(&self, limit: Duration) -> Option<CapViolation> {
        match self {
            AbortCause::Cancelled => None,
            AbortCause::Deadline(stage) => Some(CapViolation::Deadline { stage, limit }),
        }
    }
}

/// A job's scratch files: everything that must not outlive an aborted or failed run.
///
/// **The one cleanup.** Whatever ended the run — a cancel, a deadline, a cap, an error — calls
/// [`Scratch::clean_up`], and it removes each registered path exactly once: the paths are taken
/// out of the list as they are removed, so a second call (a second abort path, had one crept in)
/// finds nothing to do instead of deleting twice.
#[derive(Debug, Default)]
pub struct Scratch {
    paths: Mutex<Vec<PathBuf>>,
}

impl Scratch {
    pub fn new() -> Self {
        Self::default()
    }

    /// Remember `path` for removal. A directory is removed with its contents.
    pub fn register(&self, path: PathBuf) {
        self.paths
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(path);
    }

    /// Stop tracking `path`: it has become the user's output (the atomic rename happened).
    pub fn release(&self, path: &std::path::Path) {
        self.paths
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|kept| kept != path);
    }

    /// Remove every registered path that exists. Returns how many were removed.
    pub fn clean_up(&self) -> usize {
        let taken = std::mem::take(&mut *self.paths.lock().unwrap_or_else(PoisonError::into_inner));
        taken
            .into_iter()
            .filter(|path| {
                if path.is_dir() {
                    std::fs::remove_dir_all(path).is_ok()
                } else {
                    std::fs::remove_file(path).is_ok()
                }
            })
            .count()
    }
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Row 14.8.
// ---------------------------------------------------------------------------

/// Test 14.8.
///
/// The clock is injected, so "the deadline passes" is a line of the test rather than five
/// minutes of it. The assertions are the plan's: the deadline sets **the cancel flag** — the one
/// every loop already polls — with `AbortCause::Deadline`, and the scratch file is removed once.
/// A second, independent deadline path would show up here as a flag nobody polls (the first
/// assertion) or a second removal (the last).
#[test]
fn deadline_and_cancel_share_one_abort_path() {
    let clock = Arc::new(ManualClock::new(0));
    let cancel = Cancel::with_clock(clock.clone());
    let poller = cancel.clone(); // what a per-page loop holds

    let directory = std::env::temp_dir().join(format!(
        "oc-deadline-{}-{}",
        std::process::id(),
        clock.now_ms()
    ));
    std::fs::create_dir_all(&directory).expect("temp dir");
    let partial = directory.join("book.epub.oc-tmp-1");
    std::fs::write(&partial, b"half a book").expect("scratch file");
    let scratch = Scratch::new();
    scratch.register(partial.clone());

    let limit = Duration::from_secs(300);
    let guard = DeadlineGuard::arm("layout", limit, &cancel);
    assert!(!poller.is_cancelled(), "armed is not expired");
    clock.advance(299_999);
    assert!(!poller.is_cancelled(), "a millisecond early is not late");
    clock.advance(1);
    assert!(
        poller.is_cancelled(),
        "the deadline is noticed by the same poll a cancel is"
    );
    assert_eq!(poller.cause(), Some(AbortCause::Deadline("layout")));
    assert_eq!(
        poller.cause().and_then(|cause| cause.violation(limit)),
        Some(guard.violation())
    );

    // A cancel that arrives afterwards finds the flag already set, and does not rewrite why.
    cancel.cancel();
    assert_eq!(cancel.cause(), Some(AbortCause::Deadline("layout")));

    // The one cleanup, reached twice: once by the deadline's ending and once by the cancel's.
    assert_eq!(scratch.clean_up(), 1);
    assert!(!partial.exists(), "the partial output is gone");
    assert_eq!(scratch.clean_up(), 0, "and it is not deleted twice");
    let _ = std::fs::remove_dir_all(&directory);

    // And a cancel on its own is a cancel: no deadline, no violation.
    let plain = Cancel::with_clock(clock.clone());
    plain.cancel();
    assert_eq!(plain.cause(), Some(AbortCause::Cancelled));
    assert_eq!(AbortCause::Cancelled.violation(limit), None);
}

/// Dropping the guard disarms it: a stage that finished in time cannot fail later.
#[test]
fn a_dropped_deadline_never_fires() {
    let clock = Arc::new(ManualClock::new(0));
    let cancel = Cancel::with_clock(clock.clone());
    {
        let _guard = DeadlineGuard::arm("text", Duration::from_secs(1), &cancel);
    }
    clock.advance(10_000);
    assert!(!cancel.is_cancelled());

    // Arming the next stage replaces the previous deadline rather than adding to it, and the
    // replaced guard's drop does not disarm its successor.
    let first = DeadlineGuard::arm("text", Duration::from_secs(1), &cancel);
    let _second = DeadlineGuard::arm("layout", Duration::from_secs(5), &cancel);
    drop(first);
    clock.advance(2_000);
    assert!(!cancel.is_cancelled(), "the text deadline was replaced");
    clock.advance(3_000);
    assert!(cancel.is_cancelled(), "the layout deadline is still armed");
    assert_eq!(cancel.cause(), Some(AbortCause::Deadline("layout")));
}
