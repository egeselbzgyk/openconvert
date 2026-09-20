"""The risk-coverage curve, which is where an escalation threshold comes from.

D17 and TEST_STRATEGY §7. A pipeline that knows when it is unsure can decline the cases it is
worst at, and the question is how many it must decline to reach a risk it can live with. The
curve answers it: sort by confidence, take the most confident k, and plot the error rate among
them against the share of cases covered.

A threshold is a point on this curve, chosen for a target risk — never a round number somebody
liked the look of.
"""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass


@dataclass(frozen=True)
class Point:
    confidence: float
    correct: bool


@dataclass(frozen=True)
class Operating:
    """What accepting everything at or above `confidence` costs and buys."""

    confidence: float
    coverage: float
    risk: float
    accepted: int


def curve(points: Sequence[Point]) -> list[Operating]:
    """Every operating point, from the most confident case to all of them."""
    if not points:
        return []

    ordered = sorted(points, key=lambda point: point.confidence, reverse=True)
    total = len(ordered)
    wrong = 0
    out: list[Operating] = []

    for index, point in enumerate(ordered, start=1):
        wrong += int(not point.correct)
        out.append(
            Operating(
                confidence=point.confidence,
                coverage=index / total,
                risk=wrong / index,
                accepted=index,
            )
        )
    return out


def threshold_for(points: Sequence[Point], *, target_risk: float) -> Operating | None:
    """The widest coverage whose risk is still at or below `target_risk`.

    None when nothing meets it. Returning the best available instead would hand back a
    threshold that does not do what was asked, labelled as though it did.
    """
    meeting = [point for point in curve(points) if point.risk <= target_risk]
    if not meeting:
        return None
    return max(meeting, key=lambda point: point.coverage)
