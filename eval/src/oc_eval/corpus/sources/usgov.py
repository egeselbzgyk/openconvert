"""US-Government technical reports — real tagging of varying quality, and real Word output.

TEST_CORPUS §7.6's ~15. NASA's NTRS is the keyless route into the PD-US-Gov bucket: its
citation search reports a copyright determination per record, and `GOV_PUBLIC_USE_PERMITTED`
is the one that means "a work of the United States Government, no copyright". Anything else —
including the merely `OPEN_ACCESS` and the `MAY_INCLUDE_COPYRIGHT_MATERIAL` — is skipped, not
interpreted.

The reports themselves are a producer mix nothing else in the corpus supplies: Word, Distiller,
and decades of scanner output, all under one rights statement.
"""

from __future__ import annotations

import urllib.parse
from collections.abc import Iterator
from typing import Any

from oc_eval.corpus.sources import Candidate, slugify

SOURCE_NAME = "US-Gov"
BASE = "https://ntrs.nasa.gov"
SEARCH = f"{BASE}/api/citations/search"

# The determination that means a work of the US Government: public domain, no copyright.
PUBLIC_DOMAIN_DETERMINATION = "GOV_PUBLIC_USE_PERMITTED"

LICENSE_NAME = "PD-US-Gov"
LICENSE_URL = "https://www.usa.gov/government-works"


def search_url(query: str, *, size: int, page: int) -> str:
    params = [("q", query), ("page.size", str(size)), ("page.from", str(page * size))]
    return f"{SEARCH}?{urllib.parse.urlencode(params)}"


def parse(payload: Any, *, selection_query: str) -> list[Candidate]:
    """Turn one NTRS search response into candidates. Pure; no network."""
    results = payload.get("results", []) if isinstance(payload, dict) else []
    found: list[Candidate] = []
    for record in results:
        candidate = _candidate_from(record, selection_query=selection_query)
        if candidate is not None:
            found.append(candidate)
    return found


def _candidate_from(record: Any, *, selection_query: str) -> Candidate | None:
    copyright_block = record.get("copyright") or {}
    if copyright_block.get("determinationType") != PUBLIC_DOMAIN_DETERMINATION:
        return None

    pdf_path = _pdf_path(record)
    if pdf_path is None:
        return None

    identifier = str(record.get("id") or "")
    if not identifier:
        return None

    title = " ".join(str(record.get("title") or identifier).split())
    return Candidate(
        id=f"usgov-ntrs-{slugify(identifier, limit=30)}",
        title=title,
        source_name=SOURCE_NAME,
        landing_url=f"{BASE}/citations/{identifier}",
        pdf_url=f"{BASE}{pdf_path}",
        license_name=LICENSE_NAME,
        license_url=LICENSE_URL,
        languages=("en",),
        selection_query=selection_query,
    )


def _pdf_path(record: Any) -> str | None:
    for download in record.get("downloads", []) or []:
        path = (download.get("links") or {}).get("pdf")
        if path:
            return str(path)
    return None


def candidates(
    want: int,
    *,
    query: str = "technical report",
    size: int = 100,
    max_pages: int = 10,
) -> Iterator[Candidate]:
    """Yield up to `want` records NTRS states are works of the US Government."""
    from oc_eval.corpus import http

    seen: set[str] = set()
    yielded = 0
    for page in range(max_pages):
        if yielded >= want:
            return
        url = search_url(query, size=size, page=page)
        batch = parse(http.get_json(url), selection_query=url)
        if not batch:
            return
        for candidate in batch:
            if yielded >= want:
                return
            if candidate.id in seen:
                continue
            seen.add(candidate.id)
            yielded += 1
            yield candidate
