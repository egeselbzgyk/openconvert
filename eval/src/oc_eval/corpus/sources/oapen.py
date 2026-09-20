"""OAPEN / DOAB open-access monographs — real InDesign and LaTeX trade typography.

TEST_CORPUS §7.6's largest slice (~40). OAPEN runs DSpace, whose `/rest/filtered-items`
endpoint filters on any metadata field, so the CC BY / CC BY-SA subset is selected by the
catalogue rather than by downloading and hoping: `dc.rights.uri` carries the licence URL and
`dc.language` the language, which is how the German slice §7.6 refuses to drop is found.

The licence still goes through `stratify.license_from_url`, so a record whose `dc.rights.uri`
says CC BY-NC is named and rejected rather than quietly skipped.
"""

from __future__ import annotations

import urllib.parse
from collections.abc import Iterator
from typing import Any

from oc_eval.corpus import http, stratify
from oc_eval.corpus.sources import Candidate, slugify

SOURCE_NAME = "OAPEN"
BASE = "https://library.oapen.org"
REST = f"{BASE}/rest/filtered-items"

# The two licences §7.6 admits from this source.
LICENCE_FILTERS = (
    "creativecommons.org/licenses/by/4.0",
    "creativecommons.org/licenses/by-sa/4.0",
    "creativecommons.org/licenses/by/3.0",
)


def query_url(licence_fragment: str, *, limit: int, offset: int, language: str | None) -> str:
    fields = [("dc.rights.uri", "contains", licence_fragment)]
    if language:
        fields.append(("dc.language", "contains", language))

    params: list[tuple[str, str]] = [
        ("expand", "metadata,bitstreams"),
        ("limit", str(limit)),
        ("offset", str(offset)),
    ]
    for name, op, value in fields:
        params += [("query_field[]", name), ("query_op[]", op), ("query_val[]", value)]
    return f"{REST}?{urllib.parse.urlencode(params)}"


def parse(payload: Any, *, selection_query: str) -> list[Candidate]:
    """Turn one `/rest/filtered-items` response into candidates. Pure; no network."""
    items = payload.get("items", []) if isinstance(payload, dict) else payload
    found: list[Candidate] = []
    for item in items or []:
        candidate = _candidate_from(item, selection_query=selection_query)
        if candidate is not None:
            found.append(candidate)
    return found


def _candidate_from(item: Any, *, selection_query: str) -> Candidate | None:
    metadata = _metadata(item)
    licence_url = _first(metadata, "dc.rights.uri")
    licence_name = stratify.license_from_url(licence_url)
    if not stratify.is_acceptable_license(licence_name):
        return None

    pdf = _largest_pdf(item)
    if pdf is None:
        return None

    handle = str(item.get("handle") or "").strip("/")
    title = _first(metadata, "dc.title") or item.get("name") or "untitled"
    ident = f"oapen-{slugify(handle.replace('/', '-') or title, limit=40)}"

    return Candidate(
        id=ident,
        title=str(title),
        source_name=SOURCE_NAME,
        landing_url=f"{BASE}/handle/{handle}" if handle else BASE,
        pdf_url=f"{BASE}{pdf['retrieveLink']}",
        license_name=str(licence_name),
        license_url=str(licence_url),
        languages=tuple(_language_tags(metadata)),
        selection_query=selection_query,
        pages_hint=_int_or_none(_first(metadata, "oapen.pages")),
        size_hint=_int_or_none(pdf.get("sizeBytes")),
    )


def _metadata(item: Any) -> dict[str, list[str]]:
    collected: dict[str, list[str]] = {}
    for field in item.get("metadata", []) or []:
        key = field.get("key")
        if key:
            collected.setdefault(str(key), []).append(str(field.get("value", "")))
    return collected


def _first(metadata: dict[str, list[str]], key: str) -> str | None:
    values = metadata.get(key)
    return values[0] if values else None


def _largest_pdf(item: Any) -> dict[str, Any] | None:
    """The book, not its cover thumbnail or its extracted text sidecar."""
    pdfs = [
        bitstream
        for bitstream in item.get("bitstreams", []) or []
        if str(bitstream.get("mimeType", "")) == "application/pdf"
        and str(bitstream.get("name", "")).lower().endswith(".pdf")
        and bitstream.get("retrieveLink")
    ]
    if not pdfs:
        return None
    return max(pdfs, key=lambda b: int(b.get("sizeBytes") or 0))


# OAPEN writes language names, not tags. Only the three v1 claims are mapped; anything else is
# recorded as-is so a report can see it rather than have it silently become English.
LANGUAGE_TAGS = {
    "english": "en",
    "german": "de",
    "turkish": "tr",
}


def _language_tags(metadata: dict[str, list[str]]) -> list[str]:
    tags: list[str] = []
    for raw in metadata.get("dc.language", []):
        tags.append(LANGUAGE_TAGS.get(raw.strip().lower(), raw.strip().lower()))
    return tags


def _int_or_none(value: Any) -> int | None:
    try:
        return int(str(value).strip())
    except (TypeError, ValueError):
        return None


def candidates(
    want: int,
    *,
    language: str | None = None,
    page_size: int = 100,
    max_requests: int = 20,
) -> Iterator[Candidate]:
    """Yield up to `want` acceptable candidates, walking the catalogue a page at a time."""
    seen: set[str] = set()
    yielded = 0
    for licence_fragment in LICENCE_FILTERS:
        offset = 0
        for _ in range(max_requests):
            if yielded >= want:
                return
            url = query_url(licence_fragment, limit=page_size, offset=offset, language=language)
            batch = parse(http.get_json(url), selection_query=url)
            if not batch:
                break
            for candidate in batch:
                if candidate.id in seen or yielded >= want:
                    continue
                seen.add(candidate.id)
                yielded += 1
                yield candidate
            offset += page_size
