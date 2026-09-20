"""Turn source candidates into manifest entries: fetch, probe, admit or reject with a reason.

The admission rule is that nothing enters the manifest on a promise. A source says a document
is CC BY; the harvest downloads it, opens it, and records the digest of the bytes it actually
received, the page count the file actually has, and the producer the file actually names. An
entry that reaches `corpus/manifest.json` has therefore been seen, and `oc-eval corpus lint`
judges what was seen rather than what was advertised.

Rejections are counted and named. A harvest that admitted 40 of 300 candidates is a fact about
the licence mix of a source, and TEST_CORPUS §7.4's "licence drift is a real ongoing risk"
is only observable if the misses are reported rather than skipped in silence.
"""

from __future__ import annotations

import datetime as dt
from collections.abc import Iterable, Iterator
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from oc_eval.corpus import download, stratify
from oc_eval.corpus import manifest as mf
from oc_eval.corpus import probe as probe_mod
from oc_eval.corpus.sources import Candidate

# A single corpus file larger than this is a download cost the nightly tier does not need; the
# largest real books in TEST_CORPUS §3's table are well under it.
DEFAULT_MAX_FILE_BYTES = 40 * 1024 * 1024

# TEST_CORPUS §7.3's nightly budget is 2-5 GB for the whole corpus.
DEFAULT_MAX_TOTAL_BYTES = 3 * 1024 * 1024 * 1024

# A document with fewer pages than this is a cover sheet or an erratum, not a document the
# structure stages have anything to say about.
MIN_PAGES = 4

# How many candidates to ask a source for per document wanted. A harvest rejects more than it
# admits — landing pages that are not PDFs, files over budget, documents too short to be one —
# and the generators are lazy, so asking for six times the target costs nothing when the first
# attempt succeeds.
CANDIDATE_OVERSUPPLY = 6


def ask_for(want: int, *, already_held: int) -> int:
    """How many candidates to draw from a source to admit `want` new ones.

    A source adapter re-walks its catalogue from the beginning every run, so the first
    `already_held` candidates it offers on a top-up are the ones an earlier harvest already
    took. An ask that does not clear that number yields nothing but duplicates, however many
    documents are left in the catalogue — which is exactly what a `usgov=2` top-up did against
    thirteen NASA reports already in the manifest.
    """
    return want * CANDIDATE_OVERSUPPLY + already_held


@dataclass(frozen=True)
class Rejection:
    candidate_id: str
    reason: str

    def __str__(self) -> str:
        return f"{self.candidate_id}: {self.reason}"


@dataclass
class HarvestReport:
    admitted: list[dict[str, Any]] = field(default_factory=list)
    rejected: list[Rejection] = field(default_factory=list)
    bytes_downloaded: int = 0

    def reject(self, candidate_id: str, reason: str) -> None:
        self.rejected.append(Rejection(candidate_id, reason))

    @property
    def reasons(self) -> dict[str, int]:
        counts: dict[str, int] = {}
        for item in self.rejected:
            key = item.reason.split(":", 1)[0]
            counts[key] = counts.get(key, 0) + 1
        return dict(sorted(counts.items(), key=lambda pair: (-pair[1], pair[0])))


def entry_from(
    candidate: Candidate,
    *,
    probe: probe_mod.Probe,
    digest: str,
    size_bytes: int,
    today: str,
    verifier: str,
) -> dict[str, Any]:
    """The manifest entry for a candidate whose bytes have been seen. Pure; no I/O.

    `holdout` is set here and never unset: TEST_CORPUS §7.6 freezes membership at the moment of
    admission, which is this moment.
    """
    stratum = stratify.classify_producer(probe.producer, creator=probe.creator, real_world=True)
    return {
        "id": candidate.id,
        "title": candidate.title,
        "source": {
            "name": candidate.source_name,
            "url": candidate.landing_url,
            "retrieved_date": today,
            "selection_query": candidate.selection_query,
        },
        "license": {
            "name": candidate.license_name,
            "url": candidate.license_url,
            "verified_by": verifier,
            "verified_date": today,
        },
        "sha256": digest,
        "file_size_bytes": size_bytes,
        "pages": probe.pages,
        "category": _category(stratum, probe.pages),
        "difficulty": _difficulty(stratum, probe.pages),
        "producer_stratum": stratum,
        "producer_raw": probe.producer or probe.creator or "",
        "tagged": probe.tagged,
        "holdout": True,
        "expected_problems": _expected_problems(stratum, probe),
        "expected_output_characteristics": {"languages": list(candidate.languages)},
        "ground_truth_type": "none",
        "generator": "n/a-real-world",
        "defect_injection": [],
        "download_url": candidate.pdf_url,
    }


def _category(stratum: str, pages: int) -> str:
    """§7.1's four. Derived from what the probe saw, not from a human reading the book.

    A scan is `difficult` because its text layer is OCR, and a long book is `difficult` because
    length is where furniture detection and structure inference actually get tested. Everything
    else is `simple` until something says otherwise — `broken` and `edge-case` are claims about
    a file that only a failure or an annotation can justify.
    """
    if stratum == "ABBYY-scanner":
        return "difficult"
    return "difficult" if pages >= 300 else "simple"


def _difficulty(stratum: str, pages: int) -> int:
    score = 1
    if stratum == "ABBYY-scanner":
        score += 2
    elif stratum == "unknown":
        score += 1
    if pages >= 300:
        score += 1
    elif pages >= 100:
        score += 0
    return min(score, 5)


def _expected_problems(stratum: str, probe: probe_mod.Probe) -> list[str]:
    """Only what the probe can actually attest. The rest is item 7.5's annotation work."""
    problems: list[str] = []
    if stratum == "ABBYY-scanner":
        problems.append("ocr-text-layer")
    if probe.tagged:
        problems.append("tagged-pdf")
    return problems


def admit(
    candidates: Iterable[Candidate],
    *,
    dest_dir: Path,
    today: str | None = None,
    verifier: str = "oc-eval corpus harvest",
    max_file_bytes: int = DEFAULT_MAX_FILE_BYTES,
    max_total_bytes: int = DEFAULT_MAX_TOTAL_BYTES,
    known_ids: set[str] | None = None,
    report: HarvestReport | None = None,
    opener: download.Opener = download.default_opener,
    want: int | None = None,
) -> HarvestReport:
    """Fetch, probe and admit each candidate, recording why each rejection happened.

    `want` caps how many *this call* admits. A source generator is lazy, so a top-up run
    can ask a catalogue for far more candidates than it needs and stop the moment it has
    enough — which is the only way a second harvest reaches past the documents the first
    one already took. Without it a source that re-walks its catalogue from the start
    yields nothing but duplicates.
    """
    outcome = report if report is not None else HarvestReport()
    seen = set(known_ids or ())
    stamp = today or dt.date.today().isoformat()
    dest_dir.mkdir(parents=True, exist_ok=True)
    admitted_here = 0

    for candidate in candidates:
        if want is not None and admitted_here >= want:
            break
        if candidate.id in seen:
            outcome.reject(candidate.id, "duplicate: already in the manifest")
            continue
        if outcome.bytes_downloaded >= max_total_bytes:
            outcome.reject(candidate.id, "budget: the total download budget is spent")
            break
        if (candidate.size_hint or 0) > max_file_bytes:
            outcome.reject(candidate.id, f"too-large: {candidate.size_hint} bytes before download")
            continue
        if not stratify.is_acceptable_license(candidate.license_name):
            outcome.reject(candidate.id, f"licence: {candidate.license_name}")
            continue

        target = dest_dir / f"{candidate.id}.pdf"
        try:
            size = _place(candidate, target, max_file_bytes, opener)
        except _TooLarge as failure:
            outcome.reject(candidate.id, f"too-large: {failure}")
            continue
        except (download.NoSourceAvailable, OSError) as failure:
            outcome.reject(candidate.id, f"fetch: {failure}")
            continue

        try:
            probe = probe_mod.probe(target)
        except probe_mod.NotAPdf as failure:
            target.unlink(missing_ok=True)
            outcome.reject(candidate.id, f"not-a-pdf: {failure}")
            continue

        if not probe.usable:
            target.unlink(missing_ok=True)
            outcome.reject(
                candidate.id,
                "unreadable: encrypted" if probe.encrypted else "unreadable: no pages",
            )
            continue
        if probe.pages < MIN_PAGES:
            target.unlink(missing_ok=True)
            outcome.reject(candidate.id, f"too-short: {probe.pages} pages")
            continue

        seen.add(candidate.id)
        admitted_here += 1
        outcome.bytes_downloaded += size
        outcome.admitted.append(
            entry_from(
                candidate,
                probe=probe,
                digest=download.sha256_of(target),
                size_bytes=target.stat().st_size,
                today=stamp,
                verifier=verifier,
            )
        )
    return outcome


class _TooLarge(RuntimeError):
    """The file turned out to be over budget once it started arriving."""


def _place(candidate: Candidate, target: Path, max_file_bytes: int, opener: download.Opener) -> int:
    if target.exists():
        return target.stat().st_size
    size = download.fetch_unverified(candidate.pdf_url, target, opener=opener)
    if size > max_file_bytes:
        target.unlink(missing_ok=True)
        raise _TooLarge(f"{size} bytes")
    return size


def merge(existing: mf.Manifest, entries: Iterable[dict[str, Any]]) -> mf.Manifest:
    """Add entries to a manifest, keeping the entry already there when an id repeats.

    Already-there wins because the holdout is frozen: an entry's `holdout`, `retrieved_date` and
    digest are a record of an admission that happened, and a later harvest re-describing the
    same document would silently rewrite history.
    """
    by_id: dict[str, mf.Entry] = {item.id: item for item in existing.entries}
    for raw in entries:
        by_id.setdefault(str(raw["id"]), mf.Entry(dict(raw)))
    return mf.Manifest(
        schema_version=existing.schema_version or mf.SCHEMA_VERSION,
        entries=tuple(by_id[key] for key in sorted(by_id)),
        path=existing.path,
    )


def plan_from(spec: str) -> Iterator[tuple[str, int]]:
    """Parse `oapen=40,arxiv=15` into (source, want) pairs, in the order given."""
    for part in spec.split(","):
        chunk = part.strip()
        if not chunk:
            continue
        name, _, count = chunk.partition("=")
        yield name.strip(), int(count or 0)
