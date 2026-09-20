"""PHASE 7 rows 7.1 and 7.3b: the manifest is the corpus's contract, and CI enforces it.

The rules come from TEST_CORPUS §7.1 and §7.6 and from IMPLEMENTATION_PLAN §1.8. Two of them
are the reason this file exists at all:

* a corpus file with an unverified or unlisted licence is not redistributable, and a corpus
  that cannot be redistributed is not a corpus (§7.4);
* the ≥ 100-document holdout is a *document* count. DocLayNet's unit is the page, and a
  hundred pages from six PDFs exercise six producers — producer diversity being the one
  property the holdout exists to measure (§7.6).

Every rule is exercised on a manifest built here, so a test failure names a rule rather than
a corpus file. The two rows the plan words as gates are additionally asserted against the
committed `corpus/manifest.json`.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from oc_eval.corpus import manifest as mf
from oc_eval.corpus.lint import lint, rules_fired

REPO_ROOT = Path(__file__).resolve().parents[2]
COMMITTED_MANIFEST = REPO_ROOT / "corpus" / "manifest.json"

GOOD_LICENSE = {
    "name": "CC-BY-4.0",
    "url": "https://creativecommons.org/licenses/by/4.0/",
    "verified_by": "maintainer",
    "verified_date": "2026-09-20",
}


def entry(ident: str, **overrides: Any) -> dict[str, Any]:
    """A manifest entry that passes every rule, before `overrides` breaks one."""
    base: dict[str, Any] = {
        "id": ident,
        "title": ident,
        "source": {
            "name": "OAPEN",
            "url": f"https://library.oapen.org/handle/{ident}",
            "retrieved_date": "2026-09-20",
        },
        "license": dict(GOOD_LICENSE),
        "sha256": "0" * 64,
        "file_size_bytes": 1,
        "category": "simple",
        "producer_stratum": "InDesign",
        "producer_raw": "Adobe InDesign 19.0",
        "tagged": False,
        "holdout": False,
        "ground_truth_type": "none",
        "generator": "n/a-real-world",
        "defect_injection": [],
    }
    base.update(overrides)
    return base


def manifest_of(*entries: dict[str, Any]) -> mf.Manifest:
    return mf.from_mapping({"schema_version": 1, "files": list(entries)})


def real_holdout(count: int, *, prefix: str = "h") -> list[dict[str, Any]]:
    return [entry(f"{prefix}{i:03d}", holdout=True) for i in range(count)]


# --------------------------------------------------------------------------- row 7.1


def test_manifest_entries_have_complete_license_block() -> None:
    """Row 7.1, over the corpus as committed: a licence, a verifier and a date, on all of it."""
    committed = mf.load(COMMITTED_MANIFEST)

    fired = rules_fired(lint(committed), prefix="license")

    assert fired == [], f"licence findings against the committed manifest: {fired}"
    assert committed.entries, "the committed manifest is empty"
    for item in committed.entries:
        assert item.license_name in mf.LICENSE_ALLOWLIST
        assert item.raw["license"]["verified_by"]
        assert item.raw["license"]["verified_date"]


def test_a_licence_off_the_allowlist_is_a_finding() -> None:
    """EUPL-1.2 is a real, perfectly respectable licence. It is not one this corpus lists."""
    broken = manifest_of(entry("e1", license=dict(GOOD_LICENSE, name="EUPL-1.2")))

    assert "license-not-allowed" in rules_fired(lint(broken))


def test_the_non_commercial_creative_commons_variants_are_blocked() -> None:
    """A corpus that cannot be used commercially is not one an Apache-2.0 project ships."""
    for name in ("CC-BY-NC-4.0", "CC-BY-NC-SA-4.0", "CC-BY-NC-ND-3.0"):
        assert "license-blocked" in rules_fired(
            lint(manifest_of(entry("e1", license=dict(GOOD_LICENSE, name=name))))
        ), name


def test_a_licence_on_the_blocklist_is_a_finding() -> None:
    """§7.1: research-only, do-not-redistribute, CommonCrawl- and RVL-CDIP-derived, unknown."""
    for blocked in ("research-only", "do-not-redistribute", "unknown", "RVL-CDIP-derived"):
        broken = manifest_of(entry("e1", license=dict(GOOD_LICENSE, name=blocked)))

        assert "license-blocked" in rules_fired(lint(broken)), blocked


def test_a_licence_missing_its_verifier_or_its_date_is_a_finding() -> None:
    for field in ("verified_by", "verified_date"):
        stripped = {k: v for k, v in GOOD_LICENSE.items() if k != field}
        broken = manifest_of(entry("e1", license=stripped))

        assert "license-incomplete" in rules_fired(lint(broken)), field


def test_a_verification_date_that_is_not_a_date_is_a_finding() -> None:
    broken = manifest_of(entry("e1", license=dict(GOOD_LICENSE, verified_date="last tuesday")))

    assert "license-incomplete" in rules_fired(lint(broken))


# --------------------------------------------------------------------------- row 7.3b


def test_doclaynet_pages_do_not_count_toward_holdout() -> None:
    """Row 7.3b: a manifest that reaches 100 only by counting page-level entries fails."""
    pages = [
        entry(
            f"doclaynet-{i:04d}",
            holdout=True,
            unit="page",
            producer_stratum="page-level-layout",
            ground_truth_type="doclaynet-annotation",
        )
        for i in range(120)
    ]
    documents = real_holdout(20, prefix="doc")

    fired = rules_fired(lint(manifest_of(*pages, *documents)))

    assert "holdout-below-minimum" in fired, (
        "120 DocLayNet pages plus 20 documents is 20 documents, not 140"
    )
    assert mf.from_mapping(
        {"schema_version": 1, "files": pages + documents}
    ).holdout_document_count == len(documents)


def test_a_page_level_entry_that_calls_itself_a_document_is_a_finding() -> None:
    """The page-vs-document rule cannot be dodged by leaving `unit` off a DocLayNet page."""
    smuggled = manifest_of(
        entry("doclaynet-0001", holdout=True, ground_truth_type="doclaynet-annotation")
    )

    assert "page-unit-undeclared" in rules_fired(lint(smuggled))


def test_a_hundred_real_documents_clear_the_holdout_rule() -> None:
    enough = manifest_of(*real_holdout(mf.holdout_minimum()))

    assert "holdout-below-minimum" not in rules_fired(lint(enough))


def test_an_ours_stratum_file_cannot_be_holdout() -> None:
    """D18: the holdout is the *real-world* holdout. A file we rendered is not evidence."""
    broken = manifest_of(entry("e1", holdout=True, producer_stratum="ours(Typst)"))

    assert "holdout-is-ours" in rules_fired(lint(broken))


# --------------------------------------------------------------------------- the rest


def test_the_ours_share_rule_fires_above_the_threshold() -> None:
    ours = [entry(f"o{i}", producer_stratum="ours(Typst)") for i in range(5)]
    real = [entry(f"r{i}") for i in range(5)]

    assert "ours-share-above-maximum" in rules_fired(lint(manifest_of(*ours, *real)))
    assert "ours-share-above-maximum" not in rules_fired(
        lint(manifest_of(*ours[:3], *real, *[entry(f"r9{i}") for i in range(2)]))
    )


def test_an_unknown_producer_stratum_value_is_a_finding() -> None:
    broken = manifest_of(entry("e1", producer_stratum="Scribus"))

    assert "producer-stratum-unrecognised" in rules_fired(lint(broken))


def test_a_duplicate_id_is_a_finding() -> None:
    broken = manifest_of(entry("same"), entry("same"))

    assert "duplicate-id" in rules_fired(lint(broken))


def test_a_malformed_sha256_is_a_finding() -> None:
    broken = manifest_of(entry("e1", sha256="deadbeef"))

    assert "sha256-malformed" in rules_fired(lint(broken))


def test_a_local_eval_only_entry_may_not_be_in_the_manifest() -> None:
    """TEST_CORPUS §7.5: that set has its own list, and the manifest is redistributable."""
    broken = manifest_of(entry("e1", local_eval_only=True))

    assert "local-eval-only-in-manifest" in rules_fired(lint(broken))


def test_a_clean_manifest_produces_no_findings() -> None:
    synthetic = [
        entry(f"o{i}", producer_stratum="ours(Typst)", tagged=i == 0, generator="typst-0.15.1")
        for i in range(8)
    ]
    clean = manifest_of(*real_holdout(mf.holdout_minimum()), *synthetic)

    assert lint(clean) == []


def test_a_bucket_too_small_to_express_the_tagged_target_is_not_reported() -> None:
    """One synthetic file is 0.0 or 1.0 tagged. Neither is within five points of 0.126."""
    tiny = manifest_of(
        *real_holdout(mf.holdout_minimum()), entry("o1", producer_stratum="ours(Typst)")
    )

    assert "tagged-share-off-target" not in rules_fired(lint(tiny))


def test_a_synthetic_bucket_that_kept_its_struct_trees_is_a_finding() -> None:
    """D18: Typst tags by default, and reality is 12.6 % tagged. Not stripping is the defect."""
    all_tagged = [entry(f"o{i}", producer_stratum="ours(Typst)", tagged=True) for i in range(8)]
    broken = manifest_of(*real_holdout(mf.holdout_minimum()), *all_tagged)

    assert "tagged-share-off-target" in rules_fired(lint(broken))


def test_the_committed_manifest_is_parseable_and_declares_its_schema_version() -> None:
    raw = json.loads(COMMITTED_MANIFEST.read_text(encoding="utf-8"))

    assert raw["schema_version"] == mf.SCHEMA_VERSION
    assert mf.load(COMMITTED_MANIFEST).entries
