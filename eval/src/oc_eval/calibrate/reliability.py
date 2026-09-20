"""Reliability: does 90 % confidence mean right nine times in ten?

TEST_STRATEGY §7 and R9 §C.9. A confidence that is not calibrated is worse than no confidence,
because every downstream decision — escalate or not, warn the user or not — is taken as though
the number meant something.

Expected calibration error is the bin-weighted gap between confidence and accuracy. Zero is
calibrated; a model that says 0.99 and is right half the time scores 0.49.
"""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass

# Ten bins is the convention in the literature and the one R9 §C.9's diagrams use.
DEFAULT_BINS = 10


@dataclass(frozen=True)
class Point:
    confidence: float
    correct: bool


@dataclass(frozen=True)
class Bin:
    lower: float
    upper: float
    count: int
    mean_confidence: float
    accuracy: float

    @property
    def gap(self) -> float:
        return abs(self.mean_confidence - self.accuracy)


def diagram(points: Sequence[Point], *, bins: int = DEFAULT_BINS) -> list[Bin]:
    """One row per occupied bin. An empty bin is not a data point and is not drawn."""
    if not points or bins < 1:
        return []

    buckets: dict[int, list[Point]] = {}
    for point in points:
        index = min(int(point.confidence * bins), bins - 1)
        buckets.setdefault(index, []).append(point)

    return [
        Bin(
            lower=index / bins,
            upper=(index + 1) / bins,
            count=len(members),
            mean_confidence=sum(member.confidence for member in members) / len(members),
            accuracy=sum(int(member.correct) for member in members) / len(members),
        )
        for index, members in sorted(buckets.items())
    ]


def expected_calibration_error(points: Sequence[Point], *, bins: int = DEFAULT_BINS) -> float:
    """The bin-weighted mean gap between stated confidence and observed accuracy."""
    rows = diagram(points, bins=bins)
    if not rows:
        return 0.0
    total = sum(row.count for row in rows)
    return sum(row.count * row.gap for row in rows) / total
