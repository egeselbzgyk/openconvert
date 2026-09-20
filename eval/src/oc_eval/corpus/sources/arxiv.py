"""arXiv CC-BY papers — real pdfTeX, at the scale the pdfTeX stratum needs.

TEST_CORPUS §7.6's ~15. The Atom search API does not report a licence, so this reads OAI-PMH
with the `arXivRaw` prefix instead, which carries `<license>` per record. A paper with no
licence element is under arXiv's own non-exclusive distribution licence — redistributable by
arXiv, not by us — so it is skipped rather than assumed.
"""

from __future__ import annotations

import urllib.parse
from collections.abc import Iterator
from xml.etree import ElementTree

from oc_eval.corpus import http, stratify
from oc_eval.corpus.sources import Candidate, slugify

SOURCE_NAME = "arXiv"
OAI = "https://export.arxiv.org/oai2"

OAI_NS = {
    "oai": "http://www.openarchives.org/OAI/2.0/",
    "raw": "http://arxiv.org/OAI/arXivRaw/",
}

# `xml.etree` expands internal entities, so a document with a DTD can cost exponential memory
# ("billion laughs") whatever its source. An OAI-PMH response has no legitimate reason to carry
# a document type declaration, so one is a refusal rather than something to parse carefully.
DOCTYPE_MARKERS = ("<!DOCTYPE", "<!ENTITY")


class UnsafeXml(ValueError):
    """The response carried a document type declaration. OAI-PMH responses do not."""


def safe_fromstring(xml_text: str) -> ElementTree.Element:
    head = xml_text[:4096].upper()
    for marker in DOCTYPE_MARKERS:
        if marker in head:
            raise UnsafeXml(f"refusing XML containing {marker}")
    return ElementTree.fromstring(xml_text)  # noqa: S314 - guarded above


def list_records_url(
    *, set_spec: str, from_date: str, until_date: str, token: str | None = None
) -> str:
    if token:
        params = [("verb", "ListRecords"), ("resumptionToken", token)]
    else:
        params = [
            ("verb", "ListRecords"),
            ("metadataPrefix", "arXivRaw"),
            ("set", set_spec),
            ("from", from_date),
            ("until", until_date),
        ]
    return f"{OAI}?{urllib.parse.urlencode(params)}"


def parse(xml_text: str, *, selection_query: str) -> tuple[list[Candidate], str | None]:
    """Candidates plus the resumption token, if the response carries one. Pure; no network."""
    root = safe_fromstring(xml_text)
    found: list[Candidate] = []

    for record in root.findall(".//oai:record", OAI_NS):
        candidate = _candidate_from(record, selection_query=selection_query)
        if candidate is not None:
            found.append(candidate)

    token_node = root.find(".//oai:resumptionToken", OAI_NS)
    token = (token_node.text or "").strip() if token_node is not None else ""
    return found, token or None


def _candidate_from(record: ElementTree.Element, *, selection_query: str) -> Candidate | None:
    meta = record.find(".//raw:arXivRaw", OAI_NS)
    if meta is None:
        return None

    licence_url = _text(meta, "raw:license")
    licence_name = stratify.license_from_url(licence_url)
    if not stratify.is_acceptable_license(licence_name):
        return None

    paper_id = _text(meta, "raw:id")
    if not paper_id:
        return None

    title = _text(meta, "raw:title") or paper_id
    return Candidate(
        id=f"arxiv-{slugify(paper_id, limit=30)}",
        title=" ".join(title.split()),
        source_name=SOURCE_NAME,
        landing_url=f"https://arxiv.org/abs/{paper_id}",
        pdf_url=f"https://arxiv.org/pdf/{paper_id}",
        license_name=str(licence_name),
        license_url=licence_url or "",
        languages=("en",),
        selection_query=selection_query,
    )


def _text(node: ElementTree.Element, path: str) -> str:
    found = node.find(path, OAI_NS)
    return (found.text or "").strip() if found is not None and found.text else ""


def candidates(
    want: int,
    *,
    set_spec: str = "cs",
    from_date: str = "2025-01-01",
    until_date: str = "2025-01-15",
    max_requests: int = 12,
) -> Iterator[Candidate]:
    """Yield up to `want` CC-licensed papers, following OAI resumption tokens."""
    token: str | None = None
    yielded = 0
    seen: set[str] = set()

    for _ in range(max_requests):
        if yielded >= want:
            return
        url = list_records_url(
            set_spec=set_spec, from_date=from_date, until_date=until_date, token=token
        )
        batch, token = parse(http.get_text(url), selection_query=url)
        for candidate in batch:
            if yielded >= want:
                return
            if candidate.id in seen:
                continue
            seen.add(candidate.id)
            yielded += 1
            yield candidate
        if token is None:
            return
