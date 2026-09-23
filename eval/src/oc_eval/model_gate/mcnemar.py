"""G7's statistics: paired, per task, candidate against incumbent (D9 G7, R9 §C.7).

Each item is answered by both models and scored right or wrong against gold, so the evidence is
the discordant pairs: `b` items only the candidate got right, `c` items only the incumbent did.

- **Superiority** is McNemar's exact test: two-sided binomial on `b` of `b + c` at one half,
  significant at `alpha` and in the candidate's favour.
- **Non-inferiority** is one-sided: the paired difference in accuracy `(b - c) / n` must exceed
  `-margin` with confidence `1 - alpha`, using the variance of a paired difference of proportions.
"""

from __future__ import annotations

import math
from dataclasses import dataclass
from statistics import NormalDist


@dataclass(frozen=True)
class Paired:
    n: int
    b: int  # candidate right, incumbent wrong
    c: int  # candidate wrong, incumbent right


def tally(pairs: list[tuple[bool, bool]]) -> Paired:
    """`(candidate_correct, incumbent_correct)` per item."""
    b = sum(1 for cand, inc in pairs if cand and not inc)
    c = sum(1 for cand, inc in pairs if inc and not cand)
    return Paired(len(pairs), b, c)


def exact_p(p: Paired) -> float:
    """McNemar's exact two-sided p-value: the binomial at one half is symmetric, so it is twice
    the smaller tail, capped at one. Exact integer arithmetic until the final division."""
    n, k = p.b + p.c, min(p.b, p.c)
    if n == 0:
        return 1.0
    tail = sum(math.comb(n, i) for i in range(k + 1))
    return min(1.0, 2 * tail / 2**n)


def superior(p: Paired, alpha: float) -> bool:
    if p.b + p.c == 0 or p.b <= p.c:
        return False
    return exact_p(p) < alpha


def non_inferior(p: Paired, alpha: float, margin: float) -> bool:
    if p.n == 0:
        return False
    d = (p.b - p.c) / p.n
    variance = ((p.b + p.c) / p.n - d * d) / p.n
    if variance <= 0:
        # No discordant pairs: the two models agree on every item, and the difference is exactly 0.
        return d > -margin
    z = (d + margin) / math.sqrt(variance)
    return z > NormalDist().inv_cdf(1 - alpha)
