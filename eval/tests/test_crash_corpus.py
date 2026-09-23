"""PHASE 14 detail 9 and row 14.18: the mutated crash corpus is what its generators make.

`openconvert`'s `mutated_crash_corpus_terminates_cleanly` holds each file to the behaviour contract
and `crash_fixtures_are_manifest_keyed` holds the manifest to the files; this holds the files to the
generators, so a committed crash fixture can only change in the commit that changes how it is made.
"""

from __future__ import annotations

import json

from oc_eval.mutate import crash


def test_the_committed_crash_corpus_regenerates_byte_identically() -> None:
    assert crash.write(check=True) == []


def test_every_generator_contributes_and_names_are_unique() -> None:
    files = crash.mutated()
    names = [name for name, *_ in files]
    assert len(names) == len(set(names))
    kinds = {kind for *_, kind in files}
    assert kinds == {"xref_cycle", "xref_flip", "truncate_stream", "objstm_nest", "pages_loop"}


def test_the_manifest_keys_files_by_name_and_sha256_in_order() -> None:
    manifest = json.loads(crash.MANIFEST.read_text(encoding="utf-8"))
    names = [entry["name"] for entry in manifest["files"]]
    assert names == sorted(names)
    assert all(len(entry["sha256"]) == 64 for entry in manifest["files"])
    assert "fetch-isartor" in manifest["isartor"]
