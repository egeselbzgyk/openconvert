---
description: Verify the current phase's Definition of Done, honestly
---

Verify the Definition of Done for `CURRENT_PHASE` in `PROGRESS.md`, against `IMPLEMENTATION_PLAN.md` §0.3
and that phase's own acceptance-criteria table. Run every gate for real — do not assert from memory:

```
cargo nextest run --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo deny check
cargo xtask thresholds-lint
cargo xtask ci-lint
```

Then check, one row at a time: every test named in the phase's test table exists and passes; every
Given/When/Then acceptance criterion maps to a named test or CI job; `docs/CHANGELOG.md` has the phase entry;
no `#[ignore]`, no `TODO` without an issue number; any Verification-debt row (VD-*) that this phase depends on
is closed in `docs/DECISIONS_LOG.md`.

Report a table: criterion | pass/fail | evidence (test name or command output). Be adversarial about your own
work — a criterion that is "basically done" is a fail. Only if every row passes, tick the phase in `PROGRESS.md`.
