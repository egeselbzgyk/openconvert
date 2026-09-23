"""The four gold sets: one labelled answer per assertion, per task (PHASE 10, `eval/data/gold/`).

One JSON object per line: `id`, `task`, `source` (where the label was read), `stratum` (D18),
`language`, `category` (the assertion category the tables group by), `subject` (what the label is
about: a metadata field, a heading's text, a block's opening words) and `label`.

The committed sets are **seeds**, read off the Typst fixtures' own sources — `ours(typst)`, the
stratum D18 says a release cannot pass on alone and calibration may never fit on. They fix the
format and give the harness something real to load; the evaluation needs
`calibration.min_gold_instances_per_task` items per task from the real strata, and the report says
how far each set is from it.
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path

from oc_eval import thresholds
from oc_eval.compare.false_repair import TASKS

REPO_ROOT = Path(__file__).resolve().parents[4]
GOLD_DIR = REPO_ROOT / "eval" / "data" / "gold"
FIELDS = ("id", "task", "source", "stratum", "language", "category", "subject", "label")

#: The labels each task may carry: the prompts' own closed sets (Appendix A).
LABELS: dict[str, frozenset[str] | None] = {
    "metadata": None,  # a string copied from the page
    "heading_roles": frozenset(
        {
            "chapter_heading",
            "part_heading",
            "section_heading",
            "subsection_heading",
            "running_head",
            "epigraph",
            "body",
            "caption",
            "other",
        }
    ),
    "book_structure": frozenset({"front", "body", "back", "part"}),
    "verse_quote": frozenset({"verse", "blockquote", "preformatted", "paragraph"}),
}


class GoldError(ValueError):
    """A gold line that does not follow the format."""


@dataclass(frozen=True)
class Item:
    id: str
    task: str
    source: str
    stratum: str
    language: str
    category: str
    subject: str
    label: str


def load(task: str, directory: Path = GOLD_DIR) -> list[Item]:
    path = directory / f"{task}.jsonl"
    items = []
    seen: set[str] = set()
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if not line.strip():
            continue
        record = json.loads(line)
        if sorted(record) != sorted(FIELDS):
            raise GoldError(f"{path.name}:{number}: fields {sorted(record)}")
        item = Item(**{field: str(record[field]) for field in FIELDS})
        if item.task != task:
            raise GoldError(f"{path.name}:{number}: task {item.task!r} in the {task} set")
        allowed = LABELS[task]
        if allowed is not None and item.label not in allowed:
            raise GoldError(f"{path.name}:{number}: label {item.label!r}")
        if item.id in seen:
            raise GoldError(f"{path.name}:{number}: id {item.id!r} twice")
        seen.add(item.id)
        items.append(item)
    return items


def load_all(directory: Path = GOLD_DIR) -> dict[str, list[Item]]:
    return {task: load(task, directory) for task in TASKS}


def minimum() -> int:
    return int(thresholds.value("calibration.min_gold_instances_per_task"))
