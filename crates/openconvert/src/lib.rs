#![forbid(unsafe_code)]
//! The pipeline wiring: the part of the engine the CLI drives and the tests exercise.
//!
//! ARCHITECTURE §3.1 puts the orchestrator in `oc-core`, and it cannot go there. `oc-core`
//! owns `thresholds`, and every stage crate reads its numbers from it (D17: "code reads
//! thresholds through `T`, never by string key"), so `oc-pdf`, `oc-text` and `oc-layout` all
//! depend on `oc-core` and `oc-core` cannot depend on them without a cycle. The binary is the
//! one crate that may depend on everything, so the wiring lives here — as a library rather
//! than inside `main.rs`, so that a test can run a stage without running a process.
//!
//! What `oc-core` keeps is the part that does not need the stages: the thresholds, the
//! conservation checker, the stage declarations, cancellation, events and exit codes.

pub mod dump_layout;
pub mod dump_text;
pub mod input;
pub mod pipeline;
