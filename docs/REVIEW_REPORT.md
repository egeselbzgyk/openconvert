# OpenConvert — Documentation Consistency Review

**Date:** 2026-09-09
**Scope:** All thirteen documents in `docs/`. Applied the Chief Architect's ratified resolutions R-1 … R-19, ran a cross-document consistency audit, and spot-checked twelve numeric claims against `research/round1/` and `research/round2/`.
**Method:** Surgical edits only — no document was rewritten, and each document's voice was preserved. No substantive decision was changed beyond the ratified list. Where a genuine contradiction could not be resolved from `DECISIONS.md`, it is recorded in §2 rather than guessed.

---

## 1. Edit log

### 1.1 R-1 — I-6 is region-scoped

| File | Location | Before → After |
|---|---|---|
| `DECISIONS.md` | D13.4, invariant list | "I-6 OCR is Added-only on pages with no text, marked `provenance = ocr`, excluded from source retention" → "I-6 OCR is Added-only and **region-scoped**: an `Ocr` ledger entry is permitted on any page region whose bbox contains no PDF text runs — the whole page on an `image-only` page, each uncovered image region on a `mixed` page — and every such region is marked `provenance = ocr` and excluded from source retention" |
| `IR_SKETCH.md` | after the stage-kinds line | *(new paragraph)* → the region-scoped I-6 statement, so the IR sketch carries it rather than only the ADR |
| `IR_SKETCH.md` | `Reason` enum comment block | *(none)* → `// Ocr = OCR-inserted text; region-scoped by I-6 (see below).` |
| `TEST_STRATEGY.md` | §4, I-6 bullet | "occur only on pages with `C(text) = ∅` before the stage" → a two-fixture assertion (`image-only` page **and** a `mixed` page with a plate beside body text) requiring each entry's *region* bbox to contain no pre-existing run, plus an explicit note that a page-scoped reading must fail the fixture |

`ARCHITECTURE.md` §5.4, `PIPELINE.md` §0/§3 and `IMPLEMENTATION_PLAN.md` Phase 13 already carried the region scope; no change needed.

### 1.2 R-2 — `Ocr` is owned by `ingest`

| File | Location | Before → After |
|---|---|---|
| `IR_SKETCH.md` | stage-kinds line | `ingest` Budgeted{GeneratedSpace, ClippedOffPage, OverdrawDedup, OcrLayerDuplicate} → `ingest` Budgeted{GeneratedSpace, ClippedOffPage, **HiddenText**, OverdrawDedup, OcrLayerDuplicate, **Ocr**} + "`Ocr` is owned by `ingest`, which runs OCR after page classification whenever an engine is available" |

### 1.3 R-3 — `HiddenText` as the 15th `Reason` variant

| File | Location | Before → After |
|---|---|---|
| `DECISIONS.md` | D13.4, `Reason` enum | `… Watermark, ClippedOffPage, UserOverride` → `… Watermark, ClippedOffPage, HiddenText, UserOverride` + a parenthetical fixing the split: `ClippedOffPage` geometric, `HiddenText` rendered-but-not-visible, both owned by `ingest`, "fifteen variants" |
| `IR_SKETCH.md` | `Reason` enum | 14 variants → 15, `HiddenText` inserted after `ClippedOffPage`, with a four-line comment giving the definitions |
| `IMPLEMENTATION_PLAN.md` | Phase 1 detail 2 | reason set "`GeneratedSpace, ClippedOffPage, OverdrawDedup, OcrLayerDuplicate` per IR_SKETCH" → the full six-reason `ingest` set, plus the concrete `HiddenText` filter rule (render-mode-3 on a non-`OcrSandwich` page, or fill within delta-E of local background) and the note that `ClippedOffPage` is geometric only |
| `IMPLEMENTATION_PLAN.md` | `thresholds.toml`, `[conservation.budget.other]` | comment "every Reason not named above" → same, now enumerating `ClippedOffPage, HiddenText, Watermark, GeneratedSpace, SoftHyphen, LigatureExpand, UserOverride` so it is visible that `HiddenText` is budgeted here |

`ARCHITECTURE.md` §5.3 and `PIPELINE.md` §0.2/§3 already listed 15 variants. `SECURITY.md` contains no enum listing.

### 1.4 R-4 — LLM budget and batch size

| File | Location | Before → After |
|---|---|---|
| `DECISIONS.md` | D13.6, task 4 | "(≤ 30 blocks/book, batched)" → "(≤ 30 blocks/book, batched at **exactly 10 blocks per call**, so task 4 costs at most 3 calls and leaves 5 for tasks 1–3 including `book_structure` chunking)" |
| `DECISIONS.md` | D13.6, budgets sentence | "≤ 8 calls/book, ≤ 1,500 output tokens/call, LLM wall-clock share ≤ 25 % (hard stop)" → adds "**total** (one budget, no per-task sub-budget)", "of **total** wall-clock including the LLM", and the degradation order `verse_quote` → extra `book_structure` chunks → `heading_roles`, never `metadata` |
| `ARCHITECTURE.md` | §9.6, Task 4 pre-gate | "Batched 5–10 blocks per call." → "Batched at exactly **10 blocks per call** (ratified note N-4), so the 30-block cap costs at most 3 of the 8 calls." |
| `LLM_EVALUATION.md` | §1 task table, row 4 | "batched 5–10/call" → "batched at exactly 10/call" |
| `LLM_EVALUATION.md` | §5 cost table | "~50–300/block × 5–10/call" → "× exactly 10/call" |
| `LLM_EVALUATION.md` | §8.1 worked example | "Input (batched, one of 5–10 in a call)" → "one of exactly 10 in a call" |
| `LLM_EVALUATION.md` | §1 and §5 budget lines | "≤25% (hard stop)" → "≤25% of *total* wall-clock including the LLM (hard stop)", plus the degradation order in §1 |

The surviving "5–10" in `ARCHITECTURE.md` note N-4 is the historical statement of the problem and is correct as a record.

### 1.5 R-5 — G4 at 50 s

| File | Location | Before → After |
|---|---|---|
| `DECISIONS.md` | D9 promotion gate | "G4 full per-book call set ≤ 90 s on L for a 300-page book and LLM share of wall-clock ≤ 25 %" → "**≤ 50 s on L for the reference 300-page book AND LLM share ≤ 25 % of total wall-clock including the LLM** (both conditions, not alternatives: at 0.5 s/page a 300-page book is ~150 s deterministic, so 25 % of the total permits ≤ 50 s of LLM)" |
| `LLM_EVALUATION.md` | §6.2, G4 row | "≤ **90 s on L**" → "≤ **50 s on L**", both-conditions wording, and the derivation |

`IMPLEMENTATION_PLAN.md` already used 50 s in Phase 9, `thresholds.toml` and A9.5.

**Incidental fix (audit):** `IMPLEMENTATION_PLAN.md` `[model_gate.g4_max_seconds_on_L]` had an unterminated string — `evidence = "…300-page book` with no closing quote, which would have made `thresholds.toml` unparseable. Closing quote added.

### 1.6 R-6 / R-15 — presets are key-by-key override maps

| File | Location | Before → After |
|---|---|---|
| `DECISIONS.md` | D13.11 opening | "precedence `CLI > job-spec > user config > preset > defaults`, presets `auto\|novel\|…`" → adds "applied **key by key**", the definition of a preset as a partial override map over `thresholds.toml` keys by name with per-key provenance preserved, the requirement that overlays be declared inside `thresholds.toml`, and one clause adding the UI **quality presets** `fast\|balanced\|thorough` as the same mechanism |
| `UI_UX.md` | §2.4, "Quality vs. speed preset" | "a separate `fast`/`balanced`/`thorough` axis maps to the actual numeric thresholds" → both axes stated as key-by-key partial override maps declared inside `thresholds.toml`, resolved at the `preset` layer, with provenance intact |

### 1.7 R-7 — prompt/grammar file layout

| File | Location | Before → After |
|---|---|---|
| `ARCHITECTURE.md` | §9.2 ¶1 | "committed under `crates/oc-ai/grammars/`" → the single-directory layout `crates/oc-ai/prompts/<task>/v<N>/{system.md,user.tmpl,grammar.gbnf,schema.json}`, `src/prompt/v<N>/` modules as `include_str!` wrappers, and `grammar_hash` = SHA-256 of `grammar.gbnf` |
| `ARCHITECTURE.md` | §9.2 ¶3 | "Prompts are versioned files (`prompts/<task>/v<N>.md`)" → "versioned by directory (`prompts/<task>/v<N>/`)" |
| `ARCHITECTURE.md` | §10 repo tree, `oc-ai/` | `prompts/<task>/v1.md` + `grammars/<task>.gbnf` + `cassettes/` → `prompts/<task>/v1/` (four files) + `tests/cassettes/<task>/<key-hex>.json` + `index.json` |
| `IMPLEMENTATION_PLAN.md` | Phase 8 file list | `grammar/{mod.rs,*.gbnf}` inside `src/`, prompts as `.rs` → prompt artifacts moved to `crates/oc-ai/prompts/<task>/v1/{…}`, `src/prompt/v1/*.rs` stated to contain no prompt or grammar text, cassette path made content-addressed |
| `IMPLEMENTATION_PLAN.md` | §0.6 naming table | *(no row)* → new "Prompt artifacts" row |

### 1.8 R-8 — cassette naming

| File | Location | Before → After |
|---|---|---|
| `IMPLEMENTATION_PLAN.md` | §0.6, Cassette row | one row `<task>__<fixture>__<prompt_version>.json` → two rows: the content-addressed file `crates/oc-ai/tests/cassettes/<task>/<key-hex>.json`, and `index.json` mapping the readable name to the key |
| `TEST_STRATEGY.md` | §6 | *(no path stated)* → new paragraph giving the content-addressed path, the `index.json` mapping, and the exact-lookup rule (a miss is a failure naming the expected key, never a fuzzy match or a live call) |

### 1.9 R-9 — model-gate results

| File | Location | Before → After |
|---|---|---|
| `DECISIONS.md` | D9, end of promotion-gate paragraph | *(nothing)* → "**Results:** the machine-readable run files under `eval/results/model_gate/<model>__<build>__<machine>.json` … are the **source of truth**; `docs/MODEL_GATE.md` is the human-readable table *generated* from them (`--render-table`) and is never hand-edited." |
| `LLM_EVALUATION.md` | §6.2, after the gate table | *(nothing)* → "Where gate results live" paragraph stating the same, and extending the rule to §7's benchmark numbers so prose and artifact cannot drift |

### 1.10 R-10 — Qwen3 language facts (verified 2026-09-09)

| File | Location | Before → After |
|---|---|---|
| `DECISIONS.md` | D9 "Why" | "119 languages (model card names Turkish and German)" → "**119 languages and dialects per the Qwen3 release blog, which lists Turkish and German explicitly** (the model card states \"100+ languages\" and explicitly names llama.cpp among supported runtimes) — verified 2026-09-09"; also adds `enable_thinking=False` alongside `/no_think` and states Apache-2.0 |
| `LLM_EVALUATION.md` | §2 model table, Qwen3 row | "119 languages per D9 … **not independently re-confirmed by V1**" → the verified statement with the blog/card split |
| `LLM_EVALUATION.md` | §6.1 bullet | "the dense Qwen3 fallback claims 119 languages … (not independently re-checked by V1)" → the safer model now has the *better*-evidenced claim, which strengthens rather than neutralises the argument |
| `LLM_EVALUATION.md` | Notes item 1 | the "has not been independently re-verified" flag → the verified statement (blog: 119 + TR/DE named; card: "100+", apache-2.0, `enable_thinking=False` / `/no_think`, llama.cpp named) |
| `LLM_EVALUATION.md` | §10.1 risk | "The 119-language and 201-language claims … are both aggregate, unverified-per-language" → Qwen3's is now verified per-language, Qwen3.5's is still aggregate-only; the residual risk restated as "a *named* language is still not a *measured* one" |
| `TECHNOLOGY_EVALUATION.md` | §13 stack table, Default model row | "119 languages incl. DE/TR named" → "119 languages per the Qwen3 release blog with DE/TR listed explicitly (model card: \"100+\", and it names llama.cpp)" |

### 1.11 R-11 — thinking control across providers

| File | Location | Before → After |
|---|---|---|
| `ARCHITECTURE.md` | §9.1 | the wire-format sentence carried `chat_template_kwargs` inline as if universal → split: wire format keeps `messages`/`temperature: 0`/`response_format`; a new **Thinking control** paragraph gives `LlmProvider::thinking_control()`, the three per-provider levers, the note that `/no_think` goes on the *shared system prefix* so prefix reuse survives, and the universal gate-S fallback |
| `LLM_EVALUATION.md` | §9 | "**An open design fork, not yet resolved (see §10.2).**" → "**Thinking control (resolved).**" with a three-row provider table and the gate-S/deterministic-fallback rule |
| `LLM_EVALUATION.md` | Notes item 2 | the open-fork flag → the resolved statement |
| `DECISIONS.md` | D10 | *(nothing)* → one sentence giving `thinking_control()`, the three levers, and the universal gate-S fallback |

*Incidental:* §9's cross-reference "see §10.2" pointed at §10's calibration item, not the thinking fork — a dangling reference, removed with the paragraph it sat in.

### 1.12 R-12 — golden-decision promotion needs n ≥ 200

| File | Location | Before → After |
|---|---|---|
| `TEST_STRATEGY.md` | §7, after the promotion-artifact paragraph | *(nothing)* → "**Minimum golden-set size**" paragraph: `calibration.min_gold_instances_per_task = 200`, `source = provisional`, with the arithmetic (`1.96·√(0.9·0.1/200) ≈ 0.0416`, i.e. ±4 pp at p ≈ 0.9, against ±8 pp at n = 50), the "below the minimum the task stays provisional" rule, and per-stratum applicability |
| `IMPLEMENTATION_PLAN.md` | `thresholds.toml` | *(no entry)* → new `[calibration.min_gold_instances_per_task]` block, `value = 200`, `source = "provisional"`, rationale in the comment, owner `maintainer` |
| `IMPLEMENTATION_PLAN.md` | Appendix D (Definition of Done), AI section | *(nothing)* → checklist item forbidding promotion below the minimum, counted within the stratum |

### 1.13 R-13 — no single headline quality number

| File | Location | Before → After |
|---|---|---|
| `TEST_STRATEGY.md` | §8 opening | *(began with the nightly-record paragraph)* → new opening paragraph: no headline number in v1; the dashboard *is* the disaggregated per-stratum table; the OmniDocBench composite is computed on the academic stratum only, for external comparability |
| `TEST_STRATEGY.md` | Notes item 2 | the open "is a single rolled-up number also wanted?" question → removed, restated as the ratified decision |

### 1.14 R-14 — holdout unit and sourcing target

| File | Location | Before → After |
|---|---|---|
| `TEST_CORPUS.md` | §1 Principle 4 | *(unit unspecified)* → "**The holdout unit is the document**"; DocLayNet pages count only inside the page-level layout stratum, reported separately, and cannot substitute |
| `TEST_CORPUS.md` | §7 | *(no such section)* → new **§7.6 Holdout unit and sourcing target**: the unit rule plus the Phase-7 sourcing table (OAPEN ~40, IA PD scans ~20, arXiv ~15, US-Gov/EU ~15, DergiPark + DTA-derived ~10), and a note that the DE/TR slice is not optional filler |
| `TEST_CORPUS.md` | Notes item 1 | the open "treat DocLayNet pages as units, or expand sourcing?" question → resolved: the per-page shortcut is rejected, the gap is closed by sourcing |
| `IMPLEMENTATION_PLAN.md` | Phase 7 detail 3 | "≥ 100 real-world files" → "≥ 100 real-world **documents**", the DocLayNet exclusion, and the sourcing target inline |
| `IMPLEMENTATION_PLAN.md` | Phase 7 tests | `7.3 holdout_has_at_least_100_files` → `7.3 holdout_has_at_least_100_documents` (with the exclusion in the assertion) + new `7.3b doclaynet_pages_do_not_count_toward_holdout` |
| `IMPLEMENTATION_PLAN.md` | Phase 7 acceptance | A7.1 amended to "≥ 100 **documents** (DocLayNet pages excluded)"; new **A7.1b** on the sourcing composition with non-empty DE and TR slices |

### 1.15 R-15 — UI decisions (partial re-run, quality presets, Svelte 5)

| File | Location | Before → After |
|---|---|---|
| `UI_UX.md` | §2.3 | "triggers a **re-run** of the affected stages" → "a **partial re-run** … This is a ratified decision, not an interpretation: the re-run starts from the **cached `structure` output** and re-executes exactly `document` → `epub` → `validate` → `repair` → `report`", with the six upstream stages named as untouched and the reason (a metadata/TOC edit cannot change a glyph) |
| `UI_UX.md` | §2.4 | see R-6 above |
| `UI_UX.md` | Notes | three open questions → "All three items previously open here have been ratified", restated as decisions (partial re-run intended; quality presets are key-by-key overlays; Svelte 5 confirmed) |
| `IMPLEMENTATION_PLAN.md` | Phase 12 detail 8 | "re-conversion applies them" → the exact five-stage partial re-run from the cached `structure` output |
| `IMPLEMENTATION_PLAN.md` | Phase 12 acceptance | *(nothing)* → new **A12.4b** asserting no stage before `structure` executes |

### 1.16 R-16 — security (Landlock in v1, `--isolate-parser` post-v1, GHSA)

| File | Location | Before → After |
|---|---|---|
| `SECURITY.md` | §11 tier table, "Now" row | "Targeted for v1; a few hours to a few days" → "**In v1, Phase 14** (ratified), with a graceful recorded skip below kernel 5.13" |
| `SECURITY.md` | §13 | "a GitHub Security Advisory draft, or a maintainer contact address, whichever the project settles on at launch" → "**GitHub Security Advisories** … decided for launch in preference to a maintainer contact address", with the reason |
| `SECURITY.md` | Notes | "Notes for the Chief Architect" (three open items) → "**Ratified positions**" (three decisions) |
| `DECISIONS.md` | D16 | "PDFium parse-worker per page range (`--isolate-parser`, hardening phase)" → "**`--isolate-parser` stays a post-v1 spike** — time-boxed in Phase 14 with a recorded go/no-go … **Landlock is *not* deferred:** it is in the v1 'now' tier, delivered in Phase 14" |
| `DECISIONS.md` | D13.9 | *(nothing)* → one clause: Landlock makes the no-network claim kernel-enforced on Linux ≥ 5.13 (ABI ≥ 4 also blocks `connect()`), and security reports come through GitHub Security Advisories |

### 1.17 R-17 — Phase 12 signing dry run

| File | Location | Before → After |
|---|---|---|
| `IMPLEMENTATION_PLAN.md` | Phase 12 implementation details | *(12 items)* → new item **13**: the full Phase-15 signing/notarization workflow executed once on a throwaway tag, naming the nested binaries, the discarded artifacts and the `docs/DECISIONS_LOG.md` record |
| `IMPLEMENTATION_PLAN.md` | Phase 12 tests | *(nothing)* → new **12.14 `signing_dry_run_completes_on_a_throwaway_tag`** |
| `IMPLEMENTATION_PLAN.md` | Phase 12 acceptance | *(nothing)* → new **A12.7** (codesign/spctl/stapler on every nested binary + updater manifest verification + tag deleted) |
| `IMPLEMENTATION_PLAN.md` | Appendix F.7 | "One place where I think the plan is optimistic" (an ask) → "now resolved", pointing at item 13 / 12.14 / A12.7 |

### 1.18 R-18 — verification debt as a Phase 0 task list

| File | Location | Before → After |
|---|---|---|
| `IMPLEMENTATION_PLAN.md` | Phase 0, before "Dependencies" | *(no such section)* → new **"Verification debt (must close before the phase that depends on it)"**, owner = maintainer throughout, seven rows: **VD-a** `zip` crate major before pinning (Phase 0) · **VD-b** `hyphenation` pattern licences (Phase 3) · **VD-c** `zspell` licence, only if the dictionary pack is built (post-v1) · **VD-d** PDFium SMask/vector-path/bookmark spike on 10 real PDFs — noted as *already existing* as Phase 1's image spike, this row exists so it is not dropped (Phases 1 and 4) · **VD-e** igerman98 / TR hunspell licences, only for the optional pack (post-v1) · **VD-f** JRE licence per vendor, Temurin (Phase 6) · **VD-g** UB-Mannheim Windows Tesseract path/version (Phase 13). Each closes with a dated `docs/DECISIONS_LOG.md` note; an open row past its blocking phase is a release blocker |
| `IMPLEMENTATION_PLAN.md` | Phase 0 acceptance | *(nothing)* → new **A0.9** (VD-a closed at Phase 0 exit; every other row has an owner, a blocking phase and a log stub) |
| `IMPLEMENTATION_PLAN.md` | Appendix D | *(nothing)* → new "Process" checklist item requiring every VD row closed or explicitly deferred |
| `IMPLEMENTATION_PLAN.md` | Appendix F.6(b) | "Worth a verification pass before v1." → "**Now tracked** as row **VD-g** … blocking Phase 13." |
| `TECHNOLOGY_EVALUATION.md` | §14 closing | "tracked here so they are not silently forgotten between the research phase and Phase 0" → points at the Phase 0 table and names the four additional items carried from `LICENSE_AND_DEPENDENCIES.md` |
| `LICENSE_AND_DEPENDENCIES.md` | Notes preamble | *(unassigned questions)* → notes 1 and 2 mapped to rows VD-f, VD-b, VD-c, VD-e, owner = maintainer |

### 1.19 R-19 — cross-document uniformity sweep

Checked and found **already consistent**, no edit required:

| Item | Finding |
|---|---|
| Base install ≈ 35–45 MB | `DECISIONS.md` D12, `TECHNOLOGY_EVALUATION.md` §13, `LICENSE_AND_DEPENDENCIES.md` §6, `IMPLEMENTATION_PLAN.md` 15.15/A15.6 all agree. `UI_UX.md` and `ARCHITECTURE.md` state no install size, so there is nothing to contradict. No "20–30 MB" claim about the installer exists anywhere — the several "~20–30 MB" hits refer to the `ort` ONNX runtime in the out-of-v1 ML-layout item, which is a different quantity. |
| Linux primary artifact = AppImage | Consistent in `DECISIONS.md` D12, `TECHNOLOGY_EVALUATION.md` §13, `IMPLEMENTATION_PLAN.md` Phase 15 detail 4 + 15.7 + A15.8. |
| Default model = Qwen3-1.7B, Qwen3.5-2B experimental only | Consistent in `DECISIONS.md` D9, `LLM_EVALUATION.md` §6, `models.toml`, `UI_UX.md` §2.4, `TEST_STRATEGY.md` §8.4, `TECHNOLOGY_EVALUATION.md` §13. `models.toml`'s experimental entry is correctly tiered and carries the community-quant warning. |
| llama-server bundled as `externalBin`, never downloaded by the user | Consistent. `IMPLEMENTATION_PLAN.md` Phase 9 detail 1 is explicit ("Bundled, not downloaded"); the only "download" is `xtask fetch-llama-server` at build time, which is correct. |
| OCR = system tesseract first, optional pack later | Consistent in D4, `ARCHITECTURE.md` §12, `LICENSE_AND_DEPENDENCIES.md` §3, `IMPLEMENTATION_PLAN.md` Phase 13. |
| `ai.enabled = false` default | Consistent across nine documents. |
| Exit codes 0/1/2/3/101 | Identical in `DECISIONS.md` D13.2, `ARCHITECTURE.md` §8, `SECURITY.md` §6, `IMPLEMENTATION_PLAN.md` §2.4, `UI_UX.md` §4. |
| Events on stderr, stdout data-only | Identical in D13.2, `ARCHITECTURE.md` §8, `SECURITY.md` §6. |

### 1.20 Consistency-audit fixes (PART 2)

| # | File | Location | Before → After | Finding |
|---|---|---|---|---|
| A-1 | `IMPLEMENTATION_PLAN.md` | `thresholds.toml`, `model_gate.g4_max_seconds_on_L` | `evidence = "…300-page book` *(unterminated)* → closing `"` added | Malformed TOML in the reference snippet |
| A-2 | `IMPLEMENTATION_PLAN.md` | Phase 15 "Not in this phase", detail 3, Appendix E item 11 | "Azure **Trusted** Signing" ×3 → "Azure **Artifact** Signing" | Name disagreed with `DECISIONS.md` D12 and `SECURITY.md` §12; aligned to the ADR. *See §2.3 — the real Microsoft product name is a factual question I could not check offline.* |
| A-3 | `ARCHITECTURE.md` | §10 repo tree, `eval/` and `corpus/` | `eval/corpus_manifest.toml` + `corpus/manifest.toml` symlink + `fetch_corpus.py` + `corpus/holdout/` → `corpus/manifest.json` + `corpus/download.py` + `corpus/fixtures/` + a note that the holdout is the `holdout: true` manifest subset, not a directory. `eval/` retree'd to `src/oc_eval/` + `results/model_gate/` | `IMPLEMENTATION_PLAN.md` §1.8 and `TEST_CORPUS.md` §7.1 agree on JSON at `corpus/manifest.json` with a boolean flag; only `ARCHITECTURE.md` said TOML-with-symlink-and-directory. Two-against-one, and D14 places the manifest and script under `corpus/`. |
| A-4 | `ARCHITECTURE.md` | §10 repo tree, `crates/` | *(missing)* → `oc-testkit/` added; new top-level `xtask/` entry | Both are workspace members throughout the plan but absent from the tree. Additions to D14's Appendix A crate map, not contradictions — flagged in §2.4. |
| A-5 | `ARCHITECTURE.md` | "Notes for the Chief Architect" preamble | "`DECISIONS.md` and `IR_SKETCH.md` remain unedited by me; folding these back into the ADR is the Chief Architect's call." → "…**and now in `DECISIONS.md` (D13.4, D13.6, D13.11, D9) and `IR_SKETCH.md`**. The notes are kept below as the record of what changed and why, not as open items." | Statement became false once R-1…R-6 were folded in |
| A-6 | `IMPLEMENTATION_PLAN.md` | Appendix F.1–F.5 | five open "**Ask:**" items → five "resolved" records naming where each now lives | All five were resolved by R-4, R-5, R-7, R-8, R-9 |
| A-7 | `IMPLEMENTATION_PLAN.md` | Appendix C traceability table | *(ended at N-6)* → seven rows added for R-12 … R-18 | Ratified items had no traceability entry |
| A-8 | `LLM_EVALUATION.md` | §9 | cross-reference "(see §10.2)" pointing at §10's *calibration* item, not the thinking fork | Dangling reference; removed with its paragraph |

Checked and found consistent, no edit needed: the twelve stage names (`inspect` … `report`) in `IR_SKETCH.md`, `PROBLEM_ANALYSIS.md`, `UI_UX.md`, `PIPELINE.md`; crate names `oc-model/-pdf/-text/-layout/-structure/-epub/-validate/-ai/-net/-core`, `openconvert`, `apps/desktop`, `eval/`, `corpus/`; the four LLM task ids (`metadata`, `heading_roles`, `book_structure`, `verse_quote` — hyphenated forms appear only in running prose); budgets (8 calls, 30 blocks, 10/call, 1,500 tokens, 25 %); the six page classes (kebab-case in prose, snake_case in threshold keys, PascalCase in Rust — a consistent convention, not a conflict); phase numbering 0–15 and every phase title against `DECISIONS.md` Appendix B; every pinned version (`pdfium-render 0.9.4`, `lopdf 0.45`, `typst 0.15.1`, `quick-xml 0.42`, `zip 8.6`, `insta 1.48`, `proptest 1.11`, `cargo-deny 0.20.x`, `criterion 0.8`, `whatlang 0.18`, `hyphenation 0.8.4`); llama.cpp tag form `b<N>` (`b10456` pinned, `b5275`→`b5276` as the regression example — the `b0412` hits are block ids in JSON examples, not tags); model filenames (`Qwen3-{params}-Q4_K_M.gguf`); machines L (4c/8t AVX2 x86, 16 GB) and M (base Apple M-series, 16 GB); IPC constants (8 KiB line cap, heartbeat 2 s, progress ≤ 10/s, cancel ≤ 2 s, supervisor 5 s, `--max-pages` 3000, `--max-memory` 4 GiB, 100 MP, 256 MB); pack and output sizes (validation pack 40–50 MB, EPUB warn 50 MB, XHTML split 260 KB, image longest side 1600 px); performance budget (0.5 s/page, 500 MB peak RSS); licence names throughout.

### 1.21 Fact-check corrections (PART 3)

| File | Location | Before → After |
|---|---|---|
| `PROBLEM_ANALYSIS.md` | §3.3 taxonomy, "Heading hierarchy" row | "GROBID's trained CRF reaches only 74.86% F1 on section titles … (R2 §B.5)" → "…only **76.43%** … (R2 §B.5, PMC benchmark, 1,943 PDFs; the companion bioRxiv benchmark of 2,000 PDFs gives **74.86** soft F1 — R10 §3.5)"; DocLayNet citation extended to "(R2 §C.7, R10 §6.7)" |
| `PROBLEM_ANALYSIS.md` | §3.3 taxonomy, "Metadata" row | "title F1 only 77.26% strict (R10 **§5.16**)" → "…77.26% strict / 79.47 soft, authors 82.84, on the 2,000-PDF bioRxiv benchmark (R10 **§3.5**, restated §6.16)" |

---

## 2. Unresolved — needs the Chief Architect

### 2.1 Fixture generator: `oc-fixtures` (Rust, in-process) vs `xtask fixtures` (shelling out to the `typst` CLI)

Three documents describe the same job three ways, and they are mutually exclusive:

- `TEST_CORPUS.md` §6.1 states it as a **decision**: "a Rust dev binary, `oc-fixtures`, using the `typst` and `typst-pdf` crates … generates the Typst-sourced fixtures deterministically and **in-process — no shelling out to a `typst` CLI**", living in `eval/` as a separate Cargo binary. §7.1's manifest schema records `"generator": "oc-fixtures-<version> | …"`, and §8 refers to "regenerated `oc-fixtures` output".
- `IMPLEMENTATION_PLAN.md` Phase 0 uses **`xtask fixtures`** (`xtask/src/fixtures.rs`), and its Dependencies line requires the **`typst 0.15.1` binary** as an external tool — i.e. shelling out, exactly what TEST_CORPUS rules out. It also lists `eval/src/oc_eval/generate/typst.py` as a Phase-0 file, a third generator path.
- `ARCHITECTURE.md` §10's tree (before my edit) put generation in `eval/synth/`, under an `eval/` node labelled "Python."

`DECISIONS.md` D14 settles only that `eval/` is Python and `corpus/` holds a manifest plus a download script; it does not name the fixture generator, its language, or whether Typst is embedded or invoked. I aligned the *manifest and download-script* naming (A-3) because two of three documents agreed and D14 backs them, but I did **not** pick a fixture generator: the in-process-vs-subprocess choice has real consequences (determinism guarantees, the `typst` CLI pin in `eval/tools.lock`, whether `typst`/`typst-pdf` become workspace dependencies, and whether Phase 0's snapshot-churn mitigation still applies). **Ask:** one of `oc-fixtures` (Rust, in-process, in `eval/`) or `xtask fixtures` (Rust, shelling out to a pinned `typst` binary) — and whether `eval/src/oc_eval/generate/typst.py` survives either way.

### 2.2 `hyphenation` version: `0.8` vs `0.8.4`

The audit list names `hyphenation 0.8`; `IMPLEMENTATION_PLAN.md` says `hyphenation` 0.8 and `TECHNOLOGY_EVALUATION.md` §12 says v0.8.4. These are compatible (0.8.4 satisfies a `0.8` requirement), so I left both. Worth confirming the exact pin lands in `Cargo.toml` as `0.8.4` when VD-b closes, since the crate version and the *pattern* licences are being verified in the same pass.

### 2.3 "Azure Artifact Signing" vs "Azure Trusted Signing"

`DECISIONS.md` D12 and `SECURITY.md` §12 say **Azure Artifact Signing**; `IMPLEMENTATION_PLAN.md` said **Azure Trusted Signing** in three places. I aligned the plan to the ADR (A-2), because the ADR governs. But this may be a factual error in the ADR rather than a drafting slip in the plan — I have no web access and cannot confirm Microsoft's current product name. **Ask:** confirm the product name; if it is "Trusted Signing", the fix belongs in D12 and `SECURITY.md`, and I will re-align the plan.

### 2.4 Crates present in the plan but absent from `DECISIONS.md` Appendix A

`crates/oc-testkit` (test-only: fixture builders, assertion runner, structural digest) and the top-level `xtask/` are workspace members throughout `IMPLEMENTATION_PLAN.md` but are not in D14's crate map. I added both to `ARCHITECTURE.md`'s tree (A-4) since they are clearly intended and consistent with D14's "one Cargo workspace (`crates/oc-*`)" shape. This is an **addition** to the ADR's Appendix A, not a contradiction with it — noting it so the ADR's crate map can be updated deliberately rather than by drift. Related: `IMPLEMENTATION_PLAN.md` §0.6 names an assertion crate `oc-eval-assert` "under `crates/oc-testkit`", which reads as a module rather than a separate crate; worth one clarifying word if it is meant to be its own crate.

### 2.5 `--isolate-parser` go/no-go criterion has no threshold key

Carried forward unchanged from Appendix F.6(a): the "< 15 % wall-clock overhead, unchanged output bytes" criterion was invented by the plan's author and has no `thresholds.toml` entry, because it gates a spike rather than production code. R-16 confirms the spike stays post-v1 but does not settle whether its criterion should be an enforced key. Left as-is; flagging that it is still open.

---

## 3. Fact-check results

All twelve claims were located in the cited research sections and match the documents' use of them, with two citation errors found and corrected. Sources are `research/round1/` unless noted.

| # | Claim | Cited as | Found at | Verdict |
|---|---|---|---|---|
| 1 | XY-cut **100 %** on Manhattan layouts | R2 §B.2 / arXiv 2607.01018 | R2 lines 17, 373, 527, 671 — §B.2's table gives Manhattan **100 %** for XY-cut vs **96.0 %** for the learned method; 75 % multi-column, 49.7 % wrap-around; arXiv 2607.01018 confirmed as the source | **Confirmed.** Docs quote it correctly, including the 49.7 % weak row in `TECHNOLOGY_EVALUATION.md` §5. |
| 2 | XY-Cut++ **0.988** BLEU vs LayoutReader **0.788** | R10 §4.2 | R10 lines 220–222 (0.988 / 0.788), 233 (514 vs 22 FPS), 238 (LayoutReader 0.595 on three-column, below naive XY-Cut's 0.702) | **Confirmed.** `LLM_EVALUATION.md` §1.1 cites arXiv 2504.10258, which matches R10 §4.2's own citation. |
| 3 | LayTextLLM **34.3 %** for coordinates-as-text | R10 §4.1 | R10 lines 194–201: Llama2-7B-chat + coordinates as text = **34.3 %**; learned projection = 78.1 %; 6.2× token blow-up; arXiv 2407.01976 | **Confirmed.** Quoted correctly in D13.6, `ARCHITECTURE.md` §1, `LLM_EVALUATION.md` §1.1 and §8. |
| 4 | Boros et al. "LLMs **mostly degrade** the input text…" | R10 §4.3 | R10 lines 249, 337, 526, 624 — verbatim, 14 models (350M–7B), 8 languages incl. German, LaTeCH-CLfL 2024 | **Confirmed.** Quoted verbatim in `LLM_EVALUATION.md` §1.1 and paraphrased accurately in D16 and `ARCHITECTURE.md` §12. |
| 5 | DocLayNet `Title` human agreement **60–72** | R2 §C.7 | R2 §C.7 (heading at line 641), table row at 657: `Title` 60–72; restated at R10 §6.7 line 430 | **Confirmed.** Docs cite R10 §6.7; both sections carry it. `PROBLEM_ANALYSIS.md` extended to cite both. |
| 6 | GROBID section-title F1 **74.86** / **76.43** | R10 §3.5 vs R2 §B.5 | **74.86** = R10 §3.5, soft F1, **bioRxiv**, 2,000 PDFs, v0.8.1. **76.43** = R2 §B.5, soft F1, **PMC**, 1,943 PDFs, v0.8.1. Both real, different corpora. | **Confirmed, and one miscitation corrected.** `PIPELINE.md` §5/§13, `TECHNOLOGY_EVALUATION.md` §5 and `IMPLEMENTATION_PLAN.md` A4.3 pair 76.43 with R2 §B.5 — correct. `PROBLEM_ANALYSIS.md` paired **74.86 with R2 §B.5** — wrong corpus for that section; **fixed** (see §1.21). A second dangling citation, "R10 §5.16" (no such section — the 77.26/82.84 figures are in §3.5 and §6.16), also **fixed**. |
| 7 | Dehyphenation CRF **92.38** balanced accuracy, **85.78** recall | R2 §B.7 | R2 line 481: CRF/logreg 98.75 acc / 98.98 prec / **85.78** recall / **92.38** balanced acc; dictionary-only balanced acc 66.87; keep-hyphen recall 31.7 % | **Confirmed.** `PIPELINE.md` §7 and `IMPLEMENTATION_PLAN.md` Phase 3 quote 31.7 → 85.8 and 66.87 → 92.38 exactly. The "~32 % → ~86 %" rounding in D13.6, `PROBLEM_ANALYSIS.md` §1 and `TECHNOLOGY_EVALUATION.md` §5 is a faithful rounding of the same figures. |
| 8 | pypdfium2 **97 %** vs PyMuPDF **96 %** | R2 §A.11 | R2 lines 311–312 (both 0.1 s; 96 % vs 97 %), restated at 719 | **Confirmed.** D3 and `TECHNOLOGY_EVALUATION.md` §2 quote it correctly, including the "matches on speed, beats on quality" framing. |
| 9 | gethopp **8.6 MiB** vs **244 MiB** | R6 §2.1 | R6 line 57 (8.6 MiB vs 244 MiB bundle, 28×), 172 MB vs 409 MB RAM, N=1, 9 Apr 2025 | **Confirmed.** `TECHNOLOGY_EVALUATION.md` §1 quotes it exactly with the N=1 caveat; D2's "~9 MB" is a fair rounding. Note the docs correctly avoid the tech-insider.org figures R6 flags as `[UNVERIFIED — do not cite]`. |
| 10 | Tesseract v5 + OpenMP **1.96 s** (fast) / **3.46 s** (best) | R3 §3.1 | R3 lines 25 and 59, i7-10750H, v5.0.1 + OpenMP | **Confirmed.** `TECHNOLOGY_EVALUATION.md` §7 quotes both figures with the CPU. R3's own caveat — the test image and DPI are unstated, so these are directional rather than an A4/300 dpi figure — is worth carrying into any performance budget that leans on them; no doc currently does. |
| 11 | Playwright Chromium **~281 MB** | R5 §D1 | R5 lines 16 and 274–276: Chromium ~281 MB, Firefox ~187 MB, WebKit ~180 MB, from Playwright's own docs | **Confirmed.** D7 and `TECHNOLOGY_EVALUATION.md` §6 quote it correctly, with the WebKit/Firefox figures matching too. |
| 12 | Tagged PDF **12.6 %** | R1 §A.10 | R1 lines 12, 308, 371, 496: 12.6 % of 19,997 scholarly PDFs (2014–2023), declining since 2019, 3.2 % met all six criteria, 74.9 % met none, 95 % of alt text non-descriptive | **Confirmed.** D18, `PIPELINE.md` §2/§8, `PROBLEM_ANALYSIS.md` §1 and `IMPLEMENTATION_PLAN.md` (`corpus.tagged_share_target = 0.126`) all quote it correctly, including the decline and the alt-text figure. |

**Summary:** 12/12 claims verified present and accurately quoted in the research sections cited. Two citation defects found and corrected, both in `PROBLEM_ANALYSIS.md` §3.3 — one number attributed to the wrong GROBID benchmark corpus, and one pointer to a research section that does not exist.


---

## 4. Chief Architect rulings on §2 (applied 2026-09-09)

- **2.1 Fixture generator → `cargo xtask fixtures`**, Rust, in-process via the `typst` + `typst-pdf` crates (pinned in `Cargo.lock`, embedded fonts only). `oc-fixtures` is renamed accordingly; `eval/src/oc_eval/generate/typst.py` and `eval/tools.lock` are removed; `eval/` never touches Typst. Applied in TEST_CORPUS §6.1/§6.3/§7.1/§8, IMPLEMENTATION_PLAN §1.1/§1.7/Phase 0 files+dependencies+failure table/Phase 7 dependencies, DECISIONS Appendix A.
- **2.2** `hyphenation` is pinned at `0.8.4` when VD-b closes (no doc change needed; both statements are compatible).
- **2.3** Product name stays **Azure Artifact Signing** (R6 §3.4 cites Microsoft's current pricing page under that name); the ADR and plan now say "formerly Trusted Signing" once each.
- **2.4** `oc-testkit` (dev-only crate; `assert` is a module inside it, not a separate crate) and `xtask` (dev-only bin) are added to DECISIONS Appendix A.
- **2.5** The `--isolate-parser` spike criterion (< 15 % overhead, byte-identical output) stays a spike criterion in the plan, not a `thresholds.toml` key.
