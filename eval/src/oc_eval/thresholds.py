"""Read `thresholds.toml`. The eval tooling gets its numbers from the same file the engine does.

CLAUDE.md §2 forbids a numeric literal in production Rust; the reason is the same here. A
corpus gate written as `0.40` in Python and `corpus.ours_max_share` in Rust is two numbers
that agree today, and the day they stop agreeing nothing says so.
"""

from __future__ import annotations

import tomllib
from functools import cache
from pathlib import Path
from typing import Any

# `eval/src/oc_eval/thresholds.py` -> repository root.
REPO_ROOT = Path(__file__).resolve().parents[3]
THRESHOLDS_PATH = REPO_ROOT / "thresholds.toml"


class UnknownThreshold(KeyError):
    """A dotted key that `thresholds.toml` does not define."""


@cache
def _table(path: str) -> dict[str, Any]:
    return tomllib.loads(Path(path).read_text(encoding="utf-8"))


def entry(key: str, *, path: Path = THRESHOLDS_PATH) -> dict[str, Any]:
    """The whole `{value, source, evidence, owner, review_by}` record for a dotted key."""
    table = _table(str(path))
    node: Any = table
    for part in key.split("."):
        if not isinstance(node, dict) or part not in node:
            raise UnknownThreshold(key)
        node = node[part]
    if not isinstance(node, dict) or "value" not in node:
        raise UnknownThreshold(key)
    return node


def value(key: str, *, path: Path = THRESHOLDS_PATH) -> Any:
    return entry(key, path=path)["value"]
