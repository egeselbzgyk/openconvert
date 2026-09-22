#![forbid(unsafe_code)]
//! Pipeline orchestrator: stages, conservation-law checks, escalation, the repair
//! loop, the report, progress/cancel traits and sidecar supervision.
//!
//! Also the home of `thresholds`, the generated view of `thresholds.toml` (D17).

pub mod cancel;
pub mod conservation_diff;
pub mod escalation;
pub mod events;
pub mod exit;
pub mod jobspec;
pub mod ledger_check;
pub mod limits;
pub mod progress;
pub mod stages;
pub mod thresholds;
pub mod warnings;
