"""Reading order: edit distance over block ids, with Kendall tau as the second opinion.

TEST_STRATEGY §8.1's second row. Edit distance mirrors what OmniDocBench reports (R9 §A.11),
and tau is there because the two answer different questions: a converter that dropped a block
and one that swapped two blocks both score badly on edit distance, and only the second is an
ordering bug. Tau is computed over the blocks both sides have, so it stays at 1.0 when blocks
are merely missing and falls when the ones that survived came back in the wrong order.
"""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass

from oc_eval.metrics.cer import _levenshtein


@dataclass(frozen=True)
class ReadingOrder:
    edit_similarity: float
    kendall_tau: float
    common: int


def score(truth: Sequence[str], prediction: Sequence[str]) -> ReadingOrder:
    longest = max(len(truth), len(prediction))
    if longest == 0:
        return ReadingOrder(edit_similarity=1.0, kendall_tau=1.0, common=0)

    distance = _sequence_levenshtein(list(truth), list(prediction))
    similarity = 1.0 - distance / longest

    positions = {block: index for index, block in enumerate(truth)}
    shared = [positions[block] for block in prediction if block in positions]

    return ReadingOrder(
        edit_similarity=similarity,
        kendall_tau=_kendall_tau(shared),
        common=len(shared),
    )


def _sequence_levenshtein(left: Sequence[str], right: Sequence[str]) -> int:
    """The same recurrence as `cer._levenshtein`, over tokens rather than characters.

    Block ids are mapped to single characters first when they fit, because the character
    implementation is the one that has been made fast; ids are short and a document rarely has
    more distinct blocks than the Unicode private-use area holds.
    """
    alphabet: dict[str, str] = {}
    for token in list(left) + list(right):
        if token not in alphabet:
            alphabet[token] = chr(0xE000 + len(alphabet))
    if len(alphabet) > 0x1000:
        return _token_levenshtein(left, right)
    return _levenshtein(
        "".join(alphabet[token] for token in left),
        "".join(alphabet[token] for token in right),
    )


def _token_levenshtein(left: Sequence[str], right: Sequence[str]) -> int:
    previous = list(range(len(right) + 1))
    for i, left_token in enumerate(left, start=1):
        current = [i]
        for j, right_token in enumerate(right, start=1):
            current.append(
                min(
                    previous[j] + 1,
                    current[j - 1] + 1,
                    previous[j - 1] + (left_token != right_token),
                )
            )
        previous = current
    return previous[-1]


def _kendall_tau(ranks: Sequence[int]) -> float:
    """Tau-a over the blocks both sides have. 1.0 for a sequence that is already in order."""
    if len(ranks) < 2:
        return 1.0

    concordant = discordant = 0
    for i in range(len(ranks)):
        for j in range(i + 1, len(ranks)):
            if ranks[i] < ranks[j]:
                concordant += 1
            elif ranks[i] > ranks[j]:
                discordant += 1
    pairs = concordant + discordant
    if pairs == 0:
        return 1.0
    return (concordant - discordant) / pairs
