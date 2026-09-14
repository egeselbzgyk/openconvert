//! The decisions log: what the pipeline chose, what it chose between, and who chose
//! (IR_SKETCH, "confidence, ledger, decisions").
//!
//! A [`Confidence`](crate::confidence::Confidence) says how well one label is evidenced. A
//! [`Decision`] says that a *choice was open* and which way it went — which is the thing a
//! reader disagreeing with the output needs, and the thing a later LLM run has to be
//! auditable against (D13.5). The two are separate because most labels are not choices: a
//! block either matched the caption regex or it did not.

use serde::Serialize;

use crate::confidence::Method;
use crate::ids::BlockId;

/// What a model was asked, and what came back, in the form the cache is keyed on (D13.8).
///
/// Every field is a hash or a scalar: a trace records *that* a model said something and
/// under what identity, never the text it was shown. A diagnostic bundle a user sends is
/// therefore free of their book by construction (D13.9).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LlmTrace {
    pub model_id: String,
    pub prompt_version: String,
    pub input_sha256: String,
    pub output_sha256: String,
    pub cached: bool,
    pub ms: u32,
}

/// One choice the pipeline made, with its alternatives.
///
/// `subject` is `Option` because not every choice is about a block. The document class and
/// the preset are choices about the *book*, made once, and IR_SKETCH's `subject: BlockId`
/// has nothing to name for them — inventing a block id to fill the field would make a
/// document-level decision indistinguishable from a decision about whichever block the id
/// happened to belong to. `docs/DECISIONS_LOG.md` records the elaboration.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Decision {
    /// The stage that made it, as the stage name appears everywhere else.
    pub stage: &'static str,
    pub subject: Option<BlockId>,
    /// The task name from ARCHITECTURE §9.6's matrix where the choice is an escalatable one,
    /// so that a decision and the escalation candidate it came from carry the same string.
    pub kind: &'static str,
    pub chosen: String,
    pub alternatives: Vec<String>,
    pub method: Method,
    /// Set only when a model was actually consulted. `None` is v1's default, and a report
    /// that says "deterministic" has this empty rather than absent.
    pub llm: Option<LlmTrace>,
}

impl Decision {
    /// A choice a rule made, about the whole book.
    pub fn deterministic(
        stage: &'static str,
        kind: &'static str,
        chosen: impl Into<String>,
    ) -> Self {
        Self {
            stage,
            subject: None,
            kind,
            chosen: chosen.into(),
            alternatives: Vec::new(),
            method: Method::Deterministic,
            llm: None,
        }
    }

    /// The options that were open when the choice was made.
    pub fn against(mut self, alternatives: Vec<String>) -> Self {
        self.alternatives = alternatives;
        self
    }

    /// The block the choice was about.
    pub fn about(mut self, subject: BlockId) -> Self {
        self.subject = Some(subject);
        self
    }
}
