"""PHASE 13 follow-up: the scan simulator writes `corpus/manifest.json` in the one order.

The manifest has three writers — `xtask fixtures`, the corpus harvest (`manifest.dump`) and
the scan simulator — and CI runs two of them in a row before asserting the tree is clean.
They agree only if every writer sorts `files` by `id`; the simulator used to append its
entries at the end, so each `xtask fixtures` run moved them and `--check` then failed.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from oc_eval.generate.scan_sim import merge_manifest_files

REPO_ROOT = Path(__file__).resolve().parents[2]
COMMITTED_MANIFEST = REPO_ROOT / "corpus" / "manifest.json"


def test_scan_entries_merge_in_canonical_order() -> None:
    existing: list[dict[str, Any]] = [
        {"id": "f01_prose_single_column"},
        {"id": "f01__scan200", "stale": True},
    ]
    fresh: list[dict[str, Any]] = [{"id": "f01__scan300"}, {"id": "f01__scan200"}]

    merged = merge_manifest_files(existing, fresh)

    assert [item["id"] for item in merged] == [
        "f01__scan200",
        "f01__scan300",
        "f01_prose_single_column",
    ]
    assert all("stale" not in item for item in merged), "a regenerated entry replaces the old one"


def test_committed_manifest_is_in_canonical_order() -> None:
    files = json.loads(COMMITTED_MANIFEST.read_text(encoding="utf-8"))["files"]
    ids = [item["id"] for item in files]
    assert ids == sorted(ids), "corpus/manifest.json is not sorted by id"


def test_merging_the_committed_scan_entries_is_a_no_op() -> None:
    files = json.loads(COMMITTED_MANIFEST.read_text(encoding="utf-8"))["files"]
    scans = [item for item in files if "__scan" in item["id"]]
    assert scans, "the committed manifest has the Phase 13 scanned fixtures"
    assert merge_manifest_files(files, scans) == files
