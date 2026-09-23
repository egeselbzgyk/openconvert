"""PHASE 9 rows 9.17 and 9.20, and the harness under them: the nine gates, the probes, the
fixtures, the result files, the table and the registry fill.

No real model runs here. The gates are driven by `FakeServer`, which answers the way a well-behaved
llama-server would unless told otherwise, so what is tested is the *harness*: that every gate is
reported, that a gate which did not run is never a pass, and that one failure fails the verdict.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

import pytest

from oc_eval.model_gate import cli, fixtures, gates, mcnemar, probes, registry, results
from oc_eval.model_gate.server import Completion

REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT = REPO_ROOT / "eval" / "model_gate.py"
SHA = "ab" * 32
COMMIT = "0123456789abcdef0123456789abcdef01234567"


def answer_for(fixture: dict[str, Any]) -> str:
    """A correct answer to a prompt fixture, built from what the fixture expects."""
    expect = fixture["expect"]
    purpose = fixture["purpose"]
    if purpose == "metadata":
        return json.dumps(
            {
                "title": None,
                "subtitle": None,
                "authors": [],
                "translator": None,
                "publisher": None,
                "date": None,
            }
        )
    if purpose == "heading_roles":
        return json.dumps(
            {
                "m": [{"c": c, "r": "body"} for c in expect["clusters"]],
                "h": [{"i": i, "r": "body"} for i in expect["holdout"]],
            }
        )
    if purpose == "book_structure":
        n = expect["headings"]
        return json.dumps(
            {"frontmatter_end_idx": 0, "part_boundaries": [], "backmatter_start_idx": n}
        )
    return json.dumps({"b": [{"id": i, "k": "paragraph"} for i in expect["blocks"]]})


class FakeServer:
    def __init__(
        self,
        *,
        healthy: bool = True,
        think_on: int | None = None,
        probe_wrong: int = 0,
        rss: int = 1 << 30,
    ) -> None:
        self.healthy = healthy
        self.think_on = think_on
        self.probe_wrong = probe_wrong
        self.rss = rss
        self.calls_since_start = 0
        self.calls = 0
        self.by_user = {f["user"]: f for f in fixtures.load()}
        self.labels = {
            item["text"]: item["role"] for lang in ("de", "tr") for item in probes.load(lang)
        }
        self.wrong_left = probe_wrong

    def start(self) -> bool:
        self.calls_since_start = 0
        return self.healthy

    def bench(self) -> bool:
        return True

    def complete(self, system: str, user: str, grammar: str) -> Completion:
        assert system == fixtures.system_prompt()
        self.calls += 1
        self.calls_since_start += 1
        cold = self.calls_since_start == 1
        seconds = 4.0 if self.calls_since_start <= 8 else 1.0
        if user.startswith(gates.PROBE_INSTRUCTION):
            line = json.loads(user.split("\n", 1)[1])["line"]
            role = self.labels[line]
            if self.wrong_left:
                self.wrong_left -= 1
                role = "other" if role != "other" else "body"
            text = json.dumps(role)
        else:
            text = answer_for(self.by_user[user])
        if self.think_on == self.calls:
            text = "<think>hm</think>" + text
        return Completion(text, None, 0 if cold else 900, seconds)

    def tokenize_chat(self, system: str, user: str) -> list[int] | None:
        return [len(system), len(user)]

    def rss_bytes(self) -> int:
        return self.rss

    def stop(self) -> float | None:
        return 0.05


@pytest.fixture
def with_reference_tokens(monkeypatch: pytest.MonkeyPatch) -> None:
    """G3 needs reference tokenizations, which the committed fixtures do not carry yet."""
    loaded = fixtures.load()
    for f in loaded:
        f["reference_tokens"] = [len(fixtures.system_prompt()), len(f["user"])]
    monkeypatch.setattr(fixtures, "load", lambda *a, **k: loaded)


def pairs_file(tmp_path: Path) -> Path:
    rows = []
    for task in fixtures.PURPOSES:
        for i in range(1000):
            better = task == "heading_roles" and i < 60
            rows.append(
                {
                    "task": task,
                    "item": i,
                    "candidate_correct": True,
                    "incumbent_correct": not better,
                }
            )
    path = tmp_path / "pairs.jsonl"
    path.write_text("".join(json.dumps(r) + "\n" for r in rows), encoding="utf-8")
    return path


def unpinned(text: str) -> str:
    """The shipped registry with every pin put back to its placeholder: what `--emit-registry`
    starts from."""
    text = re.sub(r'(?m)^(revision\s*=\s*)"[0-9a-f]{40}"', r'\1"TODO_COMMIT_SHA"', text)
    return re.sub(r'(?m)^(sha256\s*=\s*)"[0-9a-f]{64}"', r'\1"TODO_SHA256"', text)


def registry_file(tmp_path: Path) -> Path:
    text = unpinned(registry.MODELS_TOML.read_text(encoding="utf-8"))
    text = text.replace('"TODO_COMMIT_SHA"', f'"{COMMIT}"').replace('"TODO_SHA256"', f'"{SHA}"')
    path = tmp_path / "models.toml"
    path.write_text(text, encoding="utf-8")
    return path


def run(tmp_path: Path, server: FakeServer, *extra: str) -> tuple[int, results.Run]:
    code = cli.main(
        [
            "--model",
            "qwen3-1.7b-q4_k_m",
            "--machine",
            "L",
            "--llama-build",
            "b1",
            "--out",
            str(tmp_path / "out"),
            "--registry",
            str(registry_file(tmp_path)),
            *extra,
        ],
        server=server,
    )
    [path] = (tmp_path / "out").glob("*.json")
    return code, results.load(path)


# ------------------------------------------------------------------------------ row 9.17


def test_model_gate_g1_to_g9_script_runs(tmp_path: Path) -> None:
    """Row 9.17: the script emits a full pass/fail table and a non-zero exit on any failure — here,
    the ordinary state of a machine without the server or the model."""
    done = subprocess.run(  # noqa: S603 - a fixed argv into this repository's own script
        [
            sys.executable,
            str(SCRIPT),
            "--model",
            "qwen3-1.7b-q4_k_m",
            "--machine",
            "sandbox",
            "--llama-server",
            str(tmp_path / "no-server"),
            "--model-path",
            str(tmp_path / "no.gguf"),
            "--out",
            str(tmp_path),
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    assert done.returncode == 1, done.stderr
    for gate in results.GATES:
        assert re.search(rf"^\| {gate} \|", done.stdout, re.MULTILINE), f"no row for {gate}"
    assert "verdict: fail" in done.stdout
    [written] = tmp_path.glob("qwen3-1.7b-q4_k_m__b10456__sandbox.json")
    run_ = results.load(written)
    statuses = {g.gate: g.status for g in run_.gates}
    assert statuses["G1"] == "fail"
    assert all(statuses[g] == "not_run" for g in results.GATES[1:]), statuses
    assert json.loads(written.read_text())["verdict"] == "fail"


def test_every_gate_passes_against_a_well_behaved_server(
    tmp_path: Path, with_reference_tokens: None
) -> None:
    code, run_ = run(
        tmp_path,
        FakeServer(),
        "--deterministic-seconds",
        "150",
        "--g7-pairs",
        str(pairs_file(tmp_path)),
        "--g9-sha256",
        SHA,
    )
    assert {g.gate: g.status for g in run_.gates} == dict.fromkeys(results.GATES, "pass")
    assert run_.verdict == "pass"
    assert code == 0


@pytest.mark.parametrize(
    ("server", "gate"),
    [
        (FakeServer(think_on=7), "G3"),
        (FakeServer(rss=3 << 30), "G6"),
        (FakeServer(probe_wrong=6), "G8"),
    ],
)
def test_one_failing_gate_fails_the_verdict_and_the_exit_code(
    tmp_path: Path, with_reference_tokens: None, server: FakeServer, gate: str
) -> None:
    code, run_ = run(
        tmp_path,
        server,
        "--deterministic-seconds",
        "150",
        "--g7-pairs",
        str(pairs_file(tmp_path)),
        "--g9-sha256",
        SHA,
    )
    statuses = {g.gate: g.status for g in run_.gates}
    assert statuses[gate] == "fail", statuses
    assert run_.verdict == "fail"
    assert code == 1


def test_a_gate_that_did_not_run_is_never_a_pass(tmp_path: Path) -> None:
    """The committed fixtures carry no reference tokenizations, and no G7 pairs or G9 hash are
    given: those three gates report not_run, and the verdict is fail with everything else green."""
    code, run_ = run(tmp_path, FakeServer(), "--deterministic-seconds", "150")
    statuses = {g.gate: g.status for g in run_.gates}
    assert statuses["G3"] == statuses["G7"] == statuses["G9"] == "not_run"
    assert statuses["G1"] == statuses["G2"] == statuses["G8"] == "pass"
    assert run_.verdict == "fail"
    assert code == 1


# ------------------------------------------------------------------------------ statistics


def test_mcnemar_superiority_and_non_inferiority() -> None:
    assert mcnemar.superior(mcnemar.Paired(n=200, b=20, c=2), 0.05)
    assert not mcnemar.superior(mcnemar.Paired(n=200, b=5, c=4), 0.05)
    assert not mcnemar.superior(mcnemar.Paired(n=200, b=2, c=20), 0.05)
    # Equal accuracy: non-inferior only when the evidence is tight enough to rule out the margin.
    assert not mcnemar.non_inferior(mcnemar.Paired(n=200, b=10, c=10), 0.05, 0.02)
    assert mcnemar.non_inferior(mcnemar.Paired(n=1000, b=10, c=10), 0.05, 0.02)
    assert not mcnemar.non_inferior(mcnemar.Paired(n=1000, b=10, c=60), 0.05, 0.02)
    assert mcnemar.non_inferior(mcnemar.Paired(n=100, b=0, c=0), 0.05, 0.02)
    assert mcnemar.tally([(True, False), (False, True), (True, True)]) == mcnemar.Paired(3, 1, 1)


# ------------------------------------------------------------------------ probes and fixtures


def test_probe_roles_are_the_heading_roles_grammar_enum() -> None:
    grammar = probes.HEADING_ROLES_GRAMMAR.read_text(encoding="utf-8")
    role_rule = grammar[grammar.index("role    ::=") :]
    assert tuple(re.findall(r'"\\"([a-z_]+)\\""', role_rule)) == probes.ROLES


@pytest.mark.parametrize("lang", ["de", "tr"])
def test_probes_are_generated_deterministically_and_committed(lang: str) -> None:
    generated = probes.generate(lang)
    assert generated == probes.load(lang), "run python -m oc_eval.model_gate.probes --write"
    assert len(generated) == 100
    assert len({item["text"] for item in generated}) == 100, "every probe is a different line"
    counts = {role: sum(item["role"] == role for item in generated) for role in probes.ROLES}
    assert min(counts.values()) >= 11, counts


def test_prompt_fixtures_are_committed_and_four_are_the_renderers_own() -> None:
    generated = fixtures.generate()
    assert generated == fixtures.load(), "run python -m oc_eval.model_gate.fixtures --write"
    assert [f["source"] for f in generated[:4]] == [f"cassette:{p}" for p in fixtures.PURPOSES]
    for purpose in fixtures.PURPOSES:
        assert sum(f["purpose"] == purpose for f in generated) == 5
    assert all(f["reference_tokens"] is None for f in generated)


def test_semantic_assertions_accept_the_worked_answers_and_refuse_wrong_ones() -> None:
    loaded = fixtures.load()
    for fixture in loaded[:4]:
        path = fixtures.seed_cassette(fixture["purpose"])
        answer = json.loads(path.read_text(encoding="utf-8"))["response"]["content"]
        assert gates.semantic_ok(fixture, answer), fixture["id"]
    metadata, roles, structure, verse = loaded[:4]
    assert not gates.semantic_ok(
        metadata,
        '{"title":"Der Prozess","subtitle":null,'
        '"authors":[],"translator":null,"publisher":null,"date":null}',
    )
    assert not gates.semantic_ok(roles, '{"m":[{"c":0,"r":"chapter_heading"}],"h":[]}')
    assert not gates.semantic_ok(
        structure, '{"frontmatter_end_idx":9,"part_boundaries":[],"backmatter_start_idx":2}'
    )
    assert not gates.semantic_ok(verse, '{"b":[{"id":"AAAAAAAAAA","k":"verse"}]}')
    assert not gates.semantic_ok(verse, "not json")


# --------------------------------------------------------------------- results and the table


def test_a_run_file_is_named_by_model_build_and_machine_and_round_trips(tmp_path: Path) -> None:
    run_ = results.Run(
        "m",
        "b1",
        "L",
        {"cpus": 8},
        "2026-09-23",
        [results.GateResult(g, "pass", 1, 1) for g in results.GATES],
    )
    assert run_.file_name() == "m__b1__L.json"
    path = tmp_path / run_.file_name()
    path.write_text(run_.to_json(), encoding="utf-8")
    assert results.load(path) == run_
    assert json.loads(path.read_text())["verdict"] == "pass"


def test_model_gate_md_is_rendered_from_the_results() -> None:
    """D9: docs/MODEL_GATE.md is generated from the result files and never edited by hand."""
    assert cli.render_table(results.RESULTS_DIR) == results.TABLE_PATH.read_text(
        encoding="utf-8"
    ), "run eval/model_gate.py --render-table"
    assert cli.main(["--render-table", "--check"]) == 0


# ------------------------------------------------------------------------------ row 9.20


def test_default_stays_qwen3_1_7b_until_gate_passes() -> None:
    """Row 9.20, on the committed files: the default names a `tier = "default"` entry, and nothing
    but D9's Qwen3-1.7B is the default without a passing gate run on both reference machines."""
    models = registry.load()
    runs = results.load_all()
    assert registry.default_problems(models, cli.passed_on(runs)) == []
    assert models["default"] == registry.DEFAULT_MODEL or {"L", "M"} <= cli.passed_on(runs).get(
        models["default"], set()
    )


def test_changing_the_default_needs_nine_green_gates_on_both_machines() -> None:
    models = registry.load()
    promoted = dict(models, default="qwen3.5-2b-q4_k_m-unsloth")
    assert registry.default_problems(promoted, {}), "an experimental entry is not a default"

    retiered = dict(
        promoted,
        model=[
            dict(m, tier="default") if m["id"] == promoted["default"] else m
            for m in models["model"]
        ],
    )
    assert registry.default_problems(retiered, {}), "re-tiering alone is not a promotion"
    assert registry.default_problems(retiered, {promoted["default"]: {"L"}}), (
        "L alone is not enough"
    )
    assert registry.default_problems(retiered, {promoted["default"]: {"L", "M"}}) == []


# ----------------------------------------------------------------------- the registry fill


class FakeHub:
    def __init__(self, files: dict[str, tuple[str, int]], listed: dict[str, tuple[str, int]]):
        self.files = files
        self.listed = listed
        self.urls: list[str] = []

    def json(self, url: str) -> Any:
        self.urls.append(url)
        if url.count("/tree/"):
            return [
                {"path": name, "size": size, "lfs": {"oid": oid, "size": size}}
                for name, (oid, size) in self.listed.items()
            ]
        return {"sha": COMMIT}

    def sha256(self, url: str) -> tuple[str, int]:
        self.urls.append(url)
        return self.files[url.rsplit("/", 1)[1]]


def test_emit_registry_fills_every_pin_with_a_hash_we_produced(tmp_path: Path) -> None:
    path = tmp_path / "models.toml"
    path.write_text(unpinned(registry.MODELS_TOML.read_text(encoding="utf-8")), encoding="utf-8")
    names = [m["file"] for m in registry.load(path)["model"]]
    digests = {name: (f"{i:064x}", 1000 + i) for i, name in enumerate(names)}
    hub = FakeHub(digests, digests)

    filled = registry.emit(path, hub)
    assert filled == [m["id"] for m in registry.load(registry.MODELS_TOML)["model"]]
    models = registry.load(path)
    assert "TODO_" not in path.read_text(encoding="utf-8")
    for model in models["model"]:
        assert model["revision"] == COMMIT
        assert (model["sha256"], model["size_bytes"]) == digests[model["file"]]
    assert all(u.startswith("https://huggingface.co/") for u in hub.urls)
    assert (
        f"https://huggingface.co/ggml-org/Qwen3-1.7B-GGUF/resolve/{COMMIT}/Qwen3-1.7B-Q4_K_M.gguf"
        in hub.urls
    )
    # The comments that are not about the placeholder survive the rewrite.
    assert "# llama.cpp project's quant" in path.read_text(encoding="utf-8")


def test_emit_registry_leaves_the_shipped_pins_alone(tmp_path: Path) -> None:
    """The shipped registry is pinned (2026-09-23): a fill asks the hub nothing and rewrites
    nothing."""
    path = tmp_path / "models.toml"
    shipped = registry.MODELS_TOML.read_text(encoding="utf-8")
    assert "TODO_" not in shipped
    path.write_text(shipped, encoding="utf-8")
    hub = FakeHub({}, {})
    assert registry.emit(path, hub) == []
    assert hub.urls == []
    assert path.read_text(encoding="utf-8") == shipped


def test_emit_registry_refuses_a_file_absent_from_the_tree(tmp_path: Path) -> None:
    """V1 §1(g): a repository can answer and still not hold the file."""
    path = tmp_path / "models.toml"
    path.write_text(unpinned(registry.MODELS_TOML.read_text(encoding="utf-8")), encoding="utf-8")
    before = path.read_text(encoding="utf-8")
    with pytest.raises(registry.RegistryError, match="has no"):
        registry.emit(path, FakeHub({}, {}))
    assert path.read_text(encoding="utf-8") == before, "nothing is written on a refusal"


def test_emit_registry_refuses_a_hash_the_hub_disagrees_with(tmp_path: Path) -> None:
    path = tmp_path / "models.toml"
    path.write_text(unpinned(registry.MODELS_TOML.read_text(encoding="utf-8")), encoding="utf-8")
    names = [m["file"] for m in registry.load(path)["model"]]
    listed = {name: ("aa" * 32, 10) for name in names}
    produced = {name: ("bb" * 32, 10) for name in names}
    with pytest.raises(registry.RegistryError, match="we hashed"):
        registry.emit(path, FakeHub(produced, listed))
