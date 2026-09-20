"""PHASE 7 rows 7.8 and 7.9: what a conversion is scored on, and how the score is reported.

Two rules run through all of it.

**No aggregate-only row.** D18 makes the gap between `ours(*)` and the real strata a
first-class metric, and an average across strata is exactly the number that hides it: a
synthetic-corpus win masks a real-book regression, which is the failure RT A9 is about. Every
report this module produces is per stratum, and `test_per_stratum_scores_are_reported_separately`
is the check.

**A pass rate is not a number, it is an interval.** 94 of 100 and 940 of 1000 are the same
rate and not the same evidence. The gate is last-green minus the interval's half-width, so a
suite small enough to have a wide interval gates on nothing and says so.
"""

from __future__ import annotations

import pytest

from oc_eval.metrics import assertions as assertion_metrics
from oc_eval.metrics import cer, footnotes, images, reading_order, report, teds, toc_f1

# --------------------------------------------------------------------------- text


def test_the_text_score_is_one_minus_normalised_edit_distance() -> None:
    assert cer.ned("abcd", "abcd") == 0.0
    assert cer.ned("abcd", "abce") == pytest.approx(0.25)
    assert cer.ned("", "") == 0.0
    assert cer.ned("abc", "") == 1.0
    assert cer.text_score("abcd", "abce") == pytest.approx(0.75)


def test_the_text_score_normalises_before_it_measures() -> None:
    """D13.4's `N`: the pipeline is *supposed* to fold these, so a metric that counts them
    as errors scores a correct conversion as wrong (R9 §A.10's gap in Nougat's metric)."""
    assert cer.ned("ﬁre", "fire") == 0.0, "the fi ligature is the same word"
    assert cer.ned("café", "café") == 0.0, "NFC: combining acute and precomposed agree"
    assert cer.ned("pipe-\nline", "pipeline") == 0.0, "a hyphen at a line break is not a hyphen"
    assert cer.ned("a  b", "a b") == 0.0, "a no-break space is a space"


def test_a_real_difference_still_registers_after_normalisation() -> None:
    assert cer.ned("pipeline", "pipelines") > 0.0
    assert cer.ned("well-known", "wellknown") > 0.0, "a hyphen inside a line is content"


# --------------------------------------------------------------------------- reading order


def test_reading_order_scores_a_permutation_by_how_far_it_moved() -> None:
    truth = ["a", "b", "c", "d"]

    assert reading_order.score(truth, truth).edit_similarity == 1.0
    assert reading_order.score(truth, ["b", "a", "c", "d"]).edit_similarity < 1.0
    assert (
        reading_order.score(truth, ["d", "c", "b", "a"]).edit_similarity
        < reading_order.score(truth, ["b", "a", "c", "d"]).edit_similarity
    )


def test_kendall_tau_separates_a_pure_ordering_bug_from_a_content_loss_bug() -> None:
    """The secondary signal TEST_STRATEGY §8.1 asks for: tau falls on a swap and stays at 1.0
    when blocks are merely missing, which edit distance cannot tell apart."""
    truth = ["a", "b", "c", "d"]

    swapped = reading_order.score(truth, ["b", "a", "c", "d"])
    dropped = reading_order.score(truth, ["a", "b", "d"])

    assert swapped.kendall_tau < 1.0
    assert dropped.kendall_tau == 1.0
    assert dropped.edit_similarity < 1.0


def test_reading_order_of_an_empty_prediction_is_zero_rather_than_undefined() -> None:
    assert reading_order.score(["a", "b"], []).edit_similarity == 0.0
    assert reading_order.score([], []).edit_similarity == 1.0


# --------------------------------------------------------------------------- headings


def test_toc_f1_scores_heading_text_and_level_together() -> None:
    truth = [(1, "Chapter I"), (2, "The Visitor")]

    assert toc_f1.score(truth, truth).f1 == 1.0
    assert toc_f1.score(truth, [(1, "Chapter I")]).recall == pytest.approx(0.5)
    assert toc_f1.score(truth, [(2, "Chapter I"), (2, "The Visitor")]).f1 < 1.0, (
        "the right text at the wrong level is not the right heading"
    )


def test_the_heading_tree_distance_counts_the_edits_between_two_outlines() -> None:
    truth = [(1, "A"), (2, "B"), (2, "C")]

    assert toc_f1.tree_distance(truth, truth) == 0
    assert toc_f1.tree_distance(truth, [(1, "A"), (2, "B")]) == 1
    assert toc_f1.tree_distance(truth, []) == 3


# --------------------------------------------------------------------------- notes, images


def test_footnote_linkage_is_scored_on_the_pair_not_on_the_note() -> None:
    truth = [("ref-1", "note-1"), ("ref-2", "note-2")]

    assert footnotes.score(truth, truth).f1 == 1.0
    assert footnotes.score(truth, [("ref-1", "note-2"), ("ref-2", "note-1")]).f1 == 0.0, (
        "both notes are present and both are attached to the wrong marker"
    )


def test_image_preservation_scores_the_count_and_the_caption_separately() -> None:
    """R7 §B.5: images silently dropped is the commonest real bug class, so it is its own
    number rather than folded into a structure score."""
    truth = [("fig-1", "The ledger"), ("fig-2", None)]

    perfect = images.score(truth, truth)
    dropped = images.score(truth, [("fig-1", "The ledger")])
    miscaptioned = images.score(truth, [("fig-1", "The wrong thing"), ("fig-2", None)])

    assert perfect.count_f1 == 1.0
    assert perfect.caption_accuracy == 1.0
    assert dropped.count_f1 < 1.0
    assert miscaptioned.count_f1 == 1.0
    assert miscaptioned.caption_accuracy < 1.0


# --------------------------------------------------------------------------- tables


def test_teds_s_scores_the_grid_and_teds_scores_the_grid_and_the_cells() -> None:
    truth = [["a", "b"], ["c", "d"]]

    assert teds.teds(truth, truth) == 1.0
    assert teds.teds_s(truth, truth) == 1.0

    wrong_text = [["a", "b"], ["c", "X"]]
    wrong_grid = [["a", "b", "c", "d"]]

    assert teds.teds_s(truth, wrong_text) == 1.0, "the grid is right; only a cell is wrong"
    assert teds.teds(truth, wrong_text) < 1.0
    assert teds.teds_s(truth, wrong_grid) < 1.0, "a wrong grid is the worse failure"


# --------------------------------------------------------------------------- row 7.8


def test_assertion_suite_pass_rate_with_ci() -> None:
    """Row 7.8: the pass rate is reported with a 95 % interval, and the gate uses its width."""
    rate = assertion_metrics.PassRate(passed=95, total=100)

    assert rate.rate == pytest.approx(0.95)
    low, high = rate.interval()

    assert low < rate.rate < high
    assert 0.0 <= low and high <= 1.0
    assert rate.margin() == pytest.approx((high - low) / 2)

    tighter = assertion_metrics.PassRate(passed=950, total=1000).margin()

    assert tighter < rate.margin(), "the same rate over ten times the evidence is a tighter bound"


def test_the_gate_fails_a_drop_past_the_margin_and_passes_noise_inside_it() -> None:
    last_green = 0.95
    steady = assertion_metrics.PassRate(passed=94, total=100)
    regressed = assertion_metrics.PassRate(passed=70, total=100)

    assert assertion_metrics.gate(steady, last_green=last_green).passed
    outcome = assertion_metrics.gate(regressed, last_green=last_green)

    assert not outcome.passed
    assert "0.70" in outcome.detail or "0.700" in outcome.detail


def test_a_suite_too_small_to_have_an_opinion_says_so_rather_than_passing() -> None:
    """A pass rate over five assertions has an interval wide enough to admit any regression."""
    tiny = assertion_metrics.PassRate(passed=5, total=5)

    outcome = assertion_metrics.gate(tiny, last_green=0.95)

    assert not outcome.passed
    assert "too few" in outcome.detail


def test_the_first_run_has_no_last_green_and_is_not_failed_for_it() -> None:
    outcome = assertion_metrics.gate(
        assertion_metrics.PassRate(passed=90, total=100), last_green=None
    )

    assert outcome.passed
    assert "no baseline" in outcome.detail


def test_a_pass_rate_over_nothing_is_not_a_hundred_percent() -> None:
    empty = assertion_metrics.PassRate(passed=0, total=0)

    assert empty.rate == 0.0
    assert not assertion_metrics.gate(empty, last_green=0.9).passed


# --------------------------------------------------------------------------- row 7.9


def sample_rows() -> list[report.Row]:
    return [
        report.Row(stratum="ours(Typst)", file_id="f01", metric="text_score", value=0.99),
        report.Row(stratum="ours(Typst)", file_id="f02", metric="text_score", value=0.97),
        report.Row(stratum="InDesign", file_id="oapen-1", metric="text_score", value=0.91),
        report.Row(stratum="ABBYY-scanner", file_id="ia-1", metric="text_score", value=0.72),
    ]


def test_per_stratum_scores_are_reported_separately() -> None:
    """Row 7.9: one row per stratum, and no aggregate-only row anywhere in the report."""
    built = report.build(sample_rows())

    strata = {row["stratum"] for row in built["per_stratum"]}

    assert strata == {"ours(Typst)", "InDesign", "ABBYY-scanner"}
    assert "aggregate" not in built
    assert all("stratum" in row for row in built["per_stratum"])
    assert all("stratum" in row for row in built["per_file"])


def test_the_report_keeps_one_record_per_file_per_metric() -> None:
    """TEST_STRATEGY §8: one structured record per (corpus file, metric), never only a total."""
    built = report.build(sample_rows())

    assert len(built["per_file"]) == 4
    assert {row["file_id"] for row in built["per_file"]} == {"f01", "f02", "oapen-1", "ia-1"}


def test_the_ours_versus_real_gap_is_a_first_class_number() -> None:
    """D18: a widening gap is the earliest signal that a heuristic has over-fitted to our own
    renderer, and it cannot be seen in an average that includes both."""
    built = report.build(sample_rows())
    gap = built["ours_vs_real"]

    assert gap["ours"] == pytest.approx(0.98)
    assert gap["real"] == pytest.approx((0.91 + 0.72) / 2)
    assert gap["gap"] == pytest.approx(0.98 - (0.91 + 0.72) / 2)


def test_a_report_with_no_real_strata_refuses_to_state_a_gap() -> None:
    """A release may not pass on `ours(*)` alone, and a gap against nothing is not a number."""
    only_ours = [report.Row("ours(Typst)", "f01", "text_score", 0.99)]

    built = report.build(only_ours)

    assert built["ours_vs_real"]["real"] is None
    assert built["ours_vs_real"]["gap"] is None


def test_the_report_is_stable_across_runs() -> None:
    """A report that reorders itself turns a one-number change into an unreviewable diff."""
    import json

    first = json.dumps(report.build(sample_rows()), sort_keys=False)
    shuffled = list(reversed(sample_rows()))
    second = json.dumps(report.build(shuffled), sort_keys=False)

    assert first == second


def test_the_interval_is_wilsons_and_matches_the_published_value() -> None:
    """A gate is only as good as its interval, and an interval with no external check drifts.

    95/100 at 95 % is (0.8882, 0.9785) by Wilson's formula — the textbook figure. The normal
    approximation gives (0.9073, 0.9927), which is both too narrow and runs past 1.0 at rates
    a little higher; that is exactly why R9 §C.5 asks for Wilson here.
    """
    low, high = assertion_metrics.PassRate(passed=95, total=100).interval()

    assert (round(low, 4), round(high, 4)) == (0.8882, 0.9785)

    half, _ = assertion_metrics.PassRate(passed=50, total=100).interval()

    assert round(half, 4) == 0.4038


def test_a_confidence_level_with_no_quantile_is_refused_rather_than_approximated() -> None:
    rate = assertion_metrics.PassRate(passed=95, total=100)

    with pytest.raises(assertion_metrics.UnknownConfidence):
        rate.interval(level=0.975)
