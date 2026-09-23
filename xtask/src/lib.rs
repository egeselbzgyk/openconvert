//! Developer tasks, as a library so the binary and the integration tests share one
//! implementation of every rule.
//!
//! Nothing here ships. `deny.tools.toml` audits this crate's dependency tree.

pub mod ci_lint;
pub mod dom_fixtures;
pub mod epubcheck_parity;
pub mod fetch_epubcheck;
pub mod fetch_epubcheck_corpus;
pub mod fetch_isartor;
pub mod fetch_llama_server;
pub mod fixtures;
pub mod fuzz_seeds;
pub mod handmade_fixtures;
pub mod mutations;
pub mod stage_sidecars;
pub mod thresholds_lint;
pub mod vendor_pdfium;
