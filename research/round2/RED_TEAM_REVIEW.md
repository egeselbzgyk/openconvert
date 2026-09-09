# RED TEAM REVIEW — OpenConvert `docs/DECISIONS.md`

**Reviewer:** adversarial systems architect (red team). **Date:** 2026-09-09.
**Scope:** `openconvert/docs/DECISIONS.md` (D1–D17), read against `round2/CONTRADICTIONS_AND_GAPS.md`, `round2/V1_llm_verification.md`, `round2/V2_rust_deps_verification.md`, and round-1 R1/R2/R6/R7/R9/R10.
**Method:** no web search. Six WebFetch calls against llama.cpp `master` sources, used only where a claim was decision-changing (recorded inline as **[NEW EVIDENCE]**).
**Posture:** I assume the decisions are wrong until the evidence says otherwise. Where I agree with a decision I say nothing — silence here is assent.

**Headline:** the architecture is sound and unusually well-sourced. Three things are actually *wrong* (not merely risky): the character-conservation invariant as literally stated, one factual claim in D8 about llama.cpp cache levers, and D17's claim that no threshold is invented. Two things are under-specified in ways that will produce shipped bugs (process-tree ownership; Tier-1 validation coverage). One decision (D9's default model) is a solo maintainer betting the critical path on the least-verified component in the plan.

---

# (A) Decision-changing objections, ranked

## A1. The "character multiset conservation" invariant is false as written, and it is guarding the wrong 5% of the pipeline

**Objection.** D13 states Gate L as: *"the edit is local and conserves the non-whitespace character multiset (only whitelisted exception: dehyphenation removes exactly one hyphen)"*, scoped to **LLM edits**. Two independent problems:

1. **Scope.** R10 §9.5 — the source of the idea — says the opposite: *"The character-conservation invariant (Gate L) is the cheapest safety property available and should be enforced globally, not per-stage."* As scoped in D13 it protects only LLM edits, which are already the most constrained operations in the system (flat enums, ≤30 blocks, budget-capped). The transformations that actually delete text are all deterministic: furniture stripping, overdraw dedup, OCR-sandwich dedup, ligature expansion, soft-hyphen stripping, decorative-glyph dropping. The invariant is currently pointed at the safe path.
2. **Truth.** Taken globally, the invariant as stated is **false** for at least seven real operations, and the failures are not edge cases:

| Operation | Why plain multiset equality breaks |
|---|---|
| Ligature expansion (`ﬁ` → `f`+`i`) | One scalar becomes two. R2 §B.8: PDFium does *not* expand; U+FB00–FB06 must be mapped explicitly. |
| Header/footer/page-number removal | Deletes non-whitespace characters by design (R10 §6.6). |
| Overdraw dedup (fake bold drawn twice) | Deletes a full duplicate copy (R1 §D.6 #2). |
| OCR-sandwich dedup (render-mode-3 layer) | Can delete ~half a page's characters. |
| OCR itself | Creates characters from nothing. |
| Unicode normalization | NFKC maps `¹`→`1` (R2 §B.8) — and destroys the superscript footnote signal at the same time. |
| Soft hyphen U+00AD | Non-whitespace under Unicode `White_Space`; stripping it violates the invariant. |
| Drop caps / small caps | Re-parenting is conserving, but the *failure mode* (drop cap emitted twice) is exactly what the invariant should catch — and can only catch if it is running on the deterministic path. |

**Evidence.** R10 §9.5 (global enforcement); R2 §B.8 (PDFium does not expand ligatures; NFKC caveat); R1 §D.6 (fake-bold overdraw is a distinct defect class from OCR sandwiching); R10 §6.14 (OCR is character-creating). Note also the invariant does **not** protect the failure R1 §D.7 calls out as visibly book-ruining: a *wrong* dehyphenation join conserves the multiset exactly. At R2 §B.7's measured keep-hyphen recall of 31.7% for dictionary-only, a 300-page novel produces hundreds of silently corrupted words while Gate L reports green.

**Recommended change.** Replace Gate L's text with a **ledger-balanced conservation law** enforced after *every* stage, with a two-class stage taxonomy (`Conserving` / `Budgeted`). Exact formulation in §C1. Additionally:
- Reword D13 so the ledger is the mechanism and the invariant is the property — currently the ledger is described as a data structure with no stated law over it.
- Add a **separate** dehyphenation safety rule (attestation-based, fail-closed to *keep hyphen*), because Gate L cannot see this class of error. And adopt R2 §B.7's finding directly: a kilobyte-scale CRF/logreg lifts keep-hyphen recall 31.7% → 85.8% at no accuracy cost. **That model is cheaper, more accurate, and more testable than the batched LLM dehyphenation call D13 currently plans.** Consider dropping the LLM dehyphenation path from v1 entirely.

---

## A2. The default model should be **Qwen3-1.7B**, not Qwen3.5-2B. Qwen3.5-2B belongs behind the gate, not in front of it

**Objection.** D9 makes the default the one component in the entire plan with the highest concentration of unverified, single-maintainer-hostile risk: no official GGUF (V1: `Qwen/Qwen3.5-2B-GGUF` → HTTP 401), a model card that never mentions llama.cpp or GGUF, a brand-new hybrid architecture, a community-only quant, an OpenConvert-operated re-quantization + mirror pipeline, and a `--chat-template-kwargs` thinking-disable mechanism that depends on a third party having embedded the correct Jinja template in the GGUF. D9 then hedges with a Phase-9 gate — which means the plan's *default* is scheduled to be re-litigated after eight phases of work have been built around it.

**Evidence.**
- V1 §1: no official GGUF (401); README never mentions llama.cpp/GGUF; llama.cpp arch support marked **[COULD NOT VERIFY]**; the "201 languages" claim is aggregate only — **Turkish is explicitly not confirmed**, yet D9 uses "the only credible Turkish coverage among candidates" to reject SmolLM3 and LFM2.5. Qwen3-1.7B (the fallback) also covers Turkish, so the Turkish argument does not actually differentiate the two.
- V1 §1: D9 itself concedes IFEval 61.2 non-thinking is "mediocre".
- **[NEW EVIDENCE]** llama.cpp `master` `src/llama-arch.cpp` *does* contain `{ LLM_ARCH_QWEN35, "qwen35" }` and `{ LLM_ARCH_QWEN35MOE, "qwen35moe" }` (alongside `qwen3next`). So "will it load" is probably **yes** on a recent build — V1's open question is now closed in the model's favour.
- **[NEW EVIDENCE]** `src/models/qwen35moe.cpp` confirms it is a **hybrid recurrent+attention** graph: layers dispatch on `hparams.is_recr(il)` to `build_layer_attn()` or `build_layer_attn_linear()` ("Linear attention layer (gated delta net)"), using `ggml_ssm_conv()`, `build_recurrent_attn()`, `llm_graph_input_rs` recurrent-state inputs, MoE FFN, plus an MTP/NextN block. This has direct consequences — see A3.

**Recommended change.** In the plan, `default = Qwen3-1.7B` (dense, Apache-2.0, official `Qwen/Qwen3-1.7B-GGUF`, `/no_think`). `Qwen3.5-2B` ships as `tier = experimental` from day one, selectable in the model registry, and is *promoted* to default only by passing the gate in §C3. Rationale specific to a solo maintainer:

1. Every unverified risk in the plan (community quant provenance, new arch, chat template, re-quantization CI, hybrid cache semantics, vision-projector stripping) is concentrated in the one component you cannot debug from your own repo.
2. D9 already says *"the model registry is data, not code — swapping the default is a one-line change plus a re-benchmark."* That argument cuts the other way: if swapping is free, **default to the safe one**. Defaulting to the risky one costs a Phase-9 crisis; defaulting to the safe one costs a Phase-9 upgrade.
3. Official GGUF eliminates the "OpenConvert-controlled mirror repo of re-quantized weights" obligation entirely — which as written is an unbudgeted recurring cost (download 3.78 GB BF16, run `convert_hf_to_gguf.py` + `llama-quantize` in CI, host it, be the licensor of record for NOTICE/LICENSE, and own a novel bug class: "our quant differs from upstream").
4. The per-book design (D13) needs **classification over 300–1200 token inputs**, not long-context reasoning. 262 K context and MoE capacity are not the binding constraint; instruction-following and grammar conformance on short flat-enum outputs is. That is the *least* differentiated axis between a 1.7B dense and a 2B hybrid MoE.

---

## A3. D8's claim that `--cache-prompt`/`--cache-reuse` are "the exact levers we need" is factually wrong for D9's default model

**Objection.** D8 lists `--cache-prompt`/`--cache-reuse`/`-np` as verified levers. `--cache-reuse` is **architecturally unavailable** for a hybrid recurrent model — which is exactly what D9 defaults to. The per-book cost argument in R10 §2.3 implicitly assumes a shared few-shot prefix is prefilled once and reused across the 4–8 calls; on the hybrid, chunk-level reuse cannot happen.

**Evidence.**
- V1 §3 (verbatim from `tools/server/README.md`): `--cache-reuse N` = *"min chunk size to attempt reusing from the cache **via KV shifting**, requires prompt caching to be enabled"*. KV shifting = partial `seq_rm` + `seq_add`.
- **[NEW EVIDENCE]** `src/llama-memory-recurrent.cpp`, `seq_rm` (≈L279–370) carries the comment: *"models like Mamba or RWKV can't have a state partially erased at the end of the sequence because their state isn't preserved for previous tokens."* `seq_add`/`seq_div` only adjust positions — there is no state-shift operation. A recurrent state is a fixed-size compression of all preceding tokens; there is no "middle" to excise.
- **[NEW EVIDENCE]** `qwen35moe.cpp` builds recurrent (gated-delta-net) layers with `llm_graph_input_rs`, i.e. the model uses hybrid memory. So the recurrent half of every hybrid layer stack cannot be KV-shifted.
- The actual mechanism that *does* work for recurrent models is **context checkpoints**: **[NEW EVIDENCE]** `--context-checkpoints`/`-ctxcp` = *"max number of context checkpoints to create per slot (default: 32)"*, with `--checkpoint-min-step` = *"minimum spacing between context checkpoints in tokens (default: 8192)"*. Checkpoints snapshot state so a prefix rollback is possible; they are not chunk reuse, they cost RAM per checkpoint, and the 8192-token default spacing is far larger than our entire per-book prompt set.

**Recommended change.** Three edits:
1. Delete `--cache-reuse` from D8's list of levers, or qualify it: *"`--cache-reuse` requires KV shifting and therefore applies only to the dense fallback family; hybrid/recurrent architectures rely on exact-prefix slot reuse and `--context-checkpoints` instead."*
2. Add **prompt-prefix reuse measurement** to the model gate (§C3, G5). If the hybrid cannot deliver ≥60% saving on a repeat call set, R10 §2.3's cost model does not hold for it and that alone disqualifies it as default.
3. Make the shared few-shot preamble an explicit design element (one stable system prefix across all six call types, so a single warm slot serves the whole book) and pin `-np 1` + a single slot so the prefix is never evicted mid-book. Currently D13 implies six differently-shaped prompts with no stated prefix discipline.

**Second-order finding, same area.** D8 pins thinking off via `--chat-template-kwargs '{"enable_thinking":false}'`, which routes through the GGUF's embedded Jinja template. For a community quant of a brand-new architecture this is a common breakage, and V1 notes Qwen3.5-**4B** is thinking-**enabled** by default, so the 4B tier depends on the same fragile link. Decide explicitly: ship our own `--chat-template-file` (or drive `/completion` with our own prompt string + grammar) rather than trusting the file. Note the tension this creates with D10: `/completion` is not OpenAI-compatible, so `LlmProvider::LocalSidecar` and `LlmProvider::OpenAiCompatible` would no longer share a wire format. That is a real design fork D10 has not chosen.

---

## A4. macOS Hardened Runtime vs. runtime-loaded PDFium and post-install downloaded executables — a packaging risk that can block shipping

**Objection.** D3 loads `libpdfium` at runtime via `libloading`; D8/D12 make `llama-server` and `tesseract` **post-install downloads**. macOS notarization requires Hardened Runtime, and Hardened Runtime enforces **library validation**: a process cannot load a dynamic library that is not signed by the same Team ID (or platform-signed) unless it holds `com.apple.security.cs.disable-library-validation`. Separately, on Apple Silicon every Mach-O requires at least an ad-hoc signature to execute at all. DECISIONS.md contains no signing plan for the vendored/downloaded native artifacts — only *"macOS .dmg (Developer ID signed + notarized, $99/yr)"* for the app.

**Evidence.** D3 (`libloading` runtime binding — corroborated V2 §1: `Pdfium::bind_to_library(path)`); D8 *"sidecar + model are post-install downloads"*; D12 same; R6 §3.4 documents the notarization requirement but not the nested-binary consequences. V2 §6(b) confirms Tauri `externalBin` bundles per-target-triple binaries **inside** the app (those get signed with the bundle); downloaded binaries are by definition outside it.

**Recommended change.**
1. **Sign `libpdfium.dylib` with our Developer ID in CI** as an explicit build step, and record it in D3. Do not rely on `disable-library-validation` — that entitlement weakens the app's security posture and is exactly the wrong signal for a tool whose pitch is "local-first and safe".
2. **Bundle `llama-server` inside the notarized .app on macOS** rather than downloading it. It is 10.6 MB (V1 §3, `llama-b10456-bin-macos-arm64.tar.gz`) — a rounding error against a 1.28 GB model download, and it removes an entire class of Gatekeeper failure. Keep the *model* as the download. This means D12's "≈20–30 MB base installer" holds on Windows/Linux but not exactly on macOS; say so rather than discovering it at notarization time.
3. If any executable must still be downloaded (tesseract), verify empirically on Apple Silicon **before** Phase 13 that the artifact runs, and plan for re-signing. Note the trap: re-signing requires `/usr/bin/codesign`, which is **not** present on machines without Xcode Command Line Tools — so "we'll re-sign at install time" is not a viable fallback.
4. V2 §7 already establishes there are **no official cross-platform prebuilt Tesseract binaries** (Linux = distro package, macOS = Homebrew, Windows = UB-Mannheim installer) — i.e. D4's "sidecar" plan requires OpenConvert to become a Tesseract *distributor* on three platforms, with the signing obligations above. For a solo maintainer that is a larger commitment than D4's one-line "Deferred to Phase 13" suggests. Consider: v1 detects image-only pages and tells the user to run OCRmyPDF/Tesseract themselves (with a copy-pasteable command), and Phase 13 becomes optional.

---

## A5. "The CLI is the engine" is under-specified in exactly the places that produce orphaned 1.3 GB processes, un-cancellable jobs, and unbounded memory

**Objection.** D13 states the boundary but settles none of its mechanics. Concretely unresolved, each with a shipped-bug consequence:

| Gap | Consequence |
|---|---|
| **Who owns `llama-server`?** If the CLI spawns it, a 1.28 GB model reloads per conversion. If the app owns it, "the CLI is the engine, one code path" is false. | Either 10–30 s of model-load per book, or two code paths. |
| **Grandchild teardown.** Tauri kills its direct child (the CLI). The CLI's child `llama-server` is a *grandchild*. | App quits/crashes → orphaned `llama-server` holds 1.3 GB indefinitely. On Windows `Child::kill()` = `TerminateProcess`, no tree teardown, no cleanup. |
| **Cancellation channel.** Progress is one-way JSON on stdout. There is no control channel. | Cancel = kill = partial `.epub` at the destination path, orphaned grandchild, temp files leaked. On Windows there is no SIGINT to a windowless child. |
| **stdout contention.** `--progress json` writes to stdout; `--dump-stage` also plausibly writes to stdout. | Interleaved/corrupt event stream the first time someone combines the flags. |
| **Resource limits.** No `--max-pages`, `--max-memory`, no per-stage deadline. | A degenerate PDF (3 pages, 40 M glyphs) OOMs the machine. The stated benefit of out-of-process execution — a hard RAM cap — is not actually taken. |
| **Concurrency.** User drops 40 PDFs. Queue depth? Parallelism? `llama-server` is `-np 1`. | Undefined; likely N simultaneous engines each trying to spawn its own `llama-server`. |
| **Windows console flash.** Rust's `std::process::Command` does not set `CREATE_NO_WINDOW`. Tauri's shell plugin handles its own child; the CLI spawning `llama-server` does not. | Visible console window pops on every conversion. |
| **`externalBin` build ordering.** Our own binary needs a `-$TARGET_TRIPLE` suffix (V2 §6b) staged before `tauri build`, and `tauri dev` will happily run a **stale** staged engine. | Days lost debugging the GUI against last week's engine. |

**Evidence.** V2 §6(b) (target-triple suffix requirement, capability scoping); R6 §3.2 (sidecar mechanics); R6 §10 (the sidecar rationale is written for *llama-server*, not for the whole pipeline); D13 (states the boundary, not the protocol).

**Recommended change.**
1. **Restate the rationale honestly.** The benefits actually obtained by exiling the *whole pipeline* are (a) deterministic RAM reclamation and (b) cancellation-by-kill. Crash isolation is only needed at the C/C++ boundaries: PDFium and llama.cpp. Say that, because it makes the boundary *movable* later without relitigating the decision.
2. **Make `oc-core` a library from day one** and have both `openconvert` (CLI) and the desktop app link it. Keep subprocess execution as the v1 default (it's simple and gives the RAM cap free), but add a short-lived **parse worker** (`openconvert __parse-worker`) around PDFium specifically, so a PDFium segfault costs one page-range, not the whole book. Today a segfault kills the entire conversion — the isolation is coarser than the "the UI survives a PDFium segfault" claim implies is valuable.
3. **Adopt the IPC protocol in §C2**: events on **stderr** (stdout reserved for data), NDJSON with a version field, cancellation via stdin, everything large via files, bounded line length, heartbeats.
4. **Own the process tree.** Windows: a **Job Object** with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` + `JOB_OBJECT_LIMIT_PROCESS_MEMORY`, created by the app, engine assigned into it, and the engine creates a nested job for its own children. Unix: `setsid` + `kill(-pgid)`; Linux additionally `prctl(PR_SET_PDEATHSIG)`. Set `CREATE_NO_WINDOW` on every Windows spawn.
5. **Resolve the double-sidecar cleanly**: the engine always speaks HTTP to an endpoint. If `--llm-endpoint` is supplied it uses it; otherwise it spawns and owns its own. The GUI supplies a long-lived, app-owned `llama-server` so model load is amortized across a batch. One code path, no reload-per-book, no orphan.
6. **Model downloads happen in the app (Rust side, `oc-net`), never during a conversion.** The engine takes `--model-path` and has no network code on the conversion path. A separate `openconvert model pull` subcommand serves CLI users. This is required anyway to keep D13's `unshare -n` CI assertion meaningful, and D13 does not currently say which process downloads.
7. Add a **`hello{engine_version, ir_version, protocol}` handshake**; the GUI hard-errors on mismatch. This is the cheap fix for the stale-sidecar footgun.

---

## A6. Tier-1 validation cannot cover the error class a hand-rolled XHTML emitter is most likely to produce

**Objection.** D5 hand-rolls EPUB with `quick-xml` + `zip` and emits a rich content model (`<figure>/<figcaption>`, `<aside epub:type="footnote">`, `page-list` nav, `epub:type` semantics). D6's Tier 1 is described as covering *"the most common converter errors ... structural and cheap to check in Rust"* — RSC-005, RSC-012, OPF-014, PKG-007. Those four are exactly the errors that *generic* converters (Word/InDesign→EPUB) make. They are **not** the errors a bespoke semantic emitter makes. The bespoke emitter's failure mode is **content-model violation**: `<figure>` inside `<p>`, `<aside>` in phrasing context, `<a>` nested in `<a>` (footnote backlink inside a noteref), `<section>` nesting depth, invalid `epub:type` vocabulary terms, `<table>` without `<tbody>` where a `<thead>` follows. None of those are caught by "XHTML well-formedness" and all of them are EPUBCheck errors. So D6's release gate — *"0 errors on the corpus"* — is being verified in CI only, while users are shipped an EPUB that Tier 1 declared clean.

**Evidence.** R5 §B6 (the four common codes are explicitly attributed to Word/Google-Docs-style source HTML and generic zip mistakes); R5 §B7 (Tier-1 scope: OCF, OPF metadata, referential integrity, properties, media types, *"XHTML well-formedness (a plain XML parse)"*); R5 §B1 (EPUBCheck's own docs concede it *"provides limited CSS validation, cannot verify scripts, and lacks comprehensive accessibility checks"* — i.e. even the authority has scoped coverage, so ours must be measured, not assumed); R5 §B3 (epubveri needed a five-layer pipeline with custom RELAX NG **and Schematron** engines to reach 98.8% parity — that is the honest cost of "deep content-model errors").

**Recommended change.**
1. **Make illegal markup unrepresentable** rather than validating for it. Emit XHTML through a typed constructor API where the Rust type system encodes the HTML content-model distinction (`Flow` vs `Phrasing` vs `Sectioning`), so `p.push(Figure)` does not compile. For a *generator* (as opposed to a validator of arbitrary input) this is dramatically cheaper than a schema engine and eliminates the entire class. Add it to D5.
2. **Measure Tier 1 instead of asserting it.** EPUBCheck's official test corpus is public and BSD-3. Run Tier 1 over it and record per-message-ID parity as a CI-tracked number. epubveri did exactly this; we can copy the *method* without touching the AGPL code. Without this, "Tier 1 misses deep content-model errors — mitigation: EPUBCheck runs in CI" is an unquantified hand-wave: we do not know what fraction of real errors reach the user.
3. **Add the checks a PDF-derived book specifically needs**, which R5 §B6 does not list because they are not generic-converter errors: `noteref`↔`footnote` **bijection** (not just "fragment resolves"), every `page-list` target resolves, `<img>` has non-empty `alt` (ACC-001 class), no `<script>`/`remote-resources`, no DOCTYPE entity declarations, EPUB entry-name collision check under case-insensitive filesystems, and image-count parity against extraction.
4. **Resolve the JRE inconsistency.** D6 rejects bundling a JRE as violating P1–P3, while D12 cheerfully downloads a 1.28 GB model post-install. A jlink'd minimal JRE + `epubcheck.jar` is ~40–50 MB — an **optional post-install pack**, using the identical mechanism as the model and tesseract. That is strictly less costly than the model download and converts "CI-only validation" into "one click for the users who care". The current position is not defensible on its own stated grounds.

---

## A7. The plan's confidence gating is currently unimplementable, and D17's "no threshold is invented" is not true

**Objection.** Gate D requires *"deterministic confidence was low"* against a *"calibrated threshold"*. No calibration exists; the gold set does not exist; R10 §4.4 states plainly that **no paper calibrates these signals for PDF→EPUB** and calls it *"the largest single risk to this architecture."* Meanwhile D17 asserts *"No threshold in these documents is invented"* — but DECISIONS.md contains at least these unanchored numbers:

| Threshold | Where | Anchored? |
|---|---|---|
| prefill **< 300 tok/s** = fail | D9 | No. And V1 §4 shows there is **no reliable CPU prefill anchor at all** — the "~96 tok/s" figure R10 built its cost model on is a misread of a *generation* number from a *regressed build* (actual: ~16 tok/s tg, no pp number in the thread). |
| **≤ 30 blocks/book** | D13 | R10 §6.13 offers it as an illustrative cap ("e.g."), not a measurement. |
| **at most 3** repair iterations | D13 | R10 §6.19 asserts it without derivation. |
| **20–30 MB** base installer | D12 | A goal, not a measurement. |
| **≤ 6 GB / ≥ 16 GB** RAM tiers | D9 | Invented. |
| floats rounded to **2 dp** | D13 | Arbitrary (fine, but say so — and say *2 dp of what unit*). |

The 300 tok/s gate is additionally **measuring the wrong quantity**. The architecture is O(1) calls per book: ~4–8 calls over ~2,000–6,000 total input tokens (R10 §2.3). At 300 tok/s that is ~20 s of prefill for an entire book — comfortably inside D13's own budget. Prefill throughput is an *input* to the thing we care about (LLM share of wall-clock), not a gate.

**Recommended change.**
1. **Ship v1 with `ai.enabled = false` by default.** The deterministic path must be the product; the LLM must be an opt-in improvement with measured benefit (R9 §C.7's McNemar design). This makes the calibration gap non-blocking instead of critical-path, and it is consistent with R10 §4.3's "fail-closed" posture.
2. **Replace scores with abstention predicates for v1.** Every escalation trigger can be expressed as a *structural predicate* requiring no calibration (§C4). This converts "fit a threshold we cannot fit" into "define a rule we can unit-test", and it **generates the calibration data as a side effect**: every escalation is a logged hard case with its signals. The first 200 books converted *are* the calibration corpus.
3. **Operationalize D17 instead of asserting it.** One `thresholds.toml`, every constant carrying `{value, source: binary|published|provisional|calibrated, evidence_url, owner, review_by}`. CI fails if a `provisional` entry has no owner or an expired `review_by`. Then D17 becomes a mechanism rather than a claim that is already false.
4. **Replace the D9 gate** with §C3's, whose primary criterion is *end-to-end LLM wall-clock share*, not tok/s.

---

## A8. Per-book inventory calls convert diffuse errors into correlated, silently-consistent catastrophes — and one of them violates Gate L

**Objection.** D13 argues the once-per-book pattern *"improves quality, because 300 independent per-page decisions disagree with each other while one global decision cannot."* The unstated corollary: **when the global decision is wrong, it is wrong 300 times, consistently, and every downstream validator passes.** R10 §6.7's validations (h1 count plausible, no level skips, monotone with page order) all check *internal consistency* — which is precisely the property a uniformly-wrong cluster mapping has. Four concrete failure modes DECISIONS.md does not bound:

1. **Clustering was wrong.** If the body-text mode is misidentified (common in books where front matter is set in a different family, or where a scanned+reset edition mixes typography), every font-size z-score is computed against the wrong baseline and the whole inventory is garbage. The LLM then labels garbage confidently.
2. **Inconsistent typography → cluster explosion.** Anthologies and multi-publisher compilations produce 40+ clusters, not the assumed 6–12. D13 caps nothing.
3. **1,000+ headings.** D13 budgets *"heading list for book-structure roles ≈ 400–1200 tokens"*. A reference work or a Bible has ~1,200 headings → ~20–30 K input tokens **and** ~10 K output tokens (one `{idx, role}` object each). At R10's own generation anchor, the decode alone is minutes. This single case blows the ≤20–25% wall-clock budget, and asking a 2B model under a grammar to emit 1,189 correctly-indexed objects makes an id-bijection failure near-certain.
4. **`running_head` is a text-deleting label.** R10 §6.7's enum — which D13 adopts — includes `running_head`. Applying it *deletes* those blocks. But Gate L says LLM edits conserve the non-whitespace character multiset. **These two decisions are in direct contradiction.**

**Recommended change.**
1. **Split label authority from deletion authority.** The LLM may *propose* `running_head`; only the deterministic furniture remover may delete, and only when its own cross-page repetition evidence independently agrees (R10 §6.6 — the highest-confidence verdict in the whole matrix). State this in D13; it resolves the Gate-L contradiction and costs nothing.
2. **Add a held-out self-consistency check** that needs no gold set: alongside the cluster inventory, send 8–10 *individual* runs sampled from those clusters but not shown as exemplars. If the per-instance labels disagree with the cluster labels on >20%, reject the whole mapping and fall back to deterministic size-rank. This is a free oracle for "the clustering was wrong", available on day one.
3. **Pre-LLM validity gate on the inventory itself.** Refuse to call the LLM at all if: cluster count > 24, or the modal (body) cluster holds < 60% of non-whitespace characters, or silhouette is below a fixed floor. In those cases the scaffolding is invalid and the LLM cannot rescue it — warn and stay deterministic.
4. **Change the structure-call output shape from per-item to boundary/run-length.** Ask for *transitions* (`"frontmatter ends at idx 12"`, `"part boundary at 13"`, `"backmatter begins at 1104"`) instead of one object per heading. This cuts output tokens ~50× on the pathological case and makes the bijection check trivial (transitions must be strictly increasing and within range). Add a chunking rule with overlap for >200 headings.
5. **Detect stylistic discontinuity** and cluster per-segment rather than per-book when the body-font mode changes across a page range.

---

## A9. A corpus rendered by our own toolchain is systematically missing the failure modes that cause most real defects

**Objection.** R7 §B.1's Standard-Ebooks-XHTML → PDF factory is the right idea and it is a real asset. But R1 §A.0 attributes ~70% of real PDF→EPUB problems to a root cause that WeasyPrint, Typst and Chromium **do not produce**: they emit clean `ToUnicode` CMaps, no Type 3 fonts, no double-drawn fake bold, no invisible OCR sandwich, consistent word spacing, stable geometry, and — worse — **Typst tags PDFs by default** (R7 §B.3), while R1 §A.10 measures the real world at **12.6% tagged, 74.9% meeting none of six accessibility criteria, and declining since 2019**. A corpus built this way is not merely easier than reality; it is *structured differently* from reality in the exact channel our heuristics consume.

The concrete over-fit risks: running heads in our synthetic PDFs are perfectly stable, so repetition-ratio thresholds calibrated on them will be far too permissive on real books with chapter-varying heads and first-page suppression. Line-final hyphen distributions come from WeasyPrint's H&J, not InDesign's. And every "we have a struct tree" fast path gets exercised on 100% of the corpus versus 12.6% of reality.

**Recommended change.**
1. **Strip struct trees from the synthetic corpus by default** (lopdf/qpdf, per R7's own mutation toolchain) and keep tagged variants as a small, separate, explicitly-labelled bucket sized to reality (~12%).
2. **Stratify by producer and report per stratum, never in aggregate.** Bucket on `/Producer`+`/Creator`: `pdfTeX/XeTeX`, `InDesign`, `Word`, `Ghostscript`, `Quark`, `ABBYY/scanner`, `ours(Typst)`, `ours(WeasyPrint)`, `unknown`. **Release gate: `ours(*)` may not exceed 40% of the corpus, and a release may not pass on `ours(*)` strata alone.** (This also delivers R1 §D.6 #4's producer-fingerprinting idea as a free by-product.)
3. **Name the real-world sources DECISIONS.md and R7 do not.** The single best untapped source for genuinely InDesign-typeset, openly-licensed *books* is **OAPEN / DOAB** open-access monographs — real trade-book typography, real publisher toolchains, CC-licensed. Add: Internet Archive public-domain scans (real ABBYY OCR layers, real scanner artifacts), arXiv CC-BY (real pdfTeX), and post-EAA government PDF/UA documents (real tagging of varying quality). Note that Standard Ebooks is a *ground-truth* source, not a real-PDF source — it distributes EPUB, so every PDF in that path is ours.
4. **Freeze a ≥100-file real-world holdout that is never used to fit a threshold** — only to report. And make the rule explicit: **thresholds are fit on real strata only; synthetic strata are for regression (byte-identical output) and unit correctness.**
5. **Use mutation for distribution shift without labelling cost.** Keep the mutation catalogue keyed to R1's failure taxonomy so coverage is auditable: strip ToUnicode, re-encode as Type 3, double-draw glyphs, jitter word spacing, insert a render-mode-3 layer, offset CropBox vs MediaBox (R1 §D.6 #1 — this silently deleted body text in a shipping 2026 tool).
6. **Track the score gap between `ours(*)` and real strata as a first-class metric.** A widening gap is the early-warning signal for renderer over-fit, and it is the only cheap defence available.

---

## A10. The repair loop has a bound but no termination argument, and the real target should be zero repairs

**Objection.** *"at most 3 iterations"* caps cost; it does not establish progress or prevent oscillation *within* the budget (A→B→A over two repairs each fixing one issue and creating another). D13 also does not say what happens after iteration 3 with errors remaining — ship anyway, or fail?

More fundamentally: we own the generator. R10 §6.19 makes this point and D6 half-adopts it, but the plan still frames repair as the mechanism. **Every repair that fires is a bug in our emitter.** A corrective patch to the zip is strictly worse than a preventive fix in the emitter: it is untyped, order-dependent, and it means the emitter stays broken.

**Recommended change.**
1. **Well-founded measure + strict decrease.** `M = (fatal_count, error_count, warning_count)` lexicographic. Apply a repair only if it strictly decreases `M` **and** introduces no message ID absent before (R10 §6.19 has the second half; the first half is missing). Then termination is guaranteed in ≤|messages| steps and "3" is a safety cap, not the argument.
2. **Cycle detection.** Hash the EPUB content (timestamps excluded) each iteration; a repeated hash halts with `status = repair_oscillation` and a report entry. Strict decrease alone does not catch every cycle if two repairs interact.
3. **Confluence by construction.** Partition the mapping table so at most one repair touches a given `(file, node)` per iteration, and apply repairs in a fixed order sorted by `(severity, message_id, location)`.
4. **Invert the metric.** Track **repair-fire rate on the corpus as a release-gate metric with target zero.** Any repair firing more than N times across the corpus opens a maintainer issue against the *emitter*. Reframed this way, "0 EPUBCheck errors on the corpus" stops being a validation goal and becomes a generator-correctness goal — which is the only version that scales for one maintainer.
5. State the post-budget behaviour explicitly: remaining errors → the EPUB is still written (a slightly-invalid EPUB is more useful than none), but the report is marked `invalid` and the UI says so plainly.

---

# (B) Non-blocking risks, with mitigations

**B1. `pdfium-render` API assumptions in D3 are partly wrong, and one load-bearing signal may not exist.** V2 §1: `bounds()`, `font_size()`, `text_render_mode()` **do not exist** (they are `tight_bounds()`/`loose_bounds()`, `unscaled_font_size()`/`scaled_font_size()`, `render_mode()`); `rotation()` is `angle_degrees()`. Cosmetic — except **`has_unicode_map_error()` could not be found**, and R2 §B.8 makes `HasUnicodeMapError` the primary detector for "this page has a broken CMap → route to OCR". *Mitigation:* spike it in Phase 1; if absent, substitute the U+FFFD/PUA share + dictionary-hit-rate signals (R10 §4.4 lists both) and record that substitution as a decision. Also: `libloading` runtime binding means a **PDFium version mismatch is a runtime failure, not a link error** — add a version probe at startup and pin the ABI expectation.

**B2. SMask/alpha handling is unverified.** V2 §1 could not find an explicit SMask API; `get_processed_image()` claims to "account for filters, masks, and object transforms" but the mechanism is unconfirmed. Scanned-book plates and any image with transparency are affected. *Mitigation:* a 10-fixture spike in Phase 2 (image with SMask, image with stencil mask, CMYK JPEG, indexed PNG, 1-bit CCITT) comparing pdfium output against a reference; decide the resampling/re-encode policy from the result (see D5 in §D).

**B3. `hayro` as a Phase-2 differential oracle is premature.** It is self-described experimental, performance-deprioritized, has no encrypted-PDF support (R6 §3.6), and — decisively — does not expose the char-level features the pipeline depends on (`font_weight`, `is_generated`, `is_hyphen`, `render_mode`). An oracle that cannot observe the same signals can only diff extracted *strings*, which `pdftotext` already does for free in CI (R9 §B.5). *Mitigation:* keep the `PdfBackend` trait (cheap insurance, correctly decided); defer the second backend to post-v1; use `pdftotext` as the differential string oracle now.

**B4. The IR will be large, and D13's snapshot strategy assumes it is not.** A 300-page book with per-block provenance, per-decision confidence + signals, a text ledger and a decisions log will serialize to tens of MB of canonical JSON. `insta` snapshots of full IRs for real books are unreviewable. *Mitigation:* snapshot full IRs only for tiny hand-made fixtures (R9 §B.2); for corpus files snapshot a **structural digest** (block counts by type, heading tree shape, ledger totals by reason, first/last 200 chars per chapter). Also stream/stage the IR per page-range where possible — holding all stages in memory contradicts P1.

**B5. Float rounding to 2 dp is under-specified.** D13 rounds floats to 2 dp "for snapshots" — but if that rounding is applied to *stored* geometry it destroys precision in any normalized coordinate space. *Mitigation:* state that geometry is stored in PDF points (f32) at full precision and 2-dp rounding is a serialization-for-snapshot concern only.

**B6. Determinism claim vs. llama.cpp reality.** D17(a) lists "deterministic-output byte-identical" as a binary conformance gate. R9 §C.3 argues (flagged unverified but high-confidence) that llama.cpp greedy decoding is **not** bit-reproducible across thread counts/backends because reduction order changes FP rounding. *Mitigation:* scope the claim — byte-identity is asserted for `--no-ai` runs and for cache-hit AI runs; the D13 content-addressed cache *is* the determinism mechanism for the AI path. Say this explicitly, and pin thread count for cassette recording.

**B7. Flatpak is "primary" on Linux but the Tauri updater does not cover it.** R6 §3.4: the updater handles `.AppImage`, `.tar.gz`, `.exe`/`.msi`; `.deb`/`.rpm`/Flatpak users update through their package manager. D12 makes the one Linux artifact with **no updater** the primary one. *Mitigation:* either make AppImage primary (updater works, matches the "download and run" story) or accept and document that Flatpak users update via Flathub — but do not leave both "Flatpak primary" and "Tauri updater" unqualified in the same decision.

**B8. Visual-QA surface is oversized for one maintainer.** D6+D7+D11 specify Tier-1 Rust validation, EPUBCheck CI, Ace CI, Playwright Chromium **and** WebKit DOM assertions on every PR, **and** per-OS golden screenshots nightly. Per-OS screenshot goldens are the highest-maintenance, lowest-yield artifact in testing (R9 §D.4: cross-OS font rasterization differences are unavoidable and must be managed at the baseline level). *Mitigation:* PR = DOM/structural assertions, Chromium only. Nightly = WebKit. Screenshots = release-time, manually reviewed, one OS. Add more only when a screenshot has actually caught a bug DOM assertions missed.

**B9. `cargo-fuzz` is aimed at the wrong target.** D11 fuzzes "the parser" — but we do not parse PDF; PDFium does, and it is already continuously fuzzed by OSS-Fuzz. *Mitigation:* redirect the fuzzing budget to the **XHTML/OPF emitter** and the **IR deserializer** (both are ours and both eat structured input), and keep crashing PDFs as regression fixtures rather than as a fuzz target.

**B10. German dictionary licensing is unresolved and the deterministic dehyphenator depends on it.** V2 §4 could **not** confirm igerman98's SPDX id; D15 excludes GPL hunspell dictionaries from bundled data and promises self-generated word-frequency lists from CC0 sources. That means the German dehyphenator's quality on a default install depends on a data asset that does not exist yet. *Mitigation:* decide now whether DE/TR frequency lists are **core data** (generated by our tooling from DTA/Wikisource-TR, license-clean, shipped) or an optional pack (German quality degrades by default). Also note V2 flags `zspell`'s crates.io license as literally `"Non-standard"` — check before depending on it in an Apache-2.0 product.

**B11. `lingua` default build pulls ~300 MB of language models.** V2 §4 confirms this verbatim. *Mitigation:* `default-features = false` with only `english`/`german`/`turkish` — and make that a recorded decision, since it silently caps multi-language block detection to three languages, which matters for the Latin/French/Greek quotations R10 §6.17 flags as the real multilingual case.

**B12. `zip` is at 8.6.0, not the 2.x/4.x the plan assumed** (V2 §9). Deterministic-zip behaviour (stored-first mimetype, fixed timestamps, no extra fields) is exactly the API surface that churns across `zip` majors. *Mitigation:* pin exactly, write the PKG-007 test first, and treat a `zip` major bump as a reviewed change with a golden-EPUB byte diff.

**B13. Gate V ("statistics do not worsen") is not well-defined.** D13 applies "Gopher-style repetition/quality stats" to *"the edited region"*, but most Gopher statistics are document-level and undefined on a region (`min_doc_words 50` fails on any block). Comparing a ~20-dimensional vector also needs a combination rule and a float tolerance. *Mitigation:* define a fixed ordered tuple of region-valid statistics, a strict-dominance rule with an explicit epsilon, and skip statistics undefined on the region. Otherwise Gate V either never fires or always fires.

**B14. The mirror/re-quantization pipeline is an unbudgeted recurring obligation.** D9's *"OpenConvert-controlled mirror repo of re-quantized weights"* implies: pull 3.78 GB BF16, run `convert_hf_to_gguf.py` + `llama-quantize`, publish, host, and author LICENSE/NOTICE — on every model or llama.cpp bump. *Mitigation:* if A2 is adopted (default = official Qwen3-1.7B GGUF), this disappears for the default path. For the experimental tier, pin the community repo by commit SHA + SHA-256 rather than mirroring.

**B15. Tauri capability scoping is weaker than D2 implies, in two ways.** (i) The scoping covers what the *app* spawns; `llama-server`/`tesseract` spawned by the engine are outside it entirely. (ii) If the UI passes arbitrary `--input`/`--output`/`--model-path` arguments, the arg validator becomes a path regex and the security benefit is largely notional. *Mitigation:* the UI passes **one argument** — a path to a validated job-spec JSON in an app-controlled directory. Everything else lives in the file.

---

# (C) Proposed precise formulations

## C1. The conservation invariant (replaces D13 Gate L)

**Normalization `N`, applied exactly once, at extraction, and never again:**
```
N = strip(U+00AD)  ∘  expand_ligatures(U+FB00..U+FB06 → ASCII sequences)  ∘  NFC
NFKC is forbidden anywhere in the pipeline.
Superscript/subscript status is captured from GEOMETRY before N and stored as a
block/run attribute; N never alters it.
Text is NEVER case-folded. Case folding (Turkish-locale-aware) is used only to
build comparison keys for lookup, never to produce emitted text.
```

**Two baselines, because dedup must not inflate the retention denominator:**
```
C_raw  = multiset of non-whitespace Unicode scalars immediately after extraction
C_0    = same, after N + overdraw-dedup + ocr-layer-dedup   ← the metric denominator
```
`C(D)` = multiset of Unicode scalars in all content-document text of state `D`, excluding scalars with the Unicode `White_Space` property. (Nav/OPF metadata text is outside `C`.)

**Each stage `s` declares statically:** `kind(s) ∈ {Conserving, Budgeted}`, `reasons(s) ⊆ Reason` (closed enum), `budget(s): Reason → f32` (fraction of `|C_0|`).

**Ledger** `L_s = (Removed_s, Added_s)`, entries are **spans, not characters**:
```rust
struct LedgerEntry { reason: Reason, block_id: BlockId, span: (u32,u32),
                     text: CompactString, page: u32 }
```

**Invariants, all checked after every stage (debug + CI always; release build = counts only):**

- **I-1 (conservation).  `C(D_i) ⊎ chars(Added_s) == C(D_{i+1}) ⊎ chars(Removed_s)`**
- **I-2 (declared reasons).** `∀e ∈ Removed_s ∪ Added_s : e.reason ∈ reasons(s)`
- **I-3 (conserving stages).** `kind(s) = Conserving ⇒ Removed_s = Added_s = ∅`, hence `C(D_i) == C(D_{i+1})` — plain multiset equality. **All of the following are `Conserving`:** reading order, block typing, heading detection/levels, chapter/structure roles, verse/quote classification, list nesting, footnote linking, image anchoring, chapter splitting, XHTML serialization, **and every LLM edit without exception**.
- **I-4 (budgets).** Per reason, cumulative removed/added chars ≤ `budget(s)(r) · |C_0|`; plus a global cap on total removals for non-OCR pages.
- **I-5 (dehyphenation is the only in-word edit).** `reason = Dehyphenate` ⇒ `Removed` is exactly one scalar ∈ `{U+002D, U+2010}`, `Added` is empty, and the resulting token differs from the concatenation of the two source tokens by exactly that character.
- **I-6 (OCR).** `reason = Ocr` is `Added`-only, permitted only on pages `p` where `C(text of p) = ∅` before the stage. Any page with an `Ocr` entry is marked `provenance = ocr` and **excluded from the source-retention metric**.
- **I-7 (end-to-end, the release gate).** `C(EPUB) ⊎ chars(all Removed) == C_0 ⊎ chars(all Added)`.

**`Reason` enum (v1, closed):** `SoftHyphen · LigatureExpand · GeneratedSpace · RunningHeader · RunningFooter · PageNumber · OverdrawDedup · OcrLayerDuplicate · Dehyphenate · Ocr · DecorativeGlyph · Watermark · ClippedOffPage · UserOverride`

**Provisional budgets (mark `source = provisional` per A7):** `RunningHeader+RunningFooter+PageNumber ≤ 0.04` · `OverdrawDedup ≤ 0.02` · `OcrLayerDuplicate ≤ 0.60 **per page**` · `Dehyphenate ≤ 0.005` · `DecorativeGlyph ≤ 0.002` · every other reason `≤ 0.001` · global non-OCR removal `≤ 0.08`.

**How each probed case is handled:** ligature expansion → paired `Removed`+`Added` (this is precisely why plain equality fails). Dehyphenation → I-5. Furniture → `Budgeted`. Overdraw dedup → `Budgeted`, and folded into `C_0` so it does not inflate retention. OCR → I-6. NFKC → banned; `N` is idempotent and applied once. Drop caps → `Conserving`; the invariant *catches* the common bug (drop cap emitted twice) as an unexplained `Added`. Small caps → CSS class only, never case-folded, therefore `Conserving`. Generated spaces → whitespace, outside `C` by construction (this is why "non-whitespace" is the right restriction). Page numbers retained in `page-list` → `Removed` from flow with reason `PageNumber`; the value reaches nav as an attribute, which is outside `C`.

**Cost.** `C(D)` is a histogram over ~1 MB of text: a `[u32; 128]` ASCII fast path plus a hashmap tail, sub-millisecond. Ten stages per book ⇒ negligible. Ledger entries are spans and number in the low thousands per book. **The "text ledger" is cheap — provided entries are spans and the ledger stores counts, not per-character provenance.** Read literally ("every character removed is recorded"), it is not.

**What it does NOT protect.** A wrong dehyphenation join, a wrong heading level, a wrong cluster label, a wrong reading order. Say so in D13, so nobody treats a green Gate L as "the text is right".

## C2. IPC protocol sketch (replaces D13's `--progress json`)

**Channels.** stdin = control (NDJSON). **stderr = events (NDJSON).** stdout = data only (`--dump-stage -`), empty in GUI mode. Rationale: Tauri's shell plugin surfaces stdout/stderr as separate event streams; reserving stdout keeps `--dump-stage` and piping usable, and avoids the flag-collision bug.

**Framing.** One JSON object per line, `\n`-terminated, UTF-8, hard cap **8 KiB** (engine truncates and sets `"truncated":true`). Common envelope: `{"v":1,"t":<type>,"seq":<u64>,"ts_ms":<u64>, ...}`. `v` is the protocol major; the GUI hard-errors on mismatch.

**Events (stderr).**
```
hello    {engine_version, ir_version, protocol, pdfium_version, capabilities:[...]}
job      {job_id, input_sha256, pages, phase:"started"}
stage    {name, phase:"begin"|"end", elapsed_ms}
progress {stage, done, total, unit}         // coalesced, ≤10/s
warning  {code, severity, args:{}, block_ids?:[]}   // code+args only; GUI localizes
llm      {call_id, purpose, cached:bool, tokens_in, tokens_out, ms}
heartbeat{}                                  // every 2s — lets the GUI distinguish
                                             // "slow stage" from "hung process"
done     {status:"ok"|"failed"|"cancelled", report_path, output_path?}
fatal    {code, message, backtrace_id}
```
**Control (stdin).** `{"t":"cancel"}` · `{"t":"ping"}` · (optional) `{"t":"pause"}`/`{"t":"resume"}`.

**Cancellation contract.** `cancel` sets an atomic flag polled at stage boundaries **and inside per-page loops**. The engine must reach `done{status:"cancelled"}` within **2 s**, delete temp files, and exit **3**. If it has not exited within 5 s, the supervisor terminates the job object.

**Exit codes.** `0` ok · `1` conversion failed (report written) · `2` usage error · `3` cancelled · `101` panic. Never parse stderr text to determine outcome.

**Bulk data never crosses the pipe.** `report.json`, `--dump-stage` output, and contact sheets go to files whose paths appear in `done`.

**Atomic output.** The EPUB is written to `<output>.oc-tmp-<rand>` in the *destination directory* (same filesystem, so rename is atomic) and renamed on success only. Cancel/failure deletes it.

**Process supervision.**
- Windows: app creates a **Job Object** (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` + `JOB_OBJECT_LIMIT_PROCESS_MEMORY` + `JOB_OBJECT_LIMIT_JOB_TIME`), assigns the engine; the engine creates a nested job for `llama-server`/`tesseract`. Every spawn sets `CREATE_NO_WINDOW`.
- Unix: engine is `setsid`'d into its own process group; supervisor kills with `kill(-pgid)`. Linux: engine sets `PR_SET_PDEATHSIG=SIGTERM`. macOS has no `PDEATHSIG` — the job-object equivalent is the supervisor's responsibility, so the app must also clean up on `applicationWillTerminate`.
- Engine installs `SIGTERM`/`SIGINT` handlers and a Windows console-ctrl handler that tear down children; a `Drop` guard alone is insufficient because a panic-abort or `TerminateProcess` skips it.

**LLM endpoint ownership.** Engine flags: `--llm-endpoint <url>` + `--llm-api-key-file <path>`. If supplied, the engine uses that endpoint and spawns nothing. Otherwise it spawns its own `llama-server` on `127.0.0.1` with a CSPRNG per-run `--api-key`, `-np 1`, idle-kill at 120 s. **The desktop app supplies a long-lived, app-owned server** so model load is amortized across a batch. One code path, no reload-per-book, no orphan.

**Resource limits, always on.** `--max-pages` (default 3000), `--max-memory` (default 4 GiB, enforced by job object / `setrlimit(RLIMIT_AS)`), per-stage wall-clock deadline, max output size, max image pixel count. A degenerate PDF must fail cleanly, not OOM the machine.

**Job spec.** The GUI passes **one** argument: a path to a validated job-spec JSON in an app-controlled directory. Everything else lives in the file. This makes the Tauri arg allow-list actually meaningful (B15).

## C3. Go/no-go gate for promoting Qwen3.5-2B to default (replaces D9's "prefill < 300 tok/s")

Two reference machines, named and fixed: **L** = 4-core/8-thread AVX2 x86 laptop, 16 GB; **M** = base Apple M-series, 16 GB. Pinned llama.cpp release tag, pinned GGUF by repo **and commit SHA**. All measured by a committed script (`eval/model_gate.py`), results in the repo.

| # | Gate | Pass condition |
|---|---|---|
| **G1** | Loads | `llama-server` loads the GGUF; `/health` → ok; `llama-bench` completes on both L and M. |
| **G2** | Grammar | 200/200 grammar-constrained generations across all 6 production schemas parse **and** pass semantic assertions (id bijection, enum legality, arity). Zero grammar failures. |
| **G3** | Prompt formatting | Our prompt renderer + this GGUF produce the same token ids as the reference tokenizer on 20 fixture prompts; thinking output absent in 200/200. |
| **G4** | **End-to-end wall-clock (the real gate)** | Full per-book call set (metadata + heading inventory + structure + dehyph residuals + ≤30 ambiguous blocks) completes in **≤ 90 s on L** for a 300-page book, and LLM share of total conversion wall-clock is **≤ 25 %**. |
| **G5** | **Prefix/state reuse** | A second identical call set with prompt caching enabled costs **≤ 40 %** of the first run's LLM wall-clock. *(Directly probes the A3 hybrid-cache risk. Failure here invalidates R10 §2.3's cost model for this model.)* |
| **G6** | RAM | `llama-server` peak RSS **≤ 2.5 GB** at `-c 8192 -np 1`; RSS returns to ~0 within 2 s of `kill()`. |
| **G7** | Quality, measured against the incumbent | On the gold set, McNemar (R9 §C.7) vs. Qwen3-1.7B across the four decision tasks: **non-inferior on all four, and significantly better on ≥1**. Equal is not a reason to switch. |
| **G8** | DE + TR probes | ≥ 95 % on 100-item Turkish and 100-item German flat-enum instruction-following probes; **zero** outputs containing characters outside the target language's alphabet. |
| **G9** | Provenance | The exact GGUF is reproducible by our own quantization from the official safetensors at a pinned revision; SHA-256 matches a hash **we produced**, not one we copied. |

Any failure ⇒ stays `experimental`; default remains Qwen3-1.7B. **Re-run the whole gate on every llama.cpp version bump** (the arch is new; regressions are likely — V1 §4 documents a CPU-backend regression that made a model 17× slower between two consecutive builds).

## C4. Default thresholds for a world with no gold set

**Principle: replace scores with structural predicates.** No calibration required, unit-testable on day one, and each escalation logs a labelled hard case.

| Decision | v1 escalation predicate (no threshold) | Deterministic fallback |
|---|---|---|
| Metadata | XMP/DocInfo `dc:title` absent **or** matches the boilerplate regex list (`^Microsoft Word - `, `\.(docx?|indd|pages)$`, `^untitled`, empty) | filename parse + largest-font block on pp.1–3 |
| Book structure | PDF outline absent **and** TOC-page parse yielded < 3 entries | heading size-rank + keyword rules |
| Heading roles | > 1 candidate style cluster **and** numbering-regex coverage < 100 % | size-rank ordering |
| Dehyphenation | joined form absent from in-doc dictionary **and** absent from lexicon **and** not both halves independently attested | **keep the hyphen** (fail-closed) |
| Verse / quote | indent present **and** short-line-ratio in `[0.35, 0.75]` **and** block budget remains | `blockquote` if indented, else `paragraph` |
| Run-in headings | bold/italic run at paragraph start, ≤ 8 words, terminated by `.`/`—`/`:` | not a heading |

**Hard budgets (all `source = provisional`, all enforced, all in `thresholds.toml`):**
```
ai.enabled                    = false        # v1 default. Opt-in.
llm.max_calls_per_book        = 8
llm.max_blocks_per_book       = 30
llm.max_wallclock_share       = 0.25         # hard stop, not a target
llm.max_output_tokens_per_call= 1500
inventory.max_clusters        = 24           # above this: no LLM call at all
inventory.min_body_char_share = 0.60         # below this: clustering is invalid
inventory.holdout_disagree_max= 0.20         # above this: reject the mapping
repair.max_iterations         = 3
repair.require_strict_decrease= true
xhtml.split_bytes             = 260_000      # source: Calibre/ADE — the one *anchored* number
```

**Replacement path.** `oc-eval calibrate` fits per-signal thresholds by the risk-coverage method (R9 §C.10, Geifman & El-Yaniv): fix a **false-repair-rate target of ≤ 1 % per category** (R9 §C.8), take whatever coverage falls out, and report the reliability diagram with adaptive binning (R9 §C.9, Nixon et al.). Promotion `provisional → calibrated` requires a reviewed commit containing the diagram, `n`, and the CI-recorded before/after false-repair rate. Bootstrap data is cheaper than it looks: Standard Ebooks → PDF supplies free labels for headings, footnote linkage, figures and reading order (R7 §B.1); only furniture removal and verse-vs-quote need the ~50-file hand-labelled set — **and both must be labelled on *real* PDFs, not ours** (A9).

---

# (D) Missing decisions the Implementation Plan will need

1. **IR versioning & stability.** `ir_version` field; migration policy; exact canonical-JSON spec (key order, float format, NaN/Inf handling, string escaping, whether the JSON itself is NFC). The IR is a *public interface* the moment `overrides.json` references it — say so.
2. **Block ID derivation and stability.** Required simultaneously by `overrides.json` (D16), the LLM cache key (D13), and report anchors. Proposal: `id = base32(blake3(page_index ‖ round(bbox,1) ‖ first 64 chars of normalized text))[0..10]` + collision suffix; and a written rule that changing the derivation is a breaking change that bumps `ir_version` and invalidates all caches and overrides.
3. **Config / preset schema.** Format (TOML), per-OS location, precedence (`CLI > job-spec > user config > preset > defaults`), preset set (`novel`, `textbook`, `poetry`, `scanned`, `academic`), and how presets interact with `thresholds.toml`. The GUI must produce the *same* job spec.
4. **Per-page classification and OCR merge policy.** R1 §D.3 argues this should be a first-class, user-overridable verdict (`text` / `mixed` / `image-only` / `ocr-sandwich`), not an implicit branch. Undecided: how OCR blocks join reading order with text blocks on `mixed` pages; whether the invisible render-mode-3 layer is used or discarded; per-page OCR language selection; ledger treatment (I-6); exclusion from retention metrics.
5. **Image policy.** Resampling rule (max longest side, target DPI); pass-through vs re-encode (pass JPEG through untouched when already within target — avoids generation loss and is faster); SMask/alpha compositing (B2); CMYK→sRGB and ICC handling; ornament/logo drop rule (size + cross-page repetition); vector regions → SVG vs raster; total EPUB size budget.
6. **CSS strategy.** D2 and D5 both invoke a "conservative CSS baseline" that is never defined. Needed: one stylesheet or per-file; class naming; whether we set `font-family` or absolute sizes at all (recommendation: **no** — readers override, and setting them is the most common source of "this ebook looks wrong on my device"); how `verse`, `blockquote`, `drop-cap`, `small-caps`, `footnote`, `figure` are expressed.
7. **Chapter splitting policy.** What triggers a new XHTML file (h1? h1+h2? part?); interaction with the 260 KB threshold; how a mid-chapter split is represented; how splits map to spine, nav, and `page-list`.
8. **Language per block.** `dc:language` selection; `xml:lang` on blocks; the ≥5-word minimum and the <20 %-override sanity rule (R10 §6.17); and the `lingua` feature-flag decision (B11) — which caps supported detection languages and is therefore a product decision, not a build detail.
9. **Report presentation.** Versioned `report.json` schema; which warnings are user-visible vs. debug; severity ladder; the `warning_code → localized template` table plus the CI check that every code has a template in **every** locale (R10 §6.20); where the report lives in the UI; and whether a headline quality number is shown (R1 §D.5: without a metric the project cannot converge).
10. **Coordinate-space normalization.** CropBox vs MediaBox vs `/UserUnit` vs page `/Rotate`. R1 §D.6 #1 documents this silently deleting body text in a shipping 2026 tool. Must be a stated invariant: all geometry lives in one normalized space fixed at extraction.
11. **Producer fingerprinting.** R1 §D.6 #4 — dispatching heuristic parameters on `/Producer`/`/Creator` is cheap, deterministic, and nobody does it. But it introduces input-dependent behaviour that complicates testing. Decide yes/no explicitly; if yes, the producer bucket must appear in the report and in corpus stratification (A9).
12. **Encrypted / password-protected PDFs.** PDFium handles them; `hayro` does not. Prompt for a password? Refuse owner-password-only files, or ignore the restriction flags? (There is a legal nuance here worth one sentence.)
13. **Font embedding escape hatch.** D5 says "no font embedding by default" — decide what happens when the text needs glyphs the reader is unlikely to have (Greek, IPA, historical ligatures, Turkish in an old reader).
14. **Determinism contract.** Exactly which outputs are byte-identical under which conditions: `--no-ai` same machine; `--no-ai` cross-machine; AI with cache hit; AI cold (B6). D17(a) currently over-claims.
15. **Telemetry / crash reporting.** D13's no-HTTP-in-core rule implies **none**. State it, because the "report this bug" UX then has to be a manual, user-initiated file export — which needs designing.
16. **Threading model.** D1 says "no `async` in the pipeline core" — good — but the plan still needs: default thread count, `rayon` or not, interaction with `llama-server -t`, and how progress callbacks and the cancellation flag are threaded through a synchronous core.
17. **Model registry distribution.** D9 calls it "data, not code". Where does it live — shipped in-app, or fetched? If fetched, how is it signed and pinned? A model registry that can be updated out-of-band is an unsigned remote-code-selection channel.
18. **Dictionary / word-list packs.** D15 makes dictionaries "separately-licensed optional packs", but the deterministic dehyphenator depends on them and V2 could not confirm igerman98's license. Decide: self-generated CC0 frequency lists as **core data** (D15 hints at this) vs. optional packs with degraded German by default (B10). Related: the tiny CRF from R2 §B.7 needs training data and a serialized model — where does it live and who retrains it?
19. **Performance budget.** D11 lists `criterion` for "per-stage budgets" but no budget is stated anywhere. R10 §2.1 anchors Docling at 0.41–1.06 s/page. Set an explicit target (e.g. ≤ 0.5 s/page and ≤ 500 MB peak RSS on reference machine L for a 300-page born-digital book) or `criterion` has nothing to gate against.
20. **What happens when the LLM path is enabled but the sidecar is missing/corrupt/mid-download.** D13's fail-closed posture implies "proceed deterministically + warn", but the UX (does the conversion block? does it silently downgrade? is the warning modal?) is undecided, and it is the single most-hit path in the first week after release.

---

## Appendix: internal inconsistencies, collected

| # | Claim A | Claim B | Where |
|---|---|---|---|
| 1 | *"No threshold in these documents is invented"* | 300 tok/s; ≤30 blocks; 3 iterations; 20–30 MB; 6/16 GB tiers | D17 vs D9/D12/D13 |
| 2 | LLM edits conserve the non-whitespace character multiset | the heading-inventory call may return `running_head`, whose application **deletes** blocks | D13 Gate L vs D13 LLM call shape (+ R10 §6.7) |
| 3 | `--cache-reuse` is one of "the exact levers we need" | the default model is hybrid recurrent; KV shifting is architecturally impossible for its recurrent layers | D8 vs D9 (**[NEW EVIDENCE]**) |
| 4 | "bundling a JRE violates P1–P3" | 1.28 GB model + Tesseract are post-install downloads | D6 vs D9/D12 |
| 5 | Tauri sidecars give "capability-scoped execution ... exactly what we need for `llama-server`" | the CLI is the engine, so the CLI spawns `llama-server` — outside Tauri's capability system entirely | D2 vs D13 |
| 6 | Flatpak is the **primary** Linux artifact | the Tauri updater does not cover Flatpak (R6 §3.4) | D12 internal |
| 7 | Qwen3.5 has "the only credible Turkish coverage among candidates" | V1 could not confirm Turkish is among the 201 languages; the dense fallback also covers Turkish | D9 vs V1 §1 |
| 8 | deterministic output is byte-identical (binary gate) | llama.cpp greedy decoding is not bit-reproducible across thread counts (R9 §C.3) | D17(a) vs D13 AI path |
| 9 | `pdfium-render` exposes `bounds()`, `font_size()`, `text_render_mode()` | V2 §1: none of those names exist; `has_unicode_map_error()` not found | D3 vs V2 §1 |
| 10 | Gate V re-runs "Gopher-style repetition/quality stats" on the edited region | most Gopher statistics are document-level and undefined on a region | D13 vs R10 §6.18 |
