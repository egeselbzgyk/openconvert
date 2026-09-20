"""PHASE 7 row 7.4: calibration may not read the holdout, and the refusal is a mechanism.

TEST_CORPUS §7.1(c) and D18: the frozen ≥ 100-document holdout is *reported on*, never fitted
against. The distinction is the whole value of it — a threshold tuned on a file is a threshold
that file can no longer test — and a rule that lives only in a document is one somebody
eventually breaks by accident on a Friday.

So the refusal lives in the code path. Anything that would fit a number takes its inputs
through `calibrate.fit`, which loads the manifest and raises on an id marked `holdout`.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from oc_eval import calibrate
from oc_eval.calibrate import reliability, risk_coverage

REPO_ROOT = Path(__file__).resolve().parents[2]
MANIFEST = REPO_ROOT / "corpus" / "manifest.json"


def a_holdout_id() -> str:
    from oc_eval.corpus import manifest as mf

    holdout = [item.id for item in mf.load(MANIFEST).documents if item.holdout]
    assert holdout, "the committed manifest has no holdout documents"
    return holdout[0]


def a_fittable_id() -> str:
    from oc_eval.corpus import manifest as mf

    fittable = [item.id for item in mf.load(MANIFEST).entries if not item.holdout]
    assert fittable, "the committed manifest has nothing that may be fitted on"
    return fittable[0]


# --------------------------------------------------------------------------- row 7.4


def test_calibrate_refuses_holdout_files() -> None:
    """Row 7.4: passing a holdout id to a calibration path raises."""
    holdout = a_holdout_id()

    with pytest.raises(calibrate.HoldoutLeak) as caught:
        calibrate.fit(
            [calibrate.Observation(file_id=holdout, confidence=0.9, correct=True)],
            manifest_path=MANIFEST,
        )

    assert holdout in str(caught.value)


def test_a_fit_over_files_that_may_be_fitted_on_is_allowed() -> None:
    fittable = a_fittable_id()

    outcome = calibrate.fit(
        [
            calibrate.Observation(file_id=fittable, confidence=0.9, correct=True),
            calibrate.Observation(file_id=fittable, confidence=0.3, correct=False),
        ],
        manifest_path=MANIFEST,
    )

    assert outcome.observations == 2


def test_one_holdout_file_among_many_still_stops_the_whole_fit() -> None:
    """A fit that quietly dropped the offending file would still be a fit nobody can trust."""
    fittable, holdout = a_fittable_id(), a_holdout_id()

    with pytest.raises(calibrate.HoldoutLeak):
        calibrate.fit(
            [
                calibrate.Observation(file_id=fittable, confidence=0.9, correct=True),
                calibrate.Observation(file_id=holdout, confidence=0.8, correct=True),
            ],
            manifest_path=MANIFEST,
        )


def test_an_id_the_manifest_does_not_know_is_refused_rather_than_assumed_safe() -> None:
    """An id nobody can check against the manifest is not evidence that it is not holdout."""
    with pytest.raises(calibrate.UnknownFile):
        calibrate.fit(
            [calibrate.Observation(file_id="not-in-the-manifest", confidence=0.5, correct=True)],
            manifest_path=MANIFEST,
        )


def test_the_refusal_names_every_offending_file_not_just_the_first() -> None:
    from oc_eval.corpus import manifest as mf

    holdout = [item.id for item in mf.load(MANIFEST).documents if item.holdout][:3]

    with pytest.raises(calibrate.HoldoutLeak) as caught:
        calibrate.fit(
            [calibrate.Observation(f, 0.5, True) for f in holdout], manifest_path=MANIFEST
        )

    for file_id in holdout:
        assert file_id in str(caught.value)


def test_a_fit_over_nothing_is_refused_rather_than_returning_a_threshold() -> None:
    with pytest.raises(calibrate.NotEnoughEvidence):
        calibrate.fit([], manifest_path=MANIFEST)


# --------------------------------------------------------------------------- the curves


def test_risk_coverage_trades_coverage_for_correctness() -> None:
    """D17: an escalation threshold is a point on this curve, chosen for a target risk."""
    observations = [
        risk_coverage.Point(confidence=0.95, correct=True),
        risk_coverage.Point(confidence=0.90, correct=True),
        risk_coverage.Point(confidence=0.60, correct=False),
        risk_coverage.Point(confidence=0.20, correct=False),
    ]

    curve = risk_coverage.curve(observations)

    assert curve[0].coverage < curve[-1].coverage
    assert curve[0].risk <= curve[-1].risk, "taking on the least confident cases cannot help"
    assert curve[-1].coverage == 1.0


def test_the_threshold_for_a_target_risk_is_the_widest_coverage_that_holds_it() -> None:
    observations = [
        risk_coverage.Point(0.95, True),
        risk_coverage.Point(0.90, True),
        risk_coverage.Point(0.60, False),
        risk_coverage.Point(0.20, False),
    ]

    chosen = risk_coverage.threshold_for(observations, target_risk=0.0)

    assert chosen is not None
    assert chosen.confidence >= 0.90
    assert chosen.risk == 0.0


def test_a_target_risk_nothing_meets_returns_nothing_rather_than_the_best_available() -> None:
    always_wrong = [risk_coverage.Point(0.99, False)]

    assert risk_coverage.threshold_for(always_wrong, target_risk=0.0) is None


def test_expected_calibration_error_is_zero_for_a_perfectly_calibrated_model() -> None:
    """90 % confidence that is right 90 % of the time is what calibrated means."""
    observations = [reliability.Point(0.9, True)] * 9 + [reliability.Point(0.9, False)]

    assert reliability.expected_calibration_error(observations, bins=10) == pytest.approx(0.0)


def test_expected_calibration_error_grows_with_overconfidence() -> None:
    overconfident = [reliability.Point(0.99, True)] * 5 + [reliability.Point(0.99, False)] * 5

    assert reliability.expected_calibration_error(overconfident, bins=10) == pytest.approx(0.49)


def test_a_reliability_diagram_has_one_row_per_occupied_bin() -> None:
    observations = [reliability.Point(0.1, False), reliability.Point(0.9, True)]

    rows = reliability.diagram(observations, bins=10)

    assert len(rows) == 2
    assert all(row.count == 1 for row in rows)
