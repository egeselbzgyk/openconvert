#![forbid(unsafe_code)]
//! Tier-1 internal validator, the structural validator, the optional EPUBCheck and
//! Ace runners, and the message-to-repair table (D6, D13.7).
//!
//! Tier 1 is the part that always runs. Tier 2 — EPUBCheck — is authoritative and is a hard CI
//! gate, but it is Java, and bundling a JRE in the base install would cost more than the whole
//! rest of the application; it is available in-app through the optional validation pack and
//! never on the default conversion path.
//!
//! The consequence is that Tier 1's coverage matters, and "Tier 1 misses deep content-model
//! errors, mitigated by EPUBCheck in CI" is an unquantified hand-wave unless the miss rate is a
//! number. It is: `xtask epubcheck-parity` runs Tier 1 over EPUBCheck's own public test corpus
//! and records per-message-id parity in `docs/TIER1_PARITY.md`, which CI holds non-decreasing.

pub mod tier1;

pub use tier1::{validate_tier1, Expectations, Finding, Severity, Tier1Report};
