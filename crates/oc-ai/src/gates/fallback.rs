//! Gate D's second half: the deterministic answer stands, and the book says so (D13.5).
//!
//! An escalated choice ends in one of two ways, and both are recorded as a `Decision` carrying the
//! call's trace — which model, which prompt version, what it was shown and what it said, by hash
//! (D13.8). Either the model's answer passed every gate and is the choice (`method = Llm`), or a
//! gate refused it and the rule's answer is the choice (`method = Deterministic`) with the gate's
//! code in `fallback`. A report can therefore say, for every escalation, whether the model was
//! used, contradicted, or never asked — and "contradicted" is the case a calibration corpus is made
//! of (ARCHITECTURE §6.1).

use oc_model::confidence::Method;
use oc_model::decision::{Decision, LlmTrace};
use oc_model::ids::BlockId;

use super::GateFailure;

/// A choice that was escalated, before its outcome is known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    /// The stage that owns the choice.
    pub stage: &'static str,
    /// The task, as `Purpose::as_str` names it.
    pub kind: &'static str,
    /// The block the choice is about, when it is about one.
    pub subject: Option<BlockId>,
    /// What the deterministic path chose: the answer that stands unless the model's passes.
    pub deterministic: String,
    /// The other options that were open.
    pub alternatives: Vec<String>,
}

/// Record how an escalated choice was settled.
///
/// `verdict` is the model's answer as the gates judged it: `Ok` with the answer's label when every
/// gate passed, `Err` with the first gate that refused.
pub fn settle(choice: Choice, trace: LlmTrace, verdict: Result<String, GateFailure>) -> Decision {
    let (method, chosen, alternatives, fallback) = match verdict {
        Ok(chosen) => {
            let mut alternatives = vec![choice.deterministic];
            alternatives.extend(
                choice
                    .alternatives
                    .into_iter()
                    .filter(|option| *option != chosen),
            );
            (Method::Llm, chosen, alternatives, None)
        }
        Err(failure) => (
            Method::Deterministic,
            choice.deterministic,
            choice.alternatives,
            Some(failure.code()),
        ),
    };
    Decision {
        stage: choice.stage,
        subject: choice.subject,
        kind: choice.kind,
        chosen,
        alternatives,
        method,
        llm: Some(trace),
        fallback,
    }
}

/// Record an escalated choice whose call was never made — the budget was spent, say
/// (`budget::BUDGET_CALLS`). The deterministic answer stands and no trace exists, which is what
/// tells this apart from a model that was asked and contradicted.
pub fn unasked(choice: Choice, why: &'static str) -> Decision {
    Decision {
        stage: choice.stage,
        subject: choice.subject,
        kind: choice.kind,
        chosen: choice.deterministic,
        alternatives: choice.alternatives,
        method: Method::Deterministic,
        llm: None,
        fallback: Some(why),
    }
}
