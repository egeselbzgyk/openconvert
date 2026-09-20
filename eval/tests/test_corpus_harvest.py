"""PHASE 7: admission. Nothing enters the manifest on a source's promise.

The harvest downloads a candidate, opens it, and records the digest of the bytes that actually
arrived, the page count the file actually has, and the producer the file actually names. These
tests drive it with an injected opener over a committed fixture, so the whole admission path
runs — fetch, probe, classify, entry, merge, lint — without a connection.
"""

from __future__ import annotations

import io
from collections.abc import Callable, Iterator
from pathlib import Path
from typing import Any

import pikepdf

from oc_eval.corpus import harvest
from oc_eval.corpus import manifest as mf
from oc_eval.corpus import probe as probe_mod
from oc_eval.corpus.lint import lint, rules_fired
from oc_eval.corpus.sources import Candidate

TODAY = "2026-09-20"


def a_real_pdf(*, pages: int = 6, producer: str = "Adobe InDesign 19.0") -> bytes:
    """A genuine multi-page PDF, built here rather than borrowed from a fixture.

    The admission path cares about page count and `/Producer`, so a test that wants a
    four-page InDesign book should say so instead of depending on whichever committed
    fixture happens to sort first.
    """
    pdf = pikepdf.Pdf.new()
    for _ in range(pages):
        pdf.add_blank_page(page_size=(612, 792))
    pdf.docinfo["/Producer"] = producer
    buffer = io.BytesIO()
    pdf.save(buffer)
    return buffer.getvalue()


def candidate(ident: str = "oapen-x", **overrides: Any) -> Candidate:
    base: dict[str, Any] = {
        "id": ident,
        "title": "A Monograph",
        "source_name": "OAPEN",
        "landing_url": f"https://library.oapen.org/handle/{ident}",
        "pdf_url": f"https://library.oapen.org/bitstream/{ident}.pdf",
        "license_name": "CC-BY-4.0",
        "license_url": "https://creativecommons.org/licenses/by/4.0/",
        "languages": ("de",),
        "selection_query": "dc.rights.uri contains by/4.0",
    }
    base.update(overrides)
    return Candidate(**base)


def serving(payload: bytes) -> Callable[[str], Iterator[bytes]]:
    def opener(_url: str) -> Iterator[bytes]:
        return iter([payload])

    return opener


def admit_one(tmp_path: Path, item: Candidate, payload: bytes) -> harvest.HarvestReport:
    return harvest.admit(
        [item], dest_dir=tmp_path, today=TODAY, opener=serving(payload), max_file_bytes=1 << 30
    )


# --------------------------------------------------------------------------- the entry


def test_an_entry_records_what_the_bytes_said_not_what_the_source_promised() -> None:
    """A candidate carries a page hint. The entry carries the page count the file has."""
    seen = probe_mod.Probe(
        pages=219,
        producer="Adobe InDesign 19.0",
        creator="",
        tagged=False,
        encrypted=False,
        pdf_version="1.6",
    )

    entry = harvest.entry_from(
        candidate(pages_hint=999),
        probe=seen,
        digest="a" * 64,
        size_bytes=4096,
        today=TODAY,
        verifier="oc-eval corpus harvest",
    )

    assert entry["pages"] == 219
    assert entry["producer_stratum"] == "InDesign"
    assert entry["producer_raw"] == "Adobe InDesign 19.0"
    assert entry["sha256"] == "a" * 64
    assert entry["file_size_bytes"] == 4096


def test_a_harvested_entry_is_frozen_into_the_holdout_at_admission() -> None:
    """TEST_CORPUS §7.6: membership is decided once, at the moment the document is admitted."""
    entry = harvest.entry_from(
        candidate(),
        probe=probe_mod.Probe(10, "pdfTeX-1.40", "", False, False, "1.5"),
        digest="b" * 64,
        size_bytes=1,
        today=TODAY,
        verifier="v",
    )

    assert entry["holdout"] is True
    assert entry["generator"] == "n/a-real-world"
    assert entry["ground_truth_type"] == "none"


def test_a_real_book_set_in_our_own_renderer_is_not_counted_as_ours() -> None:
    """D18's `ours(*)` means we rendered it, and the holdout is the real-world holdout."""
    entry = harvest.entry_from(
        candidate(),
        probe=probe_mod.Probe(10, "Typst 0.15.1", "", False, False, "1.7"),
        digest="c" * 64,
        size_bytes=1,
        today=TODAY,
        verifier="v",
    )

    assert entry["producer_stratum"] == "unknown"


def test_a_scan_is_marked_difficult_and_carries_its_text_layer_as_a_problem() -> None:
    entry = harvest.entry_from(
        candidate(),
        probe=probe_mod.Probe(120, "ABBYY FineReader 14", "", False, False, "1.4"),
        digest="d" * 64,
        size_bytes=1,
        today=TODAY,
        verifier="v",
    )

    assert entry["category"] == "difficult"
    assert "ocr-text-layer" in entry["expected_problems"]
    assert entry["difficulty"] >= 3


# --------------------------------------------------------------------------- admission


def test_a_candidate_is_admitted_once_its_bytes_have_been_seen(tmp_path: Path) -> None:
    payload = a_real_pdf()

    report = admit_one(tmp_path, candidate(), payload)

    assert len(report.admitted) == 1
    assert report.rejected == []
    assert report.bytes_downloaded == len(payload)
    assert (tmp_path / "oapen-x.pdf").read_bytes() == payload


def test_a_candidate_whose_licence_is_not_acceptable_is_rejected_by_name(tmp_path: Path) -> None:
    report = admit_one(tmp_path, candidate(license_name="CC-BY-NC-4.0"), a_real_pdf())

    assert report.admitted == []
    assert "licence" in report.reasons
    assert "CC-BY-NC-4.0" in str(report.rejected[0])


def test_a_file_that_is_not_a_pdf_is_rejected_and_not_left_on_disk(tmp_path: Path) -> None:
    report = admit_one(tmp_path, candidate(), b"<!DOCTYPE html><title>404</title>")

    assert report.admitted == []
    assert "not-a-pdf" in report.reasons
    assert not (tmp_path / "oapen-x.pdf").exists()


def test_a_file_over_the_per_file_budget_is_rejected_before_it_is_probed(tmp_path: Path) -> None:
    report = harvest.admit(
        [candidate(size_hint=500 * 1024 * 1024)],
        dest_dir=tmp_path,
        today=TODAY,
        opener=serving(a_real_pdf()),
    )

    assert report.admitted == []
    assert "too-large" in report.reasons
    assert not (tmp_path / "oapen-x.pdf").exists(), "a rejection on the hint costs no download"


def test_a_document_already_in_the_manifest_is_not_harvested_twice(tmp_path: Path) -> None:
    report = harvest.admit(
        [candidate()],
        dest_dir=tmp_path,
        today=TODAY,
        opener=serving(a_real_pdf()),
        known_ids={"oapen-x"},
    )

    assert report.admitted == []
    assert "duplicate" in report.reasons


def test_the_total_budget_stops_the_harvest_rather_than_being_exceeded(tmp_path: Path) -> None:
    payload = a_real_pdf()
    wanted = [candidate(f"oapen-{i}") for i in range(4)]

    report = harvest.admit(
        wanted,
        dest_dir=tmp_path,
        today=TODAY,
        opener=serving(payload),
        max_total_bytes=len(payload) + 1,
    )

    assert len(report.admitted) <= 2
    assert "budget" in report.reasons


# --------------------------------------------------------------------------- merge and gate


def test_merge_keeps_the_entry_already_in_the_manifest() -> None:
    """The holdout is frozen: a later harvest may not rewrite an admission that happened."""
    first = harvest.entry_from(
        candidate(),
        probe=probe_mod.Probe(10, "pdfTeX", "", False, False, "1.5"),
        digest="e" * 64,
        size_bytes=1,
        today="2026-01-01",
        verifier="v",
    )
    second = dict(first, sha256="f" * 64, pages=999)

    merged = harvest.merge(mf.from_mapping({"schema_version": 1, "files": [first]}), [second])

    assert len(merged.entries) == 1
    assert merged.entries[0].raw["sha256"] == "e" * 64
    assert merged.entries[0].raw["pages"] == 10


def test_a_harvested_entry_satisfies_every_rule_the_lint_applies_to_one(tmp_path: Path) -> None:
    """Whatever else the corpus is short of, an admitted entry is never itself a finding."""
    report = admit_one(tmp_path, candidate(), a_real_pdf())
    merged = harvest.merge(mf.from_mapping({"schema_version": 1, "files": []}), report.admitted)

    per_entry = [f for f in lint(merged) if f.entry_id is not None]

    assert per_entry == [], f"a harvested entry tripped a rule: {[str(f) for f in per_entry]}"
    assert "holdout-below-minimum" in rules_fired(lint(merged)), "one document is not a holdout"


def test_the_plan_parser_reads_a_sourcing_target() -> None:
    assert list(harvest.plan_from("oapen=40,ia=20,arxiv=15")) == [
        ("oapen", 40),
        ("ia", 20),
        ("arxiv", 15),
    ]


def test_admit_stops_at_the_number_asked_for(tmp_path: Path) -> None:
    """A source is asked for more than is needed; `want` is what stops the download."""
    payload = a_real_pdf()
    offered = [candidate(f"oapen-{i}") for i in range(10)]

    report = harvest.admit(offered, dest_dir=tmp_path, today=TODAY, opener=serving(payload), want=3)

    assert len(report.admitted) == 3
    assert len(list(tmp_path.glob("*.pdf"))) == 3


def test_a_top_up_run_reaches_past_what_an_earlier_harvest_took(tmp_path: Path) -> None:
    """The duplicates are skipped and the cap counts only what this call actually admitted."""
    payload = a_real_pdf()
    offered = [candidate(f"oapen-{i}") for i in range(10)]

    report = harvest.admit(
        offered,
        dest_dir=tmp_path,
        today=TODAY,
        opener=serving(payload),
        known_ids={f"oapen-{i}" for i in range(4)},
        want=3,
    )

    assert [entry["id"] for entry in report.admitted] == ["oapen-4", "oapen-5", "oapen-6"]
    assert report.reasons["duplicate"] == 4


def test_the_ask_clears_what_an_earlier_harvest_already_took() -> None:
    """A source re-walks its catalogue from the start, so the ask has to get past the held."""
    assert harvest.ask_for(2, already_held=0) == 2 * harvest.CANDIDATE_OVERSUPPLY
    assert harvest.ask_for(2, already_held=113) > 113
