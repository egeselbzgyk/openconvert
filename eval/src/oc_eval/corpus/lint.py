"""`oc-eval corpus lint` — the rules the corpus manifest has to satisfy.

TEST_CORPUS §7.1's CI checks and IMPLEMENTATION_PLAN §1.8's list, one function per rule and
one stable slug per rule, so that a failure names the thing that is wrong rather than a line
number. The slugs are part of the contract: `eval/tests/test_corpus_lint.py` asserts on them.
"""

from __future__ import annotations

import datetime as dt
import re
from collections import Counter
from collections.abc import Callable, Iterator
from dataclasses import dataclass

from oc_eval.corpus import manifest as mf

SHA256_RE = re.compile(r"^[0-9a-f]{64}$")

# The share of the synthetic bucket that carries a struct tree. D18 strips them by default and
# keeps a tagged variant bucket at roughly the real-world rate; ±5 points is §1.8's tolerance.
TAGGED_SHARE_TOLERANCE = 0.05


@dataclass(frozen=True)
class Finding:
    rule: str
    entry_id: str | None
    detail: str

    def __str__(self) -> str:
        where = f" [{self.entry_id}]" if self.entry_id else ""
        return f"{self.rule}{where}: {self.detail}"


Rule = Callable[[mf.Manifest], Iterator[Finding]]


def lint(manifest: mf.Manifest) -> list[Finding]:
    """Every finding, in rule order then entry order. An empty list is a clean corpus."""
    findings: list[Finding] = []
    for rule in RULES:
        findings.extend(rule(manifest))
    return findings


def rules_fired(findings: list[Finding], *, prefix: str | None = None) -> list[str]:
    """The distinct rule slugs in `findings`, optionally narrowed to one family."""
    seen = [f.rule for f in findings if prefix is None or f.rule.startswith(prefix)]
    return sorted(set(seen))


# --------------------------------------------------------------------------- schema


def schema_version(manifest: mf.Manifest) -> Iterator[Finding]:
    if manifest.schema_version != mf.SCHEMA_VERSION:
        yield Finding(
            "schema-version-unsupported",
            None,
            f"manifest declares {manifest.schema_version}, this tooling reads {mf.SCHEMA_VERSION}",
        )


def duplicate_id(manifest: mf.Manifest) -> Iterator[Finding]:
    for ident, count in sorted(Counter(item.id for item in manifest.entries).items()):
        if count > 1:
            yield Finding("duplicate-id", ident, f"{count} entries share this id")


def sha256_malformed(manifest: mf.Manifest) -> Iterator[Finding]:
    for item in manifest.entries:
        digest = str(item.raw.get("sha256", ""))
        if not SHA256_RE.match(digest):
            yield Finding("sha256-malformed", item.id, f"{digest!r} is not 64 lowercase hex")


def category_unrecognised(manifest: mf.Manifest) -> Iterator[Finding]:
    for item in manifest.entries:
        category = str(item.raw.get("category", ""))
        if category not in mf.CATEGORIES:
            yield Finding("category-unrecognised", item.id, f"{category!r}")


# --------------------------------------------------------------------------- licence


def license_incomplete(manifest: mf.Manifest) -> Iterator[Finding]:
    """§7.1: a licence block is a name, a URL, a verifier and the date they verified it."""
    for item in manifest.entries:
        licence = item.raw.get("license")
        if not isinstance(licence, dict):
            yield Finding("license-incomplete", item.id, "no license block")
            continue
        for field in ("name", "url", "verified_by", "verified_date"):
            if not str(licence.get(field, "")).strip():
                yield Finding("license-incomplete", item.id, f"license.{field} is empty")
        raw_date = str(licence.get("verified_date", ""))
        if raw_date and not _is_iso_date(raw_date):
            yield Finding(
                "license-incomplete", item.id, f"license.verified_date {raw_date!r} is not a date"
            )


def license_not_allowed(manifest: mf.Manifest) -> Iterator[Finding]:
    for item in manifest.entries:
        name = item.license_name
        if name and name not in mf.LICENSE_ALLOWLIST and not _is_blocked(name):
            yield Finding("license-not-allowed", item.id, f"{name!r} is not on the allowlist")


def license_blocked(manifest: mf.Manifest) -> Iterator[Finding]:
    for item in manifest.entries:
        name = item.license_name
        if name and _is_blocked(name):
            yield Finding("license-blocked", item.id, f"{name!r} matches the blocklist")


def _target_is_expressible(size: int, target: float) -> bool:
    """Can a bucket of `size` files land within tolerance of `target` at all?"""
    if size == 0:
        return False
    return any(abs(tagged / size - target) <= TAGGED_SHARE_TOLERANCE for tagged in range(size + 1))


def _is_blocked(name: str) -> bool:
    lowered = name.lower()
    return any(marker in lowered for marker in mf.LICENSE_BLOCKLIST_MARKERS)


def _is_iso_date(text: str) -> bool:
    try:
        dt.date.fromisoformat(text)
    except ValueError:
        return False
    return True


# --------------------------------------------------------------------------- stratification


def producer_stratum_unrecognised(manifest: mf.Manifest) -> Iterator[Finding]:
    for item in manifest.entries:
        if item.producer_stratum not in mf.STRATA:
            yield Finding(
                "producer-stratum-unrecognised",
                item.id,
                f"{item.producer_stratum!r} is not one of {sorted(mf.STRATA)}",
            )


def ours_share_above_maximum(manifest: mf.Manifest) -> Iterator[Finding]:
    """D18: `ours(*)` may not exceed 40 % of the corpus, and a release may not pass on it alone."""
    limit = mf.ours_max_share()
    share = manifest.ours_share
    if share > limit:
        ours = sum(1 for item in manifest.documents if item.is_ours)
        yield Finding(
            "ours-share-above-maximum",
            None,
            f"{ours}/{len(manifest.documents)} documents are ours ({share:.3f} > {limit})",
        )


def tagged_share_off_target(manifest: mf.Manifest) -> Iterator[Finding]:
    """§1.8: the synthetic bucket's tagged share tracks the real world's 12.6 %, ±5 points."""
    synthetic = [item for item in manifest.documents if item.is_ours]
    target = mf.tagged_share_target()
    if not _target_is_expressible(len(synthetic), target):
        # A five-file bucket can be 0.0 or 0.2 tagged and nothing in between, so it cannot be
        # within five points of 0.126 however it is built. Reporting that as a finding would
        # be reporting arithmetic, not a corpus defect.
        return
    share = sum(1 for item in synthetic if item.tagged) / len(synthetic)
    if abs(share - target) > TAGGED_SHARE_TOLERANCE:
        yield Finding(
            "tagged-share-off-target",
            None,
            f"synthetic bucket is {share:.3f} tagged, target {target} +/-{TAGGED_SHARE_TOLERANCE}",
        )


# --------------------------------------------------------------------------- holdout


def holdout_below_minimum(manifest: mf.Manifest) -> Iterator[Finding]:
    """§7.6: counted per document. A hundred pages from six PDFs exercise six producers."""
    minimum = mf.holdout_minimum()
    have = manifest.holdout_document_count
    if have < minimum:
        pages = sum(1 for item in manifest.entries if item.is_page_level and item.holdout)
        extra = f" ({pages} page-level holdout entries do not count)" if pages else ""
        yield Finding(
            "holdout-below-minimum", None, f"{have} holdout documents, {minimum} required{extra}"
        )


def holdout_is_ours(manifest: mf.Manifest) -> Iterator[Finding]:
    """D18: it is the *real-world* holdout. A file we rendered cannot be evidence about reality."""
    for item in manifest.entries:
        if item.holdout and item.is_ours:
            yield Finding("holdout-is-ours", item.id, f"stratum {item.producer_stratum}")


def page_unit_undeclared(manifest: mf.Manifest) -> Iterator[Finding]:
    """A page-level entry has to say so, or the document count is not checkable by reading it."""
    for item in manifest.entries:
        if item.is_page_level and item.declared_unit != mf.UNIT_PAGE:
            yield Finding(
                "page-unit-undeclared",
                item.id,
                f"ground truth {item.ground_truth_type!r} / stratum "
                f"{item.producer_stratum!r} is page-level, but unit is "
                f"{item.declared_unit!r}",
            )


# --------------------------------------------------------------------------- redistribution


def local_eval_only_in_manifest(manifest: mf.Manifest) -> Iterator[Finding]:
    """TEST_CORPUS §7.5: the manifest is the redistributable list. That set has its own."""
    for item in manifest.entries:
        if item.raw.get("local_eval_only"):
            yield Finding(
                "local-eval-only-in-manifest",
                item.id,
                "belongs in corpus/local_eval_only.json",
            )


RULES: tuple[Rule, ...] = (
    schema_version,
    duplicate_id,
    sha256_malformed,
    category_unrecognised,
    license_incomplete,
    license_not_allowed,
    license_blocked,
    producer_stratum_unrecognised,
    ours_share_above_maximum,
    tagged_share_off_target,
    holdout_below_minimum,
    holdout_is_ours,
    page_unit_undeclared,
    local_eval_only_in_manifest,
)
