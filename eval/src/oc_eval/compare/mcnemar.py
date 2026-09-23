"""McNemar's test on the paired 2x2 of one task and one category (PHASE 10 detail 7, R9 §C.7).

χ² = (b − c)² / (b + c), with one degree of freedom, when `b + c` is at least `ai_eval.exact_below`;
below it the approximation is poor and the **exact binomial** form is used: two-sided, `min(b, c)`
successes of `b + c` at one half. Both computed with the standard library — `math.comb` and
`math.erfc` — as the Phase 9 gate's statistics are, so there is no untyped dependency for mypy.
"""

from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Literal

from oc_eval import thresholds
from oc_eval.model_gate import mcnemar as g7

Method = Literal["chi2", "exact"]


@dataclass(frozen=True)
class Test:
    method: Method
    #: χ² for the chi-square form; `None` for the exact one, which has no statistic.
    statistic: float | None
    p: float


def exact_below() -> int:
    return int(thresholds.value("ai_eval.exact_below"))


def test(b: int, c: int) -> Test:
    """McNemar on `b` and `c`, in the form their sum calls for."""
    discordant = b + c
    if discordant < exact_below():
        return Test("exact", None, g7.exact_p(g7.Paired(discordant, b, c)))
    chi2 = (b - c) ** 2 / discordant
    # The chi-square survival function at one degree of freedom is erfc(sqrt(x / 2)).
    return Test("chi2", chi2, math.erfc(math.sqrt(chi2 / 2)))


def non_inferior(n: int, b: int, c: int) -> bool:
    """G7's one-sided non-inferiority test, with this phase's level and margin."""
    return g7.non_inferior(
        g7.Paired(n, b, c),
        float(thresholds.value("ai_eval.alpha")),
        float(thresholds.value("ai_eval.noninferiority_margin")),
    )
