"""PHASE 10 rows 10.17 and 10.18: the McNemar tables and the false-repair gate.

No model runs here — none was reachable where Phase 10 was built — so the committed outcomes file
is empty and `docs/AI_EVALUATION.md` says so. What is tested is the harness: that the tables are
per task, per language, per category with `b`, `c`, the test's form and the false-repair rate;
that the committed document is what the committed data renders to; and that the gate fails an
enabled task over the bound, or with no evidence at all, and passes only on evidence.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

import pytest

from oc_eval.compare import false_repair, gold, mcnemar, render, score
from oc_eval.compare.false_repair import Outcome

REPO_ROOT = Path(__file__).resolve().parents[2]


def outcomes_for(task: str, language: str, category: str, n: int, b: int, c: int) -> list[Outcome]:
    """`n` assertions: `b` only the LLM passed, `c` only the deterministic path, the rest both."""
    out = []
    for index in range(n):
        if index < b:
            pair = (False, True)
        elif index < b + c:
            pair = (True, False)
        else:
            pair = (True, True)
        out.append(Outcome(task, language, category, f"{category}#{index}", *pair))
    return out


def test_mcnemar_and_false_repair_reported_per_category() -> None:
    """Row 10.17. Per task, per language, per category: n, b, c, the test's form — χ² at
    `b + c >= ai_eval.exact_below`, exact below — its p, and the false-repair rate c / n; and the
    committed `docs/AI_EVALUATION.md` is exactly what the committed data renders to."""
    outcomes = (
        outcomes_for("verse_quote", "en", "verse", 200, 30, 2)
        + outcomes_for("verse_quote", "en", "blockquote", 150, 3, 1)
        + outcomes_for("heading_roles", "de", "chapter_heading", 300, 20, 10)
    )
    rows = false_repair.table(outcomes)
    assert [(row.task, row.language, row.category) for row in rows] == [
        ("heading_roles", "de", "chapter_heading"),
        ("verse_quote", "en", "blockquote"),
        ("verse_quote", "en", "verse"),
    ]
    by_category = {row.category: row for row in rows}

    verse = by_category["verse"]
    assert (verse.n, verse.b, verse.c) == (200, 30, 2)
    assert verse.test.method == "chi2", "b + c = 32 is at or over the threshold"
    assert verse.test.statistic == pytest.approx((30 - 2) ** 2 / 32)
    assert verse.false_repair_rate == pytest.approx(0.01)

    blockquote = by_category["blockquote"]
    assert blockquote.test.method == "exact", "b + c = 4 is under it"
    assert blockquote.test.statistic is None
    assert blockquote.test.p == pytest.approx(0.625)  # 2 * P(X <= 1 | 4, 1/2)

    chapters = by_category["chapter_heading"]
    assert chapters.false_repair_rate == pytest.approx(10 / 300)

    document = render.render(
        outcomes,
        {task: [] for task in false_repair.TASKS},
        {"verse_quote": ["en"], "heading_roles": [], "metadata": [], "book_structure": []},
    )
    for fragment in (
        "| `verse_quote` | en | verse | 200 | 30 | 2 | chi2 |",
        "| `verse_quote` | en | blockquote | 150 | 3 | 1 | exact |",
        "| `heading_roles` | de | chapter_heading | 300 | 20 | 10 | chi2 |",
        "0.0333 | **no** |",
    ):
        assert fragment in document, fragment

    # The committed document is the render of the committed data: regenerating changes nothing.
    assert (REPO_ROOT / "docs" / "AI_EVALUATION.md").read_text(encoding="utf-8") == (
        render.render_committed()
    )
    check = subprocess.run(
        [sys.executable, "-m", "oc_eval.compare", "--check"],
        cwd=REPO_ROOT / "eval",
        capture_output=True,
        text=True,
        check=False,
    )
    assert check.returncode == 0, check.stdout + check.stderr


def test_false_repair_rate_under_one_percent() -> None:
    """Row 10.18. Every **enabled** task's per-category false-repair rate is at most
    `ai_eval.false_repair_max`, in every language it is enabled for. Over the bound fails; no
    evidence for an enabled task fails — a gate that did not run is never a pass; and the
    shipped state, with nothing enabled, passes vacuously and says so."""
    bound = false_repair.false_repair_max()
    assert bound == pytest.approx(0.01)

    good = false_repair.table(outcomes_for("verse_quote", "en", "verse", 1000, 40, 5))
    bad = false_repair.table(outcomes_for("verse_quote", "en", "verse", 1000, 40, 20))
    enabled = {"verse_quote": ["en"]}

    assert false_repair.gate(good, enabled).passed
    failed = false_repair.gate(bad, enabled)
    assert not failed.passed
    assert "verse_quote/en/verse" in failed.failures[0]

    nothing = false_repair.gate([], enabled)
    assert not nothing.passed
    assert nothing.failures == ["verse_quote/en: enabled with no evaluation"]

    # A task that is not enabled is not gated, whatever its numbers.
    assert false_repair.gate(bad, {"verse_quote": []}).passed

    # The shipped state: no task enabled for any language, so the gate passes and says why.
    shipped = {task: false_repair.enabled_languages(task) for task in false_repair.TASKS}
    assert all(not languages for languages in shipped.values())
    committed = false_repair.table(false_repair.load(render.OUTCOMES_PATH))
    assert false_repair.gate(committed, shipped).passed
    assert "Pass, vacuously" in render.render_committed()
    gate = subprocess.run(
        [sys.executable, "-m", "oc_eval.compare", "--gate"],
        cwd=REPO_ROOT / "eval",
        capture_output=True,
        text=True,
        check=False,
    )
    assert gate.returncode == 0, gate.stdout + gate.stderr


def test_a_language_is_proposed_only_on_evidence() -> None:
    """Detail 7's language gating: non-inferior overall *and* within the bound in every category,
    per language — EN and DE pass, TR corrupts one category and is not proposed."""
    outcomes = (
        outcomes_for("verse_quote", "en", "verse", 400, 30, 2)
        + outcomes_for("verse_quote", "de", "verse", 400, 25, 3)
        + outcomes_for("verse_quote", "tr", "verse", 400, 30, 2)
        + outcomes_for("verse_quote", "tr", "blockquote", 100, 0, 5)
    )
    rows = false_repair.table(outcomes)
    assert false_repair.proposed_languages(rows, "verse_quote") == ["de", "en"]
    assert false_repair.proposed_languages(rows, "metadata") == []


def test_the_gold_sets_load_and_follow_the_format(tmp_path: Path) -> None:
    """The four committed sets load under the format's rules, each item's label is one its task
    can give, and none is yet the size an evaluation needs — which the document says."""
    sets = gold.load_all()
    assert set(sets) == set(false_repair.TASKS)
    for task, items in sets.items():
        assert items, task
        assert all(item.stratum == "ours(typst)" for item in items)
        assert len(items) < gold.minimum()

    (tmp_path / "verse_quote.jsonl").write_text(
        '{"id":"x","task":"verse_quote","source":"s","stratum":"ours(typst)","language":"en",'
        '"category":"poem","subject":"a","label":"poem"}\n',
        encoding="utf-8",
    )
    with pytest.raises(gold.GoldError):
        gold.load("verse_quote", tmp_path)


def test_scoring_pairs_every_gold_item_with_both_paths() -> None:
    """An item a path gave no answer for is a failure on that path, never skipped; answers are
    compared case- and whitespace-insensitively."""
    items = gold.load("metadata")[:3]
    deterministic = {items[0].id: items[0].label.upper(), items[1].id: "Microsoft Word - x"}
    llm = {items[0].id: f"  {items[0].label} ", items[1].id: items[1].label}
    outcomes = score.outcomes(items, deterministic, llm)
    assert [(o.deterministic_pass, o.llm_pass) for o in outcomes] == [
        (True, True),
        (False, True),
        (False, False),
    ]


def test_mcnemar_forms_agree_with_the_definitions() -> None:
    assert mcnemar.test(0, 0).p == 1.0
    assert mcnemar.test(12, 13).method == "chi2"
    assert mcnemar.test(12, 12).method == "exact"
    assert mcnemar.test(20, 5).p == pytest.approx(0.002699796063, rel=1e-6)
