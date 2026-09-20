"""PHASE 7: the four facts a downloaded PDF has to give up before it can become an entry.

Exercised against the committed Typst fixtures, which are the only PDFs in the repository —
and which happen to be exactly the pair the `tagged` field exists to distinguish, since
`cargo xtask fixtures` emits each one twice, once with its struct tree stripped and once with
it kept (D18).
"""

from __future__ import annotations

from pathlib import Path

import pytest

from oc_eval.corpus import probe as probe_mod
from oc_eval.corpus import stratify

REPO_ROOT = Path(__file__).resolve().parents[2]
FIXTURE_DIR = REPO_ROOT / "corpus" / "fixtures" / "handmade"


def a_committed_pdf() -> Path:
    candidates = sorted(FIXTURE_DIR.glob("*.pdf"))
    if not candidates:
        pytest.skip("no committed PDF fixtures in corpus/fixtures/handmade")
    return candidates[0]


def test_a_committed_fixture_yields_its_page_count_and_version() -> None:
    result = probe_mod.probe(a_committed_pdf())

    assert result.pages >= 1
    assert result.pdf_version.startswith("1.") or result.pdf_version.startswith("2.")
    assert result.usable


def test_a_file_that_is_not_a_pdf_is_a_finding_rather_than_a_crash(tmp_path: Path) -> None:
    """A source that serves an HTML error page under a `.pdf` name is a thing that happens."""
    impostor = tmp_path / "looks-like.pdf"
    impostor.write_bytes(b"<!DOCTYPE html><title>404 Not Found</title>")

    with pytest.raises(probe_mod.NotAPdf):
        probe_mod.probe(impostor)


def test_an_empty_file_is_a_finding_rather_than_a_crash(tmp_path: Path) -> None:
    empty = tmp_path / "empty.pdf"
    empty.write_bytes(b"")

    with pytest.raises(probe_mod.NotAPdf):
        probe_mod.probe(empty)


def test_the_probe_feeds_the_stratifier_directly() -> None:
    """The two halves of the same job: read what the file says, then bucket it."""
    result = probe_mod.probe(a_committed_pdf())

    stratum = stratify.classify_producer(result.producer, creator=result.creator)

    assert stratum in stratify.PRODUCER_MARKERS_STRATA | {
        "ours(Typst)",
        "ours(WeasyPrint)",
        stratify.UNKNOWN,
    }
