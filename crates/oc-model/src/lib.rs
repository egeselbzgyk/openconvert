#![forbid(unsafe_code)]
//! OpenConvert intermediate representation (D13.3, `docs/IR_SKETCH.md`).
//!
//! Types, block ids, the text ledger, canonical JSON serialisation and `ir_version`.

/// The IR contract version, present as the first key of every serialised IR and of
/// `overrides.json` (ARCHITECTURE §4.5).
///
/// A single monotonic integer. It bumps when a field is removed or retyped, when the
/// `BlockId` derivation changes, or when normalisation `N` or the geometry space changes;
/// adding an optional field or a new enum variant does not bump it. The engine hard-errors
/// on a version it does not know, on both read and IPC handshake, and never coerces.
pub const IR_VERSION: u32 = 1;

pub mod canonical;
pub mod extract;
pub mod geom;
pub mod ids;
pub mod lang;
pub mod layout;
pub mod ledger;
pub mod text;
