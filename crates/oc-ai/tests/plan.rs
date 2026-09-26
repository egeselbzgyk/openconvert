//! PHASE 10: what a book asks, with how many calls, for how long — the language gate, the
//! degradation order, and the session every call goes through.

mod common;

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use oc_ai::cache::FileCache;
use oc_ai::gates::fallback::{unasked, Choice};
use oc_ai::plan::{plan, Demand, Grant, LanguageGates, LANGUAGE_GATE};
use oc_ai::provider::{
    Constraint, LlmError, LlmProvider, LlmRequest, LlmResponse, ProviderCaps, Purpose,
    ThinkingControl,
};
use oc_ai::session::{Asker, Clock, Session, Unasked, W_LLM_TIME_EXHAUSTED};
use oc_core::thresholds::T;

/// A clock a test moves.
#[derive(Default)]
struct FakeClock(AtomicU64);

impl FakeClock {
    fn advance(&self, ms: u64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for FakeClock {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

/// A model that takes `ms` of the fake clock to answer anything with `answer`, and counts.
struct Slow<'a> {
    clock: &'a FakeClock,
    ms: u64,
    answer: String,
    calls: AtomicUsize,
}

impl LlmProvider for Slow<'_> {
    fn id(&self) -> &str {
        "slow-test-model"
    }
    fn capabilities(&self) -> ProviderCaps {
        ProviderCaps {
            constraint: Constraint::Gbnf,
        }
    }
    fn thinking_control(&self) -> ThinkingControl {
        ThinkingControl::None
    }
    fn complete(&self, _request: &LlmRequest) -> Result<LlmResponse, LlmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.clock.advance(self.ms);
        Ok(LlmResponse {
            text: self.answer.clone(),
            reasoning: None,
            tokens_in: 100,
            tokens_out: 20,
            cached: false,
            cached_tokens: Some(100),
            finish_reason: Some("stop".to_owned()),
        })
    }
}

fn max_calls() -> u32 {
    u32::try_from(T.llm.max_calls_per_book).unwrap_or_default()
}

/// Row 10.22 (ratified note N-4). Three calls left and all four tasks escalated — one metadata
/// call, one heading-roles call, one book-structure chunk, three verse batches: `verse_quote` is
/// dropped first, entirely, and the three calls go to the other three. With fewer calls still,
/// `heading_roles` goes, then the book-structure chunk; **`metadata` never**.
#[test]
fn task_priority_order_on_budget_overflow() {
    let all_four = Demand {
        metadata: true,
        heading_roles: true,
        book_structure_chunks: 1,
        verse_quote_batches: 3,
    };
    assert_eq!(
        plan(&all_four, 3),
        Grant {
            metadata: true,
            heading_roles: true,
            book_structure_chunks: 1,
            verse_quote_batches: 0,
        }
    );
    assert_eq!(
        plan(&all_four, 4),
        Grant {
            verse_quote_batches: 1,
            ..plan(&all_four, 3)
        },
        "a fourth call buys back one verse batch"
    );
    assert_eq!(
        plan(&all_four, 2),
        Grant {
            metadata: true,
            heading_roles: false,
            book_structure_chunks: 1,
            verse_quote_batches: 0,
        }
    );
    assert_eq!(
        plan(&all_four, 1),
        Grant {
            metadata: true,
            ..Grant::default()
        },
        "metadata is the last task standing"
    );
    assert_eq!(plan(&all_four, 0), Grant::default());
    assert_eq!(
        plan(&all_four, usize::try_from(max_calls()).unwrap_or(0)).calls(),
        6
    );

    // Extra book-structure chunks go before heading_roles, and the first chunk after it.
    let chunked = Demand {
        metadata: true,
        heading_roles: true,
        book_structure_chunks: 6,
        verse_quote_batches: 2,
    };
    assert_eq!(
        plan(&chunked, 8),
        Grant {
            metadata: true,
            heading_roles: true,
            book_structure_chunks: 6,
            verse_quote_batches: 0,
        }
    );
    assert_eq!(
        plan(&chunked, 3),
        Grant {
            metadata: true,
            heading_roles: true,
            book_structure_chunks: 1,
            verse_quote_batches: 0,
        }
    );
}

/// Row 10.21. With `[ai.task.verse_quote.languages] = ["en", "de"]`, a Turkish book's ambiguous
/// blocks are not asked about at all: the gate takes the task out of the demand, so no call is
/// planned or made, and each escalation is recorded with `fallback = "language.gate"` and no trace
/// — told apart from a model that was asked and contradicted.
#[test]
fn language_gate_disables_a_task_for_one_language() {
    let gates = LanguageGates {
        metadata: &["en", "de", "tr"],
        heading_roles: &["en", "de", "tr"],
        book_structure: &["en", "de", "tr"],
        verse_quote: &["en", "de"],
    };
    assert!(gates.allows(Purpose::VerseQuote, "en", false));
    assert!(
        gates.allows(Purpose::VerseQuote, "de-AT", false),
        "by primary subtag"
    );
    assert!(!gates.allows(Purpose::VerseQuote, "tr", false));
    assert!(gates.allows(Purpose::Metadata, "tr", false));
    assert!(
        gates.allows(Purpose::VerseQuote, "tr", true),
        "--ai-all-tasks sets the gate aside"
    );

    // Two batches of ambiguous blocks escalated in a Turkish book: gated out of the demand.
    let demand = Demand {
        metadata: gates.allows(Purpose::Metadata, "tr", false),
        verse_quote_batches: if gates.allows(Purpose::VerseQuote, "tr", false) {
            2
        } else {
            0
        },
        ..Demand::default()
    };
    let grant = plan(&demand, usize::try_from(max_calls()).unwrap_or(0));
    assert_eq!(grant.verse_quote_batches, 0, "zero verse/quote calls");
    assert!(grant.metadata);

    // And the verse task, granted no batch, asks nothing.
    struct Never;
    impl Asker for Never {
        fn ask(
            &mut self,
            request: &LlmRequest,
        ) -> Result<oc_ai::session::Asked, oc_ai::session::Unasked> {
            panic!("{:?} was asked in a gated language", request.purpose)
        }
    }
    let _ = oc_ai::task::verse_quote::run(
        &mut Never,
        &[],
        grant.verse_quote_batches,
        &oc_ai::task::verse_quote::VerseQuoteLimits {
            blocks_per_call: 10,
            max_blocks: 30,
            verse_min_lines: 3,
            verse_min_short_line_ratio: 0.6,
        },
        common::max_tokens(),
    );

    // Why, as the report records it.
    let decision = unasked(
        Choice {
            stage: "structure",
            kind: Purpose::VerseQuote.as_str(),
            subject: Some(common::verse_block_id()),
            deterministic: "blockquote".to_owned(),
            alternatives: vec!["verse".to_owned(), "paragraph".to_owned()],
        },
        LANGUAGE_GATE,
    );
    assert_eq!(decision.fallback, Some("language.gate"));
    assert!(decision.llm.is_none(), "no model was asked");
    assert_eq!(decision.chosen, "blockquote");

    // The shipped map enables nothing yet: no evaluation has run (PHASE 10 detail 7).
    for languages in [
        T.ai.task.metadata.languages,
        T.ai.task.heading_roles.languages,
        T.ai.task.book_structure.languages,
        T.ai.task.verse_quote.languages,
    ] {
        assert!(languages.is_empty(), "{languages:?}");
    }
}

/// The time budget is a hard stop: once the model's time reaches the book's budget, the next
/// question is refused, and every one after it, with `W_LLM_TIME_EXHAUSTED` said once.
#[test]
fn the_time_budget_stops_the_session() {
    let clock = FakeClock::default();
    let model = Slow {
        clock: &clock,
        ms: 1_000,
        answer: "{}".to_owned(),
        calls: AtomicUsize::new(0),
    };
    let mut session = Session::new(&model, None, &clock, max_calls(), 3_000);
    let requests = common::requests();
    let mut asked = 0;
    for _ in 0..8 {
        match session.ask(&requests[0]) {
            Ok(_) => asked += 1,
            Err(Unasked::Time(warning)) => {
                assert_eq!(warning.code, W_LLM_TIME_EXHAUSTED);
                break;
            }
            Err(other) => panic!("{other:?}"),
        }
    }
    // One second a call against three seconds: the fourth question is refused.
    assert_eq!(asked, 3);
    assert!(session.llm_ms() >= session.budget_ms());
    assert!(matches!(session.ask(&requests[1]), Err(Unasked::Time(_))));
    assert_eq!(
        model.calls.load(Ordering::SeqCst),
        3,
        "nothing after the stop"
    );
    assert_eq!(
        session
            .warnings()
            .iter()
            .filter(|warning| warning.code == W_LLM_TIME_EXHAUSTED)
            .count(),
        1
    );
}

/// A cached answer is a call against the budget (Phase 8's rule), costs no model time, and never
/// reaches the model; the ninth question of a book is refused either way.
#[test]
fn the_session_answers_from_the_cache_and_counts_it() {
    let root = std::env::temp_dir().join(format!("oc-ai-session-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let cache = FileCache::new(&root);
    let clock = FakeClock::default();
    let model = Slow {
        clock: &clock,
        ms: 5,
        answer: common::METADATA_ANSWER.to_owned(),
        calls: AtomicUsize::new(0),
    };
    let request = common::requests().remove(0);

    let mut cold = Session::new(&model, Some(&cache), &clock, max_calls(), u64::MAX);
    let first = cold.ask(&request).expect("answered");
    assert!(!first.response.cached);
    assert_eq!(model.calls.load(Ordering::SeqCst), 1);

    let mut warm = Session::new(&model, Some(&cache), &clock, max_calls(), u64::MAX);
    let again = warm.ask(&request).expect("answered from the cache");
    assert!(again.response.cached);
    assert_eq!(again.response.text, first.response.text);
    assert_eq!(
        model.calls.load(Ordering::SeqCst),
        1,
        "the model was not asked"
    );
    assert_eq!(warm.llm_ms(), 0);
    assert_eq!(
        warm.remaining(),
        max_calls() - 1,
        "a cached answer is a call"
    );

    for _ in 1..max_calls() {
        warm.ask(&request).expect("within the budget");
    }
    assert!(matches!(warm.ask(&request), Err(Unasked::Budget(_))));
    let _ = std::fs::remove_dir_all(&root);
}

/// A provider that cannot be reached stops the session at its first question: the book is
/// deterministic from there, and nothing waits on the same dead endpoint eight times.
#[test]
fn an_unreachable_provider_stops_the_session() {
    struct Down(AtomicUsize);
    impl LlmProvider for Down {
        fn id(&self) -> &str {
            "down"
        }
        fn capabilities(&self) -> ProviderCaps {
            ProviderCaps {
                constraint: Constraint::Gbnf,
            }
        }
        fn thinking_control(&self) -> ThinkingControl {
            ThinkingControl::None
        }
        fn complete(&self, _request: &LlmRequest) -> Result<LlmResponse, LlmError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Err(LlmError::Transport(
                oc_ai::transport::TransportError::Unreachable("connection refused".to_owned()),
            ))
        }
    }
    let clock = FakeClock::default();
    let down = Down(AtomicUsize::new(0));
    let mut session = Session::new(&down, None, &clock, max_calls(), u64::MAX);
    let requests = common::requests();
    assert!(matches!(
        session.ask(&requests[0]),
        Err(Unasked::Unavailable(_))
    ));
    assert!(matches!(
        session.ask(&requests[1]),
        Err(Unasked::Unavailable(_))
    ));
    assert_eq!(down.0.load(Ordering::SeqCst), 1);
    assert!(session.stopped().is_some());
}

/// The budget is the book's length times `llm.seconds_per_page`, held between its floor and its
/// ceiling: a short book still gets the floor, a long one no more than the ceiling.
#[test]
fn the_time_budget_follows_the_book_between_its_bounds() {
    use oc_ai::session::time_budget_ms;
    assert_eq!(time_budget_ms(100, 1.0, 60, 360), 100_000);
    assert_eq!(time_budget_ms(10, 1.0, 60, 360), 60_000);
    assert_eq!(time_budget_ms(5_000, 1.0, 60, 360), 360_000);
    assert_eq!(time_budget_ms(0, f64::NAN, 60, 360), 60_000);
}
