# PROGRESS — OpenConvert

<!-- Machine-readable state. Claude Code reads this first and rewrites it after every completed work item. -->

STATUS: IN_PROGRESS
CURRENT_PHASE: 5
CURRENT_ITEM: 5.1 — read PHASE 5 of the plan, then its first work item
LAST_UPDATED: 2026-09-14

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
- [x] **Phase 3** — Layout  *(all 17 named tests green; VD-b closed. A3.2 partial — no corpus to reproduce a gold order from, Phase 7; A3.3 partial — the holdout is English, because D15 has no German source)*
- [x] **Phase 4** — Structure  *(all 21 named tests green, plus about fifty additions; A4.3 partial — heading F1 needs the Phase 7 corpus)*
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

**Phase 4 is complete.** Its Definition of Done is checked below, with the one partial row named
as partial.

First step for Phase 5: read `docs/IMPLEMENTATION_PLAN.md` PHASE 5 and `docs/PIPELINE.md` §9–§10,
then take the first work item with the TDD loop.

Phase 5 is EPUB generation, the Tier-1 validator and the EPUBCheck CI gate. It is the first phase
whose output a person can open.

**What Phase 4 hands it**, in the order it will be wanted:

1. **`oc_structure::stage::structure` produces everything but the `Document`.** `Section` trees,
   `Note`s with their bodies, `Figure`s, `Table`s, `List`s, `Metadata`, the escalation candidates
   and the warnings. `Document` itself is the `document` stage's (PIPELINE §9) and is deliberately
   not built yet: it needs `page_breaks`, `DocClass` and `PresetName`, none of which exists.
2. **The list marker is still in the item's text**, with `ListItem.marker` beside it. `structure`
   may not remove it — `Reason` has no variant for a list marker and the stage is Conserving — so
   `epub` is the stage that elides the prefix when it emits `<ol>`, and how it *declares* that is
   Phase 5's question. `docs/DECISIONS_LOG.md` (2026-09-14) has the reasoning.
3. **A fallback table carries its data.** `Table.fallback_image` is `Some` and `rows` holds one
   cell per printed line, because an image of a table takes the content away from anyone who
   cannot see it (DAISY, R10 §6.12). `epub` emits the image *and* the `<details>`; the
   rasterising itself is Phase 5's, from `TableRegion.bbox` at `images.vector_raster_scale`.
4. **`ImageId` means two things and the boundary is `document_images`.** The backend numbers
   images per *page*, because `image_bytes(page, id)` indexes that page's draw order;
   `openconvert::structure_input::document_images` renumbers them across the book for `Figure`.
   The page-local index is recoverable as the image's position among those sharing its page.
5. **A fallback table's `ImageId`s continue past the document's**, so they name images the file
   did not contain. `extract_tables` takes the first free id as an argument.
6. **Fixture numbers.** Taken: **f01–f10**, **h01–h29**. Next free: **f11**, **h30**.

Still open, and cheap: CI's `test` job runs `xtask fixtures` but never `handmade-fixtures` or
`mutations`, so a builder change that no longer reproduces the committed fixtures is not caught.
A `--check` mode on those two tasks would close it. Phase 4 changed the builder again (rules,
placed images, the Info dictionary and the XMP packet).

## Notes` short: what a fresh session needs in order to resume, nothing else.

---

## Phases

- [x] **Phase 0** — Repo, workspace, CI, thresholds, hello-Tauri, first Typst fixtures
      *(First Milestone: `cargo nextest` green on 3 OSes · `cargo deny` clean · `cargo xtask fixtures` builds f01/f02/f03 · `openconvert inspect <fixture>.pdf --json` matches committed insta snapshots · Tauri window shows `hello` engine version · `docs/TEST_MATRIX.md` written)*
      *(Also opens the Verification-debt table VD-a…VD-g; VD-a must close before `zip` is pinned.)*
- [x] **Phase 1** — PDF inspection and ingestion  *(VD-d closed: PDFium composites masks, palettes and colour spaces correctly)*
- [x] **Phase 2** — Text assembly and normalization  *(all 22 named tests green; EN frequency list ships, DE/TR blocked on a D15 licence decision)*
- [x] **Phase 3** — Layout  *(all 17 named tests green; VD-b closed. A3.2 partial — no corpus to reproduce a gold order from, Phase 7; A3.3 partial — the holdout is English, because D15 has no German source)*
- [x] **Phase 4** — Structure  *(all 21 named tests green, plus about fifty additions; A4.3 partial — heading F1 needs the Phase 7 corpus)*
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

**Phase 3 is complete.** Its Definition of Done is checked below, with two partial rows named as partial.

First step for Phase 4: read `docs/IMPLEMENTATION_PLAN.md` PHASE 4 and the sections of
`docs/PIPELINE.md` §8 that cover the part being built, then take the first work item with the TDD loop.

Phase 4 is headings, outline/TOC matching, book structure, lists, footnotes, captions, quotes and verse,
tables, images and metadata — everything that turns ordered paragraphs into a document tree. It is
**Conserving throughout**: this stage assigns meaning and never deletes text.

**What Phase 3 hands it**, in the order it will be wanted:

1. **`Para` is deliberately half a type.** `oc_model::layout::Para` carries id, blocks, lines, text,
   `first_line_indent` and pages. IR_SKETCH also gives it `spans`, `align`, `lang`, `drop_cap` and
   `confidence`; those are `structure`'s to fill and were left out rather than shipped always-empty.
   `oc_model::confidence::{Confidence, Method, Signal}` already exists — dehyphenation uses it — so the
   pattern for recording *how* a decision was reached is set.
2. **Drop caps are detected but not attached.** `oc_layout::anchor::drop_caps` returns them per page;
   `Para::drop_cap` is where they belong once `Para` grows the field, and the `dropcap` CSS class is
   Phase 5's.
3. **Image anchors exist and are unpaired.** `oc_layout::anchor::Anchor` says which block an image comes
   before. Figure-and-caption pairing is Phase 4's (PIPELINE §8.4), and the adjacency it needs is
   already preserved.
4. **`f07_verse_and_quote` has not been written.** The plan lists it under Phase 3's Files, but no Phase 3
   test names it and verse is PIPELINE §8.6. Write it with the test that needs it.
5. **Fixture numbers.** Taken: **f01–f06**, **h01–h23**. Next free: **f07**, **h24**. The plan's Phase 4
   numbers `f06`–`f08` therefore shift to **`f08`–`f10`**, and test 4.10's `h16` — already spent twice
   over — is **`h24`**.

Still open, and cheap: CI's `test` job runs `xtask fixtures` but never `handmade-fixtures` or
`mutations`, so a builder change that no longer reproduces the committed fixtures is not caught. A
`--check` mode on those two tasks would close it. Phase 3 changed the builder again (h22's page box,
h22 and h23 themselves).

## Notes

Carried forward, in the order a fresh session needs them:

- **Phase 4's own carry-forwards are in the "Current work item" section above.** Three more that
  belong with the standing notes:
  - **A tightly set ruled table falls back to an image.** When a cell gutter is narrower than
    `text.line_split_gap_em`, `words` keeps two cells in one run and a `Run` carries a box and its
    text but not its glyphs' positions, so nothing in `structure` can split it. The table takes
    the image-plus-text fallback with `reason = "a run crosses a column rule"`. The long-term fix
    is to split runs at vertical rules in `layout`, the way `split_lines_at_gutters` splits them
    at page gutters, which needs the rules to reach `layout` — they do not today.
  - **A contents page with no drawn leader is not parsed.** `"Preface    i"` reaches the parser as
    `"Preface i"` — the gap is geometry and a line's text is not — so it is indistinguishable from
    a two-word title. Recovering it means measuring the gap between a line's last two runs, which
    is a second detector and belongs with the corpus that would say how often it is needed.
  - **`Note.body` is one paragraph.** A note that runs to several paragraphs is one paragraph
    here. Nothing is lost — the text is all there — but the structure inside a long endnote is
    not recovered.

- **`docs/DECISIONS_LOG.md` is the record of every decision made while implementing.** Read it before
  changing anything that looks arbitrary; most of it is measured rather than chosen.
- **PDFium must be vendored before the test suite passes**: `cargo run -p xtask -- vendor-pdfium`.
  Pinned to `chromium/7881` (151.0.7881.0) in `xtask/pdfium.lock`, lands in the git-ignored
  `vendor/pdfium/<triple>/`, needs `curl` and `tar` on PATH. CI runs it before `nextest`.
- **Fixtures**: `cargo run -p xtask -- fixtures` (Typst f01–f03 into the git-ignored
  `target/fixtures/`), `-- handmade-fixtures` (h01–h15, committed), `-- mutations` (committed).
- **Phase 3's own carry-forwards are in the "Current work item" section above.** The notes below are
  the standing ones: how to build, what is vendored, and what earlier phases left open.
- **Carry-forward 2 (columns) is closed.** `text` still clusters a line by baseline alone, and that is
  now deliberate: `layout` detects the columns from run coverage and splits the lines that span a gutter,
  because the split is exactly as good as the column hypothesis, and a hypothesis that can be withdrawn
  has to be able to withdraw the split with it. `words` breaks a *run* at `text.line_split_gap_em`, which
  is all the projection needs. `Line::indent_pt`/`right_gap_pt` are still page-relative — the paragraphs
  item recomputes them against the block.
- **Carry-forward 3 (a budget breach is fatal) is still open**, and still belongs to Phase 6.
- **No stage may treat the backend's glyph order as reading order.** Measured in item 1.3: PDFium
  reorders the lines of a page under `/Rotate 90`. Reading order is Phase 3's, from geometry.
- **Carry-forward 1 (soft vs hard hyphen) is still open, and is now less urgent.** Dehyphenation ships
  without it: the in-document lexicon, the German capital rule and the classifier decide on evidence the
  document carries in its text rather than on which hyphen scalar the producer wrote. Recovering the
  distinction would still help, and still needs `lopdf` content-stream access — the same mechanism
  `OverdrawDedup` needs.
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
  reached the registry from inside the namespace.
- **`ci` now watches `phase/**` as well as `main`**, so a phase branch cannot go dark again the
  way `phase/00-bootstrap` did. Opening a pull request is still worth doing when a phase is ready
  to review; what it buys is the review, not the CI, which fires either way now — and it does
  cost a second run, because the concurrency group is namespaced by event. That namespacing is
  not incidental: `head_ref` is the pull request author's branch name, this repository is public,
  and a group keyed on the bare name would let a stranger who names a fork branch `main` cancel
  the run that gates a release. `cancel-in-progress` is off for `main` for the same reason.
- **Three CI jobs are `if: false` until their phase arrives**, because the commands they call do
  not exist: `epubcheck` (Phase 5), `dom-checks` (Phase 6) and `no-network`'s
  `assert-no-net-deps` step (Phase 14). Each carries a comment naming the phase. Until Phase 14
  the socket ban is enforced by `deny.toml`'s `wrappers` rule in the `deny` job.
- **A non-embedded base-14 font makes glyph bounding boxes host-dependent**, measured on h01:
  `bbox.x1` is 80.02 on Windows and macOS, 79.85 on Ubuntu. The *advance* is identical, because
  the widths come from the PDF's own metrics. D13.8's determinism contract therefore cannot hold
  for such documents — the substitution happens below us — so **Phase 3's layout rules should
  prefer the advance box and the origin, which are stable, wherever they have the choice.**
- **VD-a, VD-b and VD-d are closed.** VD-c, VD-e, VD-f and VD-g are still open, each with an owner and a
  blocking phase; none of them blocks Phase 4. VD-f blocks Phase 6, VD-g blocks Phase 13, and VD-c and
  VD-e block only the optional dictionary pack (post-v1).
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

## Phase 4 — Definition of Done

Checked against `IMPLEMENTATION_PLAN.md` §0.3 on 2026-09-14. **One row is partial** and it is
blocked on the corpus, which is Phase 7's; it is marked as such rather than counted as a pass.

1. **Every named test exists and passes** — all twenty-one rows of the Phase 4 table (4.1–4.21),
   plus about fifty additions. `docs/TEST_MATRIX.md` lists every one and the CI job that runs it.
   The plan's fixture numbers were all spent in Phases 2 and 3, so `f06`–`f08` there are
   `f08`–`f10` here and `h15`–`h20` are `h24`–`h29`; the test *names* are unchanged.
2. **`cargo nextest run --workspace`** — 307 passed, 0 skipped, 0 ignored, locally. The three-OS
   claim is CI's and is made when this branch merges, as Phases 2 and 3 cashed theirs.
3. **clippy** `--workspace --all-targets --all-features --locked -- -D warnings` — clean.
4. **`cargo fmt --all --check`** — clean.
5. **`cargo deny check`** — advisories, bans, licenses, sources ok; `deny.tools.toml` ok. `uuid`
   (MIT OR Apache-2.0) is the only new shipped dependency, and the plan names it.
6. **`cargo run -p xtask -- thresholds-lint`** — clean. **`ci-lint`** — clean.
7. **Acceptance criteria A4.1–A4.6.**
   - **A4.1** (outline → headings 1:1) — `outline_is_used_as_heading_ground_truth` on `f09`, in
     both directions: every outline entry binds and no heading is emitted that the outline did
     not name.
   - **A4.2** (noteref↔footnote bijection total) — `footnote_marker_body_bijection` on `f08`
     (`match_rate == 1.0`, no anchor shared) and `footnote_symbol_cycle_resets_per_page` on `h24`.
   - **A4.3** (heading F1 ≥ 0.75 against ground truth on the corpus) — **PARTIAL.** There is no
     corpus and no heading ground truth to score against; both arrive in Phase 7. What exists is
     the *exactness* of the two fast paths on `f09` (outline and contents page, 7/7 each), size
     rank on `f10` (4/4), and `heading_tree_has_no_level_skips` over eight fixtures under all
     three sources. The same shape of partial as Phase 2's A2.3 and Phase 3's A3.2, and the same
     cause.
   - **A4.4** (cell multiset equals source, or the table becomes an image) — enforced as the gate
     itself in `tables::build_table`, demonstrated by `ruled_table_becomes_html_table` on `f10`
     for the markup path and `borderless_table_falls_back_to_image_with_details` on `h26` for the
     other one.
   - **A4.5** (ledger empty) — `structure_stage_is_conserving` over nine documents, through the
     real `check_invariants` with `StageKind::Conserving`.
   - **A4.6** (identical `dc:identifier` across reconversions) —
     `identifier_is_stable_across_reconversions`, as a unit test on the function and end to end on
     `h28`, including that the filename does not enter it and the hash does.
8. **`docs/CHANGELOG.md`** — Phase 4 entry written.
9. **No unnumbered TODO/FIXME** — `xtask ci-lint` clean.

**Three defects in earlier phases were found and fixed here**, each recorded in
`docs/DECISIONS_LOG.md`:

- `read_outline` expanded every `/Next` chain at every node, so a five-entry chain came back with
  thirty-two entries and `f09`'s seven headings with thirty-four. The outline is heading ground
  truth, so every duplicate would have become a heading. `h13` hid it (its chains are two long)
  and `f01` hid it (one bookmark).
- Docstrum merged every heading into the paragraph beneath it, and `paragraphs` merged them back
  when the barrier was added to `layout` alone.
- A superscript set with an OpenType `sups` glyph was read as ordinary text, so footnote markers
  were swallowed into the middle of body runs.

**Known gaps carried out of the phase**, each with a named cause, are in the Notes below.

## Phase 3 — Definition of Done

Checked against `IMPLEMENTATION_PLAN.md` §0.3 on 2026-09-13. **Two rows are partial and both are
blocked on work outside this phase**; they are marked as such rather than counted as passes.

1. **Every named test exists and passes** — all seventeen rows of the Phase 3 table (3.1–3.17), plus
   about sixty additions. `docs/TEST_MATRIX.md` lists every one and the CI job that runs it. Three of the
   plan's fixture numbers were already spent, so test 3.5 uses `h22_false_gutter`, test 3.7 uses
   `h23_paragraph_across_pages` and test 3.9 uses `f06_hyphenation_de`; the test *names* are unchanged.
2. **`cargo nextest run --workspace`** — 240 passed, 0 skipped, 0 ignored, locally. The three-OS claim
   is CI's and is made when this branch merges; Phase 2's was cashed the same way.
3. **clippy** `--workspace --all-targets --all-features --locked -- -D warnings` — clean.
4. **`cargo fmt --all --check`** — clean.
5. **`cargo deny check`** — advisories, bans, licenses, sources ok; `deny.tools.toml` ok. Both configs
   now ban `hyphenation` (VD-b).
6. **`cargo run -p xtask -- thresholds-lint`** — clean. **`ci-lint`** — clean: no `#[ignore]`, no
   unnumbered `TODO`.
7. **Acceptance criteria A3.1–A3.5.**
   - **A3.1** (`f02`, left column before right) — `two_column_reading_order_is_left_then_right`.
     Cashed properly only because the fixture was rewritten: it had never had two columns.
   - **A3.2** (100 % of gold block orders on any Manhattan corpus file) — **PARTIAL.** There is no
     corpus and no gold block order to reproduce; both arrive in Phase 7. What exists is the order
     itself, on `f01`, `f02` and `h22`, plus `prop_single_column_order_is_monotone_in_y` over generated
     pages and `prop_page_permutation_metamorphic` over three documents. The same shape of partial as
     Phase 2's A2.3, and the same cause.
   - **A3.3** (keep-hyphen recall ≥ 0.80 on the holdout, for the DE fixture) — **PARTIAL**, and split in
     two. The recall is measured and passes — 0.912 over 239 held-out keeps
     (`hyphen_classifier_keep_recall_on_holdout`) — but the holdout is **English**, because D15 has no
     German source that may be redistributed and so there is no German training data. The German fixture
     itself is checked in both directions by `dehyphenate_keeps_german_real_hyphen`, on the rule that
     needs no lexicon. Closing this row properly needs the DE list, which is the open D15 question from
     item 2.9.
   - **A3.4** (layout ledger empty) — `layout_stage_is_conserving`, and
     `layout_stage_conservation_violation_errors` for the other direction.
   - **A3.5** (I-5: exactly one hyphen, nothing else) — `dehyphenate_i5_removes_exactly_one_hyphen`
     (5,000 generated joins) for the operation, `check_invariants` for the ledger, and
     `dehyphenation_is_ledgered_one_hyphen_at_a_time` end to end on `h23`.
8. **`docs/CHANGELOG.md`** — Phase 3 entry written.
9. **No unnumbered TODO/FIXME** — `xtask ci-lint` clean.

**VD-b closed** (2026-09-13): the `hyphenation` crate ships the `hyph-utf8` pattern files with their
licence headers stripped and disclaims them; Turkish is LPPL-1.0+ upstream and the compiled dictionaries
fold in GPL/LGPL/MPL extended data. It is banned in both `deny` configs. v1 needs no patterns.

**One file from the plan's Files list is deliberately absent:** `f05_verse_and_quote.typ` (which would be
`f07` here). No Phase 3 test names it — verse and block quotes are PIPELINE §8.6, and their tests are
Phase 4's. It is written when the test that needs it is.

**Known gaps carried out of the phase**, each with a named cause:

- `f01`'s committed `pipeline` assertion is not met. The classifier reads `pipe-line` as a real compound,
  which it was in the nineteenth-century register its training corpus is written in. Fails in the safe
  direction; the fix is a modern corpus (Phase 7).
- One block of `f02` is flagged low-confidence: the cover cuts a paragraph's last line off because a line
  with no ascenders has a shorter inked box. Over-flagging a confidence signal is the safe direction, and
  the flag changes no segmentation.
- A `Dehyphenate` budget is a fraction, so a document of a hundred characters breaches it on one
  legitimate hyphen. A floor on the denominator belongs with the rest of the breach policy, in Phase 6.

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
2026-09-13  P3.1      VD-b closed: hyphenation banned, not depended on                            059c00d
2026-09-13  P3.2      oc-layout blocks: Docstrum + whitespace cover + layout stage (3.1, 3.13 + 6) 36874a1
2026-09-13  P3.3      oc-layout columns + reading order; f02 rewritten (3.2-3.4 + 7)               adb50d9
2026-09-13  P3.4      oc-layout continuity + h22; the retry compares hypotheses (3.5 + 7)          7fa85aa
2026-09-13  P3.5      oc-layout paragraphs: convention, unwrap factor, cross-page merge (3.6 + 8)  0ad7b6a
2026-09-13  P3.6      oc-text dehyphen tiers + paragraphs stage + I-5 + h23 (3.7/3.8/3.10/3.11/3.17) 7979668
2026-09-13  P3.7      oc-text compound_de + f06; the capital after the hyphen (3.9 + 8)            f417036
2026-09-13  P3.8      oc-text classifier + hyphen_clf.py + holdout; keep-recall 0.912 (3.12 + 6)   650a3e6
2026-09-13  P3.9      openconvert dump_layout + anchor + drop caps; the cover fixed (3.14-3.16 + 12) 0dfd45c
2026-09-13  PHASE 3   COMPLETE - Definition of Done checked; A3.2 and A3.3 partial, both named
2026-09-14  P4.1      oc-model doc: the semantic layer + 5 id types; Para grows its half (3)     ac2218b
2026-09-14  P4.2      oc-pdf VectorRegion + is_rule + h24; the rule predicate (3)                316611f
2026-09-14  P4.3      oc-structure cluster + validity gate; text gets a font table (4.3/4.6 + 4) 5b047af
2026-09-14  P4.4      oc-pdf: an outline entry is read once; f07-f10 land (1)                    697099b
2026-09-14  P4.5      oc-structure headings: outline/TOC/size-rank + the style barrier (4.1-4.5) 6340775
2026-09-14  P4.6      oc-structure notes + the raised-ink superscript rule (4.7, 4.8)            d00443d
2026-09-14  P4.7      oc-structure figures: captions, and the abstention (4.9, 4.10)             a434a4c
2026-09-14  P4.8      oc-structure lists + tables, and the honest fallback (4.11-4.14 + 5)       9b2b2ad
2026-09-14  P4.9      oc-structure quotes + meta; XMP over boilerplate (4.16-4.18 + 4)           4076f40
2026-09-14  P4.10     openconvert structure stage under the conservation law (4.15, 4.19-4.21)   70439b6
