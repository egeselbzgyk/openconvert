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
