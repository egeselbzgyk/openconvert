# PROGRESS — OpenConvert

<!-- Machine-readable state. Claude Code reads this first and rewrites it after every completed work item. -->

STATUS: IN_PROGRESS
CURRENT_PHASE: 1
CURRENT_ITEM: 1.2 — glyph extraction: `oc-model::extract` + `oc-pdf::glyphs` (tests 1.1–1.4)
LAST_UPDATED: 2026-09-09

---

## How to use this file

- `STATUS` is one of `IN_PROGRESS` · `BLOCKED` · `COMPLETE`.
- Set `STATUS: BLOCKED` **only** when a decision is needed that `docs/DECISIONS.md` does not settle. Write the question under `## Blocked

_(empty — Q1 resolved 2026-09-09; see `docs/DECISIONS_LOG.md`)_

## Notes` short: what a fresh session needs in order to resume, nothing else.

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

**Phase 1, item 1.2 — glyph extraction.** Item 1.1 (the hand-made fixtures) is done. Next is
`oc-model::{extract,ledger}` and `oc-pdf::glyphs`, driven by tests 1.1-1.4.

Phase 1 is XL: twenty tests covering glyphs, images, vectors, outlines, metadata, encryption, limits,
cancellation and a `pdftotext` differential oracle. Items after 1.2, roughly one per cluster:
1.3 metamorphic invariants (1.5-1.7) · 1.4 broken-text mutation (1.8) · 1.5 images (1.9) ·
1.6 limits (1.10, 1.11, 1.20) · 1.7 encryption (1.12-1.14) · 1.8 outline (1.15) · 1.9 fuzz-lite (1.16) ·
1.10 dump-stage (1.17) · 1.11 the poppler oracle (1.18) · 1.12 cancellation (1.19) · then VD-d, the
ten-PDF image spike, which blocks Phase 4's image policy.

**Read `docs/DECISIONS_LOG.md`'s PDFium overdraw entry before writing test 1.4.** PDFium's text page
already collapses identical overlapping glyphs, so `C_raw` from that API is post-dedup and test 1.4 as
the plan words it cannot hold. The entry says what to assert instead and how to keep D13.4's budget
meaningful (count glyphs a second way, from the page's text objects, and ledger the difference).

## Phase 0 — Definition of Done

Checked against `IMPLEMENTATION_PLAN.md` §0.3 on 2026-09-09:

1. **Every named test exists and passes** — all 23 rows of the Phase 0 table, plus four additions
   (0.8a, 0.12a, 0.23a, and the committed-assertion-file test), each with its reason in
   `docs/DECISIONS_LOG.md`. `docs/TEST_MATRIX.md` lists every one and the CI job that runs it.
2. **`cargo nextest run --workspace`** — 26 passed, 0 skipped, 0 ignored. **Verified on Windows only.**
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

Also delivered: the First Milestone in full — `openconvert inspect` on three Typst fixtures with
committed snapshots, `cargo deny` clean, and `docs/TEST_MATRIX.md` written.

## Blocked

_(empty — Q1 resolved 2026-09-09; see `docs/DECISIONS_LOG.md`)_

## Notes` short: what a fresh session needs in order to resume, nothing else.

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

**Phase 0, item 0.9 - `oc-pdf::inspect`.** Nothing written yet. The fixtures now exist, so this is the
last piece of the First Milestone's data path.

Next step is RED: write tests 0.14-0.16 (`inspect::inspect_f01_prose_single_column`,
`inspect_f02_two_column`, `inspect_f03_image_only`) as insta snapshots over
`openconvert.inspect/1` JSON, then implement `PdfDoc` (page_count, doc_info, page_geometry,
page_char_stats, page_image_stats) on the PDFium backend and `inspect(doc, opts) -> InspectReport`.

Two things to carry in, both in `docs/DECISIONS_LOG.md`:
- **Typst writes /Creator, not /Producer.** The plan's expected JSON has `"producer": "Typst 0.15.1"`
  and `"creator": null`; the actual files are the reverse. Snapshots follow the file. The `/Creator`
  fallback in `producer_family` is what makes these fixtures classify as `Typst` at all.
- Snapshots redact `source.path`, `source.sha256`, `source.bytes`, `engine_version` and the producer
  string; `visible_chars` is NOT redacted - it is the assertion.

Per-page counters come from PDFium characters: `render_mode()` 3 or zero-alpha `fill_color()` is
invisible, `is_generated()` is excluded from every count, U+FFFD and PUA are counted separately, and a
`font_name()` containing `GlyphLessFont` sets the flag. No text is assembled and no Glyph struct is
materialised in this phase - counters only.

Done in this branch: the workspace bootstrap (1.1-1.9) and items 0.1-0.8 - `geom::Rect` and
`ids::BlockId`; `IR_VERSION` and `canonical::to_canonical_json`; the `oc-core::thresholds` codegen and
lint; `oc-pdf::geom` page-space normalisation; `xtask vendor-pdfium` and the PDFium binding with its
startup probe; `oc-pdf::classify`; `oc-pdf::producer`; `xtask fixtures`. Every open design question
decided along the way is written up in `docs/DECISIONS_LOG.md` - read that before changing any of them.

## Blocked` and stop.
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

**Phase 0, item 0.8 - `xtask fixtures`.** Nothing written yet, and it is the last thing standing between
here and the First Milestone's `inspect` snapshots (tests 0.14-0.16), which need real PDFs.

Next step is RED: write test 0.20 `fixtures::typst_fixtures_are_reproducible` (compiling each `.typ`
twice yields byte-identical PDFs; if it does not, the fallback per R7 D.2 is to commit the PDFs as
golden binaries plus a nightly regenerate-and-diff job). Then:

1. Author `corpus/fixtures/typst/{f01_prose_single_column,f02_two_column,f03_image_only}.typ` - the
   sources are given verbatim in the plan's Phase 0 section, under "Typst fixture sources".
2. Generate `corpus/fixtures/assets/scan_page_01.png` (1240x1754, grey 235, light noise, 0.4 degree
   skew, ~60 KB). The plan says `eval/src/oc_eval/generate/scan_sim.py --make-fixture-asset` (Pillow),
   and the file is committed. `.gitattributes` already marks `*.png` binary.
3. `cargo run -p xtask -- fixtures`: `typst::compile` in-process with embedded fonts only and a fixed
   timestamp, `typst_pdf::pdf`, then `strip_structtree` over the `lopdf` document (D18 - Typst tags PDFs
   by default while reality is 12.6 percent tagged), writing `target/fixtures/<f>.pdf` and a
   `corpus/manifest.json` entry with producer_stratum "ours(Typst)". A `--keep-structtree` variant named
   `<f>__tagged.pdf` is compiled with `PdfStandard::Ua_1` for the Phase-4 tagged bucket.

The `typst` and `typst-pdf` crates (0.15.x) are not workspace dependencies yet and are xtask-only.
Check them against `deny.toml`'s allow-list before adding: Typst itself is Apache-2.0, but its
dependency tree has not been reviewed here.

Done in this branch: the workspace bootstrap (1.1-1.9) and items 0.1-0.7 - `geom::Rect` and
`ids::BlockId`; `IR_VERSION` and `canonical::to_canonical_json`; the `oc-core::thresholds` codegen and
lint; `oc-pdf::geom` page-space normalisation; `xtask vendor-pdfium` and the PDFium binding with its
startup probe; `oc-pdf::classify`; `oc-pdf::producer`. Every open design question decided along the way
is written up in `docs/DECISIONS_LOG.md` - read that before changing any of them.

## Blocked` and stop.
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

**Phase 0, item 0.7 — `oc-pdf::producer`.** Nothing written yet. Next step is RED: write test 0.13
`producer::producer_family_table` in `crates/oc-pdf/src/producer.rs` as a table-driven test mapping nine
producer strings to the nine `ProducerFamily` variants, watch it fail, then implement
`producer_family(info, xmp) -> ProducerFamily` as the ordered regex table in Phase 0 detail 4:
`pdftex|xetex|luatex` → `PdfTeX`; `indesign` → `InDesign`; `microsoft.*word|word for` → `Word`;
`ghostscript` → `Ghostscript`; `abbyy|finereader|scanner|kofax` → `Scanner`; `^typst` → `Typst`;
`weasyprint` → `WeasyPrint`; `chrom(e|ium)|skia` → `Chromium`; else `Unknown`. All case-insensitive,
`/Producer` first, then `/Creator`. The strata list in D18 and `corpus/manifest.json` must agree with
the variant set.

Done in this branch: the workspace bootstrap (§1.1–§1.9) and items 0.1–0.6 — `geom::Rect` and
`ids::BlockId`; `IR_VERSION` and `canonical::to_canonical_json`; the `oc-core::thresholds` codegen and
lint; `oc-pdf::geom` page-space normalisation; `xtask vendor-pdfium` and the PDFium binding with its
startup probe; `oc-pdf::classify` page classification. Every open design question decided along the way
is written up in `docs/DECISIONS_LOG.md` — read that before changing any of them.

## Blocked` and stop.
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

**Phase 0, item 0.6 — `oc-pdf::classify`.** Nothing written yet. Next step is RED: write tests 0.9–0.12
(`classify::classify_text_page`, `classify_image_only_page`, `classify_ocr_sandwich_page`,
`classify_broken_text_page`) in `crates/oc-pdf/src/classify.rs`, watch them fail, then implement
`classify_page(&PageGeometry, &PageCharStats, &PageImageStats, Option<f32>, &Thresholds)
-> (PageClass, f32)` as the pure function of the counters that Phase 0 detail 3 specifies, in that
exact evaluation order. No PDFium is needed: the inputs are plain counters. The dictionary-hit-rate arm
is wired but always passed `None` in Phase 0 (Phase 2 supplies it) — see the RT B1 note in
`docs/DECISIONS_LOG.md`.

Done in this branch: the workspace bootstrap (§1.1–§1.9) and items 0.1–0.5 — `geom::Rect` and
`ids::BlockId`; `IR_VERSION` and `canonical::to_canonical_json`; the `oc-core::thresholds` codegen and
lint; `oc-pdf::geom` page-space normalisation; `xtask vendor-pdfium` and the PDFium binding with its
startup probe. Every open design question decided along the way is written up in
`docs/DECISIONS_LOG.md` — read that before changing any of them.

## Blocked` and stop.
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

**Phase 0, item 0.5 — `xtask vendor-pdfium` + the PDFium binding.** Nothing written yet. Two halves,
because the test cannot pass without the library:

1. `xtask vendor-pdfium` — download the `bblanchon/pdfium-binaries` release asset for the host triple,
   verify a SHA-256 pinned in `xtask/pdfium.lock`, unpack to `vendor/pdfium/<triple>/`. This is the first
   item that needs the network, and the asset naming per triple is unverified.
2. `oc-pdf::pdfium::bind` — `Pdfium::bind_to_library`, resolving from `OC_PDFIUM_PATH`, then next to the
   executable, then `vendor/pdfium/<triple>/` (`pdfium_platform_library_name()` gives the per-OS file
   name), plus the startup ABI probe on a one-page in-memory PDF reporting `PdfError::AbiMismatch`.
   `#[allow(unsafe_code)]` goes on that module alone.

Test to write FIRST: 0.7 `pdfium::binds_and_reports_version`. Expect RED as `PdfError::LibraryNotFound`
until step 1 has run. Also note `pdfium-render` 0.9.4 currently selects the `pdfium_latest` feature; it
should be pinned to the exact `pdfium_<build>` feature matching the vendored release once that build
number is known (see `docs/DECISIONS_LOG.md`).

Done in this branch: the workspace bootstrap (§1.1–§1.9), item 0.1 (`geom::Rect`, `ids::BlockId`),
item 0.2 (`IR_VERSION`, `canonical::to_canonical_json`), item 0.3 (`oc-core::thresholds` codegen and
lint) and item 0.4 (`oc-pdf::geom` page-space normalisation, test 0.8). Test 0.8 was taken before 0.7
because it needs nothing external; the plan's table is a list, not an order. Every open design question
decided along the way is written up in `docs/DECISIONS_LOG.md` — read that before changing any of them.

## Blocked` and stop.
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

**Phase 0, item 0.4 — `oc-pdf` PDFium binding.** Nothing written yet. This item needs the vendored
library before its tests can pass, so it is two halves: `xtask vendor-pdfium` (download the
`bblanchon/pdfium-binaries` asset for the host triple, verify a SHA-256 pinned in `xtask/pdfium.lock`,
unpack to `vendor/pdfium/<triple>/`) and then `oc-pdf::pdfium::bind` (`Pdfium::bind_to_library`, resolving
the library from `OC_PDFIUM_PATH`, then next to the executable, then `vendor/pdfium/<triple>/`, plus the
startup ABI probe that calls into a one-page in-memory PDF and reports `PdfError::AbiMismatch`).

Tests to write FIRST: 0.7 `pdfium::binds_and_reports_version` and 0.8
`geom::prop_normalised_rects_are_inside_page`. Note 0.8 is listed under `oc_pdf::geom` in the plan's
table — it is the page-space normalisation property (rotate ∈ {0,90,180,270} × CropBox offsets), which
belongs to `oc-pdf`, not to `oc-model::geom`.

Done in this branch: the workspace bootstrap (§1.1–§1.9), item 0.1 (`geom::Rect`, `ids::BlockId`),
item 0.2 (`IR_VERSION`, `canonical::to_canonical_json`) and item 0.3 (`oc-core::thresholds` codegen and
lint). Every open design question decided along the way is written up in `docs/DECISIONS_LOG.md` — read
that before changing any of them.

## Blocked` and stop.
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

**Phase 0, item 0.3 — `oc-core::thresholds`.** Nothing written yet. Next step is RED: write tests 0.5
(`thresholds::every_provisional_has_owner_and_future_review`) and 0.6
(`thresholds::generated_constants_match_toml`, asserting `T.layout.furniture.band_ratio == 0.08` and that
it equals the TOML value re-read at runtime), watch them fail, then add `crates/oc-core/build.rs` which
parses `../../thresholds.toml` and emits `thresholds_generated.rs` with the typed constants and the
`PROVENANCE` table, failing the build on a malformed entry (plan Phase 0 detail 6, D17).

Done in this branch: the workspace bootstrap (§1.1–§1.9), item 0.1 (`geom::Rect`, `ids::BlockId`) and
item 0.2 (`IR_VERSION`, `canonical::to_canonical_json`). Every open design question decided along the way
is written up in `docs/DECISIONS_LOG.md` — read that before changing any of them.

## Blocked` and stop.
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

**Phase 0, item 0.2 — `oc-model::canonical`.** Nothing written yet. Next step is RED: write tests 0.3
(`canonical::canonical_json_sorts_keys_and_rounds_geometry`, an `insta` snapshot) and 0.4
(`canonical::canonical_json_rejects_nan`) in `crates/oc-model/src/canonical.rs`, watch them fail, then
implement `to_canonical_json<T: Serialize>(v: &T) -> Result<String, CanonError>` per D13.3: sorted keys,
geometry rounded to 0.01 pt **at serialization only**, NFC strings, no NaN/Inf, `ir_version` first.

Done in this branch so far: the workspace bootstrap (§1.1–§1.9) and item 0.1 — `Rect` in
`crates/oc-model/src/geom.rs`, `BlockId::{derive, with_collision_suffix, as_str}` in
`crates/oc-model/src/ids.rs`. The four derivation details D13.3 leaves open (field byte encoding,
character-wise text truncation, base32 alphabet, and the collision counter occupying the tenth character)
are written up in `docs/DECISIONS_LOG.md`.

## Blocked

### Q1. Adding the `typst` crates breaks the `cargo deny check` advisories gate. Which way out?

**The conflict.** Two committed decisions cannot both hold today:

- `DECISIONS.md` Appendix A and `TEST_CORPUS.md` 6.1 (ratified) require Typst fixtures to be compiled
  **in-process via the `typst` + `typst-pdf` crates, no CLI**, from `cargo xtask fixtures`.
- `IMPLEMENTATION_PLAN.md` 1.4 sets `[advisories] ignore = []`, `SECURITY.md` 9 makes `cargo-deny`
  advisories a hard CI gate, and Phase 0 acceptance A0.2 requires `cargo deny check` to exit 0.

Adding `typst = "=0.15.1"`, `typst-pdf = "=0.15.1"`, `typst-assets = "=0.15.1"` to `xtask` grows the
lockfile from 233 to 447 crates and makes `cargo deny check` report **advisories FAILED** with seven
findings. Licenses, bans and sources all still pass, and `cargo tree -i` confirms every finding is
reachable only through `xtask` — none of it is in any shipped crate.

| Advisory | Crate | Kind | Path | Fix available |
|---|---|---|---|---|
| RUSTSEC-2026-0194 | `quick-xml` 0.38.4 | **vulnerability** (quadratic run time on duplicate attribute names) | `citationberg` -> `hayagriva` -> `typst-library` | needs >= 0.41.0; `citationberg` 0.7.0 requires `^0.38`, so not reachable |
| RUSTSEC-2026-0195 | `quick-xml` 0.38.4 | **vulnerability** (unbounded namespace allocation, memory-exhaustion DoS) | same | same |
| RUSTSEC-2026-0206 | `rustybuzz` 0.20.1 | unmaintained | `krilla` -> `typst-pdf` | none |
| RUSTSEC-2026-0192 | `ttf-parser` 0.25.1 | unmaintained | `fontdb` -> `krilla-svg` -> `typst-pdf` | none |
| RUSTSEC-2025-0141 | `bincode` 1.3.3 | unmaintained | `syntect` -> `two-face` -> `typst-library` | none |
| RUSTSEC-2024-0320 | `yaml-rust` 0.4.5 | unmaintained | same | none |
| RUSTSEC-2024-0436 | `paste` 1.0.15 | unmaintained | `biblatex` -> `hayagriva` -> `typst-library` | none |

Our own `quick-xml` 0.42.0 in `oc-epub` is **not** affected; both vulnerabilities are in the 0.38 line
that only Typst's citation stack pulls in. `typst` 0.15.1 is the newest release, so waiting for an
upstream bump is not available today either.

**Why this is not mine to decide.** It is a security-policy call with real trade-offs, and two of the
seven are actual vulnerabilities rather than unmaintained notices. `SECURITY.md` states the gate but
says nothing about scoping it to shipped code, and `DECISIONS.md` mandates the crates that trip it.

**The options, as I see them.**

- **(a) Scoped ignores.** Add the seven ids to `[advisories] ignore` with a written reason and a review
  date each. Honest and visible; the arguments for it are that `xtask` never ships, runs only on the
  developer's and CI's machines, and processes only fixture sources we author — so a DoS in an XML
  parser reached through citation handling has no attacker-controlled input. Against: it puts two live
  vulnerabilities on an allow-list, and the list will need re-reviewing on every `cargo update`.
- **(b) `[advisories] unmaintained = "workspace"`.** Silences the five unmaintained findings (all
  transitive) as a policy statement rather than a per-id exception, leaving only the two vulnerabilities
  to handle by (a). Smaller ignore list, same underlying question.
- **(c) Move fixture generation out of the audited workspace.** A separate manifest under `tools/`
  excluded from `[workspace]`, invoked as `cargo run --manifest-path ...`. `cargo deny check` then never
  sees Typst. Against: it contradicts D14's "one Cargo workspace", and an unaudited tool tree is worse
  hygiene than an audited one with documented exceptions — it hides the finding instead of accepting it.
- **(d) Commit the three fixture PDFs as golden binaries.** R7 D.2 already names golden binaries as the
  fallback when Typst output is not byte-reproducible; this would extend that to "generated out of band".
  Against: `cargo xtask fixtures` stops being able to regenerate them, which is the whole point of
  TEST_CORPUS 6.1, and mutation fixtures in Phase 7 build on the same machinery.
- **(e) Drop Typst; use WeasyPrint (already in `eval/`) as the only synthetic renderer.** Against: D18
  wants two independent synthetic renderers, and the Phase 0 fixture sources are written in Typst.

**My recommendation if you want one:** (b) then (a) — set `unmaintained = "workspace"`, and ignore the
two `quick-xml` ids with a reason naming the `xtask`-only path and a `review_by` that forces a re-check
when `typst` next releases. It keeps one audited workspace, keeps the finding visible in `deny.toml`
rather than hidden by repo layout, and is the smallest deviation from what is already written down.

**What is already done and does not depend on the answer** (committed in `76da00c`): the three `.typ`
sources, `corpus/fixtures/assets/scan_page_01.png`, and its generator
`eval/src/oc_eval/generate/scan_sim.py`. Every option above still needs those files.

**To unblock:** answer Q1, then set `STATUS: IN_PROGRESS` and resume at item 0.8.

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
- Local tool versions: rustc 1.98.1, cargo-nextest 0.9.143, cargo-deny 0.20.2.
- **PDFium must be vendored before the test suite passes**: `cargo run -p xtask -- vendor-pdfium`. It is
  pinned to `chromium/7881` (151.0.7881.0) in `xtask/pdfium.lock`, lands in the git-ignored
  `vendor/pdfium/<triple>/`, and needs `curl` and `tar` on PATH. CI already runs it before `nextest`.
- `GOLDEN_BLOCK_ID_CHAPTER_3 = "SDMLH752SA"` in `ids.rs` is a committed golden value. If that assertion
  ever fails, the id derivation changed and `IR_VERSION` must change in the same commit (D13.3).
- Commit messages carry **no** Claude Code attribution footer (maintainer's instruction, 2026-09-09).
- `xtask` has `vendor-pdfium` and `fixtures`. Phase 0's Definition of Done still needs
  `thresholds-lint` (the rule is already implemented as `oc_core::thresholds::lint`; xtask only has to
  call it), `ci-lint` and `stage-sidecars`. Each is its own work item.
- **Two cargo-deny configs now.** `deny.toml` audits what ships and admits no exceptions;
  `deny.tools.toml` audits `xtask` with the same licence/ban/source policy and reports its advisories
  without blocking. Run both: `cargo deny check` and
  `cargo deny --config deny.tools.toml check licenses bans sources`.

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
