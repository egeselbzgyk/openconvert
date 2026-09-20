"""Internet Archive public-domain scans — real ABBYY OCR layers and real scanner artefacts.

TEST_CORPUS §7.6's ~20. The search endpoint filters on `licenseurl`, so only items whose
rights statement is the Public Domain Mark or CC0 are considered; the per-item metadata
endpoint then names the actual files, because an IA item is a directory and the scanned book
sits in it beside its OCR sidecars, its thumbnails and sometimes another book entirely.

The PDF chosen is the largest one whose name starts with the item identifier. IA's derivation
pipeline also produces `_text.pdf` and similar, and picking by size alone can land on a
sidecar.
"""

from __future__ import annotations

import urllib.parse
from collections.abc import Iterator
from typing import Any

from oc_eval.corpus import http, stratify
from oc_eval.corpus.sources import Candidate, slugify

SOURCE_NAME = "Internet Archive"
SEARCH = "https://archive.org/advancedsearch.php"
METADATA = "https://archive.org/metadata"
DOWNLOAD = "https://archive.org/download"

LICENCE_FILTERS = (
    "http://creativecommons.org/publicdomain/mark/1.0/",
    "http://creativecommons.org/publicdomain/zero/1.0/",
)

# Derivation artefacts that are not the book.
SIDECAR_SUFFIXES = ("_text.pdf", "_bw.pdf", "_chocr.pdf", "_djvu.pdf")


def search_url(licence_url: str, *, rows: int, page: int, language: str | None) -> str:
    clauses = [
        "mediatype:texts",
        f'licenseurl:"{licence_url}"',
        'format:"Text PDF"',
    ]
    if language:
        clauses.append(f'language:"{language}"')
    params = [
        ("q", " AND ".join(clauses)),
        ("fl[]", "identifier"),
        ("fl[]", "title"),
        ("fl[]", "language"),
        ("fl[]", "licenseurl"),
        ("rows", str(rows)),
        ("page", str(page)),
        ("output", "json"),
    ]
    return f"{SEARCH}?{urllib.parse.urlencode(params)}"


def parse_search(payload: Any) -> list[dict[str, Any]]:
    """The identifier rows of one search response. Pure; no network."""
    response = payload.get("response", {}) if isinstance(payload, dict) else {}
    return [row for row in response.get("docs", []) if row.get("identifier")]


def choose_pdf(metadata: Any, identifier: str) -> dict[str, Any] | None:
    """The scanned book among an item's files, or None if the item has no usable PDF."""
    files = metadata.get("files", []) if isinstance(metadata, dict) else []
    usable = [
        entry
        for entry in files
        if str(entry.get("name", "")).lower().endswith(".pdf")
        and not any(str(entry.get("name", "")).lower().endswith(s) for s in SIDECAR_SUFFIXES)
    ]
    preferred = [f for f in usable if str(f.get("name", "")).startswith(identifier)] or usable
    if not preferred:
        return None
    return max(preferred, key=lambda f: int(f.get("size") or 0))


def candidate_from(row: dict[str, Any], metadata: Any, *, selection_query: str) -> Candidate | None:
    identifier = str(row["identifier"])
    licence_name = stratify.license_from_url(str(row.get("licenseurl") or ""))
    if not stratify.is_acceptable_license(licence_name):
        return None

    pdf = choose_pdf(metadata, identifier)
    if pdf is None:
        return None

    languages = row.get("language") or []
    if isinstance(languages, str):
        languages = [languages]

    return Candidate(
        id=f"ia-{slugify(identifier, limit=45)}",
        title=str(row.get("title") or identifier),
        source_name=SOURCE_NAME,
        landing_url=f"https://archive.org/details/{identifier}",
        pdf_url=f"{DOWNLOAD}/{identifier}/{urllib.parse.quote(str(pdf['name']))}",
        license_name=str(licence_name),
        license_url=str(row.get("licenseurl") or ""),
        languages=tuple(str(item).strip().lower() for item in languages),
        selection_query=selection_query,
        size_hint=int(pdf.get("size") or 0) or None,
    )


def candidates(
    want: int,
    *,
    language: str | None = None,
    rows: int = 50,
    max_pages: int = 10,
    max_bytes: int | None = None,
) -> Iterator[Candidate]:
    """Yield up to `want` public-domain scans, skipping items whose PDF is absurdly large."""
    seen: set[str] = set()
    yielded = 0
    for licence_url in LICENCE_FILTERS:
        for page in range(1, max_pages + 1):
            if yielded >= want:
                return
            url = search_url(licence_url, rows=rows, page=page, language=language)
            found = parse_search(http.get_json(url))
            if not found:
                break
            for row in found:
                if yielded >= want:
                    return
                identifier = str(row["identifier"])
                if identifier in seen:
                    continue
                seen.add(identifier)
                try:
                    metadata = http.get_json(f"{METADATA}/{urllib.parse.quote(identifier)}")
                except http.HttpError:
                    continue
                candidate = candidate_from(row, metadata, selection_query=url)
                if candidate is None:
                    continue
                if max_bytes is not None and (candidate.size_hint or 0) > max_bytes:
                    continue
                yielded += 1
                yield candidate
