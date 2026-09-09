# OpenConvert — working agreement for Claude Code

OpenConvert is an open-source, local-first desktop app that converts PDF → high-quality
**reflowable EPUB 3.3**. It is not a format converter; it is a deterministic
structure-inference pipeline with a conservation law, a validate→repair loop, and a
small local LLM used only for four narrow, once-per-book semantic decisions.

**Principle:** Deterministic first. AI where necessary. Validate everything. Repair only what is broken.

The design is finished and evidence-backed. **Your job is implementation, not redesign.**

---

## 1. Where everything is

| Path | What it is | How you use it |
|---|---|---|
| `docs/DECISIONS.md` | **The ADR (D1–D18). Highest authority.** | Read once per session. Never contradict. |
| `docs/IMPLEMENTATION_PLAN.md` | The phase-by-phase, test-first plan you execute | Your primary working document. §0 = conventions, §1 = bootstrap, §2 = CLI spec, then PHASE 0…15, then Appendices A–F. |
| `docs/ARCHITECTURE.md` | Process model, IR, conservation law, gates, AI adapter, repo layout | Read the section that covers the crate you are touching. |
| `docs/PIPELINE.md` | Stage-by-stage algorithms with parameters + the Deterministic-vs-LLM matrix | The algorithm reference while implementing a stage. |
| `docs/IR_SKETCH.md` | The intermediate representation (`oc-model`) | Authoritative for types, `Reason` enum, stage kinds. |
| `docs/TEST_STRATEGY.md` | Test tiers, test types, metrics, calibration | Consult when writing tests beyond the phase table. |
| `docs/TEST_CORPUS.md` | Corpus sources, licensing rules, fixture generation, ground truth | Phase 0 fixtures and Phase 7 corpus. |
| `docs/SECURITY.md`, `docs/LICENSE_AND_DEPENDENCIES.md`, `docs/UI_UX.md` | Security model, license rules, UI spec | Phases 14, dependency additions, Phase 12. |
| `docs/PROBLEM_ANALYSIS.md`, `TECHNOLOGY_EVALUATION.md`, `LLM_EVALUATION.md` | Why the decisions are what they are | Background only. Do not re-litigate. |
| `docs/REVIEW_REPORT.md` | Consistency/fact-check log + the Chief Architect's rulings | Read if two documents seem to disagree. |
| `research/round1/R1–R10`, `research/round2/` | Evidence archive (raw research, verification, red-team review) | **Never read wholesale.** Open only to verify one cited claim (e.g. "R2 §B.7"). |
| `PROGRESS.md` | **Live state.** Current phase, current item, blockers. | Read at the start of every session; update after every completed item. |
| `docs/DECISIONS_LOG.md` | Append-only log of spike results and small decisions you make | Create on first use. |

**Authority order when documents disagree:** `DECISIONS.md` → `ARCHITECTURE.md` / `PIPELINE.md` / `IR_SKETCH.md` → `IMPLEMENTATION_PLAN.md` → everything else.
If `DECISIONS.md` itself is wrong or silent on something load-bearing: **stop, write the question into `PROGRESS.md` under `## Blocked`, set `STATUS: BLOCKED`, and end the turn.** Do not guess an architectural decision.

---

## 2. How you work (non-negotiable)

**One work item at a time**, following the TDD loop from `IMPLEMENTATION_PLAN.md` §0.2:

1. **RED** — write the tests named in the phase's *Tests to write FIRST* table, with the exact names given. Run them. They must fail for the stated reason.
2. **Minimal implementation** — the least code that makes exactly those tests pass. No speculative generality.
3. **GREEN** — `cargo nextest run -p <crate>` then `cargo nextest run --workspace`.
4. **Regression artefact** — add the `insta` snapshot / `.assert.json` / corpus entry named in the table, in the same commit.
5. **Refactor** — `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, re-run tests. A snapshot that changes during refactor means behaviour changed: revert or justify.
6. **Commit** — `<crate>: <imperative summary>`, body with `Tests:` (names added) and `Refs:` (D-ids, R-sections).
7. **Update `PROGRESS.md`** — tick the item, set the next one, note anything learned.

### Hard rules (CI enforces these; do not work around them)
- **No `#[ignore]`** anywhere. If a test cannot run in an environment, gate it behind a cargo feature and add that feature to a CI job.
- **No numeric literal in production code.** Every constant comes from `thresholds.toml` via `oc-core::thresholds`. New constants need `{value, source, evidence, owner, review_by}`; `source = provisional` requires an owner and an unexpired `review_by`.
- **No `unwrap()`/`expect()`** outside tests and `main()`.
- **No `HashMap` iteration** in any code path that produces output — `BTreeMap`/`IndexMap` only. Output must be deterministic.
- **No `async`** in `oc-*` crates except `oc-net`. No lifetimes in public APIs. `#![forbid(unsafe_code)]` everywhere except the pdfium binding module.
- **No network dependency** (`reqwest`/`ureq`/`hyper`-client/…) in any crate except `oc-net`. `cargo deny` bans it.
- **No new dependency** without checking the `deny.toml` allow-list (MIT, Apache-2.0, BSD-2/3, ISC, MPL-2.0, Unicode, Zlib, CC0). AGPL/GPL/LGPL are banned in shipped crates — this rules out MuPDF/PyMuPDF, Poppler (except as an external CI-only oracle binary), epubveri, Ultralytics-derived weights.
- **The conservation law is checked after every stage** (`ARCHITECTURE.md` §5, invariants I-1…I-7). A stage that removes or adds non-whitespace characters must declare a `Reason` and stay inside its budget.
- **`ai.enabled = false` is the v1 default.** The deterministic path must be complete and correct on its own; the LLM is an opt-in improvement that must prove itself (McNemar non-inferiority, false-repair rate ≤ 1 % per category) before it is enabled.

### Definition of Done for a phase (`IMPLEMENTATION_PLAN.md` §0.3)
All of: every named test exists and passes · `cargo nextest run --workspace` green on Linux/macOS/Windows CI · clippy `-D warnings` clean · `cargo fmt --check` clean · `cargo deny check` clean · `cargo xtask thresholds-lint` clean · every Given/When/Then acceptance criterion demonstrated by a named test or CI job · `docs/CHANGELOG.md` phase entry · no `TODO`/`FIXME` without an issue number.

**Never mark a phase done with a failing, skipped, or ignored test.**

---

## 3. Stack facts you will need constantly

Rust core + CLI engine (`openconvert`), Tauri 2 + Svelte 5 UI that spawns the CLI as a subprocess (job-spec file = the single argument; NDJSON events on **stderr**; stdout is data only; exit codes 0/1/2/3/101; cancellation honoured within 2 s).
PDF: **PDFium** via `pdfium-render` 0.9.4 + `lopdf` 0.45 behind a `PdfBackend` trait. EPUB: hand-rolled 3.3 via `quick-xml` + `zip` through a typed builder that makes invalid markup unrepresentable. LLM: bundled `llama-server` sidecar, default model **Qwen3-1.7B** (Qwen3.5-2B is experimental, promotable only through the nine-gate test). OCR: system `tesseract` if present; optional signed pack later. License: **Apache-2.0**.

Stages (exact names, used in code, events, `--dump-stage`, and docs):
`inspect · ingest · text · furniture · layout · paragraphs · structure · document · epub · validate · repair · report`

Crates: `oc-model · oc-pdf · oc-text · oc-layout · oc-structure · oc-epub · oc-validate · oc-ai · oc-net · oc-core` + `openconvert` (bin) + `oc-testkit` (dev) + `xtask` (dev) + `apps/desktop` + `eval/` (Python, never shipped) + `corpus/`.

Phases: 0 bootstrap · 1 PDF inspection/ingestion · 2 text · 3 layout · 4 structure · 5 EPUB+Tier-1 · 6 structural validation/repair/report · 7 corpus+eval · 8 AI abstraction · 9 local model · 10 AI decisions · 11 BYO providers · 12 desktop UI · 13 OCR · 14 security hardening · 15 packaging/release.

---

## 4. Session protocol

At the **start** of every session: read `PROGRESS.md`, then the current phase's section of `IMPLEMENTATION_PLAN.md`. Do not re-read all docs.
During work: prefer `rg`/`sed -n` over reading whole files; the plan is ~36k words.
When context gets tight: finish the current work item, commit, update `PROGRESS.md`, then `/compact`.
At the **end** of every turn: `PROGRESS.md` must be accurate enough that a fresh session can resume from it alone.

If you finish the last item of Phase 15 and the v1.0 Definition of Done in Appendix D passes, set `STATUS: COMPLETE` in `PROGRESS.md`.
