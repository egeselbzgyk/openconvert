//! The four tasks' own validations and edits (PHASE 10, ARCHITECTURE §9.6).
//!
//! Gate S admits an answer of the right *shape*. What each task checks next is what a shape cannot
//! say: that a metadata field is a verbatim substring of the pages it was read from, that a
//! heading mapping agrees with its own held-out probe, that book-structure boundaries are ordered
//! and in range, that `verse` is only said of a block with the lines of one. Each check rejects the
//! **whole** answer — a model that invented one field has shown it was not copying — and each
//! failure is a [`GateFailure`](crate::gates::GateFailure) with a stable code for the `Decision`.
//!
//! Each module also turns an admitted answer into an **edit**: a label, a level, a zone, a wrapper
//! — never text. The caller applies it through the same code the deterministic answer took, and
//! gates L and V judge the result (`gates::gate_edit`).
//!
//! Like the rest of this crate, nothing here reads a threshold: every bound is a parameter the
//! caller fills from `oc_core::thresholds::T`.

pub mod book_structure;
pub mod heading_roles;
pub mod metadata;
pub mod verse_quote;

/// Text as the verbatim check compares it: lower case, every run of whitespace one space, trimmed
/// (ARCHITECTURE §9.6: "case- and whitespace-normalised").
///
/// Lower-casing is the default Unicode mapping, applied to both sides alike: it builds a
/// comparison key and never touches the book, so the Turkish dotted-I question does not arise —
/// `İ` lowers to the same sequence on both sides.
pub fn normalise(text: &str) -> String {
    text.split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}
