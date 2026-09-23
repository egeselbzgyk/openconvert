//! Prefix discipline, checked (PHASE 9 detail 4, RT A3.3).
//!
//! One byte-identical system prefix, `-np 1` and one slot mean every call after a book's first
//! should find the prefix already in the server's KV cache. If it does not, the book is paying the
//! full prompt cost on every call — the wall-clock share budget (D13.6) was sized on the opposite
//! assumption, so the caller re-checks it at once.

use oc_model::doc::{Severity, Warning};

use crate::provider::{LlmResponse, Purpose};

pub const W_LLM_PREFIX_COLD: &str = "W_LLM_PREFIX_COLD";

/// `Err` with `W_LLM_PREFIX_COLD` when call `index` (zero-based, within one book) should have
/// found the prefix warm and the reply does not say it did.
///
/// A reply that does not say at all is treated as cold: the sidecar this check is for always says,
/// and a claim of discipline nobody can confirm is not one.
pub fn check(index: usize, purpose: Purpose, response: &LlmResponse) -> Result<(), Warning> {
    let warm = response.cached_tokens.is_some_and(|tokens| tokens > 0);
    if index == 0 || warm {
        return Ok(());
    }
    Err(Warning::new(W_LLM_PREFIX_COLD, Severity::Warn)
        .with_arg("call", index.saturating_add(1).to_string())
        .with_arg("task", purpose.as_str()))
}
