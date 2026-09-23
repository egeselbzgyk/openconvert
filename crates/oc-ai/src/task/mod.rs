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

/// What the heading-roles pre-gate reads about the style inventory.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InventoryFacts {
    pub clusters: usize,
    /// The modal (body) cluster's share of the book's non-whitespace characters.
    pub body_char_share: f32,
}

/// Why a task's pre-gate refused to ask at all (PHASE 10 detail 3, RT A8.3).
///
/// Not a gate failure: nothing was asked. The scaffolding the question would be about is not
/// something a model can rescue, so the deterministic answer stands and the call is not spent.
#[derive(Clone, Debug, PartialEq)]
pub enum PreGateFailure {
    /// More style clusters than `inventory.max_clusters`: the typography carries no usable
    /// hierarchy.
    TooManyClusters(InventoryFacts),
    /// The modal cluster holds less than `inventory.min_body_char_share` of the characters: there
    /// is no body text to be the baseline every other style is measured against.
    BodyTooThin(InventoryFacts),
    /// Fewer held-out lines than `inventory.holdout_min_probes` could be sampled, so the mapping
    /// could not be checked against itself — and an unchecked mapping is not asked for.
    TooFewProbes { probes: usize, min: usize },
}

/// The warning the style inventory's refusal carries, shared with `oc-structure`.
pub const W_STYLE_INVENTORY_INVALID: &str = "W_STYLE_INVENTORY_INVALID";

impl PreGateFailure {
    /// The `Decision.fallback` code.
    pub fn code(&self) -> &'static str {
        match self {
            PreGateFailure::TooManyClusters(_) | PreGateFailure::BodyTooThin(_) => {
                "pregate.inventory"
            }
            PreGateFailure::TooFewProbes { .. } => "pregate.holdout",
        }
    }

    /// The warning the report carries, when the refusal is one a user should hear about: an
    /// invalid inventory is (`W_STYLE_INVENTORY_INVALID`, with the arguments `oc-structure`
    /// gives it); a book too small to probe is not.
    pub fn warning(&self) -> Option<oc_model::doc::Warning> {
        use oc_model::doc::{Severity, Warning};
        match self {
            PreGateFailure::TooManyClusters(facts) | PreGateFailure::BodyTooThin(facts) => Some(
                Warning::new(W_STYLE_INVENTORY_INVALID, Severity::Warn)
                    .with_arg("clusters", facts.clusters.to_string())
                    .with_arg("body_char_share", format!("{:.3}", facts.body_char_share)),
            ),
            PreGateFailure::TooFewProbes { .. } => None,
        }
    }
}
