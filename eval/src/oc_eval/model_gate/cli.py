"""`eval/model_gate.py`: run the nine gates, render the table, or fill the registry (D9)."""

from __future__ import annotations

import argparse
import datetime
import os
import platform
import sys
import tomllib
from pathlib import Path

import psutil

from oc_eval import thresholds
from oc_eval.model_gate import gates, registry, results
from oc_eval.model_gate.server import LlamaServer, Server

REPO_ROOT = Path(__file__).resolve().parents[4]
LLAMA_LOCK = REPO_ROOT / "xtask" / "llama.lock"
TABLE_KEYS = (
    "model_gate.g2_generations",
    "model_gate.g3_fixtures",
    "model_gate.g4_max_seconds_on_l",
    "llm.max_wallclock_share",
    "model_gate.g5_max_repeat_ratio",
    "model_gate.g6_max_rss_bytes",
    "model_gate.g6_release_seconds",
    "model_gate.g7_alpha",
    "model_gate.g7_noninferiority_margin",
    "model_gate.g8_min_probe_accuracy",
    "model_gate.probe_items_per_language",
)


def pinned_build() -> str:
    return str(tomllib.loads(LLAMA_LOCK.read_text(encoding="utf-8"))["tag"])


def machine_descriptor() -> dict[str, object]:
    return {
        "system": platform.system(),
        "arch": platform.machine(),
        "cpus": os.cpu_count(),
        "ram_bytes": psutil.virtual_memory().total,
        "python": platform.python_version(),
    }


def render_table(results_dir: Path) -> str:
    rows = [(key, thresholds.value(key), thresholds.entry(key)["source"]) for key in TABLE_KEYS]
    return results.render(results.load_all(results_dir), rows)


def passed_on(runs: list[results.Run]) -> dict[str, set[str]]:
    latest: dict[tuple[str, str], results.Run] = {}
    for run in sorted(runs, key=lambda r: r.date):
        latest[(run.model_id, run.machine)] = run
    out: dict[str, set[str]] = {}
    for (model_id, machine), run in latest.items():
        if run.verdict == "pass":
            out.setdefault(model_id, set()).add(machine)
    return out


def run_gates(args: argparse.Namespace, server: Server | None = None) -> int:
    models = registry.load(args.registry)
    try:
        model = registry.entry(models, args.model)
    except registry.RegistryError as error:
        print(f"error: {error}", file=sys.stderr)
        return 2
    if server is None:
        server = LlamaServer(
            Path(args.llama_server),
            Path(args.model_path),
            context=int(model["context"]),
            cache_reuse=bool(model["cache_reuse"]),
            context_checkpoints=model.get("context_checkpoints"),
            bench=Path(args.llama_bench) if args.llama_bench else None,
            threads=os.cpu_count() or 1,
        )
    inputs = gates.Inputs(
        registry_sha256=model.get("sha256"),
        deterministic_seconds=args.deterministic_seconds,
        g7_pairs=Path(args.g7_pairs) if args.g7_pairs else None,
        g9_sha256=args.g9_sha256,
    )
    run = results.Run(
        model_id=args.model,
        llama_build=args.llama_build or pinned_build(),
        machine=args.machine,
        machine_descriptor=machine_descriptor(),
        date=datetime.datetime.now(datetime.UTC).date().isoformat(),
        gates=gates.run_all(server, inputs),
    )
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    (out / run.file_name()).write_text(run.to_json(), encoding="utf-8")
    print(results.table(run))
    print(f"\nverdict: {run.verdict} ({out / run.file_name()})")
    return 0 if run.verdict == "pass" else 1


def parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="model_gate.py", description=__doc__)
    mode = p.add_mutually_exclusive_group(required=True)
    mode.add_argument("--model", help="registry id to run the nine gates on")
    mode.add_argument(
        "--render-table", action="store_true", help="write docs/MODEL_GATE.md from the result files"
    )
    mode.add_argument(
        "--emit-registry", action="store_true", help="fill models.toml's TODO_ pins from the hub"
    )
    p.add_argument("--registry", type=Path, default=registry.MODELS_TOML)
    p.add_argument("--model-path", default="", help="the GGUF")
    p.add_argument("--llama-server", default="", help="the pinned llama-server binary")
    p.add_argument("--llama-bench", default="", help="llama-bench from the same release")
    p.add_argument(
        "--machine",
        default="unnamed",
        help="L or M for the reference machines (D9); anything else is informative",
    )
    p.add_argument("--llama-build", default="", help="default: xtask/llama.lock's tag")
    p.add_argument(
        "--deterministic-seconds",
        type=float,
        default=None,
        help="G4: the reference 300-page book's --no-ai wall clock on this machine",
    )
    p.add_argument("--g7-pairs", default="", help="G7: JSONL of paired answers")
    p.add_argument("--g9-sha256", default=None, help="G9: SHA-256 of the GGUF we produced")
    p.add_argument("--out", default=str(results.RESULTS_DIR))
    p.add_argument("--results", type=Path, default=results.RESULTS_DIR)
    p.add_argument("--table", type=Path, default=results.TABLE_PATH)
    p.add_argument(
        "--check",
        action="store_true",
        help="with --render-table: fail if the table is out of date instead of writing",
    )
    return p


def main(argv: list[str] | None = None, server: Server | None = None) -> int:
    args = parser().parse_args(argv)
    if args.render_table:
        text = render_table(args.results)
        if args.check:
            current = args.table.read_text(encoding="utf-8") if args.table.is_file() else ""
            if current != text:
                print(f"{args.table} is out of date; run --render-table", file=sys.stderr)
                return 1
            return 0
        args.table.write_text(text, encoding="utf-8")
        return 0
    if args.emit_registry:
        filled = registry.emit(args.registry, registry.HttpHub())
        print("filled: " + (", ".join(filled) or "nothing"))
        return 0
    return run_gates(args, server)
