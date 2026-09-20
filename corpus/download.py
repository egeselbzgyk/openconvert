#!/usr/bin/env python3
"""Fetch the redistributable corpus. A thin shim over `oc_eval.corpus.download`.

    python3 corpus/download.py [--dest corpus/downloads] [--mirror-base URL]

The implementation lives in `eval/src/oc_eval/corpus/download.py` because that is where it is
tested (IMPLEMENTATION_PLAN §1.7); this file exists so that a contributor who has only cloned
the repository has something obvious to run. It refuses any entry marked `local_eval_only` —
that set has its own script and its own opt-in flag (TEST_CORPUS §7.5).
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO_ROOT / "eval" / "src"))

from oc_eval.corpus import download as dl  # noqa: E402  - after sys.path is set up


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=REPO_ROOT / "corpus" / "manifest.json")
    parser.add_argument("--dest", type=Path, default=REPO_ROOT / "corpus" / "downloads")
    parser.add_argument("--mirror-base", default=None)
    parser.add_argument("--only", nargs="*", default=[])
    args = parser.parse_args()

    entries = json.loads(args.manifest.read_text(encoding="utf-8"))["files"]
    if args.only:
        entries = [entry for entry in entries if entry["id"] in set(args.only)]
    entries = [
        entry
        for entry in entries
        if not entry.get("local_eval_only")
        and str(entry.get("source", {}).get("url", "")).startswith("http")
    ]

    for entry in entries:
        try:
            got = dl.fetch_entry(entry, args.dest, mirror_base=args.mirror_base)
        except (dl.ChecksumMismatch, dl.NoSourceAvailable) as failure:
            print(f"FAIL {failure}", file=sys.stderr)
            return 1
        state = "cached" if got.reused else "fetched"
        print(f"{state} {got.path.name}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
