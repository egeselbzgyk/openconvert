//! Developer tasks, as a library so the binary and the integration tests share one
//! implementation of every rule.
//!
//! Nothing here ships. `deny.tools.toml` audits this crate's dependency tree.

pub mod ci_lint;
pub mod fixtures;
pub mod stage_sidecars;
pub mod thresholds_lint;
pub mod vendor_pdfium;
