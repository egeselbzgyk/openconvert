---
description: Do exactly one TDD work item and update PROGRESS.md
---

Continue the OpenConvert implementation.

1. Read `PROGRESS.md`. Identify `CURRENT_PHASE` and `CURRENT_ITEM`.
2. Read only the relevant section of `docs/IMPLEMENTATION_PLAN.md` for that phase (and the stage section of
   `docs/PIPELINE.md` if you are implementing a pipeline stage).
3. Execute **exactly one** work item with the full TDD loop from CLAUDE.md §2:
   RED (tests named in the phase table, exact names) → minimal implementation → GREEN
   (`cargo nextest run -p <crate>` then `--workspace`) → regression artefact → refactor
   (`cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`) → commit.
4. Update `PROGRESS.md`: tick what is done, set the next `CURRENT_ITEM`, append to the completed-items log,
   keep `## Notes` to what a fresh session needs.
5. If a phase's full Definition of Done now passes, tick the phase box and advance `CURRENT_PHASE`.

Do not start a second work item. Do not refactor code outside the item's scope.
If you need an architectural decision that `docs/DECISIONS.md` does not settle: write the question under
`## Blocked`, set `STATUS: BLOCKED`, and stop.

Finish by printing: the item you completed, the tests you added, the gate results, and the next item.
