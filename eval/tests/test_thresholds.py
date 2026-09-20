"""The eval tooling reads its numbers from `thresholds.toml`, the same file the engine does.

A corpus gate written as `0.40` in Python and as `corpus.ours_max_share` in Rust is two
numbers that agree today and nothing that says so the day they stop.
"""

from __future__ import annotations

import pytest

from oc_eval import thresholds


def test_the_corpus_gates_come_from_the_shared_file() -> None:
    assert thresholds.value("corpus.ours_max_share") == pytest.approx(0.40)
    assert thresholds.value("corpus.holdout_min_files") == 100
    assert thresholds.value("corpus.tagged_share_target") == pytest.approx(0.126)


def test_a_threshold_carries_its_provenance() -> None:
    """PROVENANCE is not decoration: 'provisional' is only actionable with an owner beside it."""
    record = thresholds.entry("corpus.ours_max_share")

    assert set(record) >= {"value", "source", "evidence", "owner", "review_by"}


def test_an_unknown_key_raises_rather_than_returning_a_default() -> None:
    with pytest.raises(thresholds.UnknownThreshold):
        thresholds.value("corpus.no_such_threshold")


def test_a_key_that_names_a_table_rather_than_a_value_raises() -> None:
    with pytest.raises(thresholds.UnknownThreshold):
        thresholds.value("corpus")
