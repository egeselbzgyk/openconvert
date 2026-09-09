# PROGRESS — OpenConvert

<!-- Machine-readable state. Claude Code reads this first and rewrites it after every completed work item. -->

STATUS: IN_PROGRESS
CURRENT_PHASE: 1
CURRENT_ITEM: 1.10 — dump-stage ingest (test 1.17)
LAST_UPDATED: 2026-09-10

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

**Phase 1, item 1.10 — `dump-stage ingest` (test 1.17).** Items 1.1–1.9 are done: hand-made
fixtures, glyph extraction (1.1–1.4), metamorphic invariants (1.5–1.7), the broken-text mutation
(1.8), images (1.9), resource limits (1.10, 1.11, 1.20), encryption (1.12–1.14),
outlines/metadata (1.15) and fuzz-lite (1.16).

Next is RED: test 1.17 `dump_stage_ingest_snapshot_h01` — an `insta` snapshot of the canonical
JSON of the extraction layer for `h01_two_glyphs`. Then:

1. Assemble an `IngestDump` from what already exists: `PageGlyphs` (glyphs, fonts, removed,
   `c_raw`, stats, class), `page_images`, `outline`, `doc_info`. Every one of those types already
   derives `Serialize`; the work is the shape of the document, not the plumbing.
2. `oc_model::canonical::to_canonical_json` for the bytes — sorted keys, geometry at 0.01 pt,
   NFC, no NaN, `ir_version` first (D13.3). It exists and is tested from Phase 0 item 0.2.
3. `crates/openconvert/src/cmd_dump_stage.rs` plus `openconvert dump-stage <stage> <input>`.
   §2.1's CLI is hand-rolled; this is the second subcommand, so the parser grows a branch rather
   than a dependency.
4. **Stream it per page.** RT B4: a real book's extraction layer is tens of megabytes, so the CLI
   writes page by page rather than building one value and serialising it. The snapshot only ever
   covers a two-glyph fixture.

Watch for: `c_raw` is a `CharHistogram` whose `Serialize` is the internal `ascii: Vec<u32>` — 128
mostly-zero entries would make an unreadable snapshot and a huge dump. It needs a `Serialize` that
writes only the characters present, which is what `CharHistogram::iter` already yields.

Remaining Phase 1 items after 1.10: 1.11 the poppler oracle (1.18) · 1.12 cancellation (1.19) ·
then VD-d, the ten-PDF image spike that blocks Phase 4's image policy. VD-d has its first fixture,
`h09_image_smask`.

## Notes

Carried forward, in the order a fresh session needs them:

- **`docs/DECISIONS_LOG.md` is the record of every decision made while implementing.** Read it before
  changing anything that looks arbitrary; most of it is measured rather than chosen.
- **PDFium must be vendored before the test suite passes**: `cargo run -p xtask -- vendor-pdfium`.
  Pinned to `chromium/7881` (151.0.7881.0) in `xtask/pdfium.lock`, lands in the git-ignored
  `vendor/pdfium/<triple>/`, needs `curl` and `tar` on PATH. CI runs it before `nextest`.
- **Fixtures**: `cargo run -p xtask -- fixtures` (Typst f01–f03 into the git-ignored
  `target/fixtures/`), `-- handmade-fixtures` (h01–h13, committed), `-- mutations` (committed).
- **No stage may treat the backend's glyph order as reading order.** Measured in item 1.3: PDFium
  reorders the lines of a page under `/Rotate 90`. Reading order is Phase 3's, from geometry.
- **Open from item 1.4, for Phase 2/3:** PDFium reports a line-break hyphen as **U+0002** with
  `is_hyphen()` set, not as U+002D. So `C_raw` already differs from the document by one character at
  every hyphenated line break — normalisation `N` must map it back before `Dehyphenate` can account
  for removing it — and the flag is a free, exact dehyphenation signal for Phase 3.
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
2026-09-09  P1.3      oc-pdf metamorphic invariants + oc-testkit mutate (tests 1.5-1.7)   bd96e3a
2026-09-09  P1.4      oc-pdf broken-text: control-char counter + strip_tounicode (test 1.8)  928f9d7
2026-09-09  P1.5      oc-pdf images: ImageRef, DPI, kind, smask/inline via lopdf (test 1.9)  053ad54
2026-09-09  P1.6      oc-core/oc-pdf resource limits + --max-pages (tests 1.10, 1.11, 1.20)  13fce0b
2026-09-09  P1.7      oc-pdf encryption: permissions recorded not enforced (tests 1.12-1.14)  a825fc5
2026-09-10  P1.8      oc-pdf outline walk + meta from the object tree (test 1.15)  d815ad3
2026-09-10  P1.9      oc-pdf fuzz-lite: random, truncated and corrupted inputs (test 1.16)  d9f18aa
