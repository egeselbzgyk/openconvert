#!/usr/bin/env python3
"""Fetch the non-redistributed local-eval-only set. Requires an explicit opt-in flag.

    python3 corpus/download_local_eval_only.py --accept-non-redistributable

TEST_CORPUS §7.5: a separate script and a separate flag, so that nothing here is fetched by a
contributor who ran the ordinary corpus download. Nothing it fetches is corpus — no CI job
reads `corpus/LOCAL_EVAL_ONLY/`, no threshold is fitted on it, and no published score includes
it.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO_ROOT / "eval" / "src"))

from oc_eval.corpus import download as dl  # noqa: E402  - after sys.path is set up

LIST_PATH = REPO_ROOT / "corpus" / "local_eval_only.json"
DEST = REPO_ROOT / "corpus" / "LOCAL_EVAL_ONLY"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--accept-non-redistributable",
        action="store_true",
        help="Required. Affirms that nothing fetched here is redistributed or scored.",
    )
    args = parser.parse_args()

    if not args.accept_non_redistributable:
        print(
            "refusing: this set is not redistributable. Read corpus/LOCAL_EVAL_ONLY.md, then "
            "pass --accept-non-redistributable.",
            file=sys.stderr,
        )
        return 2
    if not LIST_PATH.exists():
        print(f"nothing to do: {LIST_PATH.name} does not exist", file=sys.stderr)
        return 0

    entries = json.loads(LIST_PATH.read_text(encoding="utf-8"))["files"]
    for entry in entries:
        got = dl.fetch_entry(dict(entry) | {"local_eval_only": False}, DEST)
        state = "cached" if got.reused else "fetched"
        print(f"{state} {got.path.name}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
