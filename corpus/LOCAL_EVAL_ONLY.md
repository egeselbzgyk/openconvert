# `LOCAL_EVAL_ONLY` — the non-redistributed area

TEST_CORPUS §7.5. Some material is useful for local robustness work and may **not** be
redistributed as part of an open-source test suite: the CommonCrawl/SAFEDOCS PDF corpus,
RVL-CDIP/FUNSD-derived sets, anything whose licence is research-only, no-redistribution or
unknown, and anything still in copyright.

None of it is corpus. Nothing here is fetched by `corpus/download.py`, scored in a published
report, or read by any CI job.

## The boundary

It is a mechanism, not a policy note a contributor could miss:

1. **A separate list.** Entries live in `corpus/local_eval_only.json`, never in
   `corpus/manifest.json`. An entry carrying `local_eval_only: true` that reaches the
   redistributable path raises `NotRedistributable` — asserted by
   `eval/tests/test_corpus_download.py::test_the_redistributable_path_refuses_a_local_eval_only_entry`.
2. **A separate script with an explicit opt-in flag.** `corpus/download_local_eval_only.py`
   exits without fetching anything unless `--accept-non-redistributable` is passed.
3. **A separate, git-ignored directory.** Files land in `corpus/LOCAL_EVAL_ONLY/`.

`example_pdfs/` (TEST_CORPUS §7.5a) sits under the same boundary for the same reason: it is the
maintainer's local smoke set, two of its four files are in copyright, and no threshold is ever
fitted on it.

## What may not be done with anything here

- No number from it appears in `eval/out/report.json`, in a release note, or in a README.
- No threshold in `thresholds.toml` is fitted on it.
- Nothing from it is quoted in a fixture, a snapshot or a commit message.
- It is not the holdout. The frozen ≥ 100-document holdout is real, licence-cleared,
  redistributable material listed in `corpus/manifest.json` with `holdout: true`.
