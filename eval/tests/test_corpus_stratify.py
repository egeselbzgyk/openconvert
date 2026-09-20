"""PHASE 7: turning what a PDF says about itself into the stratum a report groups it by.

D18 stratifies on `/Producer` + `/Creator` and reports per stratum, never in aggregate, so the
mapping from a producer string to a stratum is the spine of every number Phase 7 produces. It
is a pure function of the two strings and is tested as one — no network, no files.
"""

from __future__ import annotations

import pytest

from oc_eval.corpus import stratify
from oc_eval.corpus.manifest import STRATA


@pytest.mark.parametrize(
    ("producer", "expected"),
    [
        ("pdfTeX-1.40.25", "pdfTeX"),
        ("XeTeX 0.999995", "pdfTeX"),
        ("LuaTeX-1.16.0", "pdfTeX"),
        ("pdfTeX, Version 3.141592653-2.6-1.40.24", "pdfTeX"),
        ("Adobe InDesign 19.0 (Macintosh)", "InDesign"),
        ("Adobe InDesign CS6 (Windows)", "InDesign"),
        ("Microsoft® Word for Microsoft 365", "Word"),
        ("Microsoft Word 2016", "Word"),
        ("GPL Ghostscript 10.02.1", "Ghostscript"),
        ("AFPL Ghostscript 8.54", "Ghostscript"),
        ("QuarkXPress(R) 17.0", "Quark"),
        ("ABBYY FineReader 14", "ABBYY-scanner"),
        ("LuraDocument PDF Compressor", "ABBYY-scanner"),
        ("Canon iR-ADV C5535", "ABBYY-scanner"),
        ("", "unknown"),
        ("Some Bespoke Publishing System 2.1", "unknown"),
    ],
)
def test_a_producer_string_lands_in_its_stratum(producer: str, expected: str) -> None:
    assert stratify.classify_producer(producer) == expected
    assert stratify.classify_producer(producer) in STRATA


def test_the_creator_is_consulted_when_the_producer_says_nothing_useful() -> None:
    """Distiller and Ghostscript overwrite `/Producer`; `/Creator` is what made the layout."""
    assert stratify.classify_producer("Acrobat Distiller 11.0", creator="Adobe InDesign 19.0") == (
        "InDesign"
    )
    assert stratify.classify_producer("GPL Ghostscript 10.0", creator="LaTeX with hyperref") == (
        "pdfTeX"
    )


def test_a_producer_the_creator_cannot_rescue_stays_unknown() -> None:
    assert stratify.classify_producer("Acrobat Distiller 11.0", creator="") == "unknown"


def test_our_own_renderers_are_only_ours_when_the_file_is_ours() -> None:
    """D18's `ours(*)` means we rendered it. A real book someone else set in Typst is not ours."""
    assert stratify.classify_producer("Typst 0.15.1") == "ours(Typst)"
    assert stratify.classify_producer("WeasyPrint 63") == "ours(WeasyPrint)"
    assert stratify.classify_producer("Typst 0.15.1", real_world=True) == "unknown"
    assert stratify.classify_producer("WeasyPrint 63", real_world=True) == "unknown"


@pytest.mark.parametrize(
    ("url", "expected"),
    [
        ("https://creativecommons.org/licenses/by/4.0/", "CC-BY-4.0"),
        ("http://creativecommons.org/licenses/by/3.0", "CC-BY-3.0"),
        ("https://creativecommons.org/licenses/by-sa/4.0/", "CC-BY-SA-4.0"),
        ("https://creativecommons.org/licenses/by-sa/3.0/", "CC-BY-SA-3.0"),
        ("https://creativecommons.org/publicdomain/zero/1.0/", "CC0-1.0"),
        ("https://creativecommons.org/publicdomain/mark/1.0/", "PD-old-work"),
    ],
)
def test_a_creative_commons_url_becomes_the_identifier_the_allowlist_uses(
    url: str, expected: str
) -> None:
    assert stratify.license_from_url(url) == expected


def test_a_non_commercial_or_no_derivatives_url_keeps_a_name_the_lint_will_block() -> None:
    """The harvester does not silently drop these: it records them so the gate rejects them."""
    assert stratify.license_from_url("https://creativecommons.org/licenses/by-nc/4.0/") == (
        "CC-BY-NC-4.0"
    )
    assert stratify.license_from_url("https://creativecommons.org/licenses/by-nc-nd/4.0/") == (
        "CC-BY-NC-ND-4.0"
    )
    assert stratify.license_from_url("https://creativecommons.org/licenses/by-nd/4.0/") == (
        "CC-BY-ND-4.0"
    )


def test_a_url_that_is_not_a_licence_is_not_guessed_at() -> None:
    assert stratify.license_from_url("https://example.org/terms") is None
    assert stratify.license_from_url("") is None


def test_the_harvest_only_accepts_licences_the_corpus_can_redistribute() -> None:
    assert stratify.is_acceptable_license("CC-BY-4.0")
    assert stratify.is_acceptable_license("PD-US-Gov")
    assert not stratify.is_acceptable_license("CC-BY-NC-4.0")
    assert not stratify.is_acceptable_license("EUPL-1.2")
    assert not stratify.is_acceptable_license(None)
