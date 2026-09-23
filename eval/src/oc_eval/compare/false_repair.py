"""The paired outcomes, their table, the false-repair gate, and the language map it proposes.

An **outcome** is one assertion scored twice: did the deterministic path pass it, did the
deterministic+LLM path pass it. Grouped by task, language and category, the four counts of the
2x2 are the table's row; `c / n` is the false-repair rate (PHASE 10 detail 7) — reported per
category, never only in aggregate, because a net-positive McNemar can hide a category where the
LLM silently corrupts output that was right.
"""

from __future__ import annotations

import json
from collections.abc import Iterable, Mapping, Sequence
from dataclasses import asdict, dataclass
from pathlib import Path

from oc_eval import thresholds
from oc_eval.compare import mcnemar

TASKS = ("metadata", "heading_roles", "book_structure", "verse_quote")


@dataclass(frozen=True)
class Outcome:
    task: str
    language: str
    category: str
    #: Which book and which assertion: for tracing a row back, never aggregated on.
    item: str
    deterministic_pass: bool
    llm_pass: bool


@dataclass(frozen=True)
class Row:
    task: str
    language: str
    category: str
    n: int
    #: The LLM path passed and the deterministic one did not.
    b: int
    #: The deterministic path passed and the LLM one did not: a false repair.
    c: int
    test: mcnemar.Test

    @property
    def false_repair_rate(self) -> float:
        return self.c / self.n if self.n else 0.0


def load(path: Path) -> list[Outcome]:
    text = path.read_text(encoding="utf-8") if path.exists() else ""
    return [Outcome(**json.loads(line)) for line in text.splitlines() if line.strip()]


def dump(outcomes: Iterable[Outcome]) -> str:
    return "".join(json.dumps(asdict(outcome), sort_keys=True) + "\n" for outcome in outcomes)


def table(outcomes: Iterable[Outcome]) -> list[Row]:
    """One row per (task, language, category), sorted, each with its own McNemar."""
    groups: dict[tuple[str, str, str], list[Outcome]] = {}
    for outcome in outcomes:
        groups.setdefault((outcome.task, outcome.language, outcome.category), []).append(outcome)
    rows = []
    for (task, language, category), group in sorted(groups.items()):
        b = sum(1 for o in group if o.llm_pass and not o.deterministic_pass)
        c = sum(1 for o in group if o.deterministic_pass and not o.llm_pass)
        rows.append(Row(task, language, category, len(group), b, c, mcnemar.test(b, c)))
    return rows


def false_repair_max() -> float:
    return float(thresholds.value("ai_eval.false_repair_max"))


def passes(rows: Sequence[Row], task: str, language: str) -> bool:
    """Detail 7's rule for one task in one language: evidence exists, the task is non-inferior
    over all its categories together, and no category's false-repair rate exceeds the bound."""
    mine = [row for row in rows if row.task == task and row.language == language]
    if not mine:
        return False
    n = sum(row.n for row in mine)
    b = sum(row.b for row in mine)
    c = sum(row.c for row in mine)
    return mcnemar.non_inferior(n, b, c) and all(
        row.false_repair_rate <= false_repair_max() for row in mine
    )


def proposed_languages(rows: Sequence[Row], task: str) -> list[str]:
    """The languages the evidence enables `task` for: what `[ai.task.<task>.languages]` should
    say. Proposed, never written: the map is a reviewed threshold like any other (D17)."""
    return sorted(
        {row.language for row in rows if row.task == task and passes(rows, task, row.language)}
    )


def enabled_languages(task: str) -> list[str]:
    """What `thresholds.toml` enables `task` for today."""
    return [str(language) for language in thresholds.value(f"ai.task.{task}.languages")]


@dataclass(frozen=True)
class GateResult:
    passed: bool
    failures: list[str]


def gate(rows: Sequence[Row], enabled: Mapping[str, Sequence[str]]) -> GateResult:
    """Row 10.18: every **enabled** task's per-category false-repair rate is within the bound,
    in every language it is enabled for — and an enabled task with no evidence is a failure,
    never a pass: a gate that did not run has not passed."""
    failures = []
    for task, languages in sorted(enabled.items()):
        for language in languages:
            mine = [row for row in rows if row.task == task and row.language == language]
            if not mine:
                failures.append(f"{task}/{language}: enabled with no evaluation")
                continue
            for row in mine:
                if row.false_repair_rate > false_repair_max():
                    failures.append(
                        f"{task}/{language}/{row.category}: false-repair rate "
                        f"{row.false_repair_rate:.4f} > {false_repair_max()}"
                    )
    return GateResult(not failures, failures)
