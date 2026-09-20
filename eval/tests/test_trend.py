"""PHASE 7 row 7.10: the `ours(*)`-versus-real gap, tracked over time rather than sampled.

D18 makes the gap a first-class metric because a *widening* one is the earliest available
signal that a heuristic or an escalation threshold has been fitted to our own renderer rather
than to books (RT A9). A single run cannot show that. A gap of 0.06 is fine or alarming
depending on whether last week's was 0.05 or 0.09, so the number has to be kept.

History lives in the repository (TEST_STRATEGY §8), so a regression is "diff against the
immediately-prior committed baseline" — the same discipline as snapshot review, and the same
reason: a baseline nobody can see is a baseline nobody checks.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from oc_eval import trend
from oc_eval.metrics import report

REPO_ROOT = Path(__file__).resolve().parents[2]
COMMITTED_TREND = REPO_ROOT / "eval" / "out" / "trend.json"


def scores() -> list[report.Row]:
    return [
        report.Row("ours(Typst)", "f01", "text_score", 0.99),
        report.Row("InDesign", "oapen-1", "text_score", 0.91),
        report.Row("ABBYY-scanner", "ia-1", "text_score", 0.85),
    ]


# --------------------------------------------------------------------------- row 7.10


def test_ours_vs_real_gap_is_tracked(tmp_path: Path) -> None:
    """Row 7.10: the gap is written to the trend file, per run and per stratum."""
    path = tmp_path / "trend.json"

    trend.record(
        report.build(scores()),
        path=path,
        commit="abc1234",
        recorded_at="2026-09-20T12:00:00Z",
    )

    history = json.loads(path.read_text(encoding="utf-8"))

    assert history["schema_version"] == trend.SCHEMA_VERSION
    assert len(history["runs"]) == 1
    run = history["runs"][0]
    assert run["commit"] == "abc1234"
    assert run["gap"] == pytest.approx(0.99 - (0.91 + 0.85) / 2)
    assert run["ours"] == pytest.approx(0.99)
    assert {row["stratum"] for row in run["per_stratum"]} == {
        "ours(Typst)",
        "InDesign",
        "ABBYY-scanner",
    }


def test_the_trend_is_append_only_and_stays_in_time_order(tmp_path: Path) -> None:
    path = tmp_path / "trend.json"

    for day, commit in enumerate(("aaa", "bbb", "ccc"), start=1):
        trend.record(
            report.build(scores()),
            path=path,
            commit=commit,
            recorded_at=f"2026-09-{day:02d}T00:00:00Z",
        )

    runs = json.loads(path.read_text(encoding="utf-8"))["runs"]

    assert [run["commit"] for run in runs] == ["aaa", "bbb", "ccc"]
    assert [run["recorded_at"] for run in runs] == sorted(run["recorded_at"] for run in runs)


def test_recording_the_same_commit_twice_replaces_it_rather_than_doubling_it(
    tmp_path: Path,
) -> None:
    """A re-run of the nightly on an unchanged tree is a correction, not a second data point."""
    path = tmp_path / "trend.json"
    trend.record(
        report.build(scores()), path=path, commit="aaa", recorded_at="2026-09-01T00:00:00Z"
    )
    trend.record(
        report.build([report.Row("ours(Typst)", "f01", "text_score", 0.50)]),
        path=path,
        commit="aaa",
        recorded_at="2026-09-01T06:00:00Z",
    )

    runs = json.loads(path.read_text(encoding="utf-8"))["runs"]

    assert len(runs) == 1
    assert runs[0]["ours"] == pytest.approx(0.50)


def test_a_run_with_no_real_strata_records_no_gap_rather_than_zero(tmp_path: Path) -> None:
    """A gap against nothing is not a gap of nothing. D18: no release passes on ours alone."""
    path = tmp_path / "trend.json"

    trend.record(
        report.build([report.Row("ours(Typst)", "f01", "text_score", 0.99)]),
        path=path,
        commit="aaa",
        recorded_at="2026-09-01T00:00:00Z",
    )

    run = json.loads(path.read_text(encoding="utf-8"))["runs"][0]

    assert run["gap"] is None
    assert run["real"] is None


def test_a_widening_gap_is_what_the_trend_is_asked_about(tmp_path: Path) -> None:
    path = tmp_path / "trend.json"
    for day, real in enumerate((0.95, 0.92, 0.85), start=1):
        trend.record(
            report.build(
                [
                    report.Row("ours(Typst)", "f01", "text_score", 0.99),
                    report.Row("InDesign", "oapen-1", "text_score", real),
                ]
            ),
            path=path,
            commit=f"c{day}",
            recorded_at=f"2026-09-{day:02d}T00:00:00Z",
        )

    widening = trend.widening(path)

    assert widening is not None
    assert widening.first < widening.last
    assert widening.is_widening


def test_a_trend_with_one_run_cannot_say_whether_anything_is_widening(tmp_path: Path) -> None:
    path = tmp_path / "trend.json"
    trend.record(
        report.build(scores()), path=path, commit="aaa", recorded_at="2026-09-01T00:00:00Z"
    )

    assert trend.widening(path) is None


def test_the_plot_is_written_and_is_a_png(tmp_path: Path) -> None:
    """Row 7.10 says plotted, and a trend nobody looks at is a trend nobody reads."""
    path = tmp_path / "trend.json"
    for day in (1, 2, 3):
        trend.record(
            report.build(scores()),
            path=path,
            commit=f"c{day}",
            recorded_at=f"2026-09-{day:02d}T00:00:00Z",
        )

    png = trend.plot(path, tmp_path / "trend.png")

    assert png.exists()
    assert png.read_bytes().startswith(b"\x89PNG\r\n\x1a\n")


def test_the_committed_trend_file_is_the_history_and_parses() -> None:
    """TEST_STRATEGY §8: history is in the repository, so a regression is a diff."""
    history = json.loads(COMMITTED_TREND.read_text(encoding="utf-8"))

    assert history["schema_version"] == trend.SCHEMA_VERSION
    assert isinstance(history["runs"], list)
