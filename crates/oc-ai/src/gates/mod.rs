//! The four gates an LLM edit passes before it is applied (D13.5, ARCHITECTURE §6.2).
//!
//! ```text
//! escalation_predicate(subject) ∧ ai.enabled ∧ budget_remains
//!         ↓ call
//!     Gate S ∧ Gate L ∧ Gate V
//!         ↓ all pass                    ↓ any fails
//!     apply the edit                  the deterministic answer stands (gate D),
//!                                     and the Decision says which gate refused
//! ```
//!
//! Gate D's *predicate* — whether the deterministic evidence is weak enough to ask at all — is
//! evaluated before any call by the stage that owns the evidence, so it is not in this module.
//! What is here is the other half of D: the fallback, and its record ([`fallback`]).

pub mod fallback;
pub mod locality;
pub mod schema;
pub mod validity;

use oc_model::document::Document;

use validity::{Region, RegionStats, Tolerance};

/// Gates L and V over an edit built from an answer gate S admitted, in that order: a change of
/// characters is refused before anything is measured about the text it would have produced.
///
/// The caller keeps `before` and applies `after` only on `Ok`, which is what "revert" means.
pub fn gate_edit(
    before: &Document,
    after: &Document,
    region: &Region,
    tolerance: &Tolerance,
) -> Result<(), GateFailure> {
    locality::gate_locality(before, after)?;
    validity::gate_validity(
        &RegionStats::measure(before, region),
        &RegionStats::measure(after, region),
        tolerance,
    )
}

/// Why a model's answer was not applied.
///
/// Each variant carries what a developer reading a failing test needs. Only [`GateFailure::code`]
/// reaches the IR: the details may quote the model, and the model may have quoted the book.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum GateFailure {
    /// Gate S: the answer holds a `<think>` block. D10 disables thinking per request and asserts
    /// it absent from the output, whatever the provider did with the request.
    #[error("the answer contains a thinking block")]
    ThinkingPresent,
    /// Gate S: the answer is not JSON — a code fence, a preamble, a trailer, a truncation.
    #[error("the answer is not JSON: {0}")]
    Unparseable(String),
    /// Gate S: JSON, but not the task's shape — a field missing, unknown or said twice, a value of
    /// the wrong type, an empty string where the task says absent is `null`.
    #[error("the answer is JSON of the wrong shape: {0}")]
    WrongShape(String),
    /// Gate S: a value outside the task's closed set.
    #[error("`{field}` is `{value}`, which is not one of the task's values")]
    OutOfEnum { field: &'static str, value: String },
    /// Gate S: the identifiers answered are not exactly the ones asked about, once each.
    #[error(
        "the {what} ids are not the ones asked about: duplicated {duplicated:?}, missing \
         {missing:?}, invented {invented:?}"
    )]
    NotBijective {
        /// Which identifiers: `cluster`, `probe` or `block`.
        what: &'static str,
        duplicated: Vec<String>,
        missing: Vec<String>,
        invented: Vec<String>,
    },
    /// Gate L: the edit changed the book's characters — the one thing an LLM edit may never do.
    /// Counted over `C`, so `lost` and `gained` are characters the conservation law counts.
    #[error("the edit changed the book's characters: {lost} lost, {gained} gained")]
    CharactersChanged { lost: u64, gained: u64 },
    /// Gate L: every character is still there, and the book no longer reads in the same order.
    #[error("the edit moved text: every character is present, and not in the order it was")]
    TextReordered,
    /// Gate V: a component of the region's statistics worsened by more than its epsilon. The
    /// first such component in the tuple's fixed order is the one named.
    #[error("the edit made `{statistic}` worse: {before} before, {after} after")]
    Worsened {
        statistic: &'static str,
        before: f32,
        after: f32,
    },
}

impl GateFailure {
    /// The stable code a `Decision` records: the gate's letter and the failure's name.
    pub fn code(&self) -> &'static str {
        match self {
            GateFailure::ThinkingPresent => "S.thinking",
            GateFailure::Unparseable(_) => "S.unparseable",
            GateFailure::WrongShape(_) => "S.shape",
            GateFailure::OutOfEnum { .. } => "S.enum",
            GateFailure::NotBijective { .. } => "S.bijection",
            GateFailure::CharactersChanged { .. } => "L.characters",
            GateFailure::TextReordered => "L.order",
            GateFailure::Worsened { .. } => "V.worsened",
        }
    }
}
