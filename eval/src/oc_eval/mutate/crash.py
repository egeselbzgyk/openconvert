"""The mutated crash corpus (PHASE 14 detail 9): write it, or check it is what the generators make.

    python -m oc_eval.mutate.crash [--check]

Writes ``corpus/fixtures/crash/mutated/<name>.pdf`` and keys every file under
``corpus/fixtures/crash/`` by ``(name, sha256)`` in ``corpus/fixtures/crash/manifest.json``. The
assertion made about each file (``openconvert``'s ``mutated_crash_corpus_terminates_cleanly``) is
weak on quality and strong on behaviour: it terminates within the stage deadline with exit 0 or 1
and a written report — never a hang, never 101, never a partial file at the output path.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from typing import Any

from oc_eval.mutate import objstm_nest, pages_loop, truncate_stream, xref_cycle, xref_flip
from oc_eval.mutate.pdfbytes import REPO_ROOT

CRASH = REPO_ROOT / "corpus" / "fixtures" / "crash"
MANIFEST = CRASH / "manifest.json"
GENERATORS = (xref_cycle, xref_flip, truncate_stream, objstm_nest, pages_loop)

# The Isartor suite is fetched by `cargo run -p xtask -- fetch-isartor` into `target/isartor/` and
# never committed; its pins live in `xtask/isartor.lock`.
ISARTOR_NOTE = "fetched by `xtask fetch-isartor` (pinned in xtask/isartor.lock), never committed"


def mutated() -> list[tuple[str, bytes, str, str]]:
    """Every mutation: (name, bytes, what it does, which generator)."""
    out = []
    for generator in GENERATORS:
        kind = generator.__name__.rsplit(".", 1)[-1]
        for name, data, note in generator.mutations():
            out.append((name, data, note, kind))
    names = [name for name, *_ in out]
    if len(names) != len(set(names)):
        raise ValueError("two mutations share a name")
    return sorted(out)


def manifest(files: list[tuple[str, bytes, str, str]]) -> dict[str, Any]:
    entries = [
        {
            "name": f"mutated/{name}.pdf",
            "sha256": hashlib.sha256(data).hexdigest(),
            "set": "mutated",
            "mutation": kind,
            "note": note,
            "expect": "terminates",
        }
        for name, data, note, kind in files
    ]
    # Minimised fuzz crashes are committed beside the fix that closed them.
    fuzz = CRASH / "fuzz"
    if fuzz.is_dir():
        for path in sorted(fuzz.iterdir()):
            if path.is_file():
                entries.append(
                    {
                        "name": f"fuzz/{path.name}",
                        "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                        "set": "fuzz",
                        "mutation": "cargo-fuzz",
                        "note": "a minimised input that once crashed a fuzz target",
                        "expect": "terminates",
                    }
                )
    return {
        "schema_version": 1,
        "isartor": ISARTOR_NOTE,
        "files": sorted(entries, key=lambda entry: str(entry["name"])),
    }


def write(check: bool) -> list[str]:
    files = mutated()
    changed = []
    wanted = {f"{name}.pdf" for name, *_ in files}
    target = CRASH / "mutated"
    target.mkdir(parents=True, exist_ok=True)
    for name, data, _, _ in files:
        path = target / f"{name}.pdf"
        if not path.exists() or path.read_bytes() != data:
            changed.append(str(path.relative_to(REPO_ROOT)))
            if not check:
                path.write_bytes(data)
    for stale in sorted(target.iterdir()):
        if stale.name not in wanted:
            changed.append(str(stale.relative_to(REPO_ROOT)))
            if not check:
                stale.unlink()
    rendered = json.dumps(manifest(files), indent=2, ensure_ascii=False) + "\n"
    current = MANIFEST.read_text(encoding="utf-8") if MANIFEST.exists() else None
    if current != rendered:
        changed.append(str(MANIFEST.relative_to(REPO_ROOT)))
        if not check:
            MANIFEST.write_text(rendered, encoding="utf-8")
    return changed


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="fail if anything would change")
    args = parser.parse_args()
    changed = write(args.check)
    for path in changed:
        print(f"{'differs' if args.check else 'written'}: {path}")
    if args.check and changed:
        sys.exit(1)


if __name__ == "__main__":
    main()
