"""Gold item by gold item, did each path get it right? — the outcomes the tables are built from.

The evaluation run converts every corpus book twice, `--no-ai` and `--ai --ai-all-tasks`, reads
each path's answer for every gold item (a title, a heading's role, a heading's zone, a block's
kind), and scores both against the gold label. What reads an answer out of a converted book is the
corpus run's (PHASE 7's runner, extended); this is the scoring, and it is the same for every task.
"""

from __future__ import annotations

from collections.abc import Iterable, Mapping

from oc_eval.compare.false_repair import Outcome
from oc_eval.compare.gold import Item


def _same(answer: str | None, label: str) -> bool:
    """Case- and whitespace-insensitive, as the metadata task's own verbatim check is."""
    if answer is None:
        return False
    return " ".join(answer.split()).casefold() == " ".join(label.split()).casefold()


def outcomes(
    items: Iterable[Item],
    deterministic: Mapping[str, str | None],
    llm: Mapping[str, str | None],
) -> list[Outcome]:
    """One outcome per gold item. An item a path gave no answer for is a failure on that path —
    never skipped, because skipping it would drop exactly the items a path cannot handle."""
    return [
        Outcome(
            task=item.task,
            language=item.language,
            category=item.category,
            item=item.id,
            deterministic_pass=_same(deterministic.get(item.id), item.label),
            llm_pass=_same(llm.get(item.id), item.label),
        )
        for item in items
    ]
