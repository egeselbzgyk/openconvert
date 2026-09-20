"""PHASE 7 rows 7.11 and 7.12, from the side that can see a whole process tree.

D13.11's 500 MB is for the converter *and whatever it spawns* — a `llama-server` from Phase 9,
a `tesseract` from Phase 13 — because a user whose machine ran out of memory does not care
which of our processes asked for it. Measuring only the parent gives a figure that quietly
stops being true the moment a sidecar exists, which is the failure TEST_STRATEGY §8.3 and
R9 §D.5 both call out.

The wrapper is honest about which of its three mechanisms it used: `/proc`'s `VmHWM` and a
Windows Job Object are exact, and polling is a floor. A report that presents a floor as a
measurement is how a budget stops being enforced without anybody deciding to stop enforcing it.
"""

from __future__ import annotations

import sys

import pytest

from oc_eval.bench import peak_rss, runner

# A child that allocates about this much and then exits, so the wrapper has something to see.
ALLOCATE_MB = 64

# How long the child lives. Comfortably over `perf.seconds_per_page_max` so that a one-page
# run is decisively over budget rather than near it — a timing test that lands within a
# tenth of the threshold is a test that fails on a busy machine.
SLEEP_SECONDS = 1.5


def allocating_argv(megabytes: int = ALLOCATE_MB) -> list[str]:
    program = (
        "import time;"
        f"blob = bytearray({megabytes} * 1024 * 1024);"
        # Touch every page: a bytearray is allocated lazily on some platforms, and memory
        # that was never resident is not resident memory.
        "blob[::4096] = b'x' * len(blob[::4096]);"
        f"time.sleep({SLEEP_SECONDS})"
    )
    return [sys.executable, "-c", program]


# --------------------------------------------------------------------------- the wrapper


def test_the_wrapper_reports_the_memory_a_child_actually_used() -> None:
    measured = peak_rss.run(allocating_argv())

    assert measured.exit_code == 0
    assert measured.wall_seconds > 0.0
    assert measured.peak_rss_bytes > ALLOCATE_MB / 2 * 1024 * 1024, (
        f"{measured.peak_rss_mb:.1f} MB seen for a child that allocated {ALLOCATE_MB} MB "
        f"via {measured.method}"
    )


def test_the_wrapper_says_which_mechanism_it_used_and_whether_it_is_exact() -> None:
    """A sampled floor and a kernel high-water mark are different claims about the same run."""
    measured = peak_rss.run([sys.executable, "-c", "pass"])

    assert measured.method
    assert isinstance(measured.exact, bool)


def test_a_failing_child_is_reported_rather_than_swallowed() -> None:
    measured = peak_rss.run([sys.executable, "-c", "raise SystemExit(3)"])

    assert measured.exit_code == 3


# --------------------------------------------------------------------------- the budgets


def test_the_budgets_come_from_the_same_file_the_engine_reads() -> None:
    assert runner.seconds_per_page_max() == pytest.approx(0.5)
    assert runner.peak_rss_bytes_max() == 500 * 1024 * 1024


def test_the_stage_budgets_sum_to_the_end_to_end_budget() -> None:
    """Row 7.12 says the split sums to the whole. The Rust gate asserts it too; this is the
    same arithmetic read from Python, so neither side can drift alone."""
    budgets = runner.stage_budgets()

    assert set(budgets) == {"ingest", "text", "layout", "structure", "epub"}
    assert sum(budgets.values()) == pytest.approx(runner.seconds_per_page_max())


def test_a_run_inside_both_budgets_passes_and_says_so() -> None:
    result = runner.run(allocating_argv(megabytes=8), pages=1000)

    assert result.passed
    assert len(result.verdicts) == 2
    assert all("PASS" in str(verdict) for verdict in result.verdicts)


def test_a_run_that_is_too_slow_per_page_fails() -> None:
    """One page and a child that sleeps is over 0.5 s/page by a wide margin."""
    result = runner.run(allocating_argv(megabytes=1), pages=1)

    slow = next(v for v in result.verdicts if v.name == "seconds per page")

    assert not slow.passed
    assert not result.passed


def test_a_sampled_measurement_carries_its_caveat_into_the_verdict() -> None:
    measured = peak_rss.Measurement(
        peak_rss_bytes=1024,
        exit_code=0,
        wall_seconds=0.001,
        method="psutil-poll",
        exact=False,
    )
    verdict = runner.Verdict(
        name="peak RSS (MB, whole tree)",
        measured=measured.peak_rss_mb,
        budget=500.0,
        passed=True,
        exact=measured.exact,
    )

    assert "floor" in str(verdict)


def test_a_per_page_budget_needs_a_page_count() -> None:
    with pytest.raises(ValueError, match="page count"):
        runner.run([sys.executable, "-c", "pass"], pages=0)
