"""Heading structure: P/R/F1 on (level, text) pairs, plus a tree-edit distance over the outline.

TEST_STRATEGY §8.1's third row, and self-defined: R9 §A.12 found no published baseline for this
exact task, so it is tracked as a trend from day one rather than compared to a literature
number. The pair is scored, not the text: the right words at the wrong level are not the right
heading, because a converted book's navigation is the levels.
"""

from __future__ import annotations

from collections.abc import Sequence

from oc_eval.metrics.prf import PRF
from oc_eval.metrics.prf import score as prf_score

# A heading, as both sides state it.
Heading = tuple[int, str]


def score(truth: Sequence[Heading], prediction: Sequence[Heading]) -> PRF:
    return prf_score(truth, prediction)


def tree_distance(truth: Sequence[Heading], prediction: Sequence[Heading]) -> int:
    """How many headings have to be inserted, deleted or relabelled to turn one outline
    into the other.

    A flat edit distance over the `(level, text)` sequence rather than a general tree edit:
    an outline *is* its sequence of levelled headings, the nesting is recoverable from the
    levels, and a general tree edit distance would cost far more to compute and agree with this
    on every outline a book actually has.
    """
    left, right = list(truth), list(prediction)
    previous = list(range(len(right) + 1))
    for i, truth_heading in enumerate(left, start=1):
        current = [i]
        for j, predicted_heading in enumerate(right, start=1):
            current.append(
                min(
                    previous[j] + 1,
                    current[j - 1] + 1,
                    previous[j - 1] + (truth_heading != predicted_heading),
                )
            )
        previous = current
    return previous[-1]
