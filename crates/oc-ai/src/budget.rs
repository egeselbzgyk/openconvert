//! The per-book call budget (D13.6, ARCHITECTURE §6.3).
//!
//! **One budget, no per-task share of it.** At most `llm.max_calls_per_book` calls, whatever they
//! are for: task 4's thirty-block cap costs at most three at ten blocks a call (ratified note N-4),
//! leaving five for tasks 1–3 and `book_structure`'s chunking. Which task goes without when the
//! calls would not fit is the degradation order's decision — `verse_quote` first, then
//! `book_structure` chunks beyond the first, then `heading_roles`, never `metadata` — taken before
//! any call is made (Phase 10, test 10.22). The budget itself only counts, and refuses.
//!
//! **A cached answer is a call.** The budget counts requests, not trips to a model, so a book
//! decides the same things whether its cache is warm or cold. If a warm cache let a book make a
//! ninth decision a cold one could not, a cache hit would change the book, and D13.8's promise that
//! an AI run is byte-identical on a cache hit would be false.

use oc_model::doc::{Severity, Warning};

use crate::provider::Purpose;

/// Raised when a call is refused because the book's calls are spent.
pub const W_LLM_BUDGET_EXHAUSTED: &str = "W_LLM_BUDGET_EXHAUSTED";

/// The `Decision.fallback` of an escalation the call budget refused.
pub const BUDGET_CALLS: &str = "budget.calls";

/// One book's calls.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Budget {
    max_calls: u32,
    spent: u32,
}

impl Budget {
    /// A fresh budget of `max_calls` — `llm.max_calls_per_book`, read by the caller.
    pub fn new(max_calls: u32) -> Self {
        Self {
            max_calls,
            spent: 0,
        }
    }

    /// Spend one call on `purpose`, or refuse it with the warning the report carries.
    ///
    /// A refused call is not spent, so every call after the first refusal is refused the same way.
    pub fn spend(&mut self, purpose: Purpose) -> Result<(), Warning> {
        if self.spent >= self.max_calls {
            return Err(Warning::new(W_LLM_BUDGET_EXHAUSTED, Severity::Warn)
                .with_arg("task", purpose.as_str())
                .with_arg("calls", self.max_calls.to_string()));
        }
        self.spent += 1;
        Ok(())
    }

    /// How many calls have been spent.
    pub fn spent(&self) -> u32 {
        self.spent
    }

    /// How many are left.
    pub fn remaining(&self) -> u32 {
        self.max_calls.saturating_sub(self.spent)
    }
}
