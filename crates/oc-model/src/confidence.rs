//! How a decision was reached, and on what evidence (IR_SKETCH, "confidence, ledger,
//! decisions").
//!
//! Every inference this pipeline makes carries one of these, and the reason is D13.5: a
//! deterministic verdict and a model's guess must never be indistinguishable in the output.
//! A report that says "this heading level was chosen by a rule" and one that says "a 1.7 B
//! model said so" are different documents, and the user is entitled to the difference.
//!
//! `score` is `Option` because v1's decisions are mostly *predicate-based*: they either fired
//! or they did not, and inventing a number for them would be a false precision that a later
//! calibration could not correct.

use serde::{Deserialize, Serialize};

/// What kind of thing decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    /// A rule, a threshold or a lookup. No model was consulted.
    Deterministic,
    /// A language model, local or remote.
    Llm,
    /// The user, through an override.
    User,
}

/// One named piece of evidence and what it measured.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Signal {
    pub name: String,
    pub value: f32,
}

impl Signal {
    pub fn new(name: &str, value: f32) -> Self {
        Self {
            name: name.to_owned(),
            value,
        }
    }
}

/// How a decision was reached.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Confidence {
    pub method: Method,
    /// `None` for a predicate-based decision, which is most of v1.
    pub score: Option<f32>,
    pub signals: Vec<Signal>,
    /// Whether the deterministic path declined and something else was asked.
    pub escalated: bool,
    /// Whether the decision is the fail-closed default rather than a positive verdict.
    pub fallback_used: bool,
}

impl Confidence {
    /// A rule fired, on the named evidence.
    pub fn deterministic(signals: Vec<Signal>) -> Self {
        Self {
            method: Method::Deterministic,
            score: None,
            signals,
            escalated: false,
            fallback_used: false,
        }
    }

    /// No rule fired and the safe default was taken. Recorded as such, because a report that
    /// cannot tell "decided" from "gave up safely" cannot be audited.
    pub fn fallback(signals: Vec<Signal>) -> Self {
        Self {
            method: Method::Deterministic,
            score: None,
            signals,
            escalated: false,
            fallback_used: true,
        }
    }

    /// Attach a score to a decision that has one — a classifier's margin, say.
    pub fn with_score(mut self, score: f32) -> Self {
        self.score = Some(score);
        self
    }
}
