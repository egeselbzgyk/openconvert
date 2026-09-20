"""Footnote linkage: P/R/F1 over (marker, note) pairs.

TEST_STRATEGY §8.1's sixth row, IE-style scoring (R9 §A.13), and derivable exactly from
Standard Ebooks ground truth where the source markup makes the noteref-to-note relationship
explicit.

The pair is the unit. A converter that recovered both notes and attached each to the other's
marker has every note and no correct linkage, and a metric that scored the notes would call
that a perfect result.
"""

from __future__ import annotations

from collections.abc import Sequence

from oc_eval.metrics.prf import PRF
from oc_eval.metrics.prf import score as prf_score

# (marker id, note id) as the source states the pairing.
Link = tuple[str, str]


def score(truth: Sequence[Link], prediction: Sequence[Link]) -> PRF:
    return prf_score(truth, prediction)
