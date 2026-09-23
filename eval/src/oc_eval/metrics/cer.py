"""Text recovery: one minus normalised edit distance, measured after normalisation.

TEST_STRATEGY §8.1's first row, and R9 §A.10's gap in Nougat's metric. The pipeline is
*supposed* to fold a ligature, join a word broken across a line and compose a combining accent
— D13.4's `N` — so a metric that counts those as errors scores a correct conversion as wrong.
The normalisation here is the metric layer's, deliberately the same shape as the engine's:
whatever both sides agree to call the same character is the same character.

What it does **not** fold is anything the pipeline has no licence to change. A hyphen inside a
line is content; only one at a line break is a line break.
"""

from __future__ import annotations

import re
import unicodedata

# Ligatures the engine expands (D13.4). Written out because `NFKC` would also fold things the
# engine must not — superscript digits into ordinary ones, which loses a footnote marker.
LIGATURES = {
    "ﬀ": "ff",
    "ﬁ": "fi",
    "ﬂ": "fl",
    "ﬃ": "ffi",
    "ﬄ": "ffl",
    "ﬅ": "st",
    "ﬆ": "st",
}

# A hyphen (or soft hyphen) immediately before a line break: the break is the hyphen's reason
# for being there, so both go.
LINE_BREAK_HYPHEN_RE = re.compile(r"[­‐-]\s*\n\s*")

# Every space-like character the engine treats as a space.
SPACE_RE = re.compile(r"[\s  -   　]+")


def normalise(text: str) -> str:
    """The form both sides are compared in."""
    folded = unicodedata.normalize("NFC", text)
    for ligature, expansion in LIGATURES.items():
        folded = folded.replace(ligature, expansion)
    folded = LINE_BREAK_HYPHEN_RE.sub("", folded)
    return SPACE_RE.sub(" ", folded).strip()


def ned(truth: str, prediction: str) -> float:
    """Normalised edit distance in [0, 1]: Levenshtein over max(len), after `normalise`."""
    left = normalise(truth)
    right = normalise(prediction)
    longest = max(len(left), len(right))
    if longest == 0:
        return 0.0
    return _levenshtein(left, right) / longest


def cer(truth: str, prediction: str) -> float:
    """Character error rate: Levenshtein distance over the ground truth's length, after
    `normalise`. Unlike `ned` it is not capped at 1 — an OCR run that invents a page of noise
    scores worse than one that reads nothing, which is the order a reader would put them in.

    The CER PHASE 13 row 13.21 reports per stratum (`ocr.max_cer_synthetic` gates the synthetic
    one). A truth with no characters has no rate: `0.0` if nothing was read, `1.0` otherwise.
    """
    left = normalise(truth)
    right = normalise(prediction)
    if not left:
        return 0.0 if not right else 1.0
    return _levenshtein(left, right) / len(left)


def text_score(truth: str, prediction: str) -> float:
    """`1 - ned`, which is the number TEST_STRATEGY §8.1 reports."""
    return 1.0 - ned(truth, prediction)


def _levenshtein(left: str, right: str) -> int:
    """Two rows rather than a full matrix: a book's chapter is long enough for that to matter."""
    if left == right:
        return 0
    if not left:
        return len(right)
    if not right:
        return len(left)

    previous = list(range(len(right) + 1))
    for i, left_char in enumerate(left, start=1):
        current = [i]
        for j, right_char in enumerate(right, start=1):
            current.append(
                min(
                    previous[j] + 1,
                    current[j - 1] + 1,
                    previous[j - 1] + (left_char != right_char),
                )
            )
        previous = current
    return previous[-1]
