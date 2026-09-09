# PROGRESS — OpenConvert

<!-- Machine-readable state. Claude Code reads this first and rewrites it after every completed work item. -->

STATUS: IN_PROGRESS
CURRENT_PHASE: 0
CURRENT_ITEM: 0.5 — `xtask vendor-pdfium` + `oc-pdf::pdfium` binding (test 0.7, not yet written)
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
- `GOLDEN_BLOCK_ID_CHAPTER_3 = "SDMLH752SA"` in `ids.rs` is a committed golden value. If that assertion
  ever fails, the id derivation changed and `IR_VERSION` must change in the same commit (D13.3).
- Commit messages carry **no** Claude Code attribution footer (maintainer's instruction, 2026-09-09).
- `xtask` is still a `todo!()` stub. Phase 0's Definition of Done needs `thresholds-lint` (the rule is
  already implemented as `oc_core::thresholds::lint`; xtask only has to call it), `ci-lint`, `fixtures`,
  `vendor-pdfium` and `stage-sidecars`. Those are separate work items, not part of item 0.4.

## Completed items log

<!-- one line per finished work item: `YYYY-MM-DD  P<phase>.<item>  <what>  <commit sha>` -->
2026-09-09  P0.setup  Cargo workspace, thresholds.toml, deny.toml, CI matrix, eval skeleton (§1.1-§1.9)  e6d02df
2026-09-09  P0.1      oc-model: BlockId derivation + Rect (tests 0.1, 0.2)                          1d32410
2026-09-09  P0.2      oc-model: canonical JSON + IR_VERSION (tests 0.3, 0.4)                        2cd8f76
2026-09-09  P0.3      oc-core: thresholds codegen + provenance lint (tests 0.5, 0.6)                3843abb
2026-09-09  P0.4      oc-pdf: page-space normalisation (test 0.8 + corner unit test)              PENDING
