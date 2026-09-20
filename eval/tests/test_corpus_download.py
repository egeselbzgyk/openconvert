"""PHASE 7 row 7.5: nothing uses a corpus file until its bytes match the manifest's digest.

The corpus is fetched, never committed (TEST_CORPUS §7.2), so the manifest's `sha256` is the
only thing standing between a test result and a file that a mirror, a proxy or a truncated
connection quietly changed. These tests drive the fetcher through an injected opener: the
suite must never reach the network (D13.9), and a checksum check is testable without one.
"""

from __future__ import annotations

import hashlib
from collections.abc import Callable, Iterator
from pathlib import Path
from typing import Any

import pytest

from oc_eval.corpus.download import (
    ChecksumMismatch,
    NoSourceAvailable,
    NotRedistributable,
    fetch_entry,
    sha256_of,
)

GOOD = b"%PDF-1.7\n% a corpus file that is exactly what the manifest says it is\n"
CORRUPT = b"%PDF-1.7\n% the same file after something in the middle rewrote a byte\n"


def entry_for(payload: bytes, *, ident: str = "e01") -> dict[str, Any]:
    return {
        "id": ident,
        "sha256": hashlib.sha256(payload).hexdigest(),
        "file_size_bytes": len(payload),
        "source": {"url": f"https://example.invalid/{ident}.pdf"},
    }


def url_of(entry: dict[str, Any]) -> str:
    return str(entry["source"]["url"])


def opener_for(routes: dict[str, bytes]) -> Callable[[str], Iterator[bytes]]:
    """An opener over a fixed URL -> bytes table; anything else is a 404."""

    def open_url(url: str) -> Iterator[bytes]:
        if url not in routes:
            raise FileNotFoundError(url)
        return iter([routes[url]])

    return open_url


def test_download_verifies_sha256(tmp_path: Path) -> None:
    entry = entry_for(GOOD)
    url = url_of(entry)

    with pytest.raises(ChecksumMismatch) as caught:
        fetch_entry(entry, tmp_path, opener=opener_for({url: CORRUPT}))

    assert entry["sha256"] in str(caught.value)
    assert list(tmp_path.iterdir()) == [], "a file that failed its digest must not be left behind"


def test_a_verified_download_is_placed_under_its_id(tmp_path: Path) -> None:
    entry = entry_for(GOOD)
    url = url_of(entry)

    got = fetch_entry(entry, tmp_path, opener=opener_for({url: GOOD}))

    assert got.path == tmp_path / "e01.pdf"
    assert got.path.read_bytes() == GOOD
    assert sha256_of(got.path) == entry["sha256"]
    assert got.from_mirror is False


def test_the_mirror_is_tried_before_the_canonical_source(tmp_path: Path) -> None:
    entry = entry_for(GOOD)
    canonical = url_of(entry)
    mirror = "https://mirror.invalid/corpus/e01.pdf"
    asked: list[str] = []

    def opener(url: str) -> Iterator[bytes]:
        asked.append(url)
        return opener_for({mirror: GOOD, canonical: GOOD})(url)

    got = fetch_entry(entry, tmp_path, opener=opener, mirror_base="https://mirror.invalid/corpus")

    assert asked == [mirror], "the canonical source is only reached for what the mirror lacks"
    assert got.from_mirror is True


def test_a_missing_mirror_falls_back_to_the_canonical_source(tmp_path: Path) -> None:
    entry = entry_for(GOOD)
    canonical = url_of(entry)

    got = fetch_entry(
        entry,
        tmp_path,
        opener=opener_for({canonical: GOOD}),
        mirror_base="https://mirror.invalid/corpus",
    )

    assert got.from_mirror is False
    assert got.path.read_bytes() == GOOD


def test_a_file_already_present_and_verified_is_not_fetched_again(tmp_path: Path) -> None:
    entry = entry_for(GOOD)
    (tmp_path / "e01.pdf").write_bytes(GOOD)

    def refuse(url: str) -> Iterator[bytes]:
        raise AssertionError(f"cache hit must not open {url}")

    got = fetch_entry(entry, tmp_path, opener=refuse)

    assert got.reused is True


def test_a_cached_file_whose_bytes_drifted_is_refetched(tmp_path: Path) -> None:
    entry = entry_for(GOOD)
    url = url_of(entry)
    (tmp_path / "e01.pdf").write_bytes(CORRUPT)

    got = fetch_entry(entry, tmp_path, opener=opener_for({url: GOOD}))

    assert got.reused is False
    assert got.path.read_bytes() == GOOD


def test_every_source_failing_names_every_url_it_tried(tmp_path: Path) -> None:
    entry = entry_for(GOOD)
    canonical = url_of(entry)
    mirror = "https://mirror.invalid/corpus/e01.pdf"

    with pytest.raises(NoSourceAvailable) as caught:
        fetch_entry(
            entry, tmp_path, opener=opener_for({}), mirror_base="https://mirror.invalid/corpus"
        )

    message = str(caught.value)
    assert mirror in message and canonical in message


def test_the_redistributable_path_refuses_a_local_eval_only_entry(tmp_path: Path) -> None:
    """TEST_CORPUS §7.5: the boundary is a mechanism, not a note a contributor could miss."""
    entry = entry_for(GOOD) | {"local_eval_only": True}
    url = url_of(entry)

    with pytest.raises(NotRedistributable):
        fetch_entry(entry, tmp_path, opener=opener_for({url: GOOD}))


def test_the_default_opener_refuses_a_non_https_url(tmp_path: Path) -> None:
    """A manifest is data. `file:` would turn one into a way to read the build machine."""
    from oc_eval.corpus.download import default_opener

    with pytest.raises(ValueError, match="non-https"):
        next(iter(default_opener("file:///etc/passwd")))
