"""PHASE 7 row 7.14: the nightly full-corpus run and the report it has to produce.

TEST_STRATEGY §8: one structured record per `(corpus file, metric)`, never only a rolled-up
score, and per stratum. The conversion is driven through the shipped binary as a subprocess,
because the thing under measurement is what a user gets and driving the library from Python
would measure a second driver nobody ships (D13.1) — so these tests inject the runner and
exercise everything around it.

The rule the tests are really about: **a file the manifest names and the download directory
does not hold is reported as missing, never skipped.** A nightly that silently scored the
forty files it happened to have is a nightly whose number means something different every
night, and the trend built on it is worthless.
"""

from __future__ import annotations

import json
from pathlib import Path

from oc_eval import run as run_mod

REPO_ROOT = Path(__file__).resolve().parents[2]
MANIFEST = REPO_ROOT / "corpus" / "manifest.json"


def fake_runner(measured: dict[str, float] | None = None):
    def convert(_binary: Path, _source: Path) -> dict[str, float]:
        return dict(measured or {run_mod.METRIC_SECONDS_PER_PAGE: 0.02})

    return convert


def a_corpus_of(tmp_path: Path, ids: list[str]) -> Path:
    """A downloads directory holding a file for each of `ids`."""
    downloads = tmp_path / "downloads"
    downloads.mkdir()
    for ident in ids:
        (downloads / f"{ident}.pdf").write_bytes(b"%PDF-1.7\n")
    return downloads


def some_manifest_ids(count: int) -> list[str]:
    from oc_eval.corpus import manifest as mf

    return [entry.id for entry in mf.load(MANIFEST).entries[:count]]


# --------------------------------------------------------------------------- row 7.14


def test_nightly_full_corpus_runs_and_reports(tmp_path: Path) -> None:
    """Row 7.14: the run produces a report with per-file, per-metric records."""
    ids = some_manifest_ids(3)
    downloads = a_corpus_of(tmp_path, ids)

    outcome = run_mod.run(
        Path("openconvert"),
        manifest_path=MANIFEST,
        downloads=downloads,
        limit=3,
        runner=fake_runner(),
    )
    built = run_mod.write_report(outcome, tmp_path / "report.json")

    assert (tmp_path / "report.json").exists()
    on_disk = json.loads((tmp_path / "report.json").read_text(encoding="utf-8"))

    assert on_disk == built
    assert on_disk["per_file"], "the report has no per-file records"
    assert {row["file_id"] for row in on_disk["per_file"]} == set(ids)
    assert {row["metric"] for row in on_disk["per_file"]} >= {
        run_mod.METRIC_CONVERTED,
        run_mod.METRIC_SECONDS_PER_PAGE,
    }
    assert on_disk["per_stratum"], "the report has no per-stratum rows"
    assert "aggregate" not in on_disk


def test_a_file_the_corpus_names_and_does_not_have_is_reported_missing(tmp_path: Path) -> None:
    ids = some_manifest_ids(3)
    downloads = a_corpus_of(tmp_path, ids[:1])

    outcome = run_mod.run(
        Path("openconvert"),
        manifest_path=MANIFEST,
        downloads=downloads,
        limit=3,
        runner=fake_runner(),
    )

    assert outcome.missing == sorted(ids[1:])
    assert run_mod.write_report(outcome, tmp_path / "report.json")["missing"] == sorted(ids[1:])


def test_one_file_that_fails_to_convert_does_not_stop_the_run(tmp_path: Path) -> None:
    ids = some_manifest_ids(3)
    downloads = a_corpus_of(tmp_path, ids)
    exploded: list[str] = []

    def runner(_binary: Path, source: Path) -> dict[str, float]:
        if source.stem == ids[1]:
            exploded.append(source.stem)
            raise RuntimeError("exit 2: the file is broken")
        return {run_mod.METRIC_SECONDS_PER_PAGE: 0.02}

    outcome = run_mod.run(
        Path("openconvert"),
        manifest_path=MANIFEST,
        downloads=downloads,
        limit=3,
        runner=runner,
    )

    assert exploded == [ids[1]]
    assert [ident for ident, _ in outcome.failed] == [ids[1]]
    assert outcome.converted == 2
    failed_row = next(
        row
        for row in outcome.rows
        if row.file_id == ids[1] and row.metric == run_mod.METRIC_CONVERTED
    )
    assert failed_row.value == 0.0, "a failure is a score of zero, not an absence"


def test_the_report_carries_the_failure_reason_rather_than_only_a_count(tmp_path: Path) -> None:
    ids = some_manifest_ids(2)
    downloads = a_corpus_of(tmp_path, ids)

    def runner(_binary: Path, _source: Path) -> dict[str, float]:
        raise RuntimeError("exit 3: PDFium refused the file")

    outcome = run_mod.run(
        Path("openconvert"),
        manifest_path=MANIFEST,
        downloads=downloads,
        limit=2,
        runner=runner,
    )
    built = run_mod.write_report(outcome, tmp_path / "report.json")

    assert len(built["failed"]) == 2
    assert "PDFium refused" in built["failed"][0]["error"]


def test_every_row_carries_the_stratum_it_belongs_to(tmp_path: Path) -> None:
    """D18 reports per stratum, so a row with no stratum is a row that cannot be reported."""
    ids = some_manifest_ids(4)
    downloads = a_corpus_of(tmp_path, ids)

    outcome = run_mod.run(
        Path("openconvert"),
        manifest_path=MANIFEST,
        downloads=downloads,
        limit=4,
        runner=fake_runner(),
    )

    assert outcome.rows
    for row in outcome.rows:
        assert row.stratum, f"{row.file_id} has no stratum"


def test_corpus_files_pairs_every_manifest_entry_with_its_file_or_none(tmp_path: Path) -> None:
    ids = some_manifest_ids(2)
    downloads = a_corpus_of(tmp_path, ids[:1])

    paired = run_mod.corpus_files(MANIFEST, downloads)

    by_id = {entry.id: path for entry, path in paired}

    assert by_id[ids[0]] is not None
    assert by_id[ids[1]] is None
    from oc_eval.corpus import manifest as mf

    assert len(paired) == len(mf.load(MANIFEST).entries), "every entry is accounted for"


# --------------------------------------------------------------------------- PHASE 13


def test_a_committed_scanned_fixture_is_its_own_source_and_has_a_ground_truth(
    tmp_path: Path,
) -> None:
    """The scanned fixtures are committed, so the nightly converts them without a download and
    scores them against their `.gt.txt` as `ocr_cer` (row 13.21)."""
    from oc_eval.corpus import manifest as mf

    paired = dict(
        (entry.id, (entry, path)) for entry, path in run_mod.corpus_files(MANIFEST, tmp_path)
    )
    entry, path = paired["f01__scan300"]
    assert path == REPO_ROOT / "corpus/fixtures/scanned/f01__scan300.pdf"
    truth = run_mod.truth_of(entry)
    assert truth is not None and "stormy night" in truth
    assert entry.is_ours, "a scan of our own render is ours(*) and counts against the share"
    assert mf.load(MANIFEST).ours_share <= mf.ours_max_share()


def test_the_reading_text_of_an_epub_is_its_spine_a_block_per_line(tmp_path: Path) -> None:
    import zipfile

    epub = tmp_path / "book.epub"
    with zipfile.ZipFile(epub, "w") as archive:
        archive.writestr("mimetype", "application/epub+zip")
        archive.writestr(
            "META-INF/container.xml",
            '<container><rootfiles><rootfile full-path="OEBPS/content.opf"/>'
            "</rootfiles></container>",
        )
        archive.writestr(
            "OEBPS/content.opf",
            '<package><manifest><item id="c1" href="text/c1.xhtml" media-type="x"/>'
            '<item id="nav" href="nav.xhtml" media-type="x"/></manifest>'
            '<spine><itemref idref="c1"/></spine></package>',
        )
        archive.writestr(
            "OEBPS/text/c1.xhtml",
            "<html><head><title>T</title></head><body><h1>Chapter 3</h1>"
            "<p>It was a dark &amp;\n stormy night</p></body></html>",
        )
        archive.writestr("OEBPS/nav.xhtml", "<html><body><p>Contents</p></body></html>")

    assert run_mod.reading_text(epub) == "Chapter 3\nIt was a dark & stormy night"
