"""Run a conversion under the memory wrapper and judge it against D13.11's budget.

PHASE 7 rows 7.11 and 7.12, TEST_STRATEGY §8.3. Two numbers, one run: seconds per page from
the wall clock, and peak RSS of the whole process tree from `peak_rss`.

Both budgets are `provisional` in `thresholds.toml`, and the report says so. A number the
project has not measured is a number a reader should be told not to trust yet, and a gate that
presents a guess as a measurement is how a guess becomes a fact nobody remembers choosing.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from oc_eval import thresholds
from oc_eval.bench import peak_rss


def seconds_per_page_max() -> float:
    return float(thresholds.value("perf.seconds_per_page_max"))


def peak_rss_bytes_max() -> int:
    return int(thresholds.value("perf.peak_rss_bytes_max"))


# Row 7.12's five stages. Named rather than discovered, so a stage silently dropped from
# `thresholds.toml` is a KeyError here instead of a budget that quietly stopped being checked.
BUDGETED_STAGES = ("ingest", "text", "layout", "structure", "epub")


def stage_budgets() -> dict[str, float]:
    """Row 7.12's split, read from the same file the Rust gate reads."""
    return {
        stage: float(thresholds.value(f"perf.stage_seconds_per_page.{stage}"))
        for stage in BUDGETED_STAGES
    }


@dataclass(frozen=True)
class Verdict:
    name: str
    measured: float
    budget: float
    passed: bool
    exact: bool

    def __str__(self) -> str:
        mark = "PASS" if self.passed else "FAIL"
        caveat = "" if self.exact else "  (floor: sampled, not measured)"
        return f"{mark} {self.name}: {self.measured:.4f} against {self.budget:.4f}{caveat}"


@dataclass(frozen=True)
class BenchResult:
    pages: int
    measurement: peak_rss.Measurement
    verdicts: tuple[Verdict, ...]

    @property
    def passed(self) -> bool:
        return all(verdict.passed for verdict in self.verdicts)

    @property
    def seconds_per_page(self) -> float:
        return self.measurement.wall_seconds / self.pages if self.pages else 0.0


def run(argv: list[str], *, pages: int, cwd: Path | None = None) -> BenchResult:
    """Run `argv`, which must convert a `pages`-page book, and judge both budgets."""
    if pages <= 0:
        raise ValueError("a per-page budget needs a page count")

    measurement = peak_rss.run(argv, cwd=cwd)
    per_page = measurement.wall_seconds / pages

    verdicts = (
        Verdict(
            name="seconds per page",
            measured=per_page,
            budget=seconds_per_page_max(),
            passed=per_page <= seconds_per_page_max(),
            exact=True,
        ),
        Verdict(
            name="peak RSS (MB, whole tree)",
            measured=measurement.peak_rss_mb,
            budget=peak_rss_bytes_max() / (1024 * 1024),
            # A sampled floor that is already over budget is over budget; one that is under
            # has only failed to catch a spike, so the verdict is a pass with its caveat
            # printed rather than a silent one.
            passed=measurement.peak_rss_bytes <= peak_rss_bytes_max(),
            exact=measurement.exact,
        ),
    )

    return BenchResult(pages=pages, measurement=measurement, verdicts=verdicts)
