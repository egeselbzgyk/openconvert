# PROGRESS — OpenConvert

<!-- Machine-readable state. Claude Code reads this first and rewrites it after every completed work item. -->

STATUS: IN_PROGRESS
CURRENT_PHASE: 6
CURRENT_ITEM: 6.7 — the Playwright DOM checks at three viewports, and turning the
              `dom-checks` CI job on (rows 6.13, 6.14, 6.15)
LAST_UPDATED: 2026-09-18

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
      *(First Milestone met; opened the Verification-debt table VD-a…VD-g)*
- [x] **Phase 1** — PDF inspection and ingestion  *(VD-d closed)*
- [x] **Phase 2** — Text assembly and normalization  *(all 22 named tests green; EN frequency list ships, DE/TR blocked on a D15 licence decision)*
- [x] **Phase 3** — Layout  *(all 17 named tests green; VD-b closed. A3.2 and A3.3 partial — both want the Phase 7 corpus)*
- [x] **Phase 4** — Structure  *(all 21 named tests green, plus about fifty additions; A4.3 partial — heading F1 needs the Phase 7 corpus)*
- [x] **Phase 5** — EPUB generation, Tier-1 validator, EPUBCheck CI gate
      *(all 20 named tests green, plus about sixty additions. **EPUBCheck 5.3.0 reports 0 errors
      and 0 warnings on all ten fixtures.** A5.3 is a CI job that cannot run on one machine and is
      unverified until the first CI run.)*
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

**Phase 6 in progress.** The structural validator (invariant I-7 end to end), the validate→repair
loop, the conversion report, and the CI DOM checks. It is the first phase whose subject is *what to
do when the output is wrong*.

Work items, in order, with the plan's test rows against each:

- [x] **6.1** I-7 and retention — `oc-validate::structural` (rows 6.1, 6.2, 6.3 + 6.1a/6.1b/6.2a/6.3a)
- [x] **6.2** the rest of the structural checks: image parity, note bijection, heading-tree
      sanity, duplicate and quality statistics (row 6.16 + additions)
- [x] **6.3** the repair loop: measure, static table, plan, loop (rows 6.4, 6.5, 6.6, 6.7, 6.9)
- [x] **6.4** the loop wired into the pipeline, fire rate zero on the fixtures (row 6.10)
- [x] **6.5** `report.json`, `--report`, and the post-cap policy (rows 6.8, 6.12)
- [x] **6.6** warning codes and the en/de/tr templates (row 6.11)
- [ ] **6.7** Playwright DOM checks (rows 6.13, 6.14, 6.15)

**`oc_validate::structural::validate_structural` is the structural validator**, and its report
holds on all ten fixtures: I-7, image parity, the note bijection, resolving hrefs, a sane heading
tree. `crates/openconvert/tests/snapshots/structural__structural_report_per_fixture.snap` is the
measurement; three findings are carried out of it, all three in `docs/DECISIONS_LOG.md`, 2026-09-18:

- **Retention is a flag, not a gate.** Four fixtures land at 0.968–0.973 because furniture removal
  is part of `C_0`, and `validate.min_char_retention = 0.98` is jointly unsatisfiable with the 0.04
  furniture budget. Appendix D's v1.0 retention gate needs Phase 7's strata to be stated at all.
- **The Gopher n-gram statistics cannot gate output text.** `f09` scores `top_3gram` 0.8008 against
  a 0.18 bound, and is correct: its printed contents page is hundreds of `. . .` leaders. The nine
  statistics are recorded; what warns is `DuplicateStats` over emitted **blocks**, at
  `validate.dup_block_frac = 0.02`.
- **The h1-count range is scoped by page count** (`validate.h1_count_min_pages = 20`), so every
  fixture answers `None` and the check is first exercised on the Phase 7 corpus.

**`oc-validate` gained `oc-text` and `oc-core` as dependencies**, which ARCHITECTURE §3.1's table
does not list; the argument is in the same log entry and in `crates/oc-validate/Cargo.toml`.

**`oc_validate::repair` is the loop.** A repair edits the `Document` and the book is re-emitted; it
never patches the zip. All four control rules are in place and each was mutation-tested — deleting
any one of strict decrease, the new-id rule or the hash rule turns exactly one of rows 6.4/6.5/6.6
red. The loop takes an `Emit` trait rather than a concrete emitter, because the emitter passes
EPUBCheck clean on every fixture and therefore cannot be made to oscillate: the cases that matter
only exist against a double. `Emit::apply` is provided (it calls `apply_fix`) so that the control
flow and the table's edits are testable apart.

**The repair table has three `AutoFix` entries, not thirty.** `ACC-001` describes a figure whose
`alt` is empty; `RSC-012` demotes a note reference whose target does not exist to plain text;
`OPF-003` drops a figure nothing references and that carries no caption. All three are Conserving:
`alt` is an attribute, a `noteref` is a link, and an uncaptioned figure carries no characters. Six
more ids are `WarnUser` — understood, and with no fix that would not change the book. Everything
else is unmapped and reported verbatim. The plan's "~30 ids" would have been thirty untested paths
for defects this emitter has never produced.

**The loop is on the real path and fires zero times on all ten fixtures** (row 6.10, A6.2). The loop
owns the emission: `epub_stage` is split into `build` and `epub_check`, because a `convert` that
built the book once for the conservation check and again for the loop would re-encode every image
twice, and PIPELINE §12 budgets one regeneration *per iteration*. `Conversion` now carries `tier1`,
`structural` and `repair` alongside the document and the bytes.

**`validate` and `repair` are declared stages and are in the ledger**, both Conserving. `repair`'s
check is the one that earns its keep: it compares `C` of the document the loop was given against `C`
of the document it settled on with an empty ledger, so a repair that changed one character of the
book fails I-1 and the conversion stops — PIPELINE §12's "none of them may change the character
content of the book", stated as an invariant instead of as a property of three functions. `repair`
is Conserving with an *empty* reason set: PIPELINE §12 words it "except `UserOverride`", and
declaring a reason a Conserving stage may never cite (I-3) would be a contract contradicting itself.
The stage becomes `Budgeted` when the review UI arrives in Phase 12.

**A defect found on the way: the `document` stage's conservation check was never recorded.** It ran
— a violation returns `DocumentError` — but `convert` dropped the `StageCheck`, so the ledger named
seven stages where the pipeline had checked eight, and every phase's "checked after every stage"
claim was one stage short in its evidence. Fixed, and `repair::the_ledger_records_validate_and_repair_as_conserving_stages`
asserts the whole list in order.

**`report.json` is written on every conversion**, versioned `openconvert.report/1`, to
`<output>.report.json` or wherever `--report` says, and **before** the atomic rename so a report
exists even for a conversion whose output could not be placed. It lives in `openconvert::report`
rather than in `oc-core` as the plan's file list has it: assembling it needs `Tier1Report`,
`StructuralReport` and `RepairOutcome`, which are `oc-validate`'s, and `oc-validate` depends on
`oc-core`.

`PROVENANCE` now carries `owner` and `review_by` as well as `source` and `evidence` — "this number
is provisional" is only actionable with "owned by whom, revisit by when" beside it — so
`build.rs` emits a `Provenance` struct instead of a 3-tuple. `convert` times each stage and carries
the producer family and the page-class histogram. The NDJSON `warning` event carries `args` now;
Phase 5 emitted the code with an empty object.

**Row 6.8's cap is demonstrated at two levels and neither is a whole-pipeline run**, because it
cannot be: the emitter passes EPUBCheck clean on all ten fixtures, so a real conversion reports
`Clean` before any repair runs. `repair_loop::the_cap_stops_a_loop_that_would_otherwise_keep_going`
exercises the cap against a scripted emitter, and `report::repair_cap_writes_epub_and_marks_invalid`
asserts the policy — report `invalid`, remaining ids verbatim, the EPUB still there. Forcing three
failing iterations out of a correct emitter would make the assertion about the break.

**Twenty-six warning codes, three locales, and a lint that keeps them total.** `oc-core::warnings`
holds the registry (`codes.rs`: the code and the argument names it carries) and the three template
files; `render` fills `{slot}`s and leaves an unfilled one *visible*, because a warning missing its
number has stopped being a factual claim and a brace is a bug report where a blank is a mystery.

The gate has four parts and each was mutation-tested: every code has a template in every locale, no
locale has a template for a code the registry does not know, every `{slot}` is an argument the code
declares, and the English text uses every argument its code carries. The registry cannot be derived
— `oc-structure` owns `W_TABLE_AS_IMAGE` and `oc-core` cannot depend on it — so
**`xtask ci-lint` holds the registry and the tree in agreement in both directions**: a `W_…`
constant anywhere with no entry fails, and an entry nothing defines fails.

`--locale en|de|tr` is new and is not in §2.1's flag list; §2.2's job spec has the field. Without it
the engine's own localisation would be unreachable from the command line and the three template files
would be readable only by a GUI that arrives in Phase 12. The CLI prints the rendered sentences to
stderr only when stderr is *not* the NDJSON channel.

**What Phase 5 hands it**, in the order it will be wanted:

1. **`openconvert::convert::convert` is the one pipeline.** It runs every stage, checks each under
   the conservation law, and returns `Conversion { document, built, extracted_images }`. The CLI,
   the tests and (from Phase 12) the desktop app all take that path. `document.ledger` carries
   every stage's `StageCheck`, including `epub`'s.
2. **`oc_validate::validate_tier1` is the issue source the repair loop consumes.** Its `Finding`
   carries an EPUBCheck message id where one exists (`RSC-005`, `RSC-007`, `RSC-012`, `OPF-014`,
   `OPF-003`, `OPF-012`, `OPF-030`, `OPF-060`, `PKG-007`, `PKG-008`, `RSC-002`, `ACC-001`) and an
   `OC-…` id where it does not (`OC-SCRIPT`, `OC-REMOTE`, `OC-ENTITY`, `OC-NOTE-BIJECTION`,
   `OC-IMAGE-PARITY`). `Severity` is already the `(fatal, error, warning)` the repair loop's
   lexicographic measure needs (D13.7).
3. **I-7 is one function call away.** `document.ledger.removed_all()` / `added_all()` are the two
   halves, `ledger.c_0` is the baseline, and `oc_epub::textcontent::body_text` is how `C(EPUB)` is
   measured — by parsing the emitted documents, not by asking the emitter.
4. **The repair-fire rate is a release metric with target zero.** Every repair that fires is an
   emitter bug, so Phase 6 starts from an emitter that EPUBCheck already passes clean; a repair
   that fires on a fixture means something regressed.
5. **`Document.warnings` is where the report's issue list comes from.** Phase 5 added
   `W_EPUB_LARGE`, `W_XHTML_OVERSIZE`, `W_PAGE_BREAK_UNPLACED`, `W_NO_TEXT_EXTRACTED`.
   `BuiltEpub.warnings` carries codes rather than `Warning`s — `oc-epub` does not depend on the
   pipeline — and nothing yet attaches them to the document; that is Phase 6's report to do.
6. **Fixture numbers.** Taken: **f01–f10**, **h01–h29**. Next free: **f11**, **h30**.
   No new fixtures in Phase 5: the emitter's subject is the ten documents that already exist.
7. **Still open, and cheap:** CI's `test` job runs `xtask fixtures` but never `handmade-fixtures`
   or `mutations`, so a builder change that no longer reproduces the committed fixtures is not
   caught. A `--check` mode on those two tasks would close it.

## Notes

Carried forward, in the order a fresh session needs them:

- **`furniture` recovers no folio from a book that changes numbering system.** `f09` paginates
  `i, ii` then `1, 2, 3`; digit masking puts the three arabic folios in one group covering 3 of 5
  pages, a repetition ratio of 0.6, inside the grey zone where the detector abstains. The folios
  stay in the flow as one-character paragraphs and every `PageRef.label` is `None` — which also
  removes the arabic-1 reset PIPELINE §9 step 1 calls a hard boundary signal. The fix is a change
  to how a folio group is scoped (per numbering system, or per pagination run) and wants the
  Phase 7 corpus to choose between them. `docs/DECISIONS_LOG.md`, 2026-09-14.
- **Two of four real books outside the corpus are refused by the conservation law**, both in
  `structure`, and both are Phase 7's to calibrate rather than Phase 5's to guess at:
  - *AI Engineering* (O'Reilly, ~500 pp): `structure` emits 21 526 characters twice. The ruled-
    table detector finds **365 tables** in a book that has perhaps twenty — it fires on figure
    boxes and code blocks — and `tables.consumed` does not cover every block whose text it
    claimed, so the same lines are in a table's cells *and* in the flow. Attributed by source,
    every duplicate but three involves a table (`list+table` 84, `caption+table` 47,
    `heading+table` 16). `table.{min_row_rules, min_column_rules, grid_snap_pt, rule_overlap_min}`
    are all `provisional` and have never met a real book.
  - *Aus dem Leben eines Taugenichts* (Project Gutenberg): 66 730 characters **lost** with no
    ledger entry — the other direction, and a different bug.
  - *Tschick* and *O Crime do Padre Amaro* convert clean, and EPUBCheck reports 0 errors and
    0 warnings on both. The law refusing two books rather than shipping duplicated or missing
    paragraphs is it working.
- **A fallback table is emitted as a grid, not as an image plus `<details>`.** PIPELINE §8.7 wants
  the image; rasterising a vector region needs a page renderer in `oc-pdf` that does not exist.
  No text is lost either way, and `El::details` is written and tested for when it does.
- **A tightly set ruled table falls back to an image.** When a cell gutter is narrower than
  `text.line_split_gap_em`, `words` keeps two cells in one run and a `Run` carries a box and its
  text but not its glyphs' positions, so nothing in `structure` can split it. The long-term fix is
  to split runs at vertical rules in `layout`, which needs the rules to reach `layout`.
- **A contents page with no drawn leader is not parsed.** `"Preface    i"` reaches the parser as
  `"Preface i"` — the gap is geometry and a line's text is not.
- **`Note.body` is one paragraph.** A note that runs to several paragraphs is one paragraph here.
- **`RunId` is page-local**, whatever IR_SKETCH calls it. Every map from a run is keyed on
  `(page, RunId)`; `oc_structure::build::NoteRefRuns` names the pair once.
- **Language detection is not wired into the driver.** `convert` uses the configured tag and falls
  back to `LangTag::UND`. `whatlang` is a Phase 2 capability that the stage driver never calls.
- **EPUBCheck and its corpus are fetched, never committed**: `cargo run -p xtask -- fetch-epubcheck`
  and `fetch-epubcheck-corpus` put them under `vendor/`, which `.gitignore` covers.
- Local tool versions: rustc 1.98.1, cargo-nextest 0.9.143, cargo-deny 0.20.2, EPUBCheck 5.3.0,
  Temurin-compatible JVM 23 locally / Temurin 21 in CI.

## Blocked

_(empty — Q1 resolved 2026-09-09; see `docs/DECISIONS_LOG.md`)_

## Phase 5 — Definition of Done

Checked against `IMPLEMENTATION_PLAN.md` §0.3 on 2026-09-14. **One row cannot be verified on a
single machine** and is named as such rather than counted as a pass.

1. **Every named test exists and passes** — all twenty rows of the Phase 5 table (5.1–5.20), plus
   about sixty additions. `docs/TEST_MATRIX.md` lists every one and the CI job that runs it.
   Row 5.17 is behind the `epubcheck` cargo feature and row 5.5 is a CI job; neither is
   `#[ignore]`d, which `xtask ci-lint` enforces.
2. **`cargo nextest run --workspace` green on three OSes** — green locally on Windows. The Linux
   and macOS legs are the `test` matrix job and are unverified until CI runs.
3. **`cargo clippy --workspace --all-targets -- -D warnings`** — clean.
4. **`cargo fmt --all --check`** — clean.
5. **`cargo deny check`** — clean (advisories, bans, licences, sources).
6. **`cargo xtask thresholds-lint`** — clean; four new entries, each with the five keys.
7. **Every acceptance criterion demonstrated:**
   - **A5.1** (every fixture, 0 EPUBCheck errors) — **met, and measured**: EPUBCheck 5.3.0 reports
     0 errors *and 0 warnings* on all ten fixtures. Row 5.17 is the standing gate.
   - **A5.2** (the same input twice is byte-identical modulo `dcterms:modified`) — row 5.4 over all
     ten fixtures, and `cli::convert_writes_a_valid_container_and_leaves_no_temporary` through the
     binary.
   - **A5.3** (identical sha256 on ubuntu/macos/windows) — **partial, and unverifiable here**: the
     `epub-bytes` matrix job and the `epub_is_byte_identical_across_os` job are written, and one
     machine cannot run them. Everything they depend on — pure-Rust codecs, a fixed JPEG quality, a
     fixed resampling filter, sorted entries, fixed timestamps, `--modified` — is in place and
     tested on this machine.
   - **A5.4** (Tier 1: bijection, page-list, alt, no-script) — `tier1_passes_on_every_fixture`,
     plus one test per check against a container broken in exactly that way.
   - **A5.5** (a footnote inside a paragraph does not compile) — `phrasing_cannot_contain_figure`
     is the same claim about the same trait bound; `El<Phrasing>` has neither `figure` nor
     `aside_footnote`, both being `FlowContext` methods.
   - **A5.6** (per-message-id parity recorded and non-decreasing) — `docs/TIER1_PARITY.md` is
     generated by `xtask epubcheck-parity` over EPUBCheck's own corpus, and `--check` is the CI
     gate. Expanded publications are zipped by this project's writer on the way in, so an
     OCF-level defect in one of those cases is repaired before Tier 1 sees it; the 25 packaged
     `.epub` files are the ones whose container bytes are measured.
8. **`docs/CHANGELOG.md`** — Phase 5 entry written.
9. **No `TODO`/`FIXME` without an issue number** — `xtask ci-lint` clean.

### What EPUBCheck found that the tests did not

Two real defects, both fixed, both now with a Tier-1 check of their own:

- Image `src` was written package-root-relative from a document in `text/`, so every figure
  resolved to `text/images/…` and was missing (`RSC-007`). Tier 1 checked fragments and not
  resources; it checks both now.
- A document that yielded no text produced an empty spine, an empty nav `<ol>` and an empty
  `navMap` — three `RSC-005`s and not a book. A book with no text now carries its pages as
  figures (PIPELINE §10).

That is the whole argument for D6's Tier 2 being a hard gate rather than a nice-to-have.

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
2026-09-14  P5.1      oc-model Document + the document stage; page breaks, class, preset (10)   5d3717a
2026-09-14  P5.2      oc-epub: the typed XHTML builder; two compile-fail rows (5.1, 5.2 + 14)    b100485
2026-09-14  P5.3      oc-epub: the deterministic OCF container (5.3 + 3)                          b57bfad
2026-09-14  P5.4      oc-epub: one stylesheet that names no typeface (5.12 + 2)                   c7cd1e5
2026-09-14  P5.5      oc-structure: spans carry their styles and their note references (2)        ea69e20
2026-09-14  P5.6      oc-epub: content documents, package, nav, ncx, images, container (5.4-5.14, 5.20 + 14)  24087ae
2026-09-14  P5.7      oc-validate: Tier 1, against real and crafted output (5.15, 5.16 + 6)       a4b8e7b
2026-09-14  P5.8      openconvert: convert + validate; the pipeline moved into the library (5.19 + 5)  19c31b1
2026-09-18  P6.1      oc-validate: I-7 over the archive, and retention as a flag (6.1-6.3 + 4)     c5a5627
2026-09-18  P6.2      oc-validate: the structural report - headings, duplicates, quality (6.16 + 15)  3bca6fd
2026-09-18  P6.3      oc-validate: the repair loop, its measure and its table (6.4-6.7, 6.9 + 12)  7b35583
2026-09-18  P6.4      openconvert: the loop on the real path, fire rate zero (6.10 + 5)              9889827
2026-09-18  P6.5      openconvert: report.json, --report, the post-cap policy (6.8, 6.12 + 4)        e445783
2026-09-18  P6.6      oc-core: the warning registry, en/de/tr templates, the registry lint (6.11 + 7)   PENDING
