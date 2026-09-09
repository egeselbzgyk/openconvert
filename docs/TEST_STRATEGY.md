# OpenConvert — Test Strategy

**Status:** Draft for v1 planning, written against `DECISIONS.md` (final for v1 planning) and `IR_SKETCH.md`. Never contradicts `DECISIONS.md`; points of disagreement are collected in the closing section rather than argued inline.
**Date:** 2026-09-09
**Scope:** how OpenConvert is tested, from the first unit test written before the first line of pipeline code to the nightly benchmark run that decides whether a model is promoted from `experimental` to default.

---

## 1. Philosophy

OpenConvert is built test-first, component by component: **RED → GREEN → refactor**, applied to every crate in the workspace (`oc-model`, `oc-pdf`, `oc-text`, `oc-layout`, `oc-structure`, `oc-epub`, `oc-validate`, `oc-ai`, `oc-net`, `oc-core`) before it is applied to the binary that links them. A stage's test file exists before its implementation file. This is not a stylistic preference; it is a direct consequence of the architecture DECISIONS.md commits to. D13.5's four-gate escalation design (S/L/V/D) and D13.4's ledger-balanced conservation law are only meaningful if the properties they assert can be checked mechanically — which means the check has to be written, and shown to fail, before the code that is supposed to satisfy it exists. A conservation invariant nobody has tried to violate is not a safety net, it is a comment.

The deeper reason TDD fits this project specifically: OpenConvert's pipeline is deterministic by design (D13.1, D17), but "deterministic" does not mean "always right." Every stage that makes a judgment call — is this a heading, is this run-in text a heading, does this indent mean verse or blockquote — attaches a `Confidence` to its answer (`IR_SKETCH.md`, `Confidence { method, score, signals, escalated, fallback_used }`). **The deterministic stage's most valuable output is its doubt.** A stage that is silently wrong 2% of the time is worse than a stage that is wrong 2% of the time *and says so*, because the second stage's uncertainty is a signal the rest of the system — the escalation predicate (D13.5), the repair loop (D13.7), the report (§8 below) — can act on. Tests therefore verify two things about every stage, not one: that its answer is correct on the cases it should get right, and that its stated confidence (or, in v1, its escalation predicate — D17 ships with `ai.enabled = false` and structural predicates, not calibrated scores) fires exactly on the cases it should be unsure about. A test suite that only checks "did it produce the expected IR" and never checks "did it correctly decline to be sure" is testing half the product.

Concretely, "tests before code" means: for a new stage or heuristic, the first artifact is a fixture (§3.2) with a hand-authored expected IR fragment and, for anything touching the character ledger, an assertion that the relevant conservation invariant (§4) holds under the change. For an LLM-touching change, the first artifact is a cassette (§6) and its semantic assertions, written against the *intended* schema before the prompt exists. Refactors lean on snapshot tests (§3.3) precisely because a green snapshot suite makes it safe to rewrite a stage's internals without re-deriving every fixture's expected output by hand.

## 2. Test tiers

Three tiers, distinguished by what they may call and how long they may take (R9 §B.9), enforced by `cargo nextest` (D11) profiles and CI job boundaries:

| Tier | Scope | Budget | May call |
|---|---|---|---|
| **Fast (unit)** | Single-function/single-stage logic; tiny hand-made fixture PDFs; property tests at low iteration counts; metamorphic relations on synthetic content-stream fixtures; JSON-schema contract tests against cassette-replayed LLM responses; EPUBCheck/Ace against tiny fixture EPUBs | < 1 s per test; whole tier in the low tens of seconds | Nothing external. No real LLM calls (cassette only), no `pdftotext`, no full corpus. Runs on every `cargo nextest run` and blocks every commit locally. |
| **Integration** | A curated ~20-file corpus slice, one file per `/Producer` stratum (D18) and page-class (D13.10); differential text-coverage checks against `pdftotext`; canonical-JSON-IR snapshots on real (non-tiny) documents; a small Playwright DOM-assertion pass | < 2 minutes total | External-binary oracles (`pdftotext`) allowed. Still cassette-only for LLM calls, unless the test explicitly exercises the record path. Runs on every PR, all three OSes. |
| **Nightly** | Full corpus (D18's stratified set, plus the frozen ≥100-file real-world holdout); the full olmOCR-bench-style fixture assertion suite with confidence intervals (R9 §A.5, §A.16); mutation testing (`cargo-mutants` on Rust, `mutmut` on `eval/`); property tests at high iteration counts; benchmark-harness timing/RSS runs (§8); paired deterministic-vs-AI comparison (McNemar, §6); calibration checks (reliability diagrams, §7) | Minutes to hours | The only tier where a bounded number of **live** LLM calls is acceptable — specifically to refresh cassettes and to re-run golden-decision accuracy checks. Non-determinism is tolerated here because nothing here is PR-blocking. |

Rationale for the boundary between fast and integration: fast-tier tests must run before every commit without friction, so they are OS-insensitive by construction (canonical JSON, redacted volatile fields, no pixels) and touch nothing outside the process. Integration tests need real corpus files and an external oracle binary, which cost seconds, not milliseconds, per file — acceptable once per PR, not once per `cargo watch` cycle.

## 3. Test types, and how each is used in OpenConvert

### 3.1 Unit tests

Ordinary `#[test]` functions inside each crate, one assertion domain per test, run under `cargo nextest`. These cover pure functions with no PDF/IR dependency: the `N` normalization pipeline (soft-hyphen strip, ligature expansion, NFC — D13.4) on hand-picked strings, `BlockId` derivation and its collision-suffix logic (`IR_SKETCH.md`), canonical-JSON serialization rules (sorted keys, 2-dp geometry rounding *at serialization only*, NFC strings, no NaN/Inf), the Turkish-aware folding key builder, and the `thresholds.toml` loader/validator (§9). Unit tests are where a bug is cheapest to find; a bug that reaches a fixture test instead of a unit test is a signal the function under test was under-decomposed.

### 3.2 Fixture tests: tiny hand-made PDFs + fact assertions

The primary regression mechanism for pipeline logic, modeled directly on olmOCR-bench's design (R9 §A.5, §B.2): each fixture is a minimal, hand-crafted PDF isolating exactly one concern — a single rotated page, a two-column layout, a table with a merged cell, a footnote with its marker, a ligature (`ﬁ`), a word hyphenated across a line break, a subset-embedded font, an encrypted-with-empty-password file, a malformed xref. Fixtures are built with the synthetic toolchain (Typst for valid-but-complex documents, `lopdf`/`qpdf --qdf` for deliberate mutation — R7 §C) so their defects are reviewable as git diffs, and each fixture ships with **explicit fact assertions** rather than only a snapshot diff — "this string appears before that string in reading order," "this footnote marker links to this note body," "this block is `SectionRole::Chapter` at `level = 1`." Assertions survive intentional, benign IR changes (a renamed field) that would break a naive snapshot; snapshots (§3.3) catch the changes assertions don't think to check. Both run against every fixture — they are complementary, not redundant.

Fixtures are the vehicle for regression testing every entry in the `Reason` enum (D13.4): a `RunningHeader` fixture, a `Dehyphenate` fixture whose expected output is the *joined* form, an `OcrLayerDuplicate` fixture, and so on — because each `Reason` is a place the conservation ledger (§4) is expected to fire, and a fixture without that ledger assertion is not actually testing the reason, only the surface behavior.

### 3.3 Snapshot tests (`insta`)

Two distinct snapshot strategies, per corpus-file size (RED_TEAM §B4 — a 300-page book's canonical IR is tens of MB and unreviewable as a diff):

- **Canonical JSON IR snapshots**, full document, for **tiny fixtures only** (§3.2). These are the primary refactor-safety net: sorted keys, 2-dp-rounded geometry, redacted volatile fields (nothing in the IR is a timestamp, but a future field that is one gets redacted, not mocked), per-OS line-ending normalization at snapshot-write time (so a CRLF-vs-LF difference between a Windows and Linux CI runner never produces a spurious diff). Reviewed on every diff, per `insta`'s workflow — a snapshot diff in a PR is a required-reading artifact, not an auto-accept.
- **Structural digests**, for corpus files (integration and nightly tiers): per `IR_SKETCH.md`'s own definition — counts per `Content` variant, heading-tree shape as `(level, text[..40])` tuples, ledger totals per `Reason`, per-page class histogram, first/last 200 chars per section. This is the only way to snapshot a real book without producing an unreviewable wall of JSON, and it is deliberately the same digest shape the IR sketch specifies for corpus snapshots, so the test harness and the format spec never drift apart.

Snapshots of generated XHTML/OPF/NCX are stored with real file extensions (`.xhtml`, `.opf`) rather than escaped inline strings, so a diff reads as a normal markup diff in code review.

### 3.4 Property-based tests (`proptest`)

Properties that must hold for *all* generated inputs in a described range, not just the fixtures anyone thought to hand-author, run at bounded iteration count in the fast tier and high iteration count nightly:

- **Normalization idempotence:** `N(N(x)) == N(x)` for generated strings, directly testing D13.4's "applied exactly once" invariant — if this property fails, either `N` is not idempotent (a bug) or something downstream is re-applying it (an architecture violation the property test is the cheapest possible way to catch).
- **No text loss modulo the ledger:** for a generated document, `C(D_i) ⊎ chars(Added) == C(D_{i+1}) ⊎ chars(Removed)` — this is I-1 (§4) expressed as a property over synthetically generated stage transformations rather than only checked on real pipeline runs, so it also exercises the invariant-checking code itself.
- **Monotonic reading order on single-column pages:** for property-generated block positions on a synthetic single-column layout, extracted block order is monotonically increasing in the vertical (then horizontal) axis. This is a property, not a golden value — it should hold for any valid single-column layout, not only the ones a fixture author picked.
- **Parser never panics:** `oc-pdf`'s wrapper layer never panics on arbitrary byte input, using raw-byte generators over the outer PDF container structure. Per RED_TEAM §B9, this is aimed at *our* wrapper and IR/job-spec code, not at re-fuzzing PDFium's own parser (already continuously fuzzed by OSS-Fuzz) — see §3.8's fuzzing note.

`proptest`'s shrinking is why this tier matters beyond what fixtures give: a failing case is automatically reduced to a minimal reproducer, which becomes the next fixture.

### 3.5 Metamorphic tests

Relations that must hold across *related* executions, where no independent ground truth is needed — the right tool exactly where "correct output for an arbitrary real PDF" has no oracle (R9 §B.4, citing Chen et al. 1998 and Segura et al. 2016's survey):

- **Page-order permutation invariance:** for a synthetic multi-page fixture, permuting page order and re-running extraction yields the same per-page text content, correspondingly permuted.
- **`/Rotate` invariance:** rotating a page 0/90/180/270° (a metadata-only change on a synthetic fixture) yields an identical extracted-text reading-order sequence, even though bounding boxes differ. This directly guards the coordinate-space bug class D13.3 calls out by name — mixing CropBox and image space "silently deleted body text in a shipping 2026 tool."
- **Content-stream reordering within a line:** shuffling the order of drawing operators within a single text line's content stream (glyph visual positions held fixed) must not change extracted text. This targets a real, common PDF-producer quirk (glyphs emitted out of visual order) that is cheap to generate synthetically — a small content-stream builder emits N random valid orderings of the same operator set — but effectively impossible to hand-author exhaustively as fixtures.

These run in the fast tier: no external oracle, synthetic-only, cheap.

### 3.6 Differential testing

`pdftotext` (Poppler, GPL, external binary, invoked via subprocess — never linked, per D15's exclusion of GPL from shipped crates) is the **external, CI-only, integration-tier string oracle**: for a corpus file, every word `pdftotext` extracts should appear somewhere in OpenConvert's produced text (modulo `N` normalization and dehyphenation), and the reverse check flags text OpenConvert extracted that `pdftotext` did not — a useful signal, not an automatic failure, since OpenConvert's font-signal-aware extraction (D3) is expected to recover text `pdftotext` misses on some producer strata. **`hayro` is explicitly not this oracle in v1** — per D3's residual risk and RED_TEAM §B3, `hayro` is experimental, has no encrypted-PDF support, and does not expose the char-level signals (`font_weight`, `is_generated`, `is_hyphen`, `render_mode`) the pipeline depends on, so a `hayro` diff could only compare raw strings — which `pdftotext` already does, for free, with a mature and stable implementation. `hayro` remains the `PdfBackend` trait's second implementation candidate (D3) and a future oracle once it matures; it is not wired into any test tier in v1.

### 3.7 Regression suite keyed by `(file, sha256)`

Every corpus-file test entry is keyed by filename **and** the source file's SHA-256, not filename alone (R9 §B.6). This makes silent fixture corruption or accidental edits visible as a hash mismatch requiring an explicit re-baseline rather than a silent drift, lets third-party corpus slices (a licensed OmniDocBench/olmOCR-bench subset, D18's real-world sources) be vendored with provenance intact, and supports content-addressed caching of expensive nightly runs (skip re-running a stage when both the input hash and the pipeline-version hash match a cached result).

### 3.8 Mutation testing

`cargo-mutants` over the Rust workspace, `mutmut` over `eval/`'s Python (D11), **nightly only** — mutation testing reruns the full suite once per mutant and is too slow for a PR gate on a non-trivial codebase. Mutation score is tracked as a trend metric from the first nightly run; a hard floor is set only once a baseline exists (consistent with §9's "provisional, not invented" discipline). Per RED_TEAM §B9, fuzzing budget (`cargo-fuzz`, also nightly-scheduled given its runtime) is aimed at **our own** structured-input parsers — the IR deserializer, the job-spec parser, and the XHTML/OPF emitter round-trip — not at PDFium's own parser, which OSS-Fuzz already fuzzes continuously and which OpenConvert does not maintain. Crash-inducing PDFs (from the Isartor test suite, R7 §A.1 item 26, and mutated corpus files) become regression fixtures under §3.2/§3.7, not fuzz targets.

### 3.9 Contract tests for the LLM adapter

The LLM adapter's boundary is tested like any other typed API: given a fixed, cassette-replayed input/response pair (§6), assert the response validates against the call's JSON Schema/GBNF grammar (D13.6), independent of whether the response content is semantically good — that is a separate concern (§3.10). Schema validity is a fast-tier test: cheap, deterministic, and (via cassettes) requires no live model.

### 3.10 Golden-decision tests with accuracy thresholds

For each of D13.6's four LLM tasks (metadata extraction, heading-style-cluster role assignment, book-structure boundaries, verse/quote classification), a human-labeled golden set — bootstrapped largely for free from Standard Ebooks → synthetic-PDF ground-truth pairs (R7 §B.1; RED_TEAM §C4), with the ~50-file hand-labeled set reserved for what has no free ground truth (furniture removal, verse-vs-quote on real, not `ours(*)`, PDFs per RED_TEAM §A9/§C4) — is scored for **aggregate accuracy**, not per-case exact match, because reasonable labelers can disagree on hard cases (R9 §C.5). The initial gate is set at the measured baseline accuracy **minus one margin of error** (mirroring olmOCR-bench's own ± reporting convention), never a round guessed number: if a golden set first measures 91% ± 3%, the CI gate is ~88%, and that derivation is recorded in `thresholds.toml` (§9) with `source = calibrated` once enough golden-set data exists, `provisional` before that.

### 3.11 Canary tests for prompt regressions

A small, fixed, cassette-backed set of prompt/input pairs whose **schema validity and semantic-assertion pass rate** (§3.9, §6) must stay stable across prompt-template edits — distinct from golden-decision tests, which measure *accuracy against human labels*; canaries measure *stability of output shape* (R9 §C.6). A prompt-version bump invalidates every cassette keyed to the old version, forcing a deliberate, reviewed re-record rather than silent staleness (§6) — the mechanism that makes a canary failure either an intentional, reviewed re-record or a caught regression, never an accident.

### 3.12 EPUB conformance tests: EPUBCheck, Ace, Tier-1 parity

Per D6: **EPUBCheck error count = 0 is a hard release gate on every corpus file**, run in CI against the real Java tool (not bundled in the default install — the "validation pack" is opt-in, D6/RT-A6). **Ace: 0 serious violations** is the corresponding accessibility gate. Both are binary/count gates, not judgment calls, and belong at the fixture level (fast-tier, tiny EPUBs) as well as the corpus level (nightly, full run).

Critically, per RED_TEAM §A6, D6's Tier-1 internal Rust validator is **not assumed to catch what EPUBCheck catches** — the four codes Tier-1 targets by default (RSC-005, RSC-012, OPF-014, PKG-007) are the errors *generic* Word/InDesign-sourced converters make, not the content-model violations (`<figure>` inside `<p>`, `<aside>` in phrasing context, invalid `epub:type` vocabulary) a hand-rolled semantic emitter is actually likely to produce. D6 therefore requires Tier-1's coverage to be **measured, not asserted**: Tier-1 runs against EPUBCheck's own public BSD-3 test corpus, and per-message-ID parity is tracked as a CI-recorded number, the same method `epubveri` used to reach its own reported parity — copying the method, not the AGPL code. Book-specific checks layered on top of the generic four (noteref↔footnote **bijection**, every `page-list` target resolves, non-empty `alt`, no `<script>`/`remote-resources`, case-insensitive entry-name collisions, image-count parity against extraction) are asserted directly, since they are OpenConvert-specific and have no EPUBCheck-corpus equivalent to measure parity against.

### 3.13 CI DOM checks (Playwright)

Structural, non-pixel DOM assertions on the generated XHTML, rendered in a real engine: **Chromium on every PR, WebKit nightly** (D7; the WebKit run matters because Tauri's own macOS/Linux webviews and most e-readers — Apple Books, Kobo — are WebKit-based, per R6 §11 Layer 2). Checks: `scrollWidth > clientWidth` overflow at three fixed viewports (a 600×800 e-reader-like viewport, a 390×844 phone viewport, a 1024×768 tablet viewport), heading-level order (no skipped levels), footnote pop-up markup (`<a epub:type="noteref">` / `<aside epub:type="footnote">` pairing resolves and is reachable, per D5's structural-semantics commitment). These are cheap, OS-insensitive assertions on rendered layout and markup — not pixels — and are the primary "does this actually work in a real engine" signal (D7's residual-risk note on in-app hidden-window DOM measurement being undocumented is why this lives in CI, not in the app).

### 3.14 Screenshot tests

**Release-time only, one OS, human-reviewed** (D7) — not a CI gate on any PR. Per-OS golden screenshots are, per D7's own rationale and RED_TEAM §B8, "the highest-maintenance, lowest-yield test artifact" given unavoidable cross-OS font-rasterization differences (FreeType/Linux vs Core Text/macOS vs DirectWrite/Windows); §12 elaborates on what pixel comparison is and is not for in this project. Screenshots are added to a heavier CI role only if a specific bug is found that DOM assertions (§3.13) provably missed — the burden of proof is on adding the check, not on skipping it.

## 4. Conservation-law tests (I-1 .. I-7)

D13.4's conservation law is the cheapest, most load-bearing safety property in the pipeline, and every stage's test suite — not a separate "conservation test file" — is required to exercise it, because the law is defined per-stage (`kind(s) ∈ {Conserving, Budgeted}`, `reasons(s)`, `budget(s)`) and only a stage's own tests know what its legitimate `Removed`/`Added` entries should look like.

Concretely, every stage-level fixture test (§3.2) asserts:

- **I-1 (conservation):** `C(D_i) ⊎ Added_s == C(D_{i+1}) ⊎ Removed_s`, checked after the stage runs, in debug and CI builds always (release builds keep counts only, per performance — the histogram is sub-millisecond per D13.4's own cost note).
- **I-2 (declared reasons):** every ledger entry's `reason` is in the stage's declared `reasons(s)` set — a stage that produces an undeclared reason is a bug in the stage's own contract, caught immediately.
- **I-3 (conserving stages produce empty ledgers):** for every stage `IR_SKETCH.md` declares `Conserving` (`layout`, `structure`, `document`, `epub`, and — with no exception — every LLM edit), a fixture asserting `Removed_s = Added_s = ∅` is mandatory. This is where the LLM-edit contradiction the red team flagged (Gate L allowing `running_head` labels, which are text-deleting, while claiming LLM edits conserve) is enforced as a **test**, not just a design note: the LLM may *propose* a `running_head` label; only the deterministic furniture remover (a `Budgeted` stage) may act on it and delete the block, and only when its own cross-page repetition evidence independently agrees (D13.5's "label authority ≠ deletion authority"). A test that feeds the LLM adapter a `running_head` proposal and then asserts the *LLM-edit stage itself* left the ledger empty is how this separation is verified, not merely documented.
- **I-4 (budgets):** for each `Reason`, cumulative removed/added characters stay within `budget(s)(r) · |C_0|` (D13.4's provisional table — `RunningHeader+RunningFooter+PageNumber ≤ 0.04`, `OverdrawDedup ≤ 0.02`, `OcrLayerDuplicate ≤ 0.60` per page, `Dehyphenate ≤ 0.005`, `DecorativeGlyph ≤ 0.002`, all others `≤ 0.001`, global non-OCR removal `≤ 0.08`); a corpus-level nightly test tracks the actual observed fraction per reason per stratum (D18) so a budget that is chronically near its ceiling on real books — not just `ours(*)` — is visible before it is silently exceeded.
- **I-5 (dehyphenation is the only in-word edit):** a dedicated fixture family (hyphen-at-line-end cases across German compound words, English common words, and Turkish agglutinative forms) asserts `Removed` is exactly one scalar from `{U+002D, U+2010}`, `Added` is empty, and the resulting token differs from the two source tokens' concatenation by exactly that character. This is the property Gate L (as originally scoped to LLM edits only) could not see — a *wrong* dehyphenation join conserves the character multiset exactly — so I-5 is tested independently of, and in addition to, I-1.
- **I-6 (OCR is added-only, region-scoped):** a fixture pair — a wholly scanned `image-only` page and a `mixed` page carrying a plate beside body text — asserts `Ocr` ledger entries are `Added`-only, that each entry's region bbox contains **no PDF text run** present before the stage (the whole page on `image-only`, each uncovered image region on `mixed`), and that every such region is marked `provenance = ocr` and correctly excluded from the source-retention denominator. A page-scoped reading of I-6 would reject the legal `mixed`-page case and must fail this fixture.
- **I-7 (end-to-end, release gate):** `C(EPUB) ⊎ Removed_all == C_0 ⊎ Added_all` for the whole document, run in the integration and nightly tiers over corpus files (not feasible to hand-verify at fixture scale for every file, but the histogram computation is cheap enough to run per corpus file every night). A release does not ship with I-7 failing on any corpus file.

What conservation tests explicitly do **not** claim to catch — and this is asserted in the suite's own documentation, not left implicit — is a wrong heading level, a wrong reading-order decision, or a wrong cluster label: I-1 through I-7 verify that text was not silently lost or fabricated, not that the *structure* imposed on surviving text is correct. Those failure modes have their own fixture and golden-decision tests (§3.2, §3.10).

## 5. Determinism tests

Scoped precisely per D13.8 and RED_TEAM §B6, which corrects an over-claim in an earlier draft of the determinism contract: llama.cpp greedy decoding is **not** guaranteed bit-reproducible across thread counts or CPU backends (floating-point summation order changes with how work is partitioned across SIMD lanes/threads, and addition is not associative — R9 §C.3, flagged high-confidence-but-not-independently-verified and treated that way here). The determinism test suite therefore asserts three distinct, narrower claims rather than one broad one:

1. **`--no-ai` byte-identity, same OS:** converting the same input twice with AI disabled produces byte-identical EPUB output (excluding nothing — there is no timestamp field to exclude, since D5 requires a fixed-epoch, deterministic zip). This is the cheapest possible regression catcher for accidental `HashMap`-iteration-order bugs and is a fast-tier assertion on every fixture.
2. **Cross-OS identity job:** a dedicated CI job (nightly, or on release-candidate commits) converts a fixed corpus subset on Windows, macOS, and Linux runners with `--no-ai` and diffs the outputs, using pure-Rust image codecs so image re-encoding does not introduce platform-specific variance. This is the test that actually exercises D13.3's "one normalized coordinate space fixed at extraction" invariant across the FreeType/Core Text/DirectWrite font-metrics differences that would otherwise leak into layout decisions.
3. **AI-path determinism is cache-scoped, not decode-scoped:** AI runs are asserted byte-identical **on a cassette/cache hit** (D13.8's content-addressed cache, keyed `sha256(model_id ‖ prompt_version ‖ grammar_hash ‖ input)`, is the determinism mechanism for the AI path) — never asserted byte-identical on a **cold** AI run. Thread count is pinned for cassette recording specifically so a re-recorded cassette is reproducible on that fixed configuration (§6), but no test anywhere asserts exact-string-match on a live, non-cassette LLM call output.

## 6. LLM tests

LLM-touching tests never make a live model call outside the nightly tier (§2). The mechanism is a **cassette** — VCR-style record/replay (R9 §C.4) adapted with an LLM-specific cache key that mirrors D13.8's cache key exactly: `hash(prompt_version + model_id + grammar_hash + input)`, not merely the raw prompt text, so a prompt-template bump or a model swap cannot silently replay a stale response. Fast and integration tiers run exclusively against cassettes; nightly is the only tier that (re-)records them, and a re-record is a reviewed diff in the PR that introduces it, exactly like an `insta` snapshot review (§3.3) — "the model's behavior for this exact prompt changed" is meaningful information, not noise to auto-accept. Thread count is pinned for every recording session (§5) so a recorded cassette is at least reproducible on the configuration that recorded it.

Cassettes are **content-addressed by that same key**: the file is `crates/oc-ai/tests/cassettes/<task>/<key-hex>.json`, and a per-task `index.json` maps the readable name `<task>__<fixture>__<prompt_version>` to the key, so a reviewer can find a cassette by fixture while a test still requests it by the one keying rule the production cache uses. Lookup is exact — a miss is a test failure naming the expected key, never a fuzzy match or a silent fall-through to a live call.

**Semantic assertions**, checked on every cassette-replayed response regardless of tier, decoupled from whether the *content* is good (that is §3.10's concern):

- **id bijection:** returned block/cluster IDs are exactly the input set — no invented IDs, none dropped.
- **enum membership:** every label is drawn from the schema's closed enum; a value outside it is a schema failure (§3.9), not a semantic one, but is asserted at both layers as defense-in-depth against a schema-bypass bug.
- **monotonic levels:** heading levels in a returned sequence do not skip (no H1 directly to H4 without an intervening H2/H3).
- **transitions strictly increasing:** for D13.6's boundary/run-length structure-role output (`frontmatter_end_idx`, `part_boundaries[]`, `backmatter_start_idx` — the format RED_TEAM §A8 substituted for a per-heading object precisely to make this check trivial and to cut output tokens on pathological large-heading-count books), the returned indices must be strictly increasing and within the input range.

**False-repair rate** is tracked as the project's key LLM-safety metric, per category (mirroring olmOCR-bench's category breakdown), never only in aggregate — a net-positive accuracy change can hide a severe per-category harm (R9 §C.8). It is measured as the McNemar 2×2 discordant-pair rate: run the deterministic-only pipeline and the deterministic+LLM pipeline over the same golden-set/corpus, tabulate (both pass / both fail / deterministic-only-pass / LLM-pipeline-pass), and apply McNemar's test — the standard χ² = (b−c)²/(b+c) form when `b+c ≥ 25`, the **exact binomial** form otherwise (R9 §C.7, explicitly flagging the chi-squared approximation's validity threshold, which matters at OpenConvert's realistic golden-set sizes). **Bootstrap confidence intervals** (resample the corpus with replacement, 1,000–10,000 resamples, recompute the metric per resample, report the empirical 95% interval) accompany every reported accuracy delta, because a point-estimate change at a ~20-file integration-tier corpus size is not by itself evidence of anything (R9 §C.7). Any category whose false-repair rate exceeds its ceiling (§7's risk-coverage-derived target, `≤ 1%` per category as an initial provisional target — RED_TEAM §C4) disqualifies that category's LLM path regardless of a net-positive aggregate McNemar result.

## 7. Confidence calibration procedure

D17 ships v1 with `ai.enabled = false` and structural escalation predicates rather than calibrated scores (RED_TEAM §A7, §C4 — no gold set exists yet, and asserting calibration against one that does not exist would itself be an invented number). The escalation predicates (metadata absent/matches boilerplate regex; outline absent and TOC-page parse `< 3` entries; `> 1` heading cluster with numbering coverage `< 100%`; dehyphenation joined form unattested → keep hyphen; verse/quote indent-and-short-line-ratio band) are themselves unit-testable on day one, with no calibration dependency — and, as a deliberate side effect, every escalation an early user's conversion triggers is a **logged, labeled hard case**. The first several hundred books converted with AI enabled by users who opt in *are* the calibration corpus's bootstrap.

The promotion path from `provisional` to `calibrated` (D17, `eval calibrate`) follows the selective-prediction / risk-coverage method (R9 §C.9, §C.10; Geifman & El-Yaniv's risk-coverage framing; Guo et al. 2017 and Nixon et al. 2019 on why raw confidence scores — heuristic or neural — are not trustworthy probabilities without validation):

1. Collect `(confidence-signal, was-the-decision-actually-correct)` pairs on the labeled golden set (§3.10), stratified per D18's `/Producer` buckets and never fit against `ours(*)`-only data (RED_TEAM §A9 — a threshold fit on synthetic-renderer output overfits to WeasyPrint/Typst's clean CMaps and stable geometry, not to real InDesign/scanner output).
2. Bin by confidence decile using **adaptive binning**, not fixed-width bins — per Nixon et al.'s critique that naive ECE with fixed bins is sensitive to binning-strategy choice in ways that can hide miscalibration.
3. Plot the **reliability diagram**: empirical accuracy vs. mean predicted confidence per bin; a perfectly calibrated signal traces the diagonal.
4. Where the curve deviates materially, apply **isotonic regression** to remap the raw signal onto empirically accurate probabilities (a standard, assumption-light monotonic-calibration technique — no claim is made here that a specific citation for isotonic calibration was independently verified this round; treat the technique choice as standard ML practice, not a load-bearing citation).
5. Set the escalate-to-LLM threshold from a **false-repair-rate risk target** (`≤ 1%` per category, provisional — §6), not from an arbitrary confidence cutoff: fix the acceptable risk, let coverage (how often the LLM is allowed to act) fall out as the dependent variable, per Geifman & El-Yaniv's selective-classification framing.

**Promotion requires a reviewed commit containing the reliability diagram, the sample size `n`, and the CI-recorded before/after false-repair rate** — D17's explicit gate, restated here as a test-suite artifact: `oc-eval calibrate`'s output is not a number pasted into a config file, it is a diagram and a dataset committed alongside the `thresholds.toml` change, reviewable the same way an `insta` snapshot diff is reviewable (§3.3).

**Minimum golden-set size (`calibration.min_gold_instances_per_task = 200`, source = provisional).** A `provisional → calibrated` promotion additionally requires **n ≥ 200 labelled instances for the task being promoted**. The rationale is the width of the interval the gate is made of: at the accuracy range these decisions actually sit in (p ≈ 0.9), a 95 % CI half-width is ≈ ±4 pp at n = 200 (`1.96·√(0.9·0.1/200) ≈ 0.0416`) — tight enough that "baseline − 1 margin of error" still gates on something, where at n = 50 the same half-width is ≈ ±8 pp and the gate passes almost anything. The number is deliberately `provisional`: it is derived from the target interval width, not measured, and should be revisited once the first golden sets exist and their true per-task accuracies are known. Below the minimum the task stays `provisional` no matter how good the point estimate looks; per-stratum promotion (D18) needs the minimum met *within* the stratum, not only in aggregate.

## 8. Benchmark framework

**There is no single headline quality number in v1.** The nightly dashboard *is* the disaggregated per-stratum table of §8.1–§8.4 — a rolled-up score across strata would average away exactly the `ours(*)`-vs-real gap D18 makes a first-class metric, and would let a synthetic-corpus win mask a real-book regression. The one exception is external comparability: the **OmniDocBench composite** (`((1 − text_NED)×100 + table_TEDS + formula_CDM)/3`, R9 §A.16) is computed **only on the academic stratum**, where the benchmark's own document population makes the comparison meaningful, and is reported as one stratum's row rather than as the project's score.

Run nightly (§2), emitting one structured record per `(corpus file, pipeline version, metric)` tuple — never only a rolled-up score — modeled on docling-eval's "materialize predictions once, evaluate many metrics against a canonical representation" pattern (R9 §A.2, §D.5), which validates exactly the canonical-JSON-IR-as-evaluation-substrate approach D13.3 already commits to for other reasons. History is stored in-repo (or an adjacent version-controlled artifact store), per `criterion`'s own trend-tracking philosophy (R9 §D.5), so a regression is "diff against the immediately-prior committed baseline," the same discipline as snapshot review.

### 8.1 Conversion quality metrics

| Metric | Method | Notes |
|---|---|---|
| Text extraction | 1 − Normalized Edit Distance, computed **after** `N` normalization (D13.4) — not Nougat's raw metric, which does no Unicode/hyphenation normalization at the metric layer (R9 §A.10's gap) | Character-level Levenshtein / max(len(pred), len(gt)); OmniDocBench's own formula family (R9 §A.1, §A.16) |
| Reading order | Normalized sequence edit distance over block IDs, plus Kendall-tau distance as a secondary signal | Edit distance mirrors OmniDocBench's own reported metric (R9 §A.11); Kendall-tau isolates pure-ordering bugs from content-loss bugs |
| Heading structure | Precision/recall/F1 on (heading text, level) pairs, plus a TEDS-style tree-edit-distance score over the full heading/nav tree | Self-defined ("TOC-F1"); no external published baseline exists for this exact task (R9 §A.12), so this is tracked as a trend from day one, self-calibrated against fixtures, not borrowed from literature |
| Chapter accuracy | Precision/recall on chapter-boundary detection (`frontmatter_end_idx`/`part_boundaries`/`backmatter_start_idx` vs. ground truth) | Direct evaluation of D13.6's boundary/run-length structure-call output |
| Image preservation/association | Count precision/recall (right number of images kept) + caption-to-image association accuracy | Catches the single most common real bug class first: images silently dropped (R7 §B.5) |
| Footnote link F1 | Precision/recall on (marker location, footnote-id) pairs | Self-defined, IE-style scoring (R9 §A.13); no external baseline — derivable exactly from Standard-Ebooks ground-truth pairs where the source markup makes the noteref↔note relationship explicit |
| Table structure | TEDS and TEDS-S (structure-only variant, cell-substitution cost held constant) | TEDS = 1 − tree-edit-distance / max(nodes) (PubTabNet formula, R9 §A.14); TEDS-S isolates "wrong grid" from "wrong cell text," and gates first since a wrong grid is the worse failure |
| Metadata field accuracy | Field-level precision/recall on title/author/language/identifier, split from instance-level "did we find metadata at all" | Mirrors Grobid's instance-vs-field split (R9 §A.8) — different failure modes need different remediation (deterministic rule vs. LLM repair) |

### 8.2 Technical metrics

EPUBCheck pass rate (target: 100% on the corpus, per D6 — this is the one metric in this table that is a binary gate, not a trend), broken-resource rate (dangling manifest reference, missing image), malformed-XHTML rate (well-formedness failures caught even before EPUBCheck runs), and **repair-fire rate, target zero** — per D13.7 and RED_TEAM §A10's reframing: every repair that fires on the corpus is logged as a bug against the *emitter*, not treated as evidence the repair loop is doing its job. A repair-fire rate trending upward is itself a release-blocking signal distinct from "did EPUBCheck pass" — a repair that successfully fixes an error before shipping still represents an emitter defect that produced the error in the first place.

### 8.3 Performance metrics

Time per page and peak RSS (**whole process tree**, not just the top-level process — Windows via Job Objects' aggregate accounting, since the pipeline plus a spawned `llama-server`/`tesseract` need to be measured as one figure; `/usr/bin/time -v`'s `%M` field or direct `/proc/[pid]/status` `VmHWM` polling on Linux/macOS — R9 §D.5), CPU utilization, LLM latency and its share of total wall-clock (the hard-stop budget is `≤ 25%`, D13.6/D17), model download size, and output EPUB size (warn threshold `> 50 MB`, D13.11). The reference-machine performance budget itself — `≤ 0.5 s/page` and `≤ 500 MB` peak RSS for a 300-page born-digital book on reference machine L — is `source = provisional` in `thresholds.toml` (§9), anchored loosely against Docling's own published 0.41–1.06 s/page range (R9 §A.2) as a "we should not be slower than a heavier, non-Rust deterministic pipeline" sanity check, not a literature-derived target.

### 8.4 AI effectiveness metrics

The corpus is run **paired**, once deterministic-only and once with each enabled model tier (`+Qwen3-1.7B` default, `+Qwen3-4B` quality tier, and the `experimental`-tier `Qwen3.5-2B`/`4B` once it exists as an installable option — D9), on the **same files**, so every quality-metric delta in §8.1 has a matched deterministic-only baseline to compare against via the McNemar/bootstrap machinery of §6. Per D18, **the `ours(*)` (synthetic Typst/WeasyPrint) vs. real-strata score gap is reported as a first-class metric on every nightly run**, not folded into an aggregate — a widening gap is the earliest available signal that a heuristic or an LLM-escalation threshold has overfit to the clean, tagged, stable-geometry synthetic corpus rather than to real `/Producer`-stratified books (RED_TEAM §A9), and `ours(*)` may never exceed 40% of the corpus nor may a release pass on `ours(*)` strata alone.

## 9. Thresholds policy

Every numeric constant in the pipeline lives in one file, `thresholds.toml`, and every entry carries `{value, source: binary | published | provisional | calibrated, evidence, owner, review_by}` (D17). This is not a documentation nicety — CI fails the build on a `provisional` entry with no owner or an expired `review_by`, which is the mechanism (not merely the assertion) that keeps "no threshold is invented" true going forward, correcting an earlier draft's claim that was not actually true of D9/D12/D13's numbers as first written (RED_TEAM's Appendix, item 1).

Every threshold's `source` column is one of:

| `source` | Meaning | Examples |
|---|---|---|
| `binary` | A hard architectural limit with no tunable value, e.g. an enum's closed set, or a spec requirement (EPUBCheck = 0 errors) | EPUBCheck error count gate; Ace serious-violation count |
| `published` | Anchored to an independently published number this project did not measure itself | `xhtml.split_bytes = 260_000` (Calibre/ADE's own empirically-tested split threshold, R5 §A13) |
| `provisional` | A reasoned but unmeasured default, explicitly flagged, with an owner and a review date | `llm.max_calls_per_book = 8`, `repair.max_iterations = 3`, `inventory.max_clusters = 24`, the performance budget in §8.3, the D13.4 per-`Reason` budget table |
| `calibrated` | Fit against the golden set/corpus by the procedure in §7, with a committed reliability diagram | Escalation confidence thresholds, once `ai.enabled` moves past v1's structural-predicate stage |

A representative slice of the initial gate table (full table lives in `thresholds.toml` itself; this is illustrative, not exhaustive):

| Threshold | Value | Source |
|---|---|---|
| `ai.enabled` | `false` | binary (v1 default, D17) |
| EPUBCheck errors on corpus | `0` | binary |
| Ace serious violations | `0` | binary |
| `xhtml.split_bytes` | `260,000` | published (Calibre/ADE) |
| `llm.max_calls_per_book` | `8` | provisional |
| `llm.max_wallclock_share` | `0.25` | provisional (hard stop) |
| `repair.max_iterations` | `3` | provisional |
| `false_repair_rate_target` | `≤ 1%` per category | provisional |
| `ours(*)` corpus share cap | `≤ 40%` | provisional (D18) |
| Conservation budgets (per `Reason`) | see D13.4 table | provisional |
| Golden-decision accuracy gate | baseline − 1 margin of error | calibrated, per-task |

## 10. CI matrix and jobs

Three OSes (Windows, macOS, Linux) × the fast and integration tiers run on every PR; these tiers are constructed to be OS-*insensitive* (canonical-JSON snapshots, no pixel comparison — R9 §B.10), so a genuine per-OS divergence in this tier is itself a bug report, not an expected variance to suppress. WebKit-engine DOM checks (§3.13) run nightly rather than per-PR, alongside Chromium, which runs on every PR. The cross-OS determinism job (§5) runs nightly and on release-candidate commits. Mutation testing, high-iteration property tests, the full corpus benchmark, and screenshot regeneration (§3.14, release-time human-reviewed) all run nightly or at release time, never per-PR. The `unshare -n` network-isolation assertion (D13.9 — the core conversion suite must succeed with zero attempted connections) runs on every PR as a separate, fast job, since it is cheap and structurally load-bearing to the project's local-first claim.

## 11. Test naming and fixture conventions

Fixture files live under `corpus/` per D14, referenced by manifest entry (`(id, source, license, sha256, category, expected_problems, ground_truth_ref)` — R7 §D.1's schema), never committed as large binaries directly; a download script fetches and verifies against the recorded SHA-256. Test function names encode the stage and the property under test, not the input file (`stage_furniture__running_header_removed_by_cross_page_repetition`, not `test_book_3`), so a failing test name alone tells a reader what invariant broke. Snapshot files (`.snap`, `.xhtml`, `.opf`) are named after the fixture id, not a counter, so a diff review does not require cross-referencing a separate index. Every corpus-derived test entry is additionally keyed by `(file, sha256)` per §3.7, independent of the human-readable test name.

## 12. What NOT to test with pixels

Per D7's own rationale, restated as a rule the test suite follows rather than a preference: pixel comparison between a PDF page render and an EPUB page render is **not a meaningful test**, because reflow is the entire point of EPUB — a correct conversion is expected to look different from the source PDF, and a pixel diff cannot distinguish "correct reflow" from "content corruption." Pixel comparison is also not the tool for cross-OS/cross-engine EPUB-vs-EPUB regression detection at the fast or integration tier, because font-rasterization variance across FreeType/Core Text/DirectWrite is real, unavoidable, and orthogonal to whether the conversion logic is correct (R9 §D.4, §B.11). Structural DOM assertions (§3.13) are the default for anything that is really a layout/content-correctness question — element presence, computed overflow, heading order, footnote markup — reserving genuine pixel-level regression (§3.14) for the narrow set of checks that are actually about visual fidelity (does an embedded image render without distortion) where no structural proxy exists, and even there, only at release time, on one OS, reviewed by a human, never as an automated PR gate.

---

## Notes for the Chief Architect

Both points raised here have since been ratified and are folded into the body of this document; they are kept as a record of what changed.

1. **The `n` for golden-decision-set margin-of-error reporting is now specified** (§7): `calibration.min_gold_instances_per_task = 200`, `source = provisional`, on the rationale that a 95 % CI half-width is ≈ ±4 pp at p ≈ 0.9 and n = 200 — tight enough for D17/RED_TEAM §C4's "baseline − 1 margin of error" gate to gate on something.
2. **There is no single headline quality number in v1** (§8). The nightly dashboard *is* the disaggregated per-stratum table; the OmniDocBench composite is computed on the academic stratum only, for external comparability.
