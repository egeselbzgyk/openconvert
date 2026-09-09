#![forbid(unsafe_code)]
//! Pipeline orchestrator: stages, conservation-law checks, escalation, the repair
//! loop, the report, progress/cancel traits and sidecar supervision.
//!
//! Also the home of `thresholds`, the generated view of `thresholds.toml` (D17).

pub mod events;
pub mod exit;
pub mod thresholds;
