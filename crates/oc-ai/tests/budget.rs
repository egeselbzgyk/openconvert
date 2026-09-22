//! The per-book call budget (D13.6): at most `llm.max_calls_per_book` calls, one budget for every
//! task, and a refusal that the book records.

use oc_ai::budget::{Budget, BUDGET_CALLS, W_LLM_BUDGET_EXHAUSTED};
use oc_ai::gates::fallback::{unasked, Choice};
use oc_ai::provider::Purpose;
use oc_core::thresholds::T;
use oc_model::confidence::Method;
use oc_model::doc::Severity;

fn max_calls() -> u32 {
    u32::try_from(T.llm.max_calls_per_book).expect("a small count")
}

/// Test 8.13. Eight calls a book (D13.6); the ninth is refused, with the warning the report
/// carries, and every call after it is refused the same way.
#[test]
fn budget_stops_after_max_calls() {
    assert_eq!(
        max_calls(),
        8,
        "D13.6: the ninth call is the first one refused"
    );
    let mut budget = Budget::new(max_calls());
    for call in 0..max_calls() {
        let purpose = Purpose::ALL[usize::try_from(call).expect("small") % Purpose::ALL.len()];
        assert_eq!(
            budget.spend(purpose),
            Ok(()),
            "call {} is within budget",
            call + 1
        );
    }
    assert_eq!(budget.remaining(), 0);

    let refused = budget
        .spend(Purpose::Metadata)
        .expect_err("the ninth call is refused");
    assert_eq!(refused.code, W_LLM_BUDGET_EXHAUSTED);
    assert_eq!(refused.severity, Severity::Warn);
    assert_eq!(refused.args["task"], "metadata");
    assert_eq!(refused.args["calls"], "8");

    assert!(budget.spend(Purpose::VerseQuote).is_err());
    assert_eq!(budget.spent(), max_calls(), "a refused call is not spent");
}

/// One budget, no per-task share of it (D13.6): eight calls of one task leave none for another.
/// Which task goes without is the degradation order's decision, made before any call, not the
/// budget's.
#[test]
fn every_task_draws_on_the_one_budget() {
    let mut budget = Budget::new(max_calls());
    for _ in 0..max_calls() {
        assert_eq!(budget.spend(Purpose::VerseQuote), Ok(()));
    }
    for purpose in Purpose::ALL {
        assert!(
            budget.spend(purpose).is_err(),
            "{purpose:?} found budget left"
        );
    }
}

/// An escalation the budget refused is still a decision: the deterministic answer, no call made,
/// and why — distinguishable in the report from a model that was asked and contradicted.
#[test]
fn an_escalation_the_budget_refused_is_recorded() {
    let decision = unasked(
        Choice {
            stage: "structure",
            kind: "verse_quote",
            subject: None,
            deterministic: "blockquote".to_owned(),
            alternatives: vec!["verse".to_owned(), "paragraph".to_owned()],
        },
        BUDGET_CALLS,
    );
    assert_eq!(decision.method, Method::Deterministic);
    assert_eq!(decision.chosen, "blockquote");
    assert_eq!(decision.llm, None, "no call was made");
    assert_eq!(decision.fallback, Some("budget.calls"));
    assert!(decision.fallback_used());
}
