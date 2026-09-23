"""The nine gates (D9, RT C3, IMPLEMENTATION_PLAN PHASE 9 detail 9).

Every gate returns a `GateResult`. A gate whose inputs are missing reports `not_run` with the
missing input named, never `pass`: a promotion decided on gates that did not run is exactly the
outcome D9 exists to prevent.
"""

from __future__ import annotations

import itertools
import json
import unicodedata
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from oc_eval import thresholds
from oc_eval.model_gate import fixtures as fixtures_mod
from oc_eval.model_gate import mcnemar, probes
from oc_eval.model_gate.results import GateResult
from oc_eval.model_gate.server import Completion, Server

THINK = "<think>"
METADATA_KEYS = {"title", "subtitle", "authors", "translator", "publisher", "date"}
VERSE_KINDS = {"verse", "blockquote", "preformatted", "paragraph"}
PROBE_INSTRUCTION = "Give this line one role."


def probe_grammar() -> str:
    """`root` is exactly the heading_roles `role` rule: one quoted enum value."""
    alternatives = " | ".join(f'"\\"{role}\\""' for role in probes.ROLES)
    return f"root ::= ( {alternatives} )\n"


@dataclass
class Inputs:
    """What a run was given besides the server."""

    registry_sha256: str | None
    deterministic_seconds: float | None = None
    g7_pairs: Path | None = None
    g9_sha256: str | None = None


@dataclass
class Context:
    server: Server
    inputs: Inputs
    started: bool = False
    generations: list[tuple[dict[str, Any], Completion]] = field(default_factory=list)
    first_set_seconds: float | None = None


def _value(key: str) -> Any:
    return thresholds.value(key)


def _fixtures() -> list[dict[str, Any]]:
    return fixtures_mod.load()


def _call_set() -> list[dict[str, Any]]:
    """One book's calls: the budget's worth (`llm.max_calls_per_book`), in the degradation
    order's reverse — metadata, heading roles, book structure, then verse/quote batches."""
    by_purpose: dict[str, list[dict[str, Any]]] = {}
    for fixture in _fixtures():
        by_purpose.setdefault(fixture["purpose"], []).append(fixture)
    order = [
        "metadata",
        "heading_roles",
        "book_structure",
        "book_structure",
        "verse_quote",
        "verse_quote",
        "verse_quote",
        "heading_roles",
    ]
    calls = int(_value("llm.max_calls_per_book"))
    counters: dict[str, int] = {}
    chosen = []
    for purpose in order[:calls]:
        index = counters.get(purpose, 0)
        counters[purpose] = index + 1
        chosen.append(by_purpose[purpose][index % len(by_purpose[purpose])])
    return chosen


def _ask(ctx: Context, fixture: dict[str, Any]) -> Completion:
    return ctx.server.complete(
        fixtures_mod.system_prompt(), fixture["user"], fixtures_mod.grammar(fixture["purpose"])
    )


# ------------------------------------------------------------------------ semantic assertions


def _norm(text: str) -> str:
    return " ".join(unicodedata.normalize("NFC", text).casefold().split())


def semantic_ok(fixture: dict[str, Any], text: str) -> bool:
    """G2's assertions for one answer: it parses, and it answers the question it was asked."""
    try:
        answer = json.loads(text)
    except ValueError:
        return False
    purpose, expect = fixture["purpose"], fixture["expect"]
    if not isinstance(answer, dict):
        return False
    if purpose == "metadata":
        if set(answer) != METADATA_KEYS:
            return False
        payload = _norm(expect["payload"])
        values = [answer[k] for k in METADATA_KEYS - {"authors"}] + list(answer["authors"] or [])
        return all(v is None or (isinstance(v, str) and _norm(v) in payload) for v in values)
    if purpose == "heading_roles":
        mapped = [e.get("c") for e in answer.get("m", [])]
        held = [e.get("i") for e in answer.get("h", [])]
        roles = [e.get("r") for e in answer.get("m", []) + answer.get("h", [])]
        return (
            sorted(mapped) == sorted(expect["clusters"])
            and sorted(held) == sorted(expect["holdout"])
            and all(r in probes.ROLES for r in roles)
        )
    if purpose == "book_structure":
        n = expect["headings"]
        front, back = answer.get("frontmatter_end_idx"), answer.get("backmatter_start_idx")
        parts = answer.get("part_boundaries")
        if not (isinstance(front, int) and isinstance(back, int) and isinstance(parts, list)):
            return False
        increasing = all(a < b for a, b in itertools.pairwise(parts))
        return 0 <= front <= back <= n and increasing and all(front <= p < back for p in parts)
    ids = [e.get("id") for e in answer.get("b", [])]
    kinds = [e.get("k") for e in answer.get("b", [])]
    return sorted(ids) == sorted(expect["blocks"]) and all(k in VERSE_KINDS for k in kinds)


# ------------------------------------------------------------------------------------ gates


def g1(ctx: Context) -> GateResult:
    ctx.started = ctx.server.start()
    if not ctx.started:
        return GateResult("G1", "fail", False, True, "the server did not become healthy")
    benched = ctx.server.bench()
    return GateResult(
        "G1",
        "pass" if benched else "fail",
        benched,
        True,
        "loaded and healthy"
        + ("; llama-bench completed" if benched else "; llama-bench did not complete"),
    )


def _not_started(gate: str) -> GateResult:
    return GateResult(gate, "not_run", detail="no healthy server (G1)")


def g2(ctx: Context) -> GateResult:
    if not ctx.started:
        return _not_started("G2")
    count = int(_value("model_gate.g2_generations"))
    fixtures = _fixtures()
    ok = 0
    for i in range(count):
        fixture = fixtures[i % len(fixtures)]
        completion = _ask(ctx, fixture)
        ctx.generations.append((fixture, completion))
        ok += semantic_ok(fixture, completion.text)
    return GateResult(
        "G2",
        "pass" if ok == count else "fail",
        f"{ok}/{count}",
        f"{count}/{count}",
        "grammar-constrained answers that parse and pass semantic assertions",
    )


def g3(ctx: Context) -> GateResult:
    if not ctx.started or not ctx.generations:
        return _not_started("G3")
    thought = sum(1 for _, c in ctx.generations if THINK in c.text or c.reasoning)
    total = len(ctx.generations)
    if thought:
        return GateResult(
            "G3", "fail", f"{thought}/{total} thought", "0", "thinking appeared in the answers"
        )
    fixtures = [f for f in _fixtures() if f.get("reference_tokens") is not None]
    needed = int(_value("model_gate.g3_fixtures"))
    if len(fixtures) < needed:
        return GateResult(
            "G3",
            "not_run",
            f"{len(fixtures)}/{needed} reference tokenizations",
            f"{needed}/{needed}",
            f"thinking absent {total}/{total}; the fixtures carry no reference "
            "tokenizer output to compare against",
        )
    matched = sum(
        1
        for f in fixtures
        if ctx.server.tokenize_chat(fixtures_mod.system_prompt(), f["user"])
        == f["reference_tokens"]
    )
    return GateResult(
        "G3",
        "pass" if matched == needed else "fail",
        f"{matched}/{needed}",
        f"{needed}/{needed}",
        f"thinking absent {total}/{total}",
    )


def _time_call_set(ctx: Context) -> float:
    """The call set's cost: the sum of each call's wall clock, as the server client measured it."""
    return sum(_ask(ctx, fixture).seconds for fixture in _call_set())


def g4(ctx: Context) -> GateResult:
    if not ctx.started:
        return _not_started("G4")
    # The call set is timed on a freshly started server, so this is the cold cost G5 compares with.
    ctx.server.stop()
    ctx.started = ctx.server.start()
    if not ctx.started:
        return GateResult("G4", "fail", detail="the server did not restart")
    seconds = _time_call_set(ctx)
    ctx.first_set_seconds = seconds
    budget = float(_value("model_gate.g4_max_seconds_on_l"))
    share_max = float(_value("llm.max_wallclock_share"))
    if ctx.inputs.deterministic_seconds is None:
        return GateResult(
            "G4",
            "not_run",
            round(seconds, 2),
            budget,
            "the deterministic wall clock of the reference 300-page book was not "
            "given (--deterministic-seconds), so the share cannot be computed",
        )
    share = seconds / (seconds + ctx.inputs.deterministic_seconds)
    passed = seconds <= budget and share <= share_max
    return GateResult(
        "G4",
        "pass" if passed else "fail",
        {"seconds": round(seconds, 2), "share": round(share, 3)},
        {"seconds": budget, "share": share_max},
        "full per-book call set",
    )


def g5(ctx: Context) -> GateResult:
    if not ctx.started or ctx.first_set_seconds is None:
        return _not_started("G5")
    second = _time_call_set(ctx)
    ratio = second / ctx.first_set_seconds if ctx.first_set_seconds > 0 else float("inf")
    limit = float(_value("model_gate.g5_max_repeat_ratio"))
    return GateResult(
        "G5",
        "pass" if ratio <= limit else "fail",
        round(ratio, 3),
        limit,
        "second identical call set over the first",
    )


def g6(ctx: Context) -> GateResult:
    if not ctx.started:
        return _not_started("G6")
    peak = max(ctx.server.rss_bytes(), getattr(ctx.server, "peak_rss", 0))
    released = ctx.server.stop()
    ctx.started = False
    rss_max = int(_value("model_gate.g6_max_rss_bytes"))
    release_max = float(_value("model_gate.g6_release_seconds"))
    passed = peak <= rss_max and released is not None and released <= release_max
    return GateResult(
        "G6",
        "pass" if passed else "fail",
        {"peak_rss_bytes": peak, "released_seconds": released},
        {"peak_rss_bytes": rss_max, "released_seconds": release_max},
        "at -c 8192 -np 1",
    )


def g7(ctx: Context) -> GateResult:
    path = ctx.inputs.g7_pairs
    if path is None or not path.is_file():
        return GateResult(
            "G7",
            "not_run",
            detail="no paired candidate/incumbent answers given "
            "(--g7-pairs); Phase 10's evaluation produces them",
        )
    alpha = float(_value("model_gate.g7_alpha"))
    margin = float(_value("model_gate.g7_noninferiority_margin"))
    by_task: dict[str, list[tuple[bool, bool]]] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        row = json.loads(line)
        by_task.setdefault(row["task"], []).append(
            (bool(row["candidate_correct"]), bool(row["incumbent_correct"]))
        )
    missing = set(fixtures_mod.PURPOSES) - set(by_task)
    if missing:
        return GateResult("G7", "fail", detail=f"no pairs for {sorted(missing)}")
    tallies = {task: mcnemar.tally(pairs) for task, pairs in sorted(by_task.items())}
    inferior = [t for t, p in tallies.items() if not mcnemar.non_inferior(p, alpha, margin)]
    better = [t for t, p in tallies.items() if mcnemar.superior(p, alpha)]
    passed = not inferior and bool(better)
    return GateResult(
        "G7",
        "pass" if passed else "fail",
        {"inferior": inferior, "better": better},
        {"alpha": alpha, "margin": margin},
        "non-inferior on all four tasks and significantly better on one",
    )


def g8(ctx: Context) -> GateResult:
    if not ctx.started:
        ctx.started = ctx.server.start()
    if not ctx.started:
        return _not_started("G8")
    minimum = float(_value("model_gate.g8_min_probe_accuracy"))
    grammar = probe_grammar()
    measured: dict[str, Any] = {}
    passed = True
    for lang in ("de", "tr"):
        items = probes.load(lang)
        right = outside = 0
        for item in items:
            user = (
                PROBE_INSTRUCTION
                + "\n"
                + fixtures_mod.compact({"language": lang, "line": item["text"]})
            )
            text = ctx.server.complete(fixtures_mod.system_prompt(), user, grammar).text
            try:
                answer = json.loads(text)
            except ValueError:
                answer = None
            if answer not in probes.ROLES:
                outside += 1
            elif answer == item["role"]:
                right += 1
        accuracy = right / len(items)
        measured[lang] = {"accuracy": round(accuracy, 3), "out_of_alphabet": outside}
        passed = passed and accuracy >= minimum and outside == 0
    ctx.server.stop()
    ctx.started = False
    return GateResult(
        "G8",
        "pass" if passed else "fail",
        measured,
        {"accuracy": minimum, "out_of_alphabet": 0},
        "100-item flat-enum probes",
    )


def g9(ctx: Context) -> GateResult:
    produced, pinned = ctx.inputs.g9_sha256, ctx.inputs.registry_sha256
    if produced is None:
        return GateResult(
            "G9",
            "not_run",
            detail="no SHA-256 of a GGUF we converted from the "
            "official safetensors was given (--g9-sha256)",
        )
    if pinned is None or pinned.startswith("TODO_"):
        return GateResult(
            "G9", "not_run", produced, None, "the registry has no SHA-256 for this model yet"
        )
    return GateResult(
        "G9",
        "pass" if produced == pinned else "fail",
        produced,
        pinned,
        "our conversion against the registry's pinned file",
    )


GATE_FUNCTIONS = (g1, g2, g3, g4, g5, g6, g7, g8, g9)


def run_all(server: Server, inputs: Inputs) -> list[GateResult]:
    ctx = Context(server, inputs)
    results = []
    try:
        for gate in GATE_FUNCTIONS:
            results.append(gate(ctx))
    finally:
        if ctx.started:
            server.stop()
    return results
