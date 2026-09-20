"""Tables: TEDS and TEDS-S.

TEST_STRATEGY §8.1's seventh row. TEDS is PubTabNet's formula (R9 §A.14),
`1 - tree_edit_distance / max(nodes)`, over the table read as a tree of rows and cells. TEDS-S
holds the cell text constant so only the grid is compared, and it gates first: a wrong grid is
the worse failure, because a table whose cells are right in the wrong shape has lost the
relationships the table existed to express, while one with a typo in a cell has lost a typo.

The tree edit is computed on the flattened `(row, column, text)` form rather than with a
general tree-edit algorithm. A table is a grid, its tree is two levels deep and perfectly
regular, and on that shape the two agree — at a fraction of the cost.
"""

from __future__ import annotations

from collections.abc import Sequence

Table = Sequence[Sequence[str]]


def teds(truth: Table, prediction: Table) -> float:
    """Structure and cell text together."""
    return _score(_nodes(truth, with_text=True), _nodes(prediction, with_text=True))


def teds_s(truth: Table, prediction: Table) -> float:
    """Structure only — cell substitution cost held constant (the TEDS-S variant)."""
    return _score(_nodes(truth, with_text=False), _nodes(prediction, with_text=False))


def _nodes(table: Table, *, with_text: bool) -> list[tuple[int, int, str]]:
    return [
        (row_index, column_index, cell if with_text else "")
        for row_index, row in enumerate(table)
        for column_index, cell in enumerate(row)
    ]


def _score(truth: list[tuple[int, int, str]], prediction: list[tuple[int, int, str]]) -> float:
    largest = max(len(truth), len(prediction))
    if largest == 0:
        return 1.0
    return 1.0 - _distance(truth, prediction) / largest


def _distance(truth: list[tuple[int, int, str]], prediction: list[tuple[int, int, str]]) -> int:
    previous = list(range(len(prediction) + 1))
    for i, truth_cell in enumerate(truth, start=1):
        current = [i]
        for j, predicted_cell in enumerate(prediction, start=1):
            current.append(
                min(
                    previous[j] + 1,
                    current[j - 1] + 1,
                    previous[j - 1] + (truth_cell != predicted_cell),
                )
            )
        previous = current
    return previous[-1]
