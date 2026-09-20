"""Precision, recall and F1 over sets of items — the shape four of §8.1's metrics share.

Headings, footnote pairs, images and metadata fields are all scored the same way: the truth is
a collection of items, the prediction is another, and what matters is how many of the right
ones came back and how many wrong ones came with them. One implementation, so a change to how
a tie or an empty set is handled reaches all four at once.
"""

from __future__ import annotations

from collections import Counter
from collections.abc import Hashable, Iterable
from dataclasses import dataclass


@dataclass(frozen=True)
class PRF:
    precision: float
    recall: float
    f1: float
    matched: int
    predicted: int
    expected: int


def score(truth: Iterable[Hashable], prediction: Iterable[Hashable]) -> PRF:
    """Multiset precision/recall/F1.

    A multiset rather than a set because a document may legitimately hold the same item twice —
    two figures with the same caption, two headings with the same text — and collapsing them
    would let a converter that emitted one of them score as if it had emitted both.
    """
    expected = Counter(truth)
    predicted = Counter(prediction)
    matched = sum((expected & predicted).values())

    total_expected = sum(expected.values())
    total_predicted = sum(predicted.values())

    if total_expected == 0 and total_predicted == 0:
        # Nothing to find and nothing claimed. Scoring that as zero would punish a document
        # for not having footnotes.
        return PRF(1.0, 1.0, 1.0, 0, 0, 0)

    precision = matched / total_predicted if total_predicted else 0.0
    recall = matched / total_expected if total_expected else 0.0
    f1 = 2 * precision * recall / (precision + recall) if precision + recall else 0.0

    return PRF(precision, recall, f1, matched, total_predicted, total_expected)
