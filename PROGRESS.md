# PROGRESS — OpenConvert

<!-- Machine-readable state. Claude Code reads this first and rewrites it after every completed work item. -->

STATUS: IN_PROGRESS
CURRENT_PHASE: 3
CURRENT_ITEM: 3.1 — read PHASE 3 of the plan, then its first work item
LAST_UPDATED: 2026-09-13

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
- [x] **Phase 1** — PDF inspection and ingestion  *(VD-d closed: PDFium composites masks, palettes and colour spaces correctly)*
- [x] **Phase 2** — Text assembly and normalization  *(all 22 named tests green; EN frequency list ships, DE/TR blocked on a D15 licence decision)*
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

**Phase 3 has not been started.** Phase 2 is complete — its Definition of Done is checked below.

First step: read `docs/IMPLEMENTATION_PLAN.md` PHASE 3 and `docs/PIPELINE.md`'s `layout` and
`paragraphs` stages, then take the first work item with the TDD loop.

Phase 3 is blocks, columns, reading order, paragraph reconstruction and dehyphenation, and
**VD-b must close**. Three things land on it from earlier phases, in priority order:

1. **The soft-vs-hard hyphen distinction, and `OverdrawDedup`'s unconsumable budget.** Both need
   the same mechanism — `lopdf` access to the `Tj`/`TJ` operands, decoded against each font's
   `/ToUnicode` — and both are dehyphenation inputs or conservation inputs rather than
   extraction ones. Item 2.4 closed the half that mattered for `C_raw` (PDFium's U+0002 marker
   now decodes to U+002D); what is left is knowing *which* hyphen it was, which PIPELINE §369's
   compound-word rule needs so that `Nord-Süd-Achse` is not rejoined.
2. **Columns.** `text` clusters a line by baseline alone, so two columns printed at the same
   height are one line and `Line::indent_pt` / `right_gap_pt` are measured against the span of
   both. That is the specified ordering (R2 §D.3, furniture before segmentation) and Phase 3 is
   where it is repaired — the paragraph rules that read those fields are written against the
   field, not against the stage that filled it in.
3. **A budget breach is currently fatal, and ARCHITECTURE §5.5 says it should not be.** The
   checker is right to refuse to call it fine; the *policy* — stop that stage's remaining
   removals, warn, finish the book — belongs to the orchestrator, and the orchestrator arrives
   in Phase 6. Until then a breach aborts the conversion. Recorded so it is a decision rather
   than an oversight.

Also open, and cheap, and more valuable now than it was: CI's `test` job runs `xtask fixtures`
but never `handmade-fixtures` or `mutations`, so a builder change that no longer reproduces the
committed fixtures is not caught. Phase 2 changed that builder three times — `text_at`,
`char_spacing`, `build_pages`, and the `/ToUnicode` object that renumbered h13's refs — and each
time the check was "run it and read the CHANGED lines by eye". A `--check` mode on those two
tasks would close it.

## Notes

Carried forward, in the order a fresh session needs them:

- **`docs/DECISIONS_LOG.md` is the record of every decision made while implementing.** Read it before
  changing anything that looks arbitrary; most of it is measured rather than chosen.
- **PDFium must be vendored before the test suite passes**: `cargo run -p xtask -- vendor-pdfium`.
  Pinned to `chromium/7881` (151.0.7881.0) in `xtask/pdfium.lock`, lands in the git-ignored
  `vendor/pdfium/<triple>/`, needs `curl` and `tar` on PATH. CI runs it before `nextest`.
- **Fixtures**: `cargo run -p xtask -- fixtures` (Typst f01–f03 into the git-ignored
  `target/fixtures/`), `-- handmade-fixtures` (h01–h15, committed), `-- mutations` (committed).
- **No stage may treat the backend's glyph order as reading order.** Measured in item 1.3: PDFium
  reorders the lines of a page under `/Rotate 90`. Reading order is Phase 3's, from geometry.
- **The hyphen marker is half closed (item 2.4).** Extraction now decodes PDFium's U+0002 to
  U+002D, so `C_raw` and every run text are clean. What is *not* recovered is whether the
  source wrote U+002D or U+00AD — PDFium collapses both — so D13.4's `SoftHyphen` reason fires
  only for a U+00AD that arrives un-printed, and PIPELINE §369's compound-word rule
  (`Nord-Süd-Achse` must not be rejoined) still needs its evidence from somewhere else.
  Recovering the distinction needs `lopdf` content-stream access to the `Tj`/`TJ` operands —
  **the same mechanism the `OverdrawDedup` gap below needs** — and is a dehyphenation input,
  so both belong to **Phase 3**.
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
- **CI works now, and 2026-09-13 was the first time it ever ran.** `ci` triggers on pushes to
  `main` and on pull requests; Phases 0-2 all happened on `phase/00-bootstrap`, which is neither.
  Five of eight jobs failed on the first run. Four fixes, all in `docs/DECISIONS_LOG.md`: the
  Tauri crate is `--exclude`d from the engine jobs and gets its own `desktop` job (it has no Rust
  tests, and buying "it compiles" inside `--workspace` costs every job a GUI toolchain); `deny`
  needed its global flags before `check`; the tagged fixtures were never built; and `no-network`
  reached the registry from inside the namespace. **Work on a branch that opens a pull request
  from Phase 3 onward** — that is what makes `ci` fire on every push.
- **Three CI jobs are `if: false` until their phase arrives**, because the commands they call do
  not exist: `epubcheck` (Phase 5), `dom-checks` (Phase 6) and `no-network`'s
  `assert-no-net-deps` step (Phase 14). Each carries a comment naming the phase. Until Phase 14
  the socket ban is enforced by `deny.toml`'s `wrappers` rule in the `deny` job.
- **A non-embedded base-14 font makes glyph bounding boxes host-dependent**, measured on h01:
  `bbox.x1` is 80.02 on Windows and macOS, 79.85 on Ubuntu. The *advance* is identical, because
  the widths come from the PDF's own metrics. D13.8's determinism contract therefore cannot hold
  for such documents — the substitution happens below us — so **Phase 3's layout rules should
  prefer the advance box and the origin, which are stable, wherever they have the choice.**
- **VD-a and VD-d are closed.** VD-b, VD-c, VD-e, VD-f, VD-g still open, each with an owner
  and a blocking phase. VD-b blocks Phase 3.
- **Open for `DECISIONS.md`, from item 2.9:** which sources build the German and Turkish
  word-frequency lists. The plan names DTA plain text and Wikisource-TR as "CC0/PD"; their
  transcriptions are CC-BY-SA, which D15 does not allow in a shipped artefact. English ships
  from CC0 Standard Ebooks. Rebuild with
  `cd eval && PYTHONPATH=src python -m oc_eval.generate.wordfreq en --out ../crates/oc-text/src/freq`.
- **R2 §B.8 is not reproduced on PDFium `chromium/7881`:** it expands the U+FB00–FB06 ligatures
  itself, even with a `/ToUnicode` CMap declaring U+FB01. `N` keeps its ligature table anyway —
  the contract is about the text, not about which component expanded it — but `LigatureExpand`
  will rarely fire on PDFium-sourced text, so its `Added` side is exercised by generated glyph
  streams rather than by any PDF.
- **`example_pdfs/` is the maintainer's local smoke set, added 2026-09-13.** Four real books —
  English, German, Portuguese and a Turkish scan — git-ignored and never redistributed; two of the
  four are in copyright. Not corpus, not holdout, no threshold fitted on it. It is what to point
  `convert` at from **Phase 5** onward, when an EPUB first comes out the other end. Described in
  `docs/TEST_CORPUS.md` §7.5a, including which two could become real corpus entries.
- Commit messages carry **no** Claude Code attribution footer (maintainer's instruction, 2026-09-09).
- Local tool versions: rustc 1.98.1, cargo-nextest 0.9.143, cargo-deny 0.20.2.

## Blocked

_(empty — Q1 resolved 2026-09-09; see `docs/DECISIONS_LOG.md`)_

## Phase 2 — Definition of Done

Checked against `IMPLEMENTATION_PLAN.md` §0.3 on 2026-09-13:

1. **Every named test exists and passes** — all twenty-two rows of the Phase 2 table (2.1–2.22),
   plus about forty additions, each of which exists because something was measured and was not
   what the plan assumed. `docs/TEST_MATRIX.md` lists every one and the CI job that runs it.
2. **`cargo nextest run --workspace`** — 156 passed, 0 skipped, 0 ignored, **on all three
   operating systems**. CI run 34758095844 on `main`, 2026-09-13: `test (ubuntu-latest)`,
   `test (macos-latest)` and `test (windows-latest)` all green, alongside `lint`, `deny`,
   `desktop`, `no-network`, `poppler-oracle` and `ui`. This is the first time that claim has
   been true rather than deferred — see the Notes below.
3. **clippy** `--workspace --all-targets --all-features --locked -- -D warnings` — clean.
4. **`cargo fmt --all --check`** — clean.
5. **`cargo deny check`** — advisories, bans, licenses, sources ok; `deny.tools.toml` ok.
6. **`cargo run -p xtask -- thresholds-lint`** — clean.
7. **Acceptance criteria A2.1–A2.6.**
   - A2.1 (I-1…I-4 hold, a violation is fatal) by `conservation_i1_holds_across_text_and_furniture`
     over `f01`/`f02` through the real checker, `conservation_i1_holds_over_generated_documents`
     over 200 generated documents, and `a_budget_breach_stops_the_stage`.
   - A2.2 (`"The Test Book"` and the page numbers absent from flow, present in the ledger) by
     tests 2.10 and 2.11.
   - A2.3 (furniture ≤ 4 % of `|C_0|`) enforced by `check_invariants` on every stage run and
     demonstrated on `f01` and `f02`; **"any corpus file" is Phase 7's corpus**, which does not
     exist yet, so this is demonstrated on the fixtures rather than at the stated scope.
   - A2.4 (10 000 random strings: idempotent, NFC, no NFKC, no case folding) by
     `normalize_is_idempotent` (10 000 cases), `normalize_never_applies_nfkc`,
     `normalize_composes_to_nfc` and `text_is_never_case_folded_in_output`.
   - A2.5 (EN/DE/TR `dc:language`) by test 2.19 over `f01`, `f04` and `f05`.
   - A2.6 (Turkish folding, emitted text unchanged) by tests 2.6 and 2.7.
8. **`docs/CHANGELOG.md`** — Phase 2 entry written.
9. **No unnumbered TODO/FIXME** — `xtask ci-lint` clean.

**One deliverable is deliberately partial, with the maintainer's agreement.** PLAN Phase 2
detail 5 asks for EN/DE/TR word-frequency lists. English ships (20 000 words from twelve CC0
Standard Ebooks, with a source manifest). German and Turkish do not: the plan names DTA plain
text and Wikisource-TR as "CC0/PD" and their transcriptions are CC-BY-SA, which is not on D15's
allow-list for a shipped artefact. The generator refuses them mechanically. `dict_hit_rate`
returns `None` for both — not zero — so nothing downstream misreads the absence as evidence.
**Open for `DECISIONS.md`: which sources build the DE and TR lists.**

## Phase 1 — Definition of Done

Checked against `IMPLEMENTATION_PLAN.md` §0.3 on 2026-09-10:

1. **Every named test exists and passes** — all twenty rows of the Phase 1 table (1.1–1.20), plus
   about twenty more, each of which exists because something was measured and was not what the
   plan assumed. `docs/TEST_MATRIX.md` lists every one and the CI job that runs it.
2. **`cargo nextest run --workspace`** — 83 passed, 0 skipped, 0 ignored; with
   `--features poppler-oracle`, 55 passed in `oc-pdf`. Verified on Windows at the time; the
   three-OS claim was cashed on 2026-09-13, when CI first ran (see Notes), and it needed two
   fixes in this phase's code to hold — the tagged-fixture dependency and h01's host-dependent
   glyph boxes.
3. **clippy** `--workspace --all-targets --all-features --locked -- -D warnings` — clean.
4. **`cargo fmt --all --check`** — clean.
5. **`cargo deny check`** — advisories, bans, licenses, sources ok; `deny.tools.toml` ok.
6. **`cargo run -p xtask -- thresholds-lint`** — clean.
7. **Acceptance criteria A1.1–A1.6.** A1.1 by `acceptance::every_glyph_carries_thirteen_real_
   signals` over all eleven fixtures; A1.2 by test 1.5; A1.3 by tests 1.10 and VD-d.7; A1.4 by
   test 1.8; A1.5 by 1.16. **A1.6 measured** on a 301-page Typst book: 10.2 ms/page against a
   150 ms budget, 98 MB peak RSS against 250 MB — but **not on D9's reference machine L**, so it
   is indicative rather than signed off. Phase 7's benchmark harness measures it properly.
8. **`docs/CHANGELOG.md`** — Phase 1 entry written.
9. **No unnumbered TODO/FIXME** — `xtask ci-lint` clean.

**VD-d closed**: PDFium's `get_processed_image()` composites soft masks, stencil masks, indexed
palettes and DeviceGray correctly, so the image policy uses it and writes no compositing of its
own. CMYK JPEG, 1-bit CCITT and JPX are uncovered — no encoder exists to build those fixtures
honestly — and are deferred to real samples in the Phase 7 corpus.

## Phase 0 — Definition of Done

Checked against `IMPLEMENTATION_PLAN.md` §0.3 on 2026-09-09:

1. **Every named test exists and passes** — all 23 rows of the Phase 0 table, plus four additions
   (0.8a, 0.12a, 0.23a, and the committed-assertion-file test), each with its reason in
   `docs/DECISIONS_LOG.md`. `docs/TEST_MATRIX.md` lists every one and the CI job that runs it.
2. **`cargo nextest run --workspace`** — green, 0 skipped, 0 ignored. Verified on Windows at the
   time, and the entry said plainly that Linux and macOS "have not run yet". They ran for the
   first time on 2026-09-13 and are green (see Notes). The caveat was correct and it stood for
   four days longer than anyone noticed, because `ci` triggers on `main` and this phase was
   never on it.
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
2026-09-10  P1.10     oc-pdf dump + openconvert dump-stage ingest (test 1.17)  0ccbd1b
2026-09-10  P1.11     oc-pdf differential pdftotext oracle behind a feature (test 1.18)  2a9aa8f
2026-09-10  P1.perf   oc-pdf: page ids read once, not per page (O(n^2) fix)                6908601
2026-09-10  P1.12     oc-core cancel/progress + openconvert control channel (test 1.19)  69734e5
2026-09-10  P1.13     oc-pdf image_bytes + VD-d known-answer spike (VD-d closed)             c73476e
2026-09-10  PHASE 1   COMPLETE - Definition of Done checked; A1.6 measured off reference machine L
2026-09-13  P2.1      oc-core ledger_check: I-1..I-4 + stage declarations (2.16, 2.17 + 6)  fad617e
2026-09-13  P2.2      oc-text normalize: N, ledgered on both sides (2.1-2.4 + 3)             b7a822c
2026-09-13  P2.3      oc-text fold_key + oc-model LangTag (2.6, 2.7 + 4)                     b5e08ad
2026-09-13  P2.4      oc-pdf: decode the U+0002 hyphen marker at extraction (3 tests)        e5ada15
2026-09-13  P2.5      oc-text words/lines + oc-model text layer, h16-h18 (2.5, 2.8, 2.9 + 9) c49ef42
2026-09-13  P2.6      oc-layout furniture + h19-h21 (2.10-2.14 + 3)                          787002f
2026-09-13  P2.7      openconvert lib: text+furniture under check_invariants (2.15 + 4)      9ce1585
2026-09-13  P2.8      oc-text stats: the nine Gopher numbers + verdict (2.18 + 5)            b20c293
2026-09-13  P2.doc    example_pdfs recorded in TEST_CORPUS 7.5a as the local smoke set       b7f2037
2026-09-13  P2.9      oc-text freq + wordfreq.py; EN ships, DE/TR blocked on D15 (2.22 + 7)  bb11165
2026-09-13  P2.10     oc-text lang + f04/f05 fixtures (2.19, 2.20 + 7)                       a184c12
2026-09-13  P2.11     openconvert dump-stage text + snapshot; min_space_ratio (2.21)         b3949e9
2026-09-13  PHASE 2   COMPLETE - Definition of Done checked; DE/TR frequency lists open (D15)
