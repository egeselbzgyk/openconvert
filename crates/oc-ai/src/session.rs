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

/// Where the time comes from. Injected, so the wall-clock hard stop is a test and not a hope.
pub trait Clock {
    /// Milliseconds since some fixed point; only differences are read.
    fn now_ms(&self) -> u64;
}

/// The process's monotonic clock.
#[derive(Debug)]
pub struct SystemClock {
    origin: std::time::Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            origin: std::time::Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        u64::try_from(self.origin.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

/// Raised when the book's LLM time reached `llm.max_wallclock_share` of the conversion's.
pub const W_LLM_TIME_EXHAUSTED: &str = "W_LLM_TIME_EXHAUSTED";

/// The banner: AI was asked for and no model could be reached, so the book is deterministic
/// (RT D20). Never a failure — the conversion completes and says so.
pub const W_LLM_UNAVAILABLE: &str = "W_LLM_UNAVAILABLE";

/// One call as the NDJSON `llm` event reports it (D13.2): the purpose, where the answer came
/// from, and what it cost. A trace says what was asked; this says what it took.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallRecord {
    pub call_id: u32,
    pub purpose: crate::provider::Purpose,
    pub cached: bool,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub ms: u32,
}

/// One book's session with a model: the [`Asker`] the pipeline uses (D13.6, D13.8).
///
/// In order, for every question: a stop already reached refuses it (the wall-clock share, a
/// provider that could not be reached); the **wall-clock share** is checked — LLM time over the
/// conversion's time so far, a hard stop at `llm.max_wallclock_share`; the **call budget** is spent,
/// cached or not, so a warm cache decides the same things as a cold one; the **cache** answers if
/// it can; and only then the provider. An answer from the provider is filed in the cache.
pub struct Session<'a> {
    provider: &'a dyn crate::provider::LlmProvider,
    cache: Option<&'a crate::cache::FileCache>,
    clock: &'a dyn Clock,
    budget: crate::budget::Budget,
    /// When the conversion began, on `clock`.
    started_ms: u64,
    max_share: f64,
    llm_ms: u64,
    stopped: Option<Unasked>,
    calls: Vec<CallRecord>,
    warnings: Vec<Warning>,
}

impl<'a> Session<'a> {
    /// A session for one book. `started_ms` is when its conversion began on `clock`, and the
    /// numbers are `llm.max_calls_per_book` and `llm.max_wallclock_share`.
    pub fn new(
        provider: &'a dyn crate::provider::LlmProvider,
        cache: Option<&'a crate::cache::FileCache>,
        clock: &'a dyn Clock,
        started_ms: u64,
        max_calls: u32,
        max_share: f64,
    ) -> Self {
        Self {
            provider,
            cache,
            clock,
            budget: crate::budget::Budget::new(max_calls),
            started_ms,
            max_share,
            llm_ms: 0,
            stopped: None,
            calls: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// The calls made, cached ones included, in order.
    pub fn calls(&self) -> &[CallRecord] {
        &self.calls
    }

    /// The calls the book has left.
    pub fn remaining(&self) -> u32 {
        self.budget.remaining()
    }

    /// Milliseconds spent waiting for the provider.
    pub fn llm_ms(&self) -> u64 {
        self.llm_ms
    }

    /// LLM time over the conversion's time so far.
    pub fn share(&self) -> f64 {
        let elapsed = self.clock.now_ms().saturating_sub(self.started_ms);
        if elapsed == 0 {
            return 0.0;
        }
        self.llm_ms as f64 / elapsed as f64
    }

    /// Why the session stopped asking, if it did.
    pub fn stopped(&self) -> Option<&Unasked> {
        self.stopped.as_ref()
    }

    /// Every warning the session raised, each once: budget, time, a cold prefix, an unconstrained
    /// provider.
    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }

    fn warn_once(&mut self, warning: Warning) {
        if !self.warnings.iter().any(|seen| seen.code == warning.code) {
            self.warnings.push(warning);
        }
    }
}

impl Asker for Session<'_> {
    fn ask(&mut self, request: &LlmRequest) -> Result<Asked, Unasked> {
        if let Some(stopped) = &self.stopped {
            return Err(stopped.clone());
        }
        if self.llm_ms > 0 && self.share() >= self.max_share {
            let warning = Warning::new(W_LLM_TIME_EXHAUSTED, oc_model::doc::Severity::Warn)
                .with_arg("task", request.purpose.as_str())
                .with_arg("share", format!("{:.3}", self.share()));
            self.warn_once(warning.clone());
            let stop = Unasked::Time(warning);
            self.stopped = Some(stop.clone());
            return Err(stop);
        }
        if let Err(warning) = self.budget.spend(request.purpose) {
            self.warn_once(warning.clone());
            return Err(Unasked::Budget(warning));
        }

        let model_id = self.provider.id().to_owned();
        let key = request.cache_key(&model_id);
        let cached = match self.cache.map(|cache| cache.get(&key)) {
            Some(Ok(Some(answer))) => Some(answer),
            // A damaged entry is not a miss (Phase 8): the call is refused rather than silently
            // re-paid, and the report says the cache is damaged.
            Some(Err(error)) => {
                let stop = Unasked::Unavailable(LlmError::Cassette(error.to_string()));
                return Err(stop);
            }
            Some(Ok(None)) | None => None,
        };

        let call_id = u32::try_from(self.calls.len()).unwrap_or(u32::MAX);
        let (response, ms) = match cached {
            Some(answer) => (
                LlmResponse {
                    text: answer.output,
                    reasoning: None,
                    tokens_in: answer.tokens_in,
                    tokens_out: answer.tokens_out,
                    cached: true,
                    cached_tokens: None,
                    finish_reason: Some("stop".to_owned()),
                },
                0,
            ),
            None => {
                let before = self.clock.now_ms();
                let answered = self.provider.complete(request);
                let ms = self.clock.now_ms().saturating_sub(before);
                self.llm_ms = self.llm_ms.saturating_add(ms);
                let response = match answered {
                    Ok(response) => response,
                    Err(error) => {
                        let stop = Unasked::Unavailable(error);
                        self.stopped = Some(stop.clone());
                        return Err(stop);
                    }
                };
                // An answer nothing constrained: gate S will refuse more of them, and the report
                // says why (PHASE 11 detail 1).
                if self.provider.capabilities().constraint == crate::provider::Constraint::None {
                    self.warn_once(
                        Warning::new(
                            crate::provider::W_LLM_UNCONSTRAINED,
                            oc_model::doc::Severity::Warn,
                        )
                        .with_arg("model", model_id.clone()),
                    );
                }
                if let Some(cache) = self.cache {
                    // An answer the cache could not keep is still an answer; the next run pays for
                    // it again. A thinking block is never filed: gate S refuses it anyway.
                    if response.reasoning.is_none() {
                        let _ = cache.put(&crate::cache::CachedAnswer::new(
                            &model_id,
                            request,
                            &response.text,
                            response.tokens_in,
                            response.tokens_out,
                        ));
                    }
                }
                if !response.cached {
                    if let Err(warning) =
                        crate::prefix::check(self.calls.len(), request.purpose, &response)
                    {
                        self.warn_once(warning);
                    }
                }
                (response, ms)
            }
        };
        let ms = u32::try_from(ms).unwrap_or(u32::MAX);
        self.calls.push(CallRecord {
            call_id,
            purpose: request.purpose,
            cached: response.cached,
            tokens_in: response.tokens_in,
            tokens_out: response.tokens_out,
            ms,
        });
        let trace = crate::provider::trace(&model_id, request, &response.text, response.cached, ms);
        Ok(Asked { response, trace })
    }
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
