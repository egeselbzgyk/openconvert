"""Fetch corpus files by URL, verifying the manifest's digest before anything may use them.

TEST_CORPUS §7.2: real-world corpus files are never committed. They are fetched from a
project-controlled mirror where one exists and from the canonical source otherwise, and the
manifest's `sha256` is checked before the bytes are handed to a test. A file whose digest does
not match is deleted rather than quarantined: a corpus file is reproducible by definition, so
the only thing a bad copy can do is make a later run look like a regression.

The opener is a parameter. The default reaches the network; the suite passes its own, because
a checksum is testable without a connection and because no test in this repository may need one
(D13.9).
"""

from __future__ import annotations

import hashlib
import shutil
from collections.abc import Callable, Iterable, Iterator, Mapping
from dataclasses import dataclass
from pathlib import Path
from typing import Any

# A byte stream per URL. `urlopen` satisfies it; so does a dict lookup.
Opener = Callable[[str], Iterable[bytes]]

# 1 MiB: large enough that a 20 MB monograph is 20 reads, small enough that a truncated
# connection is noticed before the process is holding the whole file in memory.
CHUNK_BYTES = 1 << 20

USER_AGENT = "openconvert-corpus/1 (+https://github.com/openconvert/openconvert)"


class ChecksumMismatch(RuntimeError):
    """The bytes that arrived are not the bytes the manifest describes."""


class NoSourceAvailable(RuntimeError):
    """Every URL for an entry failed. The message names all of them."""


class NotRedistributable(RuntimeError):
    """A local-eval-only entry reached the redistributable path (TEST_CORPUS §7.5)."""


@dataclass(frozen=True)
class Download:
    """Where an entry's bytes ended up, and what it cost to get them there."""

    path: Path
    from_mirror: bool
    reused: bool
    bytes_written: int


def sha256_of(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(CHUNK_BYTES), b""):
            digest.update(chunk)
    return digest.hexdigest()


def default_opener(url: str) -> Iterator[bytes]:
    """The production opener. Imported lazily so the test path never touches `urllib`.

    https only. A manifest is data, and `file:` or a custom scheme would turn one into a way
    to read the machine running the download.
    """
    import urllib.request

    if not url.startswith("https://"):
        raise ValueError(f"refusing a non-https corpus URL: {url!r}")

    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})  # noqa: S310
    with urllib.request.urlopen(request, timeout=120) as response:  # noqa: S310
        while chunk := response.read(CHUNK_BYTES):
            yield chunk


def urls_for(entry: Mapping[str, Any], mirror_base: str | None) -> list[str]:
    """Mirror first, canonical source second — TEST_CORPUS §7.2's order."""
    canonical = str(entry["source"]["url"])
    if mirror_base is None:
        return [canonical]
    return [f"{mirror_base.rstrip('/')}/{filename_for(entry)}", canonical]


def filename_for(entry: Mapping[str, Any]) -> str:
    """`<id>.pdf` — the id is the manifest's stable slug, so the name is stable too."""
    return f"{entry['id']}.pdf"


def fetch_entry(
    entry: Mapping[str, Any],
    dest_dir: Path,
    *,
    opener: Opener = default_opener,
    mirror_base: str | None = None,
) -> Download:
    """Place one manifest entry's file in `dest_dir`, verified.

    Raises `ChecksumMismatch` if a source served bytes that are not the entry's, and
    `NoSourceAvailable` if no source served anything at all.
    """
    if entry.get("local_eval_only"):
        raise NotRedistributable(
            f"{entry['id']} is local-eval-only and is not fetched by the redistributable "
            "path — see corpus/LOCAL_EVAL_ONLY.md"
        )

    expected = str(entry["sha256"])
    target = dest_dir / filename_for(entry)

    if target.exists() and sha256_of(target) == expected:
        return Download(target, from_mirror=False, reused=True, bytes_written=0)

    dest_dir.mkdir(parents=True, exist_ok=True)
    partial = target.with_suffix(target.suffix + ".part")
    failures: list[str] = []

    for index, url in enumerate(urls_for(entry, mirror_base)):
        written = _stream_to(url, partial, opener)
        if written is None:
            failures.append(f"{url}: unavailable")
            continue
        actual = sha256_of(partial)
        if actual != expected:
            partial.unlink()
            raise ChecksumMismatch(
                f"{entry['id']}: {url} served sha256 {actual}, manifest says {expected}"
            )
        partial.replace(target)
        return Download(
            target,
            from_mirror=index == 0 and mirror_base is not None,
            reused=False,
            bytes_written=written,
        )

    raise NoSourceAvailable(f"{entry['id']}: no source served the file — " + "; ".join(failures))


def _stream_to(url: str, partial: Path, opener: Opener) -> int | None:
    """Write `url` to `partial`, or return None if the source could not be read at all."""
    written = 0
    try:
        with partial.open("wb") as handle:
            for chunk in opener(url):
                handle.write(chunk)
                written += len(chunk)
    except (OSError, ValueError):
        partial.unlink(missing_ok=True)
        return None
    return written


def fetch_all(
    entries: Iterable[Mapping[str, Any]],
    dest_dir: Path,
    *,
    opener: Opener = default_opener,
    mirror_base: str | None = None,
) -> list[Download]:
    return [
        fetch_entry(entry, dest_dir, opener=opener, mirror_base=mirror_base) for entry in entries
    ]


def free_bytes(path: Path) -> int:
    return shutil.disk_usage(path).free
