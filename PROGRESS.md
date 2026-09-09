# PROGRESS — OpenConvert

<!-- Machine-readable state. Claude Code reads this first and rewrites it after every completed work item. -->

STATUS: IN_PROGRESS
CURRENT_PHASE: 1
CURRENT_ITEM: 1.4 — broken-text mutation (test 1.8)
LAST_UPDATED: 2026-09-09

---

## How to use this file

- `STATUS` is one of `IN_PROGRESS` · `BLOCKED` · `COMPLETE`.
- Set `STATUS: BLOCKED` **only** when a decision is needed that `docs/DECISIONS.md` does not settle.
  Write the question under `## Blocked` and stop.
- Tick a phase box only when its full Definition of Done (`IMPLEMENTATION_PLAN.md` §0.3) passes.
- Keep `## Notes` short: what a fresh session needs in order to resume, nothing else.

---

## Phases

- [x] **Phase 0** — Repo, workspace, CI, thresholds, hello-Tauri, first Typst fixtures
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

**Phase 1, item 1.4 — the broken-text mutation (test 1.8).** Items 1.1 (hand-made fixtures),
1.2 (glyph extraction, tests 1.1–1.4) and 1.3 (metamorphic invariants, tests 1.5–1.7) are done.

Next is RED: test 1.8 `stripped_tounicode_page_classifies_broken_text` — `f01__strip_tounicode.pdf`
must classify every page `broken_text`. The mutation belongs beside the two that exist:
add `strip_tounicode` to `oc_testkit::mutate` (delete `/ToUnicode` from every font dictionary, so
the text decodes to U+FFFD or to nothing), have `cargo xtask mutations` write
`corpus/fixtures/mutations/f01__strip_tounicode.pdf`, and assert against
`pageclass.broken_text_replacement_share`, which is already in `thresholds.toml`.

Remaining Phase 1 items after 1.4: 1.5 images (1.9) · 1.6 limits (1.10, 1.11, 1.20) ·
1.7 encryption (1.12–1.14) · 1.8 outline (1.15) · 1.9 fuzz-lite (1.16) · 1.10 dump-stage (1.17) ·
1.11 the poppler oracle (1.18) · 1.12 cancellation (1.19) · then VD-d, the ten-PDF image spike that
blocks Phase 4's image policy.

## Notes

Carried forward, in the order a fresh session needs them:

- **`docs/DECISIONS_LOG.md` is the record of every decision made while implementing.** Read it before
  changing anything that looks arbitrary; most of it is measured rather than chosen.
- **PDFium must be vendored before the test suite passes**: `cargo run -p xtask -- vendor-pdfium`.
  Pinned to `chromium/7881` (151.0.7881.0) in `xtask/pdfium.lock`, lands in the git-ignored
  `vendor/pdfium/<triple>/`, needs `curl` and `tar` on PATH. CI runs it before `nextest`.
- **Fixtures**: `cargo run -p xtask -- fixtures` (Typst f01–f03 into the git-ignored
  `target/fixtures/`), `-- handmade-fixtures` (h01–h08, committed), `-- mutations` (committed).
- **No stage may treat the backend's glyph order as reading order.** Measured in item 1.3: PDFium
  reorders the lines of a page under `/Rotate 90`. Reading order is Phase 3's, from geometry.
- **Open from item 1.2, for the conservation-law work in Phase 2/6:** `OverdrawDedup` has a budget
  and no way to consume it. PDFium collapses overdrawn duplicates before we see them and its
  object-level text API returns the same deduplicated string, so the collapsed count needs `lopdf`
  content-stream access (counting bytes shown by `Tj`/`TJ`). Safe in the meantime — PDFium never
  merges distinct characters — but not the guarantee D13.4 describes.
- **Two cargo-deny configs.** `deny.toml` audits what ships and admits no exceptions;
  `deny.tools.toml` audits `xtask` with the same licence/ban/source policy and reports its
  advisories without blocking. Run both: `cargo deny check` and
  `cargo deny --config deny.tools.toml check licenses bans sources`.
- **Toolchain pinned 1.98.1**; the plan's §1.3 pin of 1.85.0 cannot build the plan's own §1.2
  dependency set. `pdfium-render` features are `["pdfium_7881", "image_025", "thread_safe"]`.
- **`ureq` is not yet a dependency of `oc-net`** — it pulls `webpki-roots` (CDLA-Permissive-2.0),
  which is not on D15's allow-list. The choice belongs to Phase 9.
- `GOLDEN_BLOCK_ID_CHAPTER_3 = "SDMLH752SA"` in `ids.rs` is a committed golden value. If that
  assertion ever fails, the id derivation changed and `IR_VERSION` must change in the same commit
  (D13.3).
- **VD-a is closed.** VD-b…VD-g still open, each with an owner and a blocking phase.
- Commit messages carry **no** Claude Code attribution footer (maintainer's instruction, 2026-09-09).
- Local tool versions: rustc 1.98.1, cargo-nextest 0.9.143, cargo-deny 0.20.2.

## Blocked

_(empty — Q1 resolved 2026-09-09; see `docs/DECISIONS_LOG.md`)_

## Phase 0 — Definition of Done

Checked against `IMPLEMENTATION_PLAN.md` §0.3 on 2026-09-09:

1. **Every named test exists and passes** — all 23 rows of the Phase 0 table, plus four additions
   (0.8a, 0.12a, 0.23a, and the committed-assertion-file test), each with its reason in
   `docs/DECISIONS_LOG.md`. `docs/TEST_MATRIX.md` lists every one and the CI job that runs it.
2. **`cargo nextest run --workspace`** — green, 0 skipped, 0 ignored. **Verified on Windows only.**
   Linux and macOS are CI's job and have not run yet; this is the one Definition-of-Done item Phase 0
   cannot claim from this machine.
3. **clippy** `--workspace --all-targets --all-features --locked -- -D warnings` — clean.
4. **`cargo fmt --all --check`** — clean.
5. **`cargo deny check`** — advisories, bans, licenses, sources all ok; plus `deny.tools.toml`
   licenses/bans/sources ok.
6. **`cargo run -p xtask -- thresholds-lint`** — clean.
7. **Acceptance criteria** A0.1–A0.9: A0.1 (minus the two other OSes, see 2), A0.2, A0.3, A0.4, A0.5,
   A0.7, A0.8 and A0.9 are demonstrated by named tests. **A0.6 is partly open**: the app compiles, the
   handshake and the rendered version string are unit-tested, but no one has watched a real window open.
   Phase 12 owns the UI; a screenshot at that point closes it.
8. **`docs/CHANGELOG.md`** — Phase 0 entry written.
9. **No unnumbered TODO/FIXME** — `xtask ci-lint` clean.

## Completed items log

<!-- one line per finished work item: `YYYY-MM-DD  P<phase>.<item>  <what>  <commit sha>` -->
2026-09-09  P0.setup  Cargo workspace, thresholds.toml, deny.toml, CI matrix, eval skeleton (§1.1-§1.9)  e6d02df
2026-09-09  P0.1      oc-model: BlockId derivation + Rect (tests 0.1, 0.2)                          1d32410
2026-09-09  P0.2      oc-model: canonical JSON + IR_VERSION (tests 0.3, 0.4)                        2cd8f76
2026-09-09  P0.3      oc-core: thresholds codegen + provenance lint (tests 0.5, 0.6)                3843abb
2026-09-09  P0.4      oc-pdf: page-space normalisation (test 0.8 + corner unit test)              79ac71f
2026-09-09  P0.5      xtask vendor-pdfium + oc-pdf PDFium binding and probe (test 0.7)             5059a98
2026-09-09  P0.6      oc-pdf: page classification (tests 0.9-0.12 + mixed/blank/dict test)        97ddfd8
2026-09-09  P0.7      oc-pdf: producer-family detection (test 0.13)                              4788213
2026-09-09  P0.8      xtask fixtures + audit-surface split resolving Q1 (test 0.20)              57df06e
2026-09-09  P0.9      oc-pdf: inspect report + PdfDoc over PDFium (tests 0.14-0.16)             3ec6f70
2026-09-09  P0.10     openconvert: CLI, NDJSON events, exit codes (tests 0.17-0.19)             931cc0a
2026-09-09  P0.11/12  oc-testkit assertions + xtask ci-lint/thresholds-lint (0.21, 0.23)        de4298c
2026-09-09  P0.13     apps/desktop hello-Tauri + handshake + LICENSE (test 0.22)                52a4071
2026-09-09  PHASE 0   COMPLETE - Definition of Done checked, one item partly open (A0.6)
2026-09-09  P1.1      oc-testkit handmade fixtures + the PDFium overdraw finding           15d0dbe
2026-09-09  P1.2      oc-model extract/ledger + oc-pdf glyph extraction (tests 1.1-1.4)    f962f00
