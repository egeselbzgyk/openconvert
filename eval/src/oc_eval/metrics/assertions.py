"""The assertion pass rate, its confidence interval, and the gate built on both.

PHASE 7 row 7.8 and A7.4. This is the primary PR-blocking number: the share of the corpus's
assertions — hand-written in `.assert.json`, generated from ground truth, same vocabulary —
that the current build satisfies.

**A pass rate is an interval, not a number.** 94 of 100 and 940 of 1000 are the same rate and
not the same evidence, and a gate that compares two point estimates fails builds for noise and
passes real regressions. The interval is Wilson's rather than the normal approximation, which
is wrong exactly where this gate lives: at p near 1 the normal interval runs past 1.0 and is
far too narrow on the side that matters (R9 §C.5).

The gate is `last green - margin`, where the margin is the interval's half-width. A suite too
small for that margin to mean anything does not quietly pass — it fails and says why, because
"the evidence cannot tell" and "the build is fine" are different answers.
"""

from __future__ import annotations

import math
from dataclasses import dataclass

from oc_eval import thresholds


def confidence() -> float:
    return float(thresholds.value("eval.assertion_confidence"))


def min_instances() -> int:
    return int(thresholds.value("eval.assertion_min_instances"))


@dataclass(frozen=True)
class PassRate:
    passed: int
    total: int

    @property
    def rate(self) -> float:
        """Zero over nothing is zero. A suite with no assertions has passed none of them."""
        return self.passed / self.total if self.total else 0.0

    def interval(self, *, level: float | None = None) -> tuple[float, float]:
        """The Wilson score interval at `level`, clamped to [0, 1]."""
        if self.total == 0:
            return (0.0, 0.0)

        z = _z_for(level if level is not None else confidence())
        n = self.total
        p = self.rate
        denominator = 1 + z * z / n
        centre = (p + z * z / (2 * n)) / denominator
        spread = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / denominator
        return (max(0.0, centre - spread), min(1.0, centre + spread))

    def margin(self, *, level: float | None = None) -> float:
        low, high = self.interval(level=level)
        return (high - low) / 2


@dataclass(frozen=True)
class GateOutcome:
    passed: bool
    detail: str


def gate(current: PassRate, *, last_green: float | None) -> GateOutcome:
    """Row 7.8's check: a drop below `last green - margin` fails."""
    floor = min_instances()
    if current.total < floor:
        return GateOutcome(
            passed=False,
            detail=(
                f"too few assertions to gate on: {current.total} < {floor}. A pass rate over "
                "this little evidence has an interval wide enough to admit any regression"
            ),
        )

    low, high = current.interval()
    margin = (high - low) / 2

    if last_green is None:
        return GateOutcome(
            passed=True,
            detail=(
                f"no baseline yet; recording {current.rate:.4f} "
                f"[{low:.4f}, {high:.4f}] as the first green"
            ),
        )

    threshold = last_green - margin
    if current.rate < threshold:
        return GateOutcome(
            passed=False,
            detail=(
                f"pass rate {current.rate:.4f} is below last green {last_green:.4f} "
                f"minus the {confidence():.0%} margin {margin:.4f} (floor {threshold:.4f})"
            ),
        )
    return GateOutcome(
        passed=True,
        detail=(
            f"pass rate {current.rate:.4f} [{low:.4f}, {high:.4f}] holds against last green "
            f"{last_green:.4f} (floor {threshold:.4f})"
        ),
    )


# The two-sided normal quantiles this gate ever needs. A table rather than a dependency on
# `scipy` at import time: the value for 0.95 is the one the plan names, and a level that is
# not here is a level somebody chose deliberately and should have to add.
_Z = {
    0.80: 1.2815515655446004,
    0.90: 1.6448536269514722,
    0.95: 1.959963984540054,
    0.98: 2.3263478740408408,
    0.99: 2.5758293035489004,
}


class UnknownConfidence(ValueError):
    """A confidence level with no quantile in the table."""


def _z_for(level: float) -> float:
    for known, z in _Z.items():
        if math.isclose(level, known, abs_tol=1e-9):
            return z
    raise UnknownConfidence(f"no quantile for confidence {level!r}; add it to _Z")
