//! Invariants I-1 … I-4 of the conservation law, checked after every stage
//! (ARCHITECTURE §5.4, D13.4).
//!
//! This module is deliberately the first thing Phase 2 builds. Every transformation that
//! follows is born under the checker rather than retrofitted to it, because a conservation
//! law added afterwards only ever documents the losses that already happened.
//!
//! What is checked here, in the order a failure is reported:
//!
//! - **I-3** a `Conserving` stage produced no ledger entry at all,
//! - **I-2** every reason cited is one the stage declared,
//! - **I-1** `C(D_i) ⊎ Added == C(D_{i+1}) ⊎ Removed`,
//! - **I-4** the cumulative net removal per budget group, and overall, is inside its bound.
//!
//! I-3 is reported before I-2 because a `Conserving` stage declares no reasons at all, so
//! the undeclared-reason message would be true but useless: the fault is that it wrote to
//! the ledger, not which reason it wrote.
//!
//! I-5 (dehyphenation) and I-6 (OCR) belong to the stages that own those reasons and arrive
//! with them, in Phases 3 and 13. I-7 is the end-to-end release gate, in Phase 5.

use std::collections::BTreeMap;
use std::fmt;

use oc_model::extract::CharHistogram;
use oc_model::ledger::{LedgerDelta, Reason, StageCheck, StageKind};

pub use oc_model::ledger::{c_of, c_of_parts};

use crate::stages::StageDecl;
use crate::thresholds::T;

/// A budget and the name it is reported under.
///
/// Several reasons share one group — the three furniture reasons are one allowance between
/// them — so the name is not always the reason's name, and a message that said `page_number`
/// when the bound is `furniture` would send a reader to the wrong threshold.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BudgetGroup {
    pub name: &'static str,
    /// The bound, as a fraction of `|C_0|`.
    pub fraction: f64,
}

/// Which budget a reason draws on (ARCHITECTURE §5.5).
///
/// `None` for `Ocr`: it is Added-only and region-scoped by I-6, so a bound stated as a
/// fraction of the source text would forbid transcribing a scanned book at all.
pub fn budget_group(reason: Reason) -> Option<BudgetGroup> {
    let budget = &T.conservation.budget;
    let group = match reason {
        Reason::Ocr => return None,
        Reason::RunningHeader | Reason::RunningFooter | Reason::PageNumber => BudgetGroup {
            name: "furniture",
            fraction: budget.furniture,
        },
        Reason::OverdrawDedup => BudgetGroup {
            name: "overdraw_dedup",
            fraction: budget.overdraw_dedup,
        },
        // Stated per page by ARCHITECTURE §5.5; the document-wide form here is the weaker of
        // the two and is a backstop. The per-page check belongs to `ingest`, the only stage
        // that can see a page's own OCR layer (Phase 13).
        Reason::OcrLayerDuplicate => BudgetGroup {
            name: "ocr_layer_duplicate",
            fraction: budget.ocr_layer_duplicate_per_page,
        },
        Reason::Dehyphenate => BudgetGroup {
            name: "dehyphenate",
            fraction: budget.dehyphenate,
        },
        Reason::DecorativeGlyph => BudgetGroup {
            name: "decorative_glyph",
            fraction: budget.decorative_glyph,
        },
        _ => BudgetGroup {
            name: "other",
            fraction: budget.other,
        },
    };
    Some(group)
}

/// The running totals I-4 is stated over: `|C_0|` and the net removal charged to each budget
/// group so far.
///
/// Cumulative rather than per stage because the budget is cumulative: three stages each
/// taking 3 % of a book under one reason have taken 9 % of it, and a per-stage check would
/// pass all three.
#[derive(Clone, Debug, Default)]
pub struct ReasonTotals {
    c0_total: u64,
    per_group: BTreeMap<&'static str, u64>,
    non_ocr_removed: u64,
}

impl ReasonTotals {
    /// Open the account against `C_0`, the retention denominator (ARCHITECTURE §5.2).
    pub fn new(c0: &CharHistogram) -> Self {
        Self {
            c0_total: c0.total(),
            per_group: BTreeMap::new(),
            non_ocr_removed: 0,
        }
    }

    pub fn c0_total(&self) -> u64 {
        self.c0_total
    }

    /// Net characters charged to one budget group so far.
    pub fn group_total(&self, group: &str) -> u64 {
        self.per_group.get(group).copied().unwrap_or_default()
    }

    /// Net non-OCR characters removed so far, the quantity the global cap bounds.
    pub fn non_ocr_removed(&self) -> u64 {
        self.non_ocr_removed
    }
}

/// A stage broke the conservation law. Always an error: a warning here would make the law a
/// preference (`IMPLEMENTATION_PLAN` Phase 2 detail 4).
///
/// What a caller then *does* with a budget breach is the orchestrator's, in Phase 6:
/// ARCHITECTURE §5.5 lets it stop that stage's remaining removals, warn, and finish the
/// book. The checker's job is to refuse to call it fine.
#[derive(Clone, Debug, PartialEq)]
pub enum ConservationError {
    /// I-3: a stage that promised not to touch the text touched it.
    ConservingStageMutated { stage: &'static str, reason: Reason },
    /// I-5: a `Dehyphenate` entry was not exactly one line-break hyphen leaving.
    DehyphenateNotOneHyphen {
        stage: &'static str,
        text: String,
        added: bool,
    },
    /// I-2: a stage cited a reason outside its declared set.
    UndeclaredReason { stage: &'static str, reason: Reason },
    /// I-1: the equation does not balance, so text moved without being accounted for.
    NotConserved {
        stage: &'static str,
        /// Characters that left with no ledger entry.
        unexplained_removed: u64,
        /// Characters that appeared with no ledger entry.
        unexplained_added: u64,
    },
    /// I-4: a budget group is over its bound.
    BudgetExceeded {
        stage: &'static str,
        group: &'static str,
        budget: f64,
        measured: f64,
    },
    /// I-4: the global cap on non-OCR removal is over its bound.
    GlobalRemovalExceeded {
        stage: &'static str,
        budget: f64,
        measured: f64,
    },
}

impl fmt::Display for ConservationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConservationError::DehyphenateNotOneHyphen { stage, text, added } => {
                let side = if *added { "added" } else { "removed" };
                write!(
                    f,
                    "I-5: stage {stage} {side} {text:?} under Dehyphenate; a dehyphenation \
                     removes exactly one U+002D or U+2010 and nothing else"
                )
            }
            ConservationError::ConservingStageMutated { stage, reason } => {
                write!(
                    f,
                    "I-3: stage {stage} is Conserving but ledgered {reason:?}"
                )
            }
            ConservationError::UndeclaredReason { stage, reason } => write!(
                f,
                "I-2: stage {stage} cited {reason:?}, which it does not declare"
            ),
            ConservationError::NotConserved {
                stage,
                unexplained_removed,
                unexplained_added,
            } => write!(
                f,
                "I-1: stage {stage} does not balance: {unexplained_removed} characters left \
                 and {unexplained_added} appeared with no ledger entry"
            ),
            ConservationError::BudgetExceeded {
                stage,
                group,
                budget,
                measured,
            } => write!(
                f,
                "I-4: stage {stage} put the {group} budget at {measured} of |C_0|, over {budget}"
            ),
            ConservationError::GlobalRemovalExceeded {
                stage,
                budget,
                measured,
            } => write!(
                f,
                "I-4: stage {stage} put total non-OCR removal at {measured} of |C_0|, over \
                 {budget}"
            ),
        }
    }
}

impl std::error::Error for ConservationError {}

/// The scalars a dehyphenation may remove, and the only ones (PIPELINE §7, I-5).
///
/// U+00AD is not among them: a soft hyphen leaves in `text`, under its own reason, and one
/// arriving here would mean `N` had not run.
const DEHYPHEN_SCALARS: [char; 2] = ['\u{002D}', '\u{2010}'];

/// Check invariants I-1 … I-5 for one stage.
///
/// `before` and `after` are `C(D)` on either side of the stage — the caller computes them
/// with [`c_of`], because only the caller knows which of a document's strings are content
/// text and which are attribute values outside `C` (ARCHITECTURE §5.2).
///
/// `totals` is updated *before* the budget test, so a caller that chooses to continue past a
/// breach keeps counting from the true figure rather than from the last one that passed.
pub fn check_invariants(
    before: &CharHistogram,
    after: &CharHistogram,
    delta: &LedgerDelta,
    decl: StageDecl,
    totals: &mut ReasonTotals,
) -> Result<StageCheck, ConservationError> {
    // I-3 — a Conserving stage has an empty ledger, so plain multiset equality holds.
    if decl.kind == StageKind::Conserving {
        if let Some(entry) = delta.entries().first() {
            return Err(ConservationError::ConservingStageMutated {
                stage: decl.name,
                reason: entry.reason,
            });
        }
    }

    // I-2 — every reason cited is one the stage declared.
    for reason in delta.reasons() {
        if !decl.declares(reason) {
            return Err(ConservationError::UndeclaredReason {
                stage: decl.name,
                reason,
            });
        }
    }

    // I-5 — a `Dehyphenate` entry removes exactly one scalar in {U+002D, U+2010} and adds
    // nothing. This is the half of I-5 the ledger can check on its own; the other half — that
    // the word left behind is the two pieces concatenated and nothing else — is a property of
    // the join and is checked where the join is made (PIPELINE §7, `oc_text::dehyphen`).
    //
    // It is here rather than in the stage because a wrong dehyphenation is invisible to every
    // other invariant: it removes one character, records it, and every equation still
    // balances while the book is quietly wrong (RT A1).
    for entry in delta.entries() {
        if entry.reason != Reason::Dehyphenate {
            continue;
        }
        let mut scalars = entry.text.chars();
        let one = scalars.next();
        let single = matches!(one, Some(ch) if DEHYPHEN_SCALARS.contains(&ch));
        if entry.added || scalars.next().is_some() || !single {
            return Err(ConservationError::DehyphenateNotOneHyphen {
                stage: decl.name,
                text: entry.text.clone(),
                added: entry.added,
            });
        }
    }

    // I-1 — C(D_i) ⊎ Added == C(D_{i+1}) ⊎ Removed.
    let added = delta.added();
    let removed = delta.removed();
    let left = before.union(&added);
    let right = after.union(&removed);
    if left != right {
        return Err(ConservationError::NotConserved {
            stage: decl.name,
            unexplained_removed: left.difference(&right).total(),
            unexplained_added: right.difference(&left).total(),
        });
    }

    // I-4 — cumulative net removal per budget group, then the global cap. Charged first and
    // tested afterwards so that every group this stage touched is measured against its true
    // running total, not against the order the reasons happened to arrive in.
    let mut touched: Vec<BudgetGroup> = Vec::new();
    for reason in delta.reasons() {
        let net = delta.net_removed(reason);
        if net == 0 {
            continue;
        }
        if reason != Reason::Ocr {
            totals.non_ocr_removed = totals.non_ocr_removed.saturating_add(net);
        }
        if let Some(group) = budget_group(reason) {
            let slot = totals.per_group.entry(group.name).or_default();
            *slot = slot.saturating_add(net);
            if !touched.iter().any(|g| g.name == group.name) {
                touched.push(group);
            }
        }
    }
    for group in touched {
        let measured = share(totals.group_total(group.name), totals.c0_total);
        if measured > group.fraction {
            return Err(ConservationError::BudgetExceeded {
                stage: decl.name,
                group: group.name,
                budget: group.fraction,
                measured,
            });
        }
    }
    let global = share(totals.non_ocr_removed, totals.c0_total);
    if global > T.conservation.global_non_ocr_removal {
        return Err(ConservationError::GlobalRemovalExceeded {
            stage: decl.name,
            budget: T.conservation.global_non_ocr_removal,
            measured: global,
        });
    }

    Ok(StageCheck {
        stage: decl.name,
        kind: decl.kind,
        removed_chars: removed.total(),
        added_chars: added.total(),
        retention: share(after.total(), totals.c0_total) as f32,
    })
}

/// `part / whole`, with an empty document scoring zero rather than dividing by it.
fn share(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        return 0.0;
    }
    part as f64 / whole as f64
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Rows 2.16 and 2.17 of the
// Phase 2 table, plus the unit-level I-1 and I-2 cases row 2.15 exercises over
// whole documents once the `text` and `furniture` stages exist.
// ---------------------------------------------------------------------------

#[cfg(test)]
use oc_model::ledger::LedgerEntry;

/// A stage that removes nothing and adds nothing, used by the I-3 test.
#[cfg(test)]
const LAYOUT: StageDecl = StageDecl {
    name: "layout",
    kind: StageKind::Conserving,
    reasons: &[],
};

#[cfg(test)]
fn repeated(ch: char, times: usize) -> String {
    std::iter::repeat_n(ch, times).collect()
}

#[test]
fn conservation_i3_conserving_stage_has_empty_ledger() {
    let c0 = c_of("Call me Ishmael");
    let mut totals = ReasonTotals::new(&c0);
    let before = c_of("Call me Ishmael");
    let after = c_of("Call me Ishmae");

    let delta = LedgerDelta::new(vec![LedgerEntry::removed(
        LAYOUT.name,
        Reason::RunningHeader,
        0,
        (14, 15),
        "l".to_owned(),
    )]);

    let err = check_invariants(&before, &after, &delta, LAYOUT, &mut totals)
        .expect_err("a Conserving stage that emitted a ledger entry must fail I-3");
    assert_eq!(
        err,
        ConservationError::ConservingStageMutated {
            stage: "layout",
            reason: Reason::RunningHeader,
        }
    );
}

#[test]
fn conservation_i4_budget_exceeded_is_fatal() {
    // A thousand non-whitespace characters, of which furniture takes a hundred: 10 %,
    // against the 4 % the furniture reasons share.
    let c0 = c_of(&repeated('a', 1_000));
    let mut totals = ReasonTotals::new(&c0);
    let before = c_of(&repeated('a', 1_000));
    let after = c_of(&repeated('a', 900));

    let delta = LedgerDelta::new(vec![LedgerEntry::removed(
        crate::stages::FURNITURE.name,
        Reason::RunningHeader,
        0,
        (0, 100),
        repeated('a', 100),
    )]);

    let err = check_invariants(
        &before,
        &after,
        &delta,
        crate::stages::FURNITURE,
        &mut totals,
    )
    .expect_err("removing 10 % of C_0 as furniture must exceed the 4 % budget");

    match err {
        ConservationError::BudgetExceeded {
            stage,
            group,
            budget,
            measured,
        } => {
            assert_eq!(stage, "furniture");
            assert_eq!(group, "furniture");
            assert!((budget - 0.04).abs() < 1e-9, "budget was {budget}");
            assert!((measured - 0.10).abs() < 1e-9, "measured was {measured}");
        }
        other => panic!("expected a budget breach, got {other:?}"),
    }
    let rendered = err.to_string();
    assert!(rendered.contains("furniture"), "{rendered}");
    assert!(rendered.contains("0.04"), "{rendered}");
}

#[test]
fn conservation_i1_unexplained_removal_is_fatal() {
    let c0 = c_of("Ishmael");
    let mut totals = ReasonTotals::new(&c0);
    let before = c_of("Ishmael");
    // The stage dropped a character and said nothing about it.
    let after = c_of("Ishmae");

    let err = check_invariants(
        &before,
        &after,
        &LedgerDelta::new(Vec::new()),
        crate::stages::TEXT,
        &mut totals,
    )
    .expect_err("a removal with no ledger entry must fail I-1");
    assert!(matches!(
        err,
        ConservationError::NotConserved { stage: "text", .. }
    ));
}

#[test]
fn conservation_i1_balances_a_ligature_expansion() {
    // The paired Removed + Added case: one scalar becomes two and plain multiset
    // equality fails, which is exactly why I-1 is stated with both sides.
    let c0 = c_of("fire");
    let mut totals = ReasonTotals::new(&c0);
    let before = c_of("\u{FB01}re");
    let after = c_of("fire");

    let delta = LedgerDelta::new(vec![
        LedgerEntry::removed(
            crate::stages::TEXT.name,
            Reason::LigatureExpand,
            0,
            (0, 1),
            "\u{FB01}".to_owned(),
        ),
        LedgerEntry::added(
            crate::stages::TEXT.name,
            Reason::LigatureExpand,
            0,
            (0, 2),
            "fi".to_owned(),
        ),
    ]);

    let check = check_invariants(&before, &after, &delta, crate::stages::TEXT, &mut totals)
        .expect("a ledgered ligature expansion conserves");
    assert_eq!(check.removed_chars, 1);
    assert_eq!(check.added_chars, 2);
}

#[test]
fn conservation_i2_undeclared_reason_is_fatal() {
    let c0 = c_of("Ishmael");
    let mut totals = ReasonTotals::new(&c0);
    let before = c_of("Ishmael");
    let after = c_of("Ishmae");

    // `text` declares SoftHyphen and LigatureExpand, and nothing else.
    let delta = LedgerDelta::new(vec![LedgerEntry::removed(
        crate::stages::TEXT.name,
        Reason::PageNumber,
        0,
        (6, 7),
        "l".to_owned(),
    )]);

    let err = check_invariants(&before, &after, &delta, crate::stages::TEXT, &mut totals)
        .expect_err("a reason the stage never declared must fail I-2");
    assert_eq!(
        err,
        ConservationError::UndeclaredReason {
            stage: "text",
            reason: Reason::PageNumber,
        }
    );
}

#[test]
fn c_of_counts_only_non_whitespace() {
    // C is defined over scalars without the Unicode White_Space property (ARCHITECTURE
    // §5.2), which is what makes a generated space invisible to the ledger and a soft
    // hyphen visible to it.
    let histogram = c_of("a b\u{00A0}c\u{00AD}");
    assert_eq!(histogram.total(), 4);
    assert_eq!(histogram.count('\u{00AD}'), 1);
    assert_eq!(histogram.count(' '), 0);
    assert_eq!(histogram.count('\u{00A0}'), 0);
}

#[test]
fn budgets_accumulate_across_stages() {
    // I-4 is cumulative: two stages each under the cap can breach it together.
    let c0 = c_of(&repeated('a', 1_000));
    let mut totals = ReasonTotals::new(&c0);

    let first = LedgerDelta::new(vec![LedgerEntry::removed(
        crate::stages::FURNITURE.name,
        Reason::RunningHeader,
        0,
        (0, 30),
        repeated('a', 30),
    )]);
    check_invariants(
        &c_of(&repeated('a', 1_000)),
        &c_of(&repeated('a', 970)),
        &first,
        crate::stages::FURNITURE,
        &mut totals,
    )
    .expect("3 % is inside the 4 % budget");

    let second = LedgerDelta::new(vec![LedgerEntry::removed(
        crate::stages::FURNITURE.name,
        Reason::PageNumber,
        1,
        (0, 20),
        repeated('a', 20),
    )]);
    let err = check_invariants(
        &c_of(&repeated('a', 970)),
        &c_of(&repeated('a', 950)),
        &second,
        crate::stages::FURNITURE,
        &mut totals,
    )
    .expect_err("3 % + 2 % of the same shared budget is 5 %");
    assert!(matches!(
        err,
        ConservationError::BudgetExceeded {
            group: "furniture",
            ..
        }
    ));
}

#[test]
fn budget_charges_net_loss_not_churn() {
    // A budget bounds text *loss*. `LigatureExpand` takes one scalar and puts two back, so
    // a book full of `ﬁ` must not spend its way through the 0.001 "other" allowance by
    // spelling them out. See docs/DECISIONS_LOG.md.
    let c0 = c_of(&repeated('a', 1_000));
    let mut totals = ReasonTotals::new(&c0);

    let mut delta = LedgerDelta::default();
    let mut before_text = repeated('a', 900);
    let mut after_text = repeated('a', 900);
    for index in 0..100u32 {
        before_text.push('\u{FB01}');
        after_text.push_str("fi");
        delta.push(LedgerEntry::removed(
            crate::stages::TEXT.name,
            Reason::LigatureExpand,
            0,
            (index, index + 1),
            "\u{FB01}".to_owned(),
        ));
        delta.push(LedgerEntry::added(
            crate::stages::TEXT.name,
            Reason::LigatureExpand,
            0,
            (index, index + 2),
            "fi".to_owned(),
        ));
    }

    let check = check_invariants(
        &c_of(&before_text),
        &c_of(&after_text),
        &delta,
        crate::stages::TEXT,
        &mut totals,
    )
    .expect("expanding a tenth of the book's ligatures loses nothing");
    assert_eq!(check.removed_chars, 100);
    assert_eq!(check.added_chars, 200);
    assert_eq!(totals.group_total("other"), 0);
    assert_eq!(totals.non_ocr_removed(), 0);
}
