# PROGRESS — OpenConvert

<!-- Machine-readable state. Claude Code reads this first and rewrites it after every completed work item. -->

STATUS: IN_PROGRESS
CURRENT_PHASE: 10
CURRENT_ITEM: 10.1 — not started. Phase 9 is complete and merged (2026-09-23); its provisional
              decisions are listed under ## Blocked

STATUS stays IN_PROGRESS: each item below was decided in the most conservative way consistent with
DECISIONS.md, logged in `docs/DECISIONS_LOG.md` (2026-09-23) as **PROVISIONAL — needs maintainer
ratification**, and worked around. None of them blocks Phase 10's deterministic-side work.

1. **Registry pins** — `models.toml` still has `TODO_` `revision`/`sha256` and `size_bytes = 0` for
   all four entries: huggingface.co is refused by this sandbox's egress policy. Fill them on a
   machine that can reach it with `eval/model_gate.py --emit-registry`, which hashes the download
   itself. Until then `model list`/`pull` on the bundled registry exit 2.
2. **llama.cpp digests** — `xtask/llama.lock` pins `b10456` with `TODO_SHA256` digests (github.com
   release assets refused). `xtask fetch-llama-server` refuses to run until they are hashed, so the
   nightly live job fails at its first step until then.
3. **`LLAMA_API_KEY`** — the sidecar's key goes in that environment variable, never in argv. That
   the pinned build reads it could not be run here. `--api-key-file` is the fallback.
4. **The allowlist's CDN hosts** are the plan's three, and today's redirect target could not be
   observed. An off-list redirect fails closed and names the host.
5. **PDEATHSIG and Windows job objects** (crash and SIGKILL teardown) need `unsafe` and are
   deferred to Phase 14.
6. **G3/G7/G9 inputs, and G7's numbers** — no reference tokenizations, no paired answers, no own
   conversion. `model_gate.g7_alpha = 0.05` and `g7_noninferiority_margin = 0.02` are invented.
7. **`apps/desktop/src-tauri/src/llm.rs`** is deferred to Phase 12, which owns the desktop app and
   is being built concurrently.
8. **G8's probes are generated, not native-speaker authored**, and their labels follow from their
   templates.

## Phase 7 — Definition of Done

`IMPLEMENTATION_PLAN.md` §0.3, row by row. Checked on this machine unless the row says otherwise.

| Row | State |
|---|---|
| Every named test exists and passes | **Yes.** All 14 of PHASE 7's rows, plus ~180 additions. Rows 7.11 and 7.12 are behind the `bench` cargo feature and pass with it on. |
| `cargo nextest run --workspace` green | **Yes**, 466 tests. |
| Green on Linux/macOS/Windows CI | **Not verifiable here.** Windows only. |
| clippy `-D warnings` clean | **Yes**, workspace, all targets, all features. |
| `cargo fmt --check` clean | **Yes.** |
| `cargo deny check` clean | **Yes** — no new Rust dependency; `criterion` was already in the workspace manifest. |
| `cargo xtask thresholds-lint` clean | **Yes.** Eight thresholds added, each with source, evidence, owner and an unexpired `review_by`. |
| Every Given/When/Then demonstrated | **A7.1, A7.1b, A7.2 and A7.3 yes** (below). **A7.4 partially**: the gate is written and tested, and there is no "last green" until the nightly has run once. |
| `docs/CHANGELOG.md` entry | **Yes.** |
| No `TODO`/`FIXME` without an issue number | **Yes**, `xtask ci-lint` clean. |

### Acceptance criteria

- **A7.1** — `oc-eval corpus lint` is clean. `ours(*)` is 0.103 against the 0.40 cap; the
  holdout is 104 **documents**, counted with page-level entries excluded.
- **A7.1b** — the composition meets §7.6's target as written: OAPEN 44 (~40), Internet Archive
  20 (~20), arXiv 15 (~15), US-Gov 15 (~15), DergiPark 10 (~10), and the German (34) and
  Turkish (10) slices are non-empty.
- **A7.2** — `report.build` emits `per_file`, `per_stratum` and `ours_vs_real`, and
  `test_per_stratum_scores_are_reported_separately` asserts no `aggregate` key exists.
- **A7.3** — **0.0217 s/page over 300 pages**, unoptimised `test` profile, against 0.5. The
  gate fails when the budget is lowered below the measurement. Reference machine L will differ.
- **A7.4** — the gate is `last green - margin` where the margin is the Wilson half-width, and
  it refuses to gate at all below `eval.assertion_min_instances`. The first nightly records the
  baseline it will compare against.

### What Phase 7 was asked to settle, and did not

The seven items the phase was carrying are not all closed, and it is worth saying which.

1. **`validate.min_char_retention` is still not a gate.** The corpus now exists to choose
   between the two candidate definitions, and nothing has run against it yet. Still open.
2. **`validate.h1_count_min_pages = 20`** is first exercisable now — 104 real documents,
   most well over twenty pages — but the first exercise is the nightly's, not this branch's.
3. **`validate.dup_block_frac = 0.02` has still met no real book.** Same reason.
4. **The two real books the conservation law refuses** have not been re-run. They are in
   `example_pdfs/`, which is not corpus, and the table thresholds are still `provisional`.
5. **A2.3, A3.2, A3.3 and A4.3** are still partial. The corpus exists; the gold data for them
   does not, and `corpus/gt/` holds one hand-written sample rather than §5.5's ~50-file set.
6. **The benchmark harness is done**, and A1.6 is a measurement on this machine rather than
   on reference machine L.
7. **Repair fires per id per stratum** — `oc-eval run` records `repairs_fired` per file and
   the report groups per stratum, so RT A10.4 is answerable on the first nightly.

## Phase 6 — Definition of Done

Checked against `IMPLEMENTATION_PLAN.md` §0.3 on 2026-09-18, every gate run for real. **Two rows
cannot be verified on a single machine** and are named as such rather than counted as passes.

1. **Every named test exists and passes** — all sixteen rows of the Phase 6 table (6.1–6.16), each
   run individually by name, plus about sixty additions. `docs/TEST_MATRIX.md` lists every one and
   the CI job that runs it. Rows 6.13–6.15 are the Playwright specs (44 + 20 + 2 assertions at one
   viewport, 198 across three); row 6.25 is behind the `ace` cargo feature; neither is `#[ignore]`d,
   which `xtask ci-lint` enforces.
2. **`cargo nextest run --workspace` green on three OSes** — **cashed.** CI run 35462059355 on `main`:
   `test (ubuntu-latest)`, `test (macos-latest)` and `test (windows-latest)` all green, alongside
   every other job. 456 locally. It was **not** green on the first attempt: macOS failed
   `golden_epub_bytes_f01` and Ubuntu ran out of disk, both for real reasons, both fixed.
3. **`cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`** — clean.
4. **`cargo fmt --all --check`** — clean.
5. **`cargo deny --all-features check`** — advisories, bans, licences, sources ok.
6. **`xtask thresholds-lint`** — clean; four new entries, each with the five keys. **`ci-lint`** —
   clean, including its new rule that the warning registry and the tree agree in both directions.
7. **Every acceptance criterion demonstrated:**
   - **A6.1** (I-7 holds) — `i7_holds_end_to_end_on_all_fixtures` over all ten, measured over the
     **archive** through the package document's spine, plus
     `the_archive_and_the_emitter_agree_about_the_text` for the two measurements agreeing. "Every
     corpus file" is Phase 7's corpus; the same scope limit as A2.3 and A3.2, for the same reason.
   - **A6.2** (zero repairs fire) — `repair_fire_rate_is_zero_on_corpus`, against
     `repair.corpus_fire_rate_max = 0`. Every fixture reports `RepairStatus::Clean`.
   - **A6.3** (≤ 3 iterations, strictly decreasing `M`, or a named status) — the four control rules,
     each **mutation-tested**: deleting strict decrease, the new-id rule or the hash rule turns
     exactly one of rows 6.4/6.5/6.6 red, and nothing else.
   - **A6.4** (the report's contents) — `the_report_carries_every_part_the_plan_names` asserts each
     part the plan's detail 6 names, by name; `report_schema_is_valid_and_snapshotted` snapshots
     `f07` with the timings redacted.
   - **A6.5** (3 viewports, no overflow, nav/DOM agreement, noterefs resolve) — **cashed.** The
     `dom-checks` job passed on its first real run in CI 35462059355, and nightly `webkit-dom` passed
     too, so the assertions hold in both engines. 198 Chromium assertions, each spec
     mutation-tested (a 3000px block, two swapped nav entries, a `display: none` footnote).
8. **`docs/CHANGELOG.md`** — Phase 6 entry written.
9. **No `TODO`/`FIXME` without an issue number, no `#[ignore]`** — `xtask ci-lint` clean; zero
   `test.skip` or `.only` in the Playwright specs either.

**One file of the plan's Files list is elsewhere**, with the reason recorded: `report.rs` is in
`openconvert`, not `oc-core`. Assembling the report needs `Tier1Report`, `StructuralReport` and
`RepairOutcome`, which are `oc-validate`'s, and `oc-validate` depends on `oc-core` — the plan's
placement is a cycle.

**One deliverable is deliberately smaller than the plan asks.** The repair table has three `AutoFix`
entries where the plan says "the ~30 ids our own generator can plausibly trigger". This emitter
triggers none of them: EPUBCheck reports zero errors on all ten fixtures. Thirty speculative repairs
would be thirty untested paths, against the one gate (A6.2) that says every repair firing is an
emitter bug.

### What Phase 6 found that Phase 5 did not

- **The `document` stage's conservation check was never recorded in the ledger.** It ran — a
  violation returns `DocumentError` — but `convert` dropped the `StageCheck`, so every phase's
  "I-1 … I-4 were checked after every stage" rested on a record naming seven stages where the
  pipeline had checked eight.
- **`validate.min_char_retention` and `conservation.budget.furniture` are jointly unsatisfiable.**
  Four fixtures retain 0.968–0.973, all of it ledgered furniture inside its budget.
- **The Gopher n-gram statistics cannot gate output text.** `f09` scores `top_3gram` 0.8008 against
  a 0.18 bound and is correct: its contents page is hundreds of `. . .` leaders.

### What CI found that Phase 6's own testing did not

Seven defects, none a flake, none findable on one machine. The full argument for each is in
`docs/DECISIONS_LOG.md`, 2026-09-19; in one line each:

1. **The container's bytes differed on Windows.** `zip` fills the "version made by" host byte from
   the building platform; the field is fixed-width, so the EPUB came out the same length with
   different bytes. `zip.rs` already *claimed* the field was pinned.
2. **Ubuntu ran out of disk mid-link.** Phase 6's eight new integration-test binaries;
   `debug = "line-tables-only"` cut the workspace's test executables from 5.7 GB to 488 MB.
3. **`epub-pagesource`, serious.** A book publishing page numbers did not say where they came from.
4. **`metadata-accessmodesufficient`.** The condition was inverted — textual sufficiency claimed for
   books that *had* images and withheld from books that were nothing but text.
5. **`epub-type-has-matching-role`**, on every content document: no `role="doc-chapter"`.
6. **The Ace runner read `data.metadata`**, a key Ace does not write, so the metadata half of its own
   gate would have reported everything missing on every book. Its unit fixture had been composed from
   the documentation by the same hand as the parser.
7. **`xtask fetch-epubcheck` unpacked a zip with `tar`.** Works on Windows, where `tar` is
   libarchive; GNU tar refuses it. Phase 5 code, and `epubcheck` and `tier1-parity` had never once
   run in CI because `needs: test` had never passed.

Nightly is green too (run 35462081049): `webkit-dom`, and `ace-a11y` once Ace's Electron was given
a root-owned setuid `chrome-sandbox` and an Xvfb display.

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
2026-09-18  P6.6      oc-core: the warning registry, en/de/tr templates, the registry lint (6.11 + 7)   167227d
2026-09-18  P6.7      tests/dom: the Playwright DOM checks, three viewports, CI on (6.13-6.15 + 4)    4a71fc0
2026-09-18  P6.8      oc-validate: the Tier-3 Ace runner and the nightly ace-a11y job (5 tests)      5855c53
2026-09-19  P6.ci     seven defects CI found: cross-OS bytes, Ace a11y x3, disk, tar/zip     23c7064
2026-09-20  P7.1      corpus: sha256 before use, mirror-then-source, the LOCAL_EVAL boundary (9)   3bf9c1c
2026-09-20  P7.2      corpus: the manifest vocabulary and fourteen lint rules (7.1, 7.3b + 21)     5281b83
2026-09-20  P7.3      corpus: the frozen holdout, 104 real documents, five sources (7.2, 7.3 + 50)  86848f8
2026-09-20  P7.4      oc-testkit: the mutation catalogue, ten recipes with declared effects (7.6 + 3)  3b55dbb
2026-09-20  P7.5      oc-eval: ground truth from XHTML, struct trees and LaTeX (7.7 + 16)     36b6384
2026-09-20  P7.6      oc-eval: metrics, the Wilson gate, the per-stratum report (7.8, 7.9 + 21)  ee146db
2026-09-20  P7.7      oc-eval: the ours-vs-real gap recorded and plotted (7.10 + 7)      8a056c0
2026-09-20  P7.8      oc-eval: calibration refuses the holdout; risk-coverage (7.4 + 11)  bb253f3
2026-09-20  P7.9      openconvert: the perf budget, 0.0217 s/page on 300 pages (7.11, 7.12 + 14)  1f94be4
2026-09-20  P7.10     ci: the python job, corpus lint, and four nightly bodies (7.13, 7.14 + 16)  61edb1d
2026-09-22  P8.1      oc-ai: v1 prompts, a llama.cpp GBNF parser, the request (8.1, 8.14 + 19)  42807c2
2026-09-22  P8.2      oc-ai: gate S, and gate D's record in Decision.fallback (8.2-8.4, 8.9, 8.12 + 6)  6c47025
2026-09-22  P8.3      oc-ai: gate L - C unchanged without the ledger, then reading order (8.5, 8.6 + 3)  7b83a2a
2026-09-22  P8.4      oc-ai: gate V - the fixed tuple, undefined skipped, oc-text's statistics (8.7, 8.8 + 5)  e635630
2026-09-22  P8.5      oc-ai: one call budget per book, W_LLM_BUDGET_EXHAUSTED, unasked decisions (8.13 + 2)  3af6cef
2026-09-22  P8.6      oc-ai: the cache key - one rule for cache and cassettes - and the file cache (8.10 + 5)  e1a7b8a
2026-09-22  P8.7      oc-ai: one OpenAI-compatible client over a Transport; the adversarial stub (+ 7)  cc95e87
2026-09-22  P8.8      oc-ai: cassettes at the provider seam, replay exact, four seeds (8.11 + 5)  6294cc6
2026-09-22  P8.9      oc-core: the six escalation predicates, pure, table-tested (8.16)  1074970
2026-09-22  P8.10     oc-ai: no socket by dependency or by std; CI unshare step; the DoD (8.15)  e5ad4ef
2026-09-22  PHASE 8   COMPLETE on worktree-phase8 - Definition of Done checked; Linux/macOS CI and the unshare step unverified until merge
2026-09-23  P9.1      oc-net: models.toml registry refuses TODO_ pins and non-commit revisions (9.1, 9.2)  fe32587
2026-09-23  P9.2      oc-net: pinned download, allowlist per hop, SHA-256 while streaming, atomic (9.3-9.6)  5ffd509
2026-09-23  P9.3      oc-net: store list/remove, HttpTransport (9.19 + 4)  5086e76
2026-09-23  P9.4      oc-core: no net dependency, walked from Cargo.lock; CI step on (9.7)  1774052
2026-09-23  P9.5      oc-core: llama-server argv from the registry, key never in argv (9.12, 9.14 + 1)  595be0b
2026-09-23  P9.6      oc-core: OwnedServer, supervise, endpoint; teardown on exit/panic/signal (9.8-9.11, 9.13 + 4)  6ac5582
2026-09-23  P9.7      openconvert: model pull|list|remove, ModelReadiness (9.18 + 4)  1a8c4ac
2026-09-23  P9.8      oc-testkit: live tests behind live-llm; W_LLM_PREFIX_COLD; fetch-llama-server (9.15, 9.16 + 5)  9f223a0
2026-09-23  P9.9      eval: model_gate.py, probes, fixtures, MODEL_GATE.md (9.17, 9.20 + 17)  b9c0ab4
2026-09-23  PHASE 9   COMPLETE on phase/09-local-model - DoD checked; live model, gate runs, macOS/Windows and CI unverified here
