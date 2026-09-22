//! Progress reporting: what a long loop tells whoever is watching (D13.2 `progress` events).
//!
//! A trait rather than a concrete sink because the three callers want three different things:
//! the CLI turns it into NDJSON on stderr, a test turns it into an assertion, and a library
//! caller usually wants nothing at all. `Sync` because Phase 12's UI reads it from a different
//! thread than the one producing it.

use std::sync::Arc;

/// Something that wants to know how far along a stage is.
pub trait Progress: Send + Sync {
    /// One unit of work finished — a page, in every Phase 1 loop.
    ///
    /// `index` is zero-based and `total` is what the stage expects to do, which for a page
    /// loop is the page count. Both are given because a percentage computed by the caller is a
    /// percentage the UI cannot re-label.
    fn advance(&self, stage: &str, index: u32, total: u32);

    /// A stage began or ended (§2.3's `stage{name, phase, elapsed_ms}`).
    ///
    /// Provided, as a no-op: most reporters want only the counts, and a stage with no natural
    /// count is exactly the one a UI shows as a spinner between these two calls (UI_UX §2.2).
    fn stage(&self, _name: &str, _phase: StagePhase) {}
}

/// Which edge of a stage is being reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StagePhase {
    Begin,
    /// The stage finished, after this much wall-clock.
    End {
        elapsed_ms: u64,
    },
}

/// Run `work` as the stage `name`: report its beginning, run it, report its end with the
/// wall-clock it took.
pub fn timed<R>(progress: &dyn Progress, name: &str, work: impl FnOnce() -> R) -> R {
    progress.stage(name, StagePhase::Begin);
    let started = std::time::Instant::now();
    let result = work();
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    progress.stage(name, StagePhase::End { elapsed_ms });
    result
}

/// Reports nothing.
///
/// The default for a library caller and for every test that is not about progress. Named
/// rather than an `Option<Arc<dyn Progress>>` at each call site, because a loop that has to
/// ask whether anyone is listening before saying anything is a loop with a branch in it for no
/// reason.
#[derive(Clone, Copy, Debug, Default)]
pub struct Silent;

impl Progress for Silent {
    fn advance(&self, _stage: &str, _index: u32, _total: u32) {}
}

/// The `Silent` reporter, shared.
pub fn silent() -> Arc<dyn Progress> {
    Arc::new(Silent)
}

#[test]
fn silent_reports_nothing_and_does_not_panic() {
    let progress = silent();
    progress.advance("ingest", 0, 1);
    progress.advance("ingest", u32::MAX, 0);
}

#[test]
fn timed_reports_both_edges_of_a_stage_in_order() {
    use std::sync::Mutex;

    #[derive(Default)]
    struct Record(Mutex<Vec<(String, StagePhase)>>);
    impl Progress for Record {
        fn advance(&self, _stage: &str, _index: u32, _total: u32) {}
        fn stage(&self, name: &str, phase: StagePhase) {
            self.0
                .lock()
                .expect("not poisoned")
                .push((name.to_owned(), phase));
        }
    }

    let record = Record::default();
    let answer = timed(&record, "text", || 3);
    assert_eq!(answer, 3);
    let seen = record.0.into_inner().expect("not poisoned");
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0], ("text".to_owned(), StagePhase::Begin));
    assert!(matches!(seen[1], (ref name, StagePhase::End { .. }) if name == "text"));
}
