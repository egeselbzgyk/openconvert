Read CLAUDE.md and PROGRESS.md, then continue the OpenConvert implementation.

Do exactly one work item using the mandatory TDD loop (RED → minimal implementation → GREEN → regression
artefact → refactor → commit), then update PROGRESS.md (tick what is done, set the next CURRENT_ITEM, append
to the completed-items log).

If the current phase's Definition of Done now passes, verify it by actually running the gates, tick the phase,
and advance CURRENT_PHASE.

Stop immediately and set `STATUS: BLOCKED` with the question under `## Blocked` if you need an architectural
decision that docs/DECISIONS.md does not settle, or if a gate fails for a reason you cannot fix inside this
work item.

When Phase 15 is done and Appendix D (Definition of Done for v1.0) passes, set `STATUS: COMPLETE`.
