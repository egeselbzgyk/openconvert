"""Image preservation: count P/R/F1, and caption association accuracy beside it.

TEST_STRATEGY §8.1's fifth row. R7 §B.5 calls images silently dropped the commonest real bug
class, so the count is its own number rather than something folded into a structure score — and
the caption is a second number, because an image that came back attached to the wrong caption
is a different bug from one that did not come back at all.
"""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass

from oc_eval.metrics.prf import score as prf_score

# (image id, caption or None)
Image = tuple[str, str | None]


@dataclass(frozen=True)
class ImageScore:
    count_f1: float
    count_precision: float
    count_recall: float
    caption_accuracy: float
    captions_checked: int


def score(truth: Sequence[Image], prediction: Sequence[Image]) -> ImageScore:
    counts = prf_score([ident for ident, _ in truth], [ident for ident, _ in prediction])

    predicted_captions = dict(prediction)
    checked = 0
    correct = 0
    for ident, caption in truth:
        if ident not in predicted_captions:
            # An image that was not recovered is counted by `count_recall`. Counting it again
            # here would charge one bug twice and hide how often a *recovered* image is
            # mis-captioned, which is the thing this number exists to watch.
            continue
        checked += 1
        correct += int(predicted_captions[ident] == caption)

    accuracy = correct / checked if checked else 1.0

    return ImageScore(
        count_f1=counts.f1,
        count_precision=counts.precision,
        count_recall=counts.recall,
        caption_accuracy=accuracy,
        captions_checked=checked,
    )
