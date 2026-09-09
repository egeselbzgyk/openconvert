# PROGRESS — OpenConvert

<!-- Machine-readable state. Claude Code reads this first and rewrites it after every completed work item. -->

STATUS: IN_PROGRESS
CURRENT_PHASE: 0
CURRENT_ITEM: 0.1 — `oc-model::ids::BlockId` (tests 0.1 and 0.2 are written and RED; implement `BlockId` + `Rect` next)
LAST_UPDATED: 2026-09-09

---

## How to use this file

- `STATUS` is one of `IN_PROGRESS` · `BLOCKED` · `COMPLETE`.
- Set `STATUS: BLOCKED` **only** when a decision is needed that `docs/DECISIONS.md` does not settle. Write the question under `## Blocked` and stop.
- Tick a phase box only when its full Definition of Done (`IMPLEMENTATION_PLAN.md` §0.3) passes.
- Keep `## Notes` short: what a fresh session needs in order to resume, nothing else.

---

## Phases

- [ ] **Phase 0** — Repo, workspace, CI, thresholds, hello-Tauri, first Typst fixtures
      *(First Milestone: `cargo nextest` green on 3 OSes · `cargo deny` clean · `cargo xtask fixtures` builds f01/f02/f03 · `openconvert inspect <fixture>.pdf --json` matches committed insta snapshots · Tauri window shows `hello` engine version · `docs/TEST_MATRIX.md` written)*
      *(Also opens the Verification-debt table VD-a…VD-g; VD-a must close before `zip` is pinned.)*
- [ ] **Phase 1** — PDF inspection and ingestion  *(includes the PDFium image/SMask spike = VD-d)*
- [ ] **Phase 2** — Text assembly and normalization  *(normalization `N`, ledger, furniture inputs, language)*
- [ ] **Phase 3** — Layout  *(blocks, columns, reading order, paragraphs, dehyphenation; VD-b must close)*
- [ ] **Phase 4** — Structure  *(headings, outline/TOC, book structure, lists, footnotes, captions, quotes/verse, tables, images, metadata)*
- [ ] **Phase 5** — EPUB generation, Tier-1 validator, EPUBCheck CI gate
- [ ] **Phase 6** — Structural validation, repair loop, report, CI DOM checks  *(VD-f if the validation pack ships)*
- [ ] **Phase 7** — Corpus v1, eval harness, benchmarks, real-world holdout
- [ ] **Phase 8** — AI abstraction (no real model yet)
- [ ] **Phase 9** — Local model integration: sidecar lifecycle, model manager, promotion gate
- [ ] **Phase 10** — AI-assisted decisions (the four tasks)
- [ ] **Phase 11** — BYO providers
- [ ] **Phase 12** — Desktop UI  *(includes the early signing/notarization dry run)*
- [ ] **Phase 13** — OCR  *(VD-g must close)*
- [ ] **Phase 14** — Security hardening
- [ ] **Phase 15** — Packaging & release  *(then check Appendix D: Definition of Done for v1.0)*

## Current work item

**Phase 0, item 0.1 — `oc-model::ids::BlockId`.** Workspace bootstrap (§1.1–§1.9) is done and committed on
branch `phase/00-bootstrap`. Tests 0.1 (`ids::block_id_is_stable_for_same_inputs`) and 0.2
(`ids::prop_block_id_collision_suffix_is_unique`) are written in `crates/oc-model/src/ids.rs` and are RED
with the expected reason: `no BlockId in ids`, `no Rect in geom` (the sanctioned compile-error RED for the
first test of a new module, §0.2 step 1).

Next: implement `Rect` in `crates/oc-model/src/geom.rs` and `BlockId::{derive, with_collision_suffix,
as_str}` in `ids.rs` per D13.3 — `base32(blake3(page_index ‖ bbox rounded to 1 pt ‖ first 64 NFC chars))[..10]`
— then fill `GOLDEN_BLOCK_ID_CHAPTER_3` (currently `"__unset__"`) from the first green run, run
`cargo nextest run -p oc-model`, then `--workspace`, fmt/clippy, commit, tick this file.

## Blocked

_(empty)_

## Notes

- **Toolchain pin moved 1.85.0 → 1.98.1.** The plan's §1.3 pin cannot build the plan's own §1.2 dependency
  set (`zip 8.6`, `lopdf 0.45`, `image 0.25.10`, `libloading 0.9`, `time 0.3.55` all need ≥ 1.88; `aes 0.9.3`
  needs 1.89). Pinned to the stable current at bootstrap. Rationale and evidence: `docs/DECISIONS_LOG.md`.
- **`pdfium-render` features corrected** to `["pdfium_latest", "image_025", "thread_safe"]`; the plan's
  `["image", "libloading"]` do not exist in 0.9.4. `libloading` is unconditional, so D3's runtime binding is
  unaffected. See `docs/DECISIONS_LOG.md`.
- **`ureq` is not yet a dependency of `oc-net`.** It pulls `webpki-roots` (CDLA-Permissive-2.0), which is not
  on D15's allow-list, and Phase 0 implements no network code. The choice belongs to Phase 9; three options
  are written up in `docs/DECISIONS_LOG.md`. `cargo deny check` is clean today.
- **VD-a is closed** (zip 8.6.0 confirmed as the current stable major against the crates.io index).
  VD-b…VD-g still open, each with an owner and a blocking phase; stubs are in `docs/DECISIONS_LOG.md`.
- Local tool versions: rustc 1.98.1, cargo-nextest 0.9.143, cargo-deny 0.20.2. PDFium is **not** vendored yet
  (`xtask vendor-pdfium` is unimplemented), so test 0.7 will fail with `LibraryNotFound` until it lands.

## Completed items log

<!-- one line per finished work item: `YYYY-MM-DD  P<phase>.<item>  <what>  <commit sha>` -->
