"""The full-corpus run: convert every corpus file, score it, write `eval/out/report.json`.

PHASE 7 row 7.14 and TEST_STRATEGY §8. One structured record per `(corpus file, metric)`,
never only a rolled-up score, and the report is per stratum — `metrics.report` refuses to grow
an aggregate row, so this module cannot produce one by accident.

A file the corpus manifest names and the download directory does not hold is **reported as
missing**, not skipped. A nightly that silently scored the forty files it happened to have is
a nightly whose number means something different every night.

The conversion itself is the shipped binary, run as a subprocess. That is deliberate: the
thing under measurement is what a user gets, and driving the library from Python would measure
a second driver that nobody ships (D13.1).
"""

from __future__ import annotations

import json
import subprocess
import tempfile
from collections.abc import Callable, Iterable
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from oc_eval.corpus import manifest as mf
from oc_eval.metrics import report as report_mod

# `eval/src/oc_eval/run.py` -> repository root.
REPO_ROOT = Path(__file__).resolve().parents[3]
DEFAULT_MANIFEST = REPO_ROOT / "corpus" / "manifest.json"
DEFAULT_DOWNLOADS = REPO_ROOT / "corpus" / "downloads"
DEFAULT_REPORT = REPO_ROOT / "eval" / "out" / "report.json"

# A conversion that has not finished in this long has failed at something other than being
# slow, and the run has 100+ more files to get through.
CONVERT_TIMEOUT_SECONDS = 600

# The metrics a conversion yields without ground truth. Ground-truth-bearing metrics are added
# per file by `oc_eval.metrics` when `ground_truth_ref` names one; these are what every file
# in the corpus can be scored on.
METRIC_CONVERTED = "converted"
METRIC_SECONDS_PER_PAGE = "seconds_per_page"
METRIC_EPUB_BYTES_PER_PAGE = "epub_bytes_per_page"
METRIC_REPAIRS_FIRED = "repairs_fired"


@dataclass
class RunOutcome:
    rows: list[report_mod.Row] = field(default_factory=list)
    missing: list[str] = field(default_factory=list)
    failed: list[tuple[str, str]] = field(default_factory=list)

    @property
    def converted(self) -> int:
        return sum(1 for row in self.rows if row.metric == METRIC_CONVERTED and row.value == 1.0)


def corpus_files(
    manifest_path: Path = DEFAULT_MANIFEST, downloads: Path = DEFAULT_DOWNLOADS
) -> list[tuple[mf.Entry, Path | None]]:
    """Every manifest entry paired with its local file, or None when it is not there."""
    out: list[tuple[mf.Entry, Path | None]] = []
    for entry in mf.load(manifest_path).entries:
        candidate = downloads / f"{entry.id}.pdf"
        out.append((entry, candidate if candidate.exists() else None))
    return out


def run(
    binary: Path,
    *,
    manifest_path: Path = DEFAULT_MANIFEST,
    downloads: Path = DEFAULT_DOWNLOADS,
    limit: int | None = None,
    runner: Callable[[Path, Path], dict[str, float]] | None = None,
) -> RunOutcome:
    """Convert what the corpus holds and score it. `runner` is injected by the tests."""
    convert = runner if runner is not None else _convert_one
    outcome = RunOutcome()

    for index, (entry, path) in enumerate(corpus_files(manifest_path, downloads)):
        if limit is not None and index >= limit:
            break
        if path is None:
            outcome.missing.append(entry.id)
            continue

        try:
            measured = convert(binary, path)
        except Exception as failure:  # one bad file may not stop a nightly
            outcome.failed.append((entry.id, str(failure)))
            outcome.rows.append(_row(entry, METRIC_CONVERTED, 0.0))
            continue

        outcome.rows.extend(_rows_for(entry, measured))

    return outcome


def _rows_for(entry: mf.Entry, measured: dict[str, float]) -> Iterable[report_mod.Row]:
    yield _row(entry, METRIC_CONVERTED, 1.0)
    for metric, value in sorted(measured.items()):
        yield _row(entry, metric, value)


def _row(entry: mf.Entry, metric: str, value: float) -> report_mod.Row:
    return report_mod.Row(
        stratum=entry.producer_stratum or "unknown",
        file_id=entry.id,
        metric=metric,
        value=value,
    )


def _convert_one(binary: Path, source: Path) -> dict[str, float]:
    """Run the shipped binary over one file and read its own report back."""
    with tempfile.TemporaryDirectory() as workspace:
        out = Path(workspace) / "out.epub"
        report_path = Path(workspace) / "out.report.json"
        result = subprocess.run(  # noqa: S603 - a fixed argv into our own binary
            [
                str(binary),
                "convert",
                str(source),
                "--output",
                str(out),
                "--report",
                str(report_path),
            ],
            capture_output=True,
            text=True,
            timeout=CONVERT_TIMEOUT_SECONDS,
            check=False,
        )
        if result.returncode != 0:
            raise RuntimeError(f"exit {result.returncode}: {result.stderr.strip()[:300]}")

        payload = (
            json.loads(report_path.read_text(encoding="utf-8")) if report_path.exists() else {}
        )
        pages = max(int(payload.get("pages", 0) or 0), 1)
        seconds = sum(float(v) for v in (payload.get("stage_millis") or {}).values()) / 1000.0
        size = out.stat().st_size if out.exists() else 0

        return {
            METRIC_SECONDS_PER_PAGE: seconds / pages,
            METRIC_EPUB_BYTES_PER_PAGE: size / pages,
            METRIC_REPAIRS_FIRED: float(payload.get("repair_iterations", 0) or 0),
        }


def write_report(outcome: RunOutcome, path: Path = DEFAULT_REPORT) -> dict[str, Any]:
    """Build the per-stratum report and write it, with what was missing recorded beside it."""
    built = report_mod.build(outcome.rows)
    built["missing"] = sorted(outcome.missing)
    built["failed"] = [{"file_id": ident, "error": message} for ident, message in outcome.failed]

    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(built, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    return built
