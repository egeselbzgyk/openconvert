"""PHASE 7 row 7.13: PyMuPDF is AGPL and may not be here, in the environment or in the lock.

D15 and IMPLEMENTATION_PLAN §1.7. `eval/` is never a build or runtime dependency of the
shipped product, but it is committed, redistributed under Apache-2.0 and run by contributors,
and an AGPL library in it is a licence problem whichever side of the ship boundary it sits on.

Two checks, because they fail at different times. Importability catches the wheel somebody
installed by hand into their virtualenv; the lock catches the dependency somebody added to
`pyproject.toml`, which is the one that would reach everybody else.
"""

from __future__ import annotations

import importlib.util
import tomllib
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
LOCK = REPO_ROOT / "eval" / "uv.lock"
PYPROJECT = REPO_ROOT / "eval" / "pyproject.toml"

# PyMuPDF ships under several distribution names and imports as `fitz`. All of them are the
# same AGPL library, so all of them are named here.
BANNED_IMPORTS = ("pymupdf", "fitz")
BANNED_DISTRIBUTIONS = ("pymupdf", "pymupdf-fonts", "pymupdfb", "fitz", "frontend")


def test_no_agpl_in_eval_env() -> None:
    """Row 7.13: not importable, and not in the lock."""
    for banned in BANNED_IMPORTS:
        assert importlib.util.find_spec(banned) is None, f"{banned} is AGPL and banned (D15)"

    assert LOCK.exists(), f"{LOCK.name} is the thing this test reads; it has to be committed"
    locked = {
        str(package.get("name", "")).lower()
        for package in tomllib.loads(LOCK.read_text(encoding="utf-8")).get("package", [])
    }

    offenders = sorted(locked & set(BANNED_DISTRIBUTIONS))

    assert not offenders, f"AGPL in the lock: {offenders} (D15)"


def test_the_lock_is_the_one_this_project_declares() -> None:
    """A lock for a different project would pass the check above and mean nothing."""
    lock = tomllib.loads(LOCK.read_text(encoding="utf-8"))
    declared = tomllib.loads(PYPROJECT.read_text(encoding="utf-8"))["project"]["name"]

    names = {str(package.get("name", "")) for package in lock.get("package", [])}

    assert declared in names, f"{declared} is not in {LOCK.name}"


def test_every_direct_dependency_is_in_the_lock() -> None:
    """A lock that has drifted from `pyproject.toml` is a lock that checks nothing."""
    import re

    declared = tomllib.loads(PYPROJECT.read_text(encoding="utf-8"))["project"]
    wanted = {
        re.split(r"[<>=!~\[]", requirement, maxsplit=1)[0].strip().lower()
        for requirement in declared.get("dependencies", [])
    }
    locked = {
        str(package.get("name", "")).lower()
        for package in tomllib.loads(LOCK.read_text(encoding="utf-8")).get("package", [])
    }

    missing = sorted(wanted - locked)

    assert not missing, f"declared but not locked: {missing}; run `uv lock` in eval/"
