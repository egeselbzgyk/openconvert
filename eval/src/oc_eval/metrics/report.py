"""The evaluation report: per file, per stratum, and never only a total.

PHASE 7 row 7.9, D18, TEST_STRATEGY §8. There is no headline quality number in v1, and that is
a decision rather than an omission: an average across strata is exactly the number that hides
the thing D18 makes first-class, because a synthetic-corpus win masks a real-book regression
and the two cancel. So this module has no `aggregate` key, and a test asserts it never grows
one.

What it does report is the `ours(*)`-versus-real gap, which is the same comparison stated so it
cannot be averaged away. A widening gap is the earliest available signal that a heuristic has
been fitted to our own renderer rather than to books (RT A9).
"""

from __future__ import annotations

import statistics
from collections.abc import Iterable, Sequence
from dataclasses import dataclass
from typing import Any

# A stratum whose name starts with this was rendered by us (D18).
OURS_PREFIX = "ours("


@dataclass(frozen=True)
class Row:
    """One structured record per (corpus file, metric) — TEST_STRATEGY §8's contract."""

    stratum: str
    file_id: str
    metric: str
    value: float


def build(rows: Iterable[Row]) -> dict[str, Any]:
    """The report, in a stable order so a one-number change is a one-line diff."""
    ordered = sorted(rows, key=lambda row: (row.stratum, row.file_id, row.metric))

    return {
        "schema_version": 1,
        "per_file": [
            {
                "stratum": row.stratum,
                "file_id": row.file_id,
                "metric": row.metric,
                "value": row.value,
            }
            for row in ordered
        ],
        "per_stratum": _per_stratum(ordered),
        "ours_vs_real": _gap(ordered),
        "ocr_cer": _ocr_cer(ordered),
    }


def _per_stratum(rows: Sequence[Row]) -> list[dict[str, Any]]:
    buckets: dict[tuple[str, str], list[float]] = {}
    for row in rows:
        buckets.setdefault((row.stratum, row.metric), []).append(row.value)

    return [
        {
            "stratum": stratum,
            "metric": metric,
            "n": len(values),
            "mean": statistics.fmean(values),
            "median": statistics.median(values),
            "min": min(values),
            "max": max(values),
        }
        for (stratum, metric), values in sorted(buckets.items())
    ]


def _gap(rows: Sequence[Row]) -> dict[str, Any]:
    ours = [row.value for row in rows if row.stratum.startswith(OURS_PREFIX)]
    real = [row.value for row in rows if not row.stratum.startswith(OURS_PREFIX)]

    ours_mean = statistics.fmean(ours) if ours else None
    real_mean = statistics.fmean(real) if real else None
    gap = ours_mean - real_mean if ours_mean is not None and real_mean is not None else None

    return {
        "ours": ours_mean,
        "real": real_mean,
        "gap": gap,
        "ours_n": len(ours),
        "real_n": len(real),
        "note": (
            "D18: a release may not pass on ours(*) alone, and a widening gap is the earliest "
            "signal that a heuristic has been fitted to our renderer rather than to books"
        ),
    }


# The metric the OCR rows carry (PHASE 13 row 13.21).
OCR_CER = "ocr_cer"


def _ocr_cer(rows: Sequence[Row]) -> dict[str, Any]:
    """CER per stratum, and the real-minus-synthetic gap, stated on its own.

    Separate from `ours_vs_real`, which averages every metric together: a CER is an error rate
    where lower is better, and averaging it with scores where higher is better would produce a
    number that means nothing. A synthetic scan is far easier than a 1910 German printing, and
    the gap between them is the number D18 makes first-class — so it is printed even when the
    synthetic stratum passes its gate, and refused (`None`) when either side has no documents.
    """
    per_stratum: dict[str, list[float]] = {}
    for row in rows:
        if row.metric == OCR_CER:
            per_stratum.setdefault(row.stratum, []).append(row.value)
    synthetic = [
        value
        for stratum, values in per_stratum.items()
        if stratum.startswith(OURS_PREFIX)
        for value in values
    ]
    real = [
        value
        for stratum, values in per_stratum.items()
        if not stratum.startswith(OURS_PREFIX)
        for value in values
    ]
    synthetic_mean = statistics.fmean(synthetic) if synthetic else None
    real_mean = statistics.fmean(real) if real else None
    return {
        "per_stratum": {
            stratum: {"n": len(values), "mean": statistics.fmean(values)}
            for stratum, values in sorted(per_stratum.items())
        },
        "synthetic": synthetic_mean,
        "real": real_mean,
        "gap": (
            real_mean - synthetic_mean
            if real_mean is not None and synthetic_mean is not None
            else None
        ),
    }
