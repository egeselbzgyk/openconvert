"""PHASE 7: the five source adapters, tested on their catalogues' payloads and not on the net.

Each adapter splits into a pure parser over whatever its catalogue returns and a thin fetch
loop around it. The parser is where every decision lives — which bitstream is the book, which
licence the record actually carries, which file is the scan rather than its OCR sidecar — so it
is the parser that is tested, with payloads shaped like the ones the live APIs return.
"""

from __future__ import annotations

from oc_eval.corpus.sources import arxiv, dergipark, internet_archive, oapen, slugify, usgov
from oc_eval.corpus.sources.registry import DEFAULT_PLAN, SOURCES

# --------------------------------------------------------------------------- shared


def test_a_slug_is_ascii_lowercase_and_survives_a_turkish_title() -> None:
    assert slugify("Kültürel Çalışmalar: Bir Giriş") == "k-lt-rel-al-malar-bir-giri"
    assert slugify("") == "untitled"
    assert slugify("---") == "untitled"


# --------------------------------------------------------------------------- OAPEN


def oapen_item(*, rights: str, bitstreams: list[dict[str, object]]) -> dict[str, object]:
    return {
        "handle": "20.500.12657/115627",
        "name": "Ein Buch",
        "metadata": [
            {"key": "dc.title", "value": "Ein Buch"},
            {"key": "dc.language", "value": "German"},
            {"key": "dc.rights.uri", "value": rights},
            {"key": "oapen.pages", "value": "219"},
        ],
        "bitstreams": bitstreams,
    }


PDF_BITSTREAM = {
    "name": "book.pdf",
    "mimeType": "application/pdf",
    "sizeBytes": 3844613,
    "retrieveLink": "/rest/bitstreams/abc/retrieve",
}


def test_oapen_takes_the_book_and_leaves_the_thumbnail_and_the_text_sidecar() -> None:
    item = oapen_item(
        rights="https://creativecommons.org/licenses/by/4.0/",
        bitstreams=[
            {
                "name": "book.pdf.jpg",
                "mimeType": "image/jpeg",
                "sizeBytes": 9,
                "retrieveLink": "/j",
            },
            {
                "name": "book.pdf.txt",
                "mimeType": "text/plain",
                "sizeBytes": 99,
                "retrieveLink": "/t",
            },
            PDF_BITSTREAM,
        ],
    )

    found = oapen.parse({"items": [item]}, selection_query="q")

    assert len(found) == 1
    assert found[0].pdf_url.endswith("/rest/bitstreams/abc/retrieve")
    assert found[0].license_name == "CC-BY-4.0"
    assert found[0].languages == ("de",)
    assert found[0].pages_hint == 219
    assert found[0].size_hint == 3844613


def test_oapen_drops_a_record_whose_rights_uri_is_not_redistributable() -> None:
    item = oapen_item(
        rights="https://creativecommons.org/licenses/by-nc-nd/4.0/", bitstreams=[PDF_BITSTREAM]
    )

    assert oapen.parse({"items": [item]}, selection_query="q") == []


def test_oapen_drops_a_record_with_no_pdf_at_all() -> None:
    item = oapen_item(
        rights="https://creativecommons.org/licenses/by/4.0/",
        bitstreams=[{"name": "cover.jpg", "mimeType": "image/jpeg", "retrieveLink": "/c"}],
    )

    assert oapen.parse({"items": [item]}, selection_query="q") == []


def test_oapen_asks_the_catalogue_for_the_language_it_wants() -> None:
    url = oapen.query_url("licenses/by/4.0", limit=10, offset=0, language="German")

    assert "dc.rights.uri" in url
    assert "dc.language" in url
    assert "German" in url


# --------------------------------------------------------------------------- Internet Archive


def test_internet_archive_prefers_the_scan_over_its_derived_sidecars() -> None:
    metadata = {
        "files": [
            {"name": "book_text.pdf", "size": "9000000"},
            {"name": "book_djvu.pdf", "size": "8000000"},
            {"name": "book.pdf", "size": "5000000"},
            {"name": "book_thumb.jpg", "size": "900"},
        ]
    }

    chosen = internet_archive.choose_pdf(metadata, "book")

    assert chosen is not None
    assert chosen["name"] == "book.pdf"


def test_internet_archive_falls_back_when_nothing_carries_the_identifier() -> None:
    metadata = {
        "files": [{"name": "scan-01.pdf", "size": "5"}, {"name": "scan-02.pdf", "size": "9"}]
    }

    chosen = internet_archive.choose_pdf(metadata, "book")

    assert chosen is not None
    assert chosen["name"] == "scan-02.pdf"


def test_internet_archive_only_admits_a_public_domain_rights_statement() -> None:
    metadata = {"files": [{"name": "book.pdf", "size": "5000000"}]}
    row = {
        "identifier": "book",
        "title": "A Scan",
        "language": "German",
        "licenseurl": "http://creativecommons.org/publicdomain/mark/1.0/",
    }

    admitted = internet_archive.candidate_from(row, metadata, selection_query="q")
    refused = internet_archive.candidate_from(
        dict(row, licenseurl="http://example.org/rights"), metadata, selection_query="q"
    )

    assert admitted is not None
    assert admitted.license_name == "PD-old-work"
    assert admitted.languages == ("german",)
    assert refused is None


# --------------------------------------------------------------------------- arXiv

OAI_RESPONSE = """<?xml version="1.0" encoding="UTF-8"?>
<OAI-PMH xmlns="http://www.openarchives.org/OAI/2.0/">
  <ListRecords>
    <record><metadata>
      <arXivRaw xmlns="http://arxiv.org/OAI/arXivRaw/">
        <id>2501.01234</id>
        <title>A Paper   With  Loose
        Spacing</title>
        <license>http://creativecommons.org/licenses/by/4.0/</license>
      </arXivRaw>
    </metadata></record>
    <record><metadata>
      <arXivRaw xmlns="http://arxiv.org/OAI/arXivRaw/">
        <id>2501.09999</id>
        <title>No Licence Here</title>
      </arXivRaw>
    </metadata></record>
    <resumptionToken>tok-123</resumptionToken>
  </ListRecords>
</OAI-PMH>"""


def test_arxiv_takes_the_licensed_record_and_leaves_the_one_with_no_licence() -> None:
    found, token = arxiv.parse(OAI_RESPONSE, selection_query="q")

    assert [c.id for c in found] == ["arxiv-2501-01234"]
    assert found[0].pdf_url == "https://arxiv.org/pdf/2501.01234"
    assert found[0].title == "A Paper With Loose Spacing"
    assert token == "tok-123"


def test_arxiv_refuses_xml_that_declares_entities() -> None:
    """`xml.etree` expands internal entities, so a DTD is a refusal rather than a parse."""
    bomb = '<?xml version="1.0"?><!DOCTYPE lolz [<!ENTITY lol "lol">]><OAI-PMH/>'

    try:
        arxiv.parse(bomb, selection_query="q")
    except arxiv.UnsafeXml:
        return
    raise AssertionError("a document type declaration was parsed instead of refused")


# --------------------------------------------------------------------------- US-Gov


def ntrs_record(determination: str, *, with_pdf: bool = True) -> dict[str, object]:
    record: dict[str, object] = {
        "id": 19740009451,
        "title": "A  Technical\n Report",
        "copyright": {"determinationType": determination},
    }
    if with_pdf:
        record["downloads"] = [{"links": {"pdf": "/api/citations/19740009451/downloads/r.pdf"}}]
    return record


def test_usgov_admits_only_the_government_public_use_determination() -> None:
    payload = {
        "results": [
            ntrs_record("GOV_PUBLIC_USE_PERMITTED"),
            ntrs_record("PUBLIC_USE_PERMITTED"),
            ntrs_record("MAY_INCLUDE_COPYRIGHT_MATERIAL"),
            ntrs_record("OTHER"),
        ]
    }

    found = usgov.parse(payload, selection_query="q")

    assert len(found) == 1
    assert found[0].license_name == "PD-US-Gov"
    assert found[0].title == "A Technical Report"


def test_usgov_drops_a_record_with_no_pdf_to_download() -> None:
    payload = {"results": [ntrs_record("GOV_PUBLIC_USE_PERMITTED", with_pdf=False)]}

    assert usgov.parse(payload, selection_query="q") == []


# --------------------------------------------------------------------------- Turkish

JOURNALS_PAYLOAD = {
    "results": [
        {
            "bibjson": {
                "title": "Bir CC BY Dergisi",
                "pissn": "2587-2559",
                "license": [
                    {"type": "CC BY", "url": "https://creativecommons.org/licenses/by/4.0/"}
                ],
            }
        },
        {
            "bibjson": {
                "title": "Bir CC BY-NC Dergisi",
                "pissn": "1304-7639",
                "license": [
                    {"type": "CC BY-NC", "url": "https://creativecommons.org/licenses/by-nc/4.0/"}
                ],
            }
        },
        {"bibjson": {"title": "Lisanssiz", "pissn": "0000-0000", "license": []}},
    ]
}


def test_the_turkish_slice_selects_journals_by_their_curated_licence() -> None:
    """Roughly one Turkish journal in seven is CC BY; the rest are NC or NC-ND."""
    found = dergipark.parse_journals(JOURNALS_PAYLOAD)

    assert [row["issn"] for row in found] == ["2587-2559"]
    assert found[0]["license_url"] == "https://creativecommons.org/licenses/by/4.0/"


def test_a_journal_with_no_issn_cannot_be_asked_for_its_articles() -> None:
    payload = {
        "results": [
            {
                "bibjson": {
                    "title": "No ISSN",
                    "license": [
                        {"type": "CC BY", "url": "https://creativecommons.org/licenses/by/4.0/"}
                    ],
                }
            }
        ]
    }

    assert dergipark.parse_journals(payload) == []


def test_an_articles_response_becomes_candidates_under_the_journal_s_licence() -> None:
    journal = {
        "issn": "2587-2559",
        "license_url": "https://creativecommons.org/licenses/by/4.0/",
        "title": "Bir Dergi",
    }
    payload = {
        "results": [
            {
                "bibjson": {
                    "title": "TÜRKİYE'DE   BİR\n MAKALE",
                    "link": [{"url": "https://dergipark.org.tr/tr/download/article-file/2201587"}],
                }
            }
        ]
    }

    found = dergipark.parse_articles(payload, journal)

    assert len(found) == 1
    assert found[0].id == "tr-tr-download-article-file-2201587"
    assert found[0].license_name == "CC-BY-4.0"
    assert found[0].languages == ("tr",)
    assert found[0].title == "TÜRKİYE'DE BİR MAKALE"


def test_the_direct_pdf_link_wins_over_a_landing_page() -> None:
    """A landing page is HTML the probe would reject after paying for the download."""
    journal = {
        "issn": "1",
        "license_url": "https://creativecommons.org/licenses/by/4.0/",
        "title": "t",
    }
    payload = {
        "results": [
            {
                "bibjson": {
                    "title": "t",
                    "link": [
                        {"url": "https://dergipark.org.tr/tr/pub/iid/issue/49437/521716"},
                        {"url": "https://dergipark.org.tr/tr/download/article-file/521716"},
                    ],
                }
            }
        ]
    }

    found = dergipark.parse_articles(payload, journal)

    assert found[0].pdf_url.endswith("/download/article-file/521716")


def test_a_journal_whose_licence_url_is_not_on_the_allowlist_yields_nothing() -> None:
    journal = {
        "issn": "1",
        "license_url": "https://creativecommons.org/licenses/by-nc-nd/4.0/",
        "title": "t",
    }
    payload = {"results": [{"bibjson": {"title": "t", "link": [{"url": "https://x/y.pdf"}]}}]}

    assert dergipark.parse_articles(payload, journal) == []


def test_the_turkish_adapter_asks_doaj_for_turkish_publishers() -> None:
    url = dergipark.journals_url(dergipark.TURKISH_PUBLISHER_QUERY, page=1, page_size=100)

    assert "publisher.country" in url
    assert "TR" in url


# --------------------------------------------------------------------------- the plan


def test_every_source_the_default_plan_names_is_registered() -> None:
    """§7.6's sourcing target is executed, not aspired to, so a typo in it is a test failure."""
    from oc_eval.corpus.harvest import plan_from

    for name, want in plan_from(DEFAULT_PLAN):
        assert name in SOURCES, f"{name!r} is in the default plan and not in the registry"
        assert want > 0


def test_the_default_plan_asks_for_at_least_the_hundred_documents_the_holdout_needs() -> None:
    from oc_eval.corpus.harvest import plan_from
    from oc_eval.corpus.manifest import holdout_minimum

    assert sum(want for _, want in plan_from(DEFAULT_PLAN)) >= holdout_minimum()


def test_the_default_plan_sources_the_german_and_turkish_slices_explicitly() -> None:
    from oc_eval.corpus.harvest import plan_from

    named = {name for name, _ in plan_from(DEFAULT_PLAN)}

    assert "oapen-de" in named, "German is sourced, not hoped for (TEST_CORPUS §7.6)"
    assert "dergipark" in named, "Turkish is sourced, not hoped for (TEST_CORPUS §7.6)"
