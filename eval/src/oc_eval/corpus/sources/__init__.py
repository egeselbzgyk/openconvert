"""Where real-world corpus documents come from, one adapter per source.

TEST_CORPUS §7.6's sourcing target: OAPEN/DOAB monographs ~40, Internet Archive PD scans ~20,
arXiv CC-BY ~15, US-Gov/EU ~15, DergiPark CC-BY + DTA-derived ~10. Each adapter answers one
question — "give me `want` candidates I am allowed to redistribute" — and splits into a pure
parser over the catalogue's payload and a thin fetch loop around it, so the parsing is tested
without a connection and only the loop needs one.

An adapter never downloads the PDF. It yields a `Candidate`; `harvest.py` decides what to
fetch, and `download.py` fetches it.
"""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass(frozen=True)
class Candidate:
    """A document a source believes we may redistribute, before we have seen its bytes."""

    id: str
    title: str
    source_name: str
    landing_url: str
    pdf_url: str
    license_name: str
    license_url: str
    languages: tuple[str, ...] = ()
    selection_query: str = ""
    pages_hint: int | None = None
    size_hint: int | None = None
    notes: tuple[str, ...] = field(default_factory=tuple)


def slugify(text: str, *, limit: int = 60) -> str:
    """A manifest id is a stable slug and part of a filename, so it stays ASCII and lowercase."""
    kept: list[str] = []
    previous_dash = False
    for char in text.lower():
        if char.isascii() and char.isalnum():
            kept.append(char)
            previous_dash = False
        elif not previous_dash and kept:
            kept.append("-")
            previous_dash = True
    slug = "".join(kept).strip("-")
    return slug[:limit].strip("-") or "untitled"
