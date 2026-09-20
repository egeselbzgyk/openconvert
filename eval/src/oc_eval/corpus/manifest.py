"""The corpus manifest: what an entry is, and what counts as what.

Schema: IMPLEMENTATION_PLAN §1.8, extended by TEST_CORPUS §7.1 and §7.6. This module holds
the vocabulary — the licence allowlist, the producer strata, the document-vs-page unit — and
`lint.py` holds the rules that use it. The split is so that a harvester can build an entry
against the same definitions the gate will judge it by.
"""

from __future__ import annotations

import json
from collections.abc import Iterable, Mapping
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from oc_eval import thresholds

SCHEMA_VERSION = 1

# TEST_CORPUS §7.1 and IMPLEMENTATION_PLAN §1.8. A licence not on this list is not a licence
# this corpus can be redistributed under, whatever it says about itself.
LICENSE_ALLOWLIST = frozenset(
    {
        "CC0-1.0",
        "CC-BY-3.0",
        "CC-BY-4.0",
        "CC-BY-SA-3.0",
        "CC-BY-SA-4.0",
        "PD-US-Gov",
        "PD-old-work",
        "CDLA-Permissive-1.0",
        "Apache-2.0",
        "MIT",
        "BSD-3-Clause",
    }
)

# Matched case-insensitively as substrings, because these arrive as prose far more often than
# as identifiers: "research use only", "Do Not Redistribute", "license unknown".
LICENSE_BLOCKLIST_MARKERS = (
    "research-only",
    "research only",
    "research use",
    "non-commercial",
    "noncommercial",
    "-nc-",
    "do-not-redistribute",
    "do not redistribute",
    "no redistribution",
    "commoncrawl",
    "common crawl",
    "safedocs",
    "rvl-cdip",
    "funsd",
    "unknown",
    "unclear",
    "tbd",
)

# D18's eight, plus Quark from IMPLEMENTATION_PLAN §1.8, plus the page-level layout stratum
# TEST_CORPUS §5.3 and §7.6 keep separate from every document stratum.
DOCUMENT_STRATA = frozenset(
    {
        "pdfTeX",
        "InDesign",
        "Word",
        "Ghostscript",
        "Quark",
        "ABBYY-scanner",
        "ours(Typst)",
        "ours(WeasyPrint)",
        "unknown",
    }
)
PAGE_LEVEL_STRATUM = "page-level-layout"
STRATA = DOCUMENT_STRATA | {PAGE_LEVEL_STRATUM}

OURS_PREFIX = "ours("

# Ground-truth kinds whose unit is the page, not the document (TEST_CORPUS §5.3).
PAGE_UNIT_GROUND_TRUTH = frozenset({"doclaynet-annotation"})

UNIT_DOCUMENT = "document"
UNIT_PAGE = "page"

CATEGORIES = frozenset({"simple", "difficult", "broken", "edge-case"})


def ours_max_share() -> float:
    return float(thresholds.value("corpus.ours_max_share"))


def holdout_minimum() -> int:
    return int(thresholds.value("corpus.holdout_min_files"))


def tagged_share_target() -> float:
    return float(thresholds.value("corpus.tagged_share_target"))


def tagged_share_tolerance() -> float:
    return float(thresholds.value("corpus.tagged_share_tolerance"))


@dataclass(frozen=True)
class Entry:
    """One manifest entry, with the questions the rules ask it."""

    raw: Mapping[str, Any]

    @property
    def id(self) -> str:
        return str(self.raw.get("id", ""))

    @property
    def license_name(self) -> str:
        licence = self.raw.get("license")
        return str(licence.get("name", "")) if isinstance(licence, Mapping) else ""

    @property
    def producer_stratum(self) -> str:
        return str(self.raw.get("producer_stratum", ""))

    @property
    def is_ours(self) -> bool:
        return self.producer_stratum.startswith(OURS_PREFIX)

    @property
    def holdout(self) -> bool:
        return bool(self.raw.get("holdout", False))

    @property
    def tagged(self) -> bool:
        return bool(self.raw.get("tagged", False))

    @property
    def ground_truth_type(self) -> str:
        return str(self.raw.get("ground_truth_type", "none"))

    @property
    def declared_unit(self) -> str:
        """`unit` as written. Absent means `document` — the overwhelmingly common case."""
        return str(self.raw.get("unit", UNIT_DOCUMENT))

    @property
    def is_page_level(self) -> bool:
        """True for anything whose unit is the page, however it was declared.

        TEST_CORPUS §7.6 closes the per-page shortcut, so this deliberately does not trust
        `unit` alone: a DocLayNet entry is page-level whether or not it says so, and the lint
        separately requires it to say so.
        """
        return (
            self.declared_unit == UNIT_PAGE
            or self.producer_stratum == PAGE_LEVEL_STRATUM
            or self.ground_truth_type in PAGE_UNIT_GROUND_TRUTH
        )


@dataclass(frozen=True)
class Manifest:
    schema_version: int
    entries: tuple[Entry, ...]
    path: Path | None = None

    @property
    def documents(self) -> tuple[Entry, ...]:
        return tuple(item for item in self.entries if not item.is_page_level)

    @property
    def holdout_document_count(self) -> int:
        """TEST_CORPUS §7.6: the unit is the document. Pages are counted elsewhere or not at all."""
        return sum(1 for item in self.documents if item.holdout)

    @property
    def ours_share(self) -> float:
        documents = self.documents
        if not documents:
            return 0.0
        return sum(1 for item in documents if item.is_ours) / len(documents)

    def by_stratum(self) -> dict[str, list[Entry]]:
        buckets: dict[str, list[Entry]] = {}
        for item in self.entries:
            buckets.setdefault(item.producer_stratum, []).append(item)
        return dict(sorted(buckets.items()))


def from_mapping(payload: Mapping[str, Any], *, path: Path | None = None) -> Manifest:
    files: Iterable[Mapping[str, Any]] = payload.get("files", ())
    return Manifest(
        schema_version=int(payload.get("schema_version", 0)),
        entries=tuple(Entry(dict(item)) for item in files),
        path=path,
    )


def load(path: Path) -> Manifest:
    return from_mapping(json.loads(path.read_text(encoding="utf-8")), path=path)


def dump(manifest: Manifest, path: Path) -> None:
    """Write a manifest back. Sorted by id, two-space indent, trailing newline.

    Deterministic for the same reason the engine's output is: a manifest that reorders itself
    on every harvest turns a one-entry change into an unreviewable diff.
    """
    payload = {
        "schema_version": manifest.schema_version,
        "files": [dict(item.raw) for item in sorted(manifest.entries, key=lambda e: e.id)],
    }
    path.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
