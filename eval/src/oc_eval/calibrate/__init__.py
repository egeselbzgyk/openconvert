"""Calibration, and the one thing it is not allowed to read.

TEST_CORPUS §7.1(c) and D18: the frozen holdout is reported on, never fitted against. That
distinction is the whole value of a holdout — a threshold tuned on a file is a threshold that
file can no longer test — and a rule living only in a document is one somebody eventually
breaks by accident.

So the refusal is here, in the path. Every fit takes its observations through [`fit`], which
loads the manifest and raises on any id marked `holdout`. Two details make it a mechanism
rather than a gesture:

* **an unknown id is refused too.** An id the manifest cannot account for is not evidence that
  it is not holdout, and "I could not check" must not read as "it is fine";
* **one offending file stops the whole fit.** Dropping it and carrying on would produce a
  number that looks fitted on what was asked for and was not.
"""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path

from oc_eval.corpus import manifest as mf

# `eval/src/oc_eval/calibrate/__init__.py` -> repository root.
REPO_ROOT = Path(__file__).resolve().parents[4]
DEFAULT_MANIFEST = REPO_ROOT / "corpus" / "manifest.json"


class HoldoutLeak(RuntimeError):
    """A calibration path was handed a file from the frozen holdout."""


class UnknownFile(RuntimeError):
    """A calibration path was handed an id the manifest does not account for."""


class NotEnoughEvidence(ValueError):
    """A fit over no observations is not a fit."""


@dataclass(frozen=True)
class Observation:
    """One decision the pipeline made, with how sure it was and whether it was right."""

    file_id: str
    confidence: float
    correct: bool


@dataclass(frozen=True)
class Fit:
    observations: int
    files: tuple[str, ...]
    accuracy: float


def fit(
    observations: Sequence[Observation],
    *,
    manifest_path: Path = DEFAULT_MANIFEST,
) -> Fit:
    """Check the inputs may be fitted on, then summarise them.

    Anything in `oc_eval` that would produce a number destined for `thresholds.toml` goes
    through here first.
    """
    if not observations:
        raise NotEnoughEvidence("a fit over no observations is not a fit")

    guard_files({observation.file_id for observation in observations}, manifest_path)

    correct = sum(1 for observation in observations if observation.correct)
    return Fit(
        observations=len(observations),
        files=tuple(sorted({observation.file_id for observation in observations})),
        accuracy=correct / len(observations),
    )


def guard_files(file_ids: set[str], manifest_path: Path) -> None:
    """Raise unless every id names a manifest entry that is not holdout."""
    manifest = mf.load(manifest_path)
    by_id = {entry.id: entry for entry in manifest.entries}

    unknown = sorted(file_id for file_id in file_ids if file_id not in by_id)
    if unknown:
        raise UnknownFile(
            "not in the corpus manifest, so it cannot be shown not to be holdout: "
            + ", ".join(unknown)
        )

    holdout = sorted(file_id for file_id in file_ids if by_id[file_id].holdout)
    if holdout:
        raise HoldoutLeak(
            "the frozen holdout is reported on, never fitted against "
            "(TEST_CORPUS §7.1c, D18): " + ", ".join(holdout)
        )
