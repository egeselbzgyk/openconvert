"""PHASE 7 rows 7.13 and 7.14: the jobs that enforce this phase exist and do what they say.

A gate described in a plan and absent from `.github/workflows/` enforces nothing, and the way
that happens is not carelessness — it is a job left as a placeholder while the work it was
waiting for landed elsewhere. Phase 0 created these job names precisely so no later phase had
to invent them, and this is the check that the bodies caught up.

The tests read the workflow files as YAML rather than grepping them, so a job renamed or
re-indented is a failure with a name on it.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

import yaml

REPO_ROOT = Path(__file__).resolve().parents[2]
CI = REPO_ROOT / ".github" / "workflows" / "ci.yml"
NIGHTLY = REPO_ROOT / ".github" / "workflows" / "nightly.yml"

# Phase 9's, and correctly still a placeholder: there is no model to record a cassette from.
STILL_PENDING = {"live-llm-cassette-refresh"}


def workflow(path: Path) -> dict[str, Any]:
    return yaml.safe_load(path.read_text(encoding="utf-8"))


def steps_of(path: Path, job: str) -> list[dict[str, Any]]:
    return list(workflow(path)["jobs"][job].get("steps") or [])


def run_text(path: Path, job: str) -> str:
    return "\n".join(str(step.get("run", "")) for step in steps_of(path, job))


# --------------------------------------------------------------------------- row 7.13


def test_the_python_job_runs_the_agpl_check_the_lint_the_types_and_the_tests() -> None:
    commands = run_text(CI, "python")

    assert "test_no_agpl.py" in commands, "row 7.13's gate is not in the python job"
    assert "ruff check" in commands
    assert "ruff format --check" in commands
    assert "mypy" in commands
    assert "pytest eval/tests" in commands


def test_the_agpl_check_runs_before_anything_that_could_import_the_banned_package() -> None:
    """The point is to fail on a dependency somebody added, not on a test that happened to
    use it, so the order is part of the check."""
    commands = [str(step.get("run", "")) for step in steps_of(CI, "python")]
    agpl = next(i for i, command in enumerate(commands) if "test_no_agpl.py" in command)
    suite = next(i for i, command in enumerate(commands) if "pytest eval/tests " in command + " ")

    assert agpl < suite


def test_the_corpus_lint_is_a_job_of_its_own() -> None:
    """A7.1. It reads only the manifest, so it costs a second and needs no downloads."""
    commands = run_text(CI, "corpus-lint")

    assert "corpus lint" in commands


# --------------------------------------------------------------------------- row 7.14


def test_nightly_full_corpus_runs_and_reports() -> None:
    """Row 7.14: the nightly job produces `eval/out/report.json`."""
    commands = run_text(NIGHTLY, "full-corpus")

    assert "corpus download" in commands, "a full-corpus run needs the corpus"
    assert "oc_eval run" in commands
    assert "eval/out/report.json" in commands
    assert "report trend" in commands, "row 7.10's gap is recorded on the same run"


def test_the_full_corpus_job_lints_the_manifest_before_it_spends_an_hour_downloading() -> None:
    commands = [str(step.get("run", "")) for step in steps_of(NIGHTLY, "full-corpus")]
    lint = next(i for i, command in enumerate(commands) if "corpus lint" in command)
    download = next(i for i, command in enumerate(commands) if "corpus download" in command)

    assert lint < download


def test_the_bench_job_turns_on_the_feature_the_budget_assertions_live_behind() -> None:
    commands = run_text(NIGHTLY, "bench")

    assert "--features bench" in commands
    assert "perf_budget" in commands
    assert "cargo bench" in commands, "the criterion trend runs beside the gate"


def test_the_deep_property_job_actually_raises_the_case_count() -> None:
    job = workflow(NIGHTLY)["jobs"]["proptest-deep"]

    assert str(job.get("env", {}).get("PROPTEST_CASES", "")) == "4096"


def test_the_mutation_job_runs_over_the_crates_whose_correctness_is_arithmetic() -> None:
    commands = run_text(NIGHTLY, "mutation-testing")

    assert "cargo mutants" in commands
    for crate in ("oc-layout", "oc-structure", "oc-validate"):
        assert crate in commands


# --------------------------------------------------------------------------- no stragglers


def test_no_phase_7_job_is_still_a_placeholder() -> None:
    for path in (CI, NIGHTLY):
        for name, job in workflow(path)["jobs"].items():
            if name in STILL_PENDING:
                continue
            body = "\n".join(str(step.get("run", "")) for step in (job.get("steps") or []))
            assert "not yet implemented (Phase 7)" not in body, (
                f"{path.name}:{name} is still a Phase 7 placeholder"
            )


def test_every_job_this_phase_owns_exists_by_name() -> None:
    """Phase 0 created these names so no later phase had to invent them."""
    nightly_jobs = set(workflow(NIGHTLY)["jobs"])
    ci_jobs = set(workflow(CI)["jobs"])

    assert {"full-corpus", "bench", "proptest-deep", "mutation-testing"} <= nightly_jobs
    assert {"python", "corpus-lint"} <= ci_jobs
