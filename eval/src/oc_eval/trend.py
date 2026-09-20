"""The `ours(*)`-versus-real gap over time.

PHASE 7 row 7.10, D18. The gap is a first-class metric because a *widening* one is the earliest
available signal that a heuristic or an escalation threshold has been fitted to our own
renderer rather than to books (RT A9) — and "widening" is a statement about history, which a
single run cannot make. A gap of 0.06 is fine or alarming depending on what last week's was.

The history is kept in the repository (TEST_STRATEGY §8) so that a regression is a diff against
the immediately-prior committed baseline, the same discipline as snapshot review. It is written
sorted, with one entry per commit, so re-running the nightly on an unchanged tree corrects the
record rather than doubling it.
"""

from __future__ import annotations

import datetime as dt
import json
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Any

SCHEMA_VERSION = 1

# `eval/src/oc_eval/trend.py` -> repository root.
REPO_ROOT = Path(__file__).resolve().parents[3]
DEFAULT_TREND = REPO_ROOT / "eval" / "out" / "trend.json"
DEFAULT_PLOT = REPO_ROOT / "eval" / "out" / "trend.png"


@dataclass(frozen=True)
class Widening:
    first: float
    last: float
    runs: int

    @property
    def is_widening(self) -> bool:
        return self.last > self.first


def record(
    built_report: dict[str, Any],
    *,
    path: Path = DEFAULT_TREND,
    commit: str | None = None,
    recorded_at: str | None = None,
) -> dict[str, Any]:
    """Add this run's gap to the history, replacing any earlier run of the same commit."""
    gap = built_report["ours_vs_real"]
    entry = {
        "recorded_at": recorded_at or dt.datetime.now(dt.UTC).isoformat().replace("+00:00", "Z"),
        "commit": commit if commit is not None else _head_commit(),
        "ours": gap["ours"],
        "real": gap["real"],
        "gap": gap["gap"],
        "ours_n": gap["ours_n"],
        "real_n": gap["real_n"],
        "per_stratum": built_report["per_stratum"],
    }

    history = load(path)
    runs = [run for run in history["runs"] if run.get("commit") != entry["commit"]]
    runs.append(entry)
    runs.sort(key=lambda run: (str(run.get("recorded_at", "")), str(run.get("commit", ""))))
    history["runs"] = runs

    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(history, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    return entry


def load(path: Path = DEFAULT_TREND) -> dict[str, Any]:
    if not path.exists():
        return {"schema_version": SCHEMA_VERSION, "runs": []}
    payload = json.loads(path.read_text(encoding="utf-8"))
    payload.setdefault("schema_version", SCHEMA_VERSION)
    payload.setdefault("runs", [])
    return payload


def widening(path: Path = DEFAULT_TREND) -> Widening | None:
    """The first and last recorded gap, or None if the history cannot say.

    A run with no real strata states no gap (D18: a gap against nothing is not a number), so it
    is not a data point about widening either.
    """
    gaps = [run["gap"] for run in load(path)["runs"] if run.get("gap") is not None]
    if len(gaps) < 2:
        return None
    return Widening(first=float(gaps[0]), last=float(gaps[-1]), runs=len(gaps))


def plot(path: Path = DEFAULT_TREND, out: Path = DEFAULT_PLOT) -> Path:
    """Draw the gap over time. Row 7.10 says plotted; a trend nobody reads is not a signal."""
    import matplotlib

    # No display on a CI runner, and none wanted: the output is a file.
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    runs = [run for run in load(path)["runs"] if run.get("gap") is not None]
    labels = [str(run.get("commit", ""))[:7] for run in runs]
    gaps = [float(run["gap"]) for run in runs]

    figure, axes = plt.subplots(figsize=(8, 3.5))
    axes.plot(range(len(gaps)), gaps, marker="o")
    axes.axhline(0.0, linewidth=0.8)
    axes.set_xticks(range(len(labels)))
    axes.set_xticklabels(labels, rotation=45, ha="right", fontsize=8)
    axes.set_ylabel("ours(*) - real")
    axes.set_title("Synthetic-versus-real score gap (D18)")
    figure.tight_layout()

    out.parent.mkdir(parents=True, exist_ok=True)
    figure.savefig(out, dpi=120, format="png")
    plt.close(figure)
    return out


def _head_commit() -> str:
    try:
        result = subprocess.run(
            ["git", "rev-parse", "HEAD"],  # noqa: S607 - git is the tool, by name
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
            check=True,
        )
    except (OSError, subprocess.CalledProcessError):
        return "unknown"
    return result.stdout.strip() or "unknown"
