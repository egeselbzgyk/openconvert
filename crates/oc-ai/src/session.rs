//! One book's conversation with a model: how a task asks, and what can stop it being asked.
//!
//! A task never calls a provider directly. It asks an [`Asker`], and the asker is what knows
//! whether the book may still ask — the call budget, the wall-clock share — and where an answer
//! comes from: the cache, the provider, a cassette. So a task's code is the same whether the
//! answer is a model's, a recording's or a test double's, and "no call was made" is something a
//! test can count rather than infer.

use oc_model::decision::LlmTrace;
use oc_model::doc::Warning;

use crate::provider::{LlmError, LlmRequest, LlmResponse};

/// An answer, and the trace the `Decision` records for it (D13.8).
#[derive(Clone, Debug, PartialEq)]
pub struct Asked {
    pub response: LlmResponse,
    pub trace: LlmTrace,
}

/// Why a question was not answered. None of these is a gate failure: nothing came back to judge,
/// and the deterministic answer stands (RT D20).
#[derive(Clone, Debug, PartialEq)]
pub enum Unasked {
    /// The book's calls are spent (`W_LLM_BUDGET_EXHAUSTED`).
    Budget(Warning),
    /// The wall-clock share is spent, and the rest of the book is deterministic.
    Time(Warning),
    /// The provider could not answer at all: unreachable, not a chat completion, no cassette.
    Unavailable(LlmError),
}

impl Unasked {
    /// The `Decision.fallback` code for a choice this left unasked.
    pub fn code(&self) -> &'static str {
        match self {
            Unasked::Budget(_) => crate::budget::BUDGET_CALLS,
            Unasked::Time(_) => BUDGET_TIME,
            Unasked::Unavailable(_) => LLM_UNAVAILABLE,
        }
    }
}

/// The `Decision.fallback` of a choice the wall-clock share stopped.
pub const BUDGET_TIME: &str = "budget.time";

/// The `Decision.fallback` of a choice whose model could not be reached.
pub const LLM_UNAVAILABLE: &str = "llm.unavailable";

/// Something a task asks.
pub trait Asker {
    fn ask(&mut self, request: &LlmRequest) -> Result<Asked, Unasked>;
}

/// How one task's question ended.
#[derive(Clone, Debug, PartialEq)]
pub enum TaskResult<A> {
    /// The task's pre-gate refused to ask at all: the deterministic scaffolding is not something
    /// a model can rescue (RT A8.3). No call was made.
    Refused(crate::task::PreGateFailure),
    /// The question was not answered.
    Unasked(Unasked),
    /// An answer came back and gate S or the task's validation refused it.
    Rejected {
        trace: LlmTrace,
        failure: crate::gates::GateFailure,
    },
    /// An answer came back and was admitted. Gates L and V have not seen its edit yet.
    Admitted { trace: LlmTrace, answer: A },
}
