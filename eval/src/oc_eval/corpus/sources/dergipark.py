"""Turkish CC-BY journal articles — the Turkish slice TEST_CORPUS §7.6 refuses to let go.

§7.6 is explicit that the Turkish and German slices are not optional filler: they are the only
real-document evidence the pipeline gets for the two non-English languages it claims. Most of
them are on DergiPark, and they are real InDesign and Word output set in a language whose
dotted and dotless i break naive case folding (R10 §6.3).

The licence comes from DOAJ's **journal** records rather than from an article page, because
that is where it is curated: a journal record carries `license: [{type: "CC BY", url: …}]`,
and roughly one Turkish journal in seven qualifies — the rest are CC BY-NC or CC BY-NC-ND,
which this corpus cannot redistribute. Selecting journals first and articles second does the
licence filtering once per journal instead of once per article, and does it against metadata
someone curated rather than against whatever an article page happens to link. An earlier
version of this adapter read the licence off the page and admitted one article in ten.

DOAJ's `fulltext` link for these journals is the direct PDF, so nothing here scrapes a page.
"""

from __future__ import annotations

import urllib.parse
from collections.abc import Iterator
from typing import Any

from oc_eval.corpus import stratify
from oc_eval.corpus.sources import Candidate, slugify

SOURCE_NAME = "DergiPark"
DOAJ_ARTICLES = "https://doaj.org/api/search/articles"
DOAJ_JOURNALS = "https://doaj.org/api/search/journals"

# DOAJ records the publisher's country, not the journal's language.
TURKISH_PUBLISHER_QUERY = "bibjson.publisher.country:TR"

# The licence types DOAJ writes that TEST_CORPUS §7.1's allowlist accepts.
ACCEPTED_LICENSE_TYPES = ("CC BY", "CC BY-SA")


def journals_url(query: str, *, page: int, page_size: int) -> str:
    return f"{DOAJ_JOURNALS}/{urllib.parse.quote(query)}?page={page}&pageSize={page_size}"


def articles_url(issn: str, *, page: int, page_size: int) -> str:
    query = f'bibjson.journal.issns:"{issn}"'
    return f"{DOAJ_ARTICLES}/{urllib.parse.quote(query)}?page={page}&pageSize={page_size}"


def parse_journals(payload: Any) -> list[dict[str, str]]:
    """The CC-BY and CC-BY-SA journals of one DOAJ response, as {issn, license_url}. Pure."""
    found: list[dict[str, str]] = []
    for result in (payload.get("results", []) if isinstance(payload, dict) else []) or []:
        bibjson = result.get("bibjson") or {}
        licence = _acceptable_licence(bibjson)
        if licence is None:
            continue
        issn = bibjson.get("pissn") or bibjson.get("eissn")
        if not issn:
            continue
        found.append(
            {
                "issn": str(issn),
                "license_url": str(licence.get("url") or ""),
                "title": " ".join(str(bibjson.get("title") or "").split()),
            }
        )
    return found


def _acceptable_licence(bibjson: Any) -> dict[str, Any] | None:
    for licence in bibjson.get("license") or []:
        if licence.get("type") in ACCEPTED_LICENSE_TYPES:
            return dict(licence)
    return None


def parse_articles(payload: Any, journal: dict[str, str]) -> list[Candidate]:
    """Articles of one journal whose fulltext link is a PDF we can name. Pure; no network."""
    licence_name = stratify.license_from_url(journal["license_url"])
    if not stratify.is_acceptable_license(licence_name):
        return []

    found: list[Candidate] = []
    for result in (payload.get("results", []) if isinstance(payload, dict) else []) or []:
        bibjson = result.get("bibjson") or {}
        url = _fulltext_url(bibjson)
        if url is None:
            continue
        found.append(
            Candidate(
                id=f"tr-{slugify(_identifier(url), limit=40)}",
                title=" ".join(str(bibjson.get("title") or "").split()) or url,
                source_name=SOURCE_NAME if "dergipark" in url else "DOAJ (TR)",
                landing_url=url,
                pdf_url=url,
                license_name=str(licence_name),
                license_url=journal["license_url"],
                languages=("tr",),
                selection_query=f"{TURKISH_PUBLISHER_QUERY} / issn {journal['issn']}",
            )
        )
    return found


# DergiPark's direct-PDF path. A record sometimes carries both this and a landing page, and
# the landing page is HTML that the probe would reject after paying for the download.
DIRECT_PDF_MARKER = "/download/article-file/"


def _fulltext_url(bibjson: Any) -> str | None:
    links = [
        str(link.get("url") or "")
        for link in bibjson.get("link") or []
        if str(link.get("url") or "").startswith("https://")
    ]
    direct = [url for url in links if DIRECT_PDF_MARKER in url]
    if direct:
        return direct[0]
    return links[0] if links else None


def _identifier(url: str) -> str:
    """A stable id from the URL's own path — an article-file number where there is one."""
    path = urllib.parse.urlsplit(url).path.strip("/")
    return path.replace("/", "-") or url


def candidates(
    want: int,
    *,
    journal_pages: int = 8,
    journal_page_size: int = 100,
    articles_per_journal: int = 3,
) -> Iterator[Candidate]:
    """Yield up to `want` articles from Turkish journals DOAJ records as CC BY or CC BY-SA.

    A few articles per journal rather than many from one: a stratum that is one journal's
    house style is evidence about that journal, not about Turkish typesetting.
    """
    from oc_eval.corpus import http

    seen: set[str] = set()
    yielded = 0
    for page in range(1, journal_pages + 1):
        if yielded >= want:
            return
        try:
            journals = parse_journals(
                http.get_json(
                    journals_url(TURKISH_PUBLISHER_QUERY, page=page, page_size=journal_page_size)
                )
            )
        except http.HttpError:
            return
        if not journals:
            return
        for journal in journals:
            if yielded >= want:
                return
            try:
                payload = http.get_json(
                    articles_url(journal["issn"], page=1, page_size=articles_per_journal)
                )
            except http.HttpError:
                continue
            for candidate in parse_articles(payload, journal):
                if yielded >= want:
                    return
                if candidate.id in seen:
                    continue
                seen.add(candidate.id)
                yielded += 1
                yield candidate
