//! Gate V (D13.5, ARCHITECTURE §6.2, RT B13): an edit may not make the region it touches read
//! worse, as measured by a **fixed ordered tuple** of statistics that are defined on a region —
//! and a statistic that is not defined on it is skipped, not defaulted.

mod common;

use std::collections::BTreeSet;

use common::book::{chapter, document, heading, para, verse};
use oc_ai::gates::fallback::{settle, Choice};
use oc_ai::gates::validity::{gate_validity, Region, RegionStats, Tolerance};
use oc_ai::gates::{gate_edit, GateFailure};
use oc_ai::provider::trace;
use oc_core::thresholds::T;
use oc_model::confidence::Method;
use oc_model::doc::Content;
use oc_model::document::Document;
use oc_model::lang::LangTag;

fn tolerance() -> Tolerance {
    Tolerance {
        ratio: T.llm.gate_v_ratio_eps as f32,
        violations: u32::try_from(T.llm.gate_v_violations_eps).expect("a small count"),
    }
}

fn words(count: usize) -> String {
    const VOCABULARY: [&str; 7] = ["der", "Käfer", "lag", "still", "und", "sah", "hinaus"];
    (0..count)
        .map(|index| VOCABULARY[index % VOCABULARY.len()])
        .collect::<Vec<_>>()
        .join(" ")
}

fn block_of(content: &Content) -> oc_model::ids::BlockId {
    match content {
        Content::Paragraph(para) => para.id,
        other => panic!("not a paragraph: {other:?}"),
    }
}

/// Test 8.7 (RT B13). Most Gopher statistics are document-level: datatrove's quality filter
/// wants at least fifty words, so on a twenty-word region it fails every block ever measured,
/// and a gate that consulted it would either never fire or always fire. The tuple is closed,
/// holds only statistics defined on a region, and a twenty-word region is judged on them alone.
#[test]
fn gate_v_skips_undefined_document_statistics() {
    assert_eq!(
        RegionStats::STATISTICS,
        [
            "dup_line_ratio",
            "top_2gram_ratio",
            "top_3gram_ratio",
            "non_alpha_word_ratio",
            "heading_tree_violations",
        ],
        "the tuple is fixed and ordered, and nothing document-level is in it"
    );
    assert!(!RegionStats::STATISTICS.contains(&"min_doc_words"));

    let twenty = para(&words(20));
    let region = Region::Blocks(BTreeSet::from([block_of(&twenty)]));
    let book = document(vec![chapter(1, "Erstes Kapitel", 1, vec![twenty])]);
    let stats = RegionStats::measure(&book, &region);
    assert_eq!(stats.words, 20);
    assert!(stats.dup_line_ratio.is_some() && stats.top_3gram_ratio.is_some());
    assert_eq!(gate_validity(&stats, &stats, &tolerance()), Ok(()));

    // Undefined on a region is `None`, never a default: two words hold no 3-gram.
    let two = para("zwei Worte");
    let region = Region::Blocks(BTreeSet::from([block_of(&two)]));
    let small = RegionStats::measure(&document(vec![chapter(1, "K", 1, vec![two])]), &region);
    assert_eq!(small.top_3gram_ratio, None);
    assert!(small.top_2gram_ratio.is_some());

    // And a statistic undefined on either side is skipped rather than compared. At 0.5 against
    // nothing, a gate that defaulted the absent value to 0 fails one direction and a gate that
    // defaulted it to 1 fails the other.
    let defined = RegionStats {
        top_3gram_ratio: Some(0.5),
        ..small
    };
    let undefined = RegionStats {
        top_3gram_ratio: None,
        ..small
    };
    assert_eq!(gate_validity(&defined, &undefined, &tolerance()), Ok(()));
    assert_eq!(gate_validity(&undefined, &defined, &tolerance()), Ok(()));
}

/// Test 8.8. A paragraph made verse whose lines turn out to repeat — a refrain, or a model that
/// broke prose into lines that happen to match — raises the duplicate-line ratio by far more than
/// epsilon, and the edit reverts: the book stays as it was and the decision says gate V refused.
#[test]
fn gate_v_reverts_on_worsened_statistic() {
    let before = document(vec![chapter(
        1,
        "Regen",
        1,
        vec![
            para("the rain fell the rain fell the rain fell"),
            para("and then it stopped"),
        ],
    )]);
    let mut after = before.clone();
    after.sections[0].content[0] = verse(&["the rain fell", "the rain fell", "the rain fell"]);

    let verdict = gate_edit(&before, &after, &Region::Whole, &tolerance());
    match &verdict {
        Err(GateFailure::Worsened {
            statistic,
            before,
            after,
        }) => {
            assert_eq!(*statistic, "dup_line_ratio");
            assert_eq!(*before, 0.0);
            assert!(*after > *before + tolerance().ratio, "{after}");
        }
        other => panic!("a repeated-line edit was admitted: {other:?}"),
    }

    let kept: &Document = if verdict.is_ok() { &after } else { &before };
    assert_eq!(kept, &before, "the edit reverts");

    let request = &common::requests()[3];
    let decision = settle(
        Choice {
            stage: "structure",
            kind: "verse_quote",
            subject: None,
            deterministic: "paragraph".to_owned(),
            alternatives: vec!["verse".to_owned()],
        },
        trace("qwen3-1.7b-q4_k_m", request, "{}", false, 90),
        verdict.map(|()| "verse".to_owned()),
    );
    assert_eq!(decision.method, Method::Deterministic);
    assert_eq!(decision.fallback, Some("V.worsened"));
}

/// Within epsilon is not worse. Gate V's comparisons are relative and forgiving by design — the
/// Gopher bounds are not absolute filters here (ARCHITECTURE §6.2) — and a component that did not
/// move by more than its epsilon does not refuse an edit.
#[test]
fn gate_v_admits_a_change_within_epsilon() {
    let before = RegionStats {
        dup_line_ratio: Some(0.10),
        top_2gram_ratio: Some(0.05),
        top_3gram_ratio: Some(0.04),
        non_alpha_word_ratio: Some(0.02),
        heading_tree_violations: 1,
        lines: 40,
        words: 400,
    };
    let within = RegionStats {
        dup_line_ratio: Some(0.10 + tolerance().ratio / 2.0),
        ..before
    };
    assert_eq!(gate_validity(&before, &within, &tolerance()), Ok(()));

    let better = RegionStats {
        heading_tree_violations: 0,
        top_2gram_ratio: Some(0.0),
        ..before
    };
    assert_eq!(gate_validity(&before, &better, &tolerance()), Ok(()));
}

/// The integer component, which is load-bearing for `heading_roles` and `book_structure`: an edit
/// that turns an `h2` into an `h3` straight under an `h1` adds a level skip, and its epsilon is
/// zero.
#[test]
fn gate_v_refuses_a_new_heading_level_skip() {
    let before = document(vec![chapter(
        1,
        "Erster Teil",
        1,
        vec![heading(2, "Kapitel Eins"), para(&words(12))],
    )]);
    let mut after = before.clone();
    if let Content::Heading(heading) = &mut after.sections[0].content[0] {
        heading.level = 3;
    }

    let verdict = gate_edit(&before, &after, &Region::Whole, &tolerance());
    match verdict {
        Err(GateFailure::Worsened {
            statistic,
            before,
            after,
        }) => {
            assert_eq!(statistic, "heading_tree_violations");
            assert_eq!((before, after), (0.0, 1.0));
        }
        other => panic!("a new level skip was admitted: {other:?}"),
    }
}

/// A heading whose page comes before the previous heading's is out of page order, and counts.
#[test]
fn headings_out_of_page_order_are_violations() {
    let book = document(vec![
        chapter(1, "Zwei", 20, vec![para(&words(5))]),
        chapter(1, "Eins", 3, vec![para(&words(5))]),
    ]);
    assert_eq!(
        RegionStats::measure(&book, &Region::Whole).heading_tree_violations,
        1
    );
}

/// The region is the blocks the edit touched. A line repeated elsewhere in the book is not this
/// edit's doing and does not count against it.
#[test]
fn a_region_is_only_the_blocks_it_names() {
    let touched = para("ein ganz gewöhnlicher Satz");
    let book = document(vec![chapter(
        1,
        "Kapitel",
        1,
        vec![para("ein Refrain"), para("ein Refrain"), touched.clone()],
    )]);
    let whole = RegionStats::measure(&book, &Region::Whole);
    let region = RegionStats::measure(&book, &Region::Blocks(BTreeSet::from([block_of(&touched)])));
    assert!(whole.dup_line_ratio > Some(0.0));
    assert_eq!(region.dup_line_ratio, Some(0.0));
    assert_eq!(region.lines, 1);
}

/// Gate V restates oc-text's Gopher statistics, because `oc-ai` may not depend on `oc-text`
/// (`docs/DECISIONS_LOG.md`, 2026-09-22). This test is what keeps it one definition in two places:
/// on the same lines, the four text components equal `oc_text::stats::quality_stats` exactly.
#[test]
fn gate_v_statistics_are_oc_texts() {
    let samples: [&[&str]; 3] = [
        &[
            "Kapitel Eins",
            "der Käfer lag still",
            "der Käfer lag still",
            "und sah hinaus",
        ],
        &[
            "1.",
            "2.",
            "— 3 —",
            "Er lag auf seinem Rücken",
            "Er lag auf seinem Rücken",
        ],
        &[
            "Önsöz",
            "İstanbul'da bir sabah",
            "bir sabah bir sabah bir sabah",
        ],
    ];
    for lines in samples {
        let book = document(vec![chapter(
            1,
            lines[0],
            1,
            lines[1..].iter().map(|line| para(line)).collect(),
        )]);
        let ours = RegionStats::measure(&book, &Region::Whole);
        let theirs = oc_text::stats::quality_stats(
            &lines.join("\n"),
            LangTag::UND,
            oc_text::stats::Region::Document,
            None,
        );
        assert_eq!(ours.dup_line_ratio, Some(theirs.dup_line_frac), "{lines:?}");
        assert_eq!(ours.top_2gram_ratio, Some(theirs.top_2gram), "{lines:?}");
        assert_eq!(ours.top_3gram_ratio, Some(theirs.top_3gram), "{lines:?}");
        assert_eq!(
            ours.non_alpha_word_ratio,
            Some(theirs.non_alpha_word_ratio),
            "{lines:?}"
        );
    }
}
