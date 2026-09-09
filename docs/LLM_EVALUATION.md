# LLM Evaluation — Small Local Models and Runtimes for OpenConvert

**Status:** Reference document supporting `DECISIONS.md` D8, D9, D10, D13.5–D13.8, D17. Does not itself decide anything; where this document and `DECISIONS.md` appear to differ, `DECISIONS.md` governs and the difference is a documentation bug — report it via the Notes section at the end.
**Date:** 2026-09-09.
**Sources:** `research/round1/R4_small_llms_and_runtimes.md`, `research/round1/R10_deterministic_vs_llm.md`, `research/round1/R7_test_corpus_and_synthetic.md`, `research/round1/R9_benchmarks_metrics_testing.md`, `research/round1/R8_security_and_licensing.md`, `research/round2/V1_llm_verification.md`, `research/round2/RED_TEAM_REVIEW.md`. Cited as `R4 §x`, `R10 §x`, `V1 §x`, `RT §x`, etc. Load-bearing claims carry a URL; claims not independently re-verified in round 2 are marked `[UNVERIFIED]` or `[ESTIMATE]`, matching the source documents' own conventions.

---

## 1. What the LLM is for in OpenConvert

OpenConvert's LLM is a **narrow, once-per-book classifier**, not a document-understanding engine. Four tasks, all defined in `DECISIONS.md` D13.6, all called once per book (never per page), all outputting a flat enum or a small index array against a hand-written grammar:

| # | Task | Input | Output shape | Approx. input tokens |
|---|---|---|---|---|
| 1 | **Metadata extraction** | Verbatim text of pages 1–3 with `[LARGE]`/`[MEDIUM]`/`[SMALL]`/`[CENTERED]` style annotations, no coordinates | Flat object of nullable strings + `authors: [string]`, validated by verbatim-substring check against the input | ~200–400 |
| 2 | **Heading-style inventory → role per cluster** | One entry per style cluster (`{cluster_id, size_z, weight, italic, alignment, count, examples[≤5]}`); 6–12 clusters typical | `[{cluster_id, role}]`, role ∈ `{chapter_heading, section_heading, subsection_heading, part_heading, running_head, epigraph, body, caption, other}` | ~300–800 |
| 3 | **Book-structure roles** | Flat heading list `[{idx, text, page, style_cluster}]`, 20–200+ entries, chunked with overlap above 200 | **Boundary/run-length**, not per-item: `frontmatter_end_idx`, `part_boundaries[]`, `backmatter_start_idx` | ~400–1,200 |
| 4 | **Verse / quote / preformatted classification** | Batched ambiguous indented blocks, ≤30/book, verbatim text + categorical geometry (`indent: shallow|deep`, `lines: N`, `centered: bool`, `monospace: bool`) | `[{id, type}]`, type ∈ `{verse, blockquote, preformatted, paragraph}` | ~50–300/block, batched at exactly 10/call |

Pre-call gates apply before any of the four fire: metadata is skipped when XMP/DocInfo is present and non-boilerplate; the heading-inventory call is skipped when clusters are well-separated and numbering coverage is complete; the structure call is skipped whenever a PDF outline (`get_toc()`) already exists; the verse/quote call fires only on blocks the deterministic detector marked genuinely ambiguous. Budgets, per D13.6: **≤8 calls/book total, ≤1,500 output tokens/call, LLM share ≤25% of *total* wall-clock including the LLM (hard stop)**, with the degradation order `verse_quote` → extra `book_structure` chunks → `heading_roles`, never `metadata`.

### 1.1 What the LLM is explicitly not for

The following are deterministic, with **no LLM path in v1**, and the evidence against an LLM path is direct rather than merely "more expensive":

- **Transcription / OCR error correction.** Boros et al. tested 14 models (350M–7B) across EN/FR/**DE**/PL/CS/BG/SL/EL and found *"LLMs mostly degrade the input text, occasionally leave it unchanged, and rarely improve it"* (LaTeCH-CLfL 2024, https://aclanthology.org/2024.latechclfl-1.14/, cited R10 §4.3/§6.14). This directly covers OpenConvert's size class and includes German. D16 lists LLM OCR post-correction as out of v1 for exactly this reason.
- **Reading order.** XY-Cut++ (a deterministic algorithm) scores **0.988 BLEU-4** vs. LayoutReader's (a learned model) **0.788** on DocBench-100, at **23× the speed** (514 vs. 22 FPS), and LayoutReader falls *below* naive XY-Cut on three-column pages — the learned approach generalizes worse outside its training distribution (arXiv 2504.10258, R10 §4.2). A 7B text LLM given coordinates as plain-text numbers scores **34.3%** on layout-dependent extraction (LayTextLLM, arXiv 2407.01976, R10 §4.1) — a 0.5–4B model is strictly worse, at 6.2× the tokens.
- **Header/footer detection.** The solving signal is cross-page repetition, which no per-page model — text or vision — can access. Marker's CPU-only, no-OCR deterministic mode still scores 92.8/100 on this category while a small VLM (Nanonets-OCR2-3B) scores 32.1 on the same benchmark category (R10 §6.6).
- **Language detection.** Lingua reaches 89.27% average / 99.70% on full sentences for German at ~1ms/book; an LLM would be roughly 10⁴× more expensive for a solved problem (R10 §6.17).
- **EPUBCheck message interpretation → repair action.** The message set is finite (~180+ documented IDs), versioned, and stable (https://w3c.github.io/epubcheck/docs/messages/). A static lookup table is 100% accurate on covered IDs; an LLM here injects nondeterminism into the one layer — correctness verification — that must be trustworthy (R10 §6.19).
- **User-visible warnings.** Localized templates, not generated prose — auditability, testability, and reproducibility all argue against generation, and a small model's German/Turkish is worse than its English, so the users most in need of clarity would get the worst text (R10 §6.20).

### 1.2 Why these four tasks specifically

The pattern (R10 §7.1, "where the LLM adds the most value per CPU-second") is that geometry and lexicon settle everything *perceptual*; the four surviving tasks are the ones that are *interpretive*:

- **Metadata**: GROBID — purpose-built, CRF-trained on exactly this task — reaches only **title F1 77.26 strict / authors 82.84** on a 2,000-PDF bioRxiv benchmark (https://grobid.readthedocs.io/en/latest/Benchmarking-biorxiv/, R10 §3.5). That is the deterministic ceiling, and it leaves ~20% of the most user-visible field in the book wrong.
- **Heading roles**: DocLayNet's own inter-annotator agreement on the `Title` class is **only 60–72%** (arXiv 2206.01062, R10 §6.7). When *humans* agree only 60–72% of the time, geometry cannot close the gap — the residual is interpretive ("Chapter Three" vs. a bold run-in vs. a large-font epigraph is meaning, not measurement).
- **Book structure**: deciding that `["Preface", "Part One", "1. The Beginning", …, "Notes", "Index"]` maps to `frontmatter/part/chapter/.../backmatter` needs world knowledge no regex has (R10 §6.8), but only when a PDF outline is absent.
- **Verse/quote/preformatted**: DocLayNet's 11 layout classes contain none of `verse`, `quote`, `epigraph` — no dataset exists, no heuristic distinguishes them, because verse, a long indented quotation, a narrow inset column, and an epigraph are geometrically identical (R10 §6.13). It is also the lowest-risk LLM call in the matrix: the edit is a CSS class only, so a wrong answer degrades presentation but structurally cannot corrupt text.

---

## 2. Candidate models

Sizes are Q4_K_M GGUF unless noted. "RAM@8k" is weights + KV + runtime overhead. Figures marked `[DERIVED]` are arithmetic on cited anchors, not measurements; `[UNVERIFIED]` claims were not independently re-checked by V1.

| Model | Params | License / redistribution | Ctx | Q4 size | RAM@8k | IFEval (mode) | Multilingual / TR / DE evidence | Thinking default | GGUF | llama.cpp | Verdict |
|---|---|---|---|---|---|---|---|---|---|---|---|
| **Qwen3-0.6B** | 0.6B dense | Apache-2.0, confirmed via `Qwen/Qwen3-32B/LICENSE` fetch (R8 §B2); uniform across sizes, unlike Qwen2.5 | 32K native (YaRN to 131K) `[UNVERIFIED — not re-checked this round, general knowledge]` | ~0.4–0.5 GB `[ESTIMATE]` | ~0.7–0.9 GB `[ESTIMATE]` | not verified this round `[UNVERIFIED]` | **119 languages and dialects** per the Qwen3 release blog, which lists **Turkish and German explicitly** (the model card says "100+ languages" and names llama.cpp support); verified 2026-09-09 | off by default, `/no_think` soft switch | **Official** `Qwen/Qwen3-0.6B-GGUF` | Mature, in-tree since 2025 | Low-RAM/fast tier |
| **Qwen3-1.7B** | 1.7B dense | Apache-2.0, same | 32K native (YaRN 131K) `[UNVERIFIED]` | ~1.1–1.4 GB `[ESTIMATE]` | ~1.6–2.0 GB `[ESTIMATE]` | not verified this round `[UNVERIFIED]` | same as above | off by default | **Official** `Qwen/Qwen3-1.7B-GGUF` | Mature | **Default (D9)** |
| **Qwen3-4B** | 4B dense | Apache-2.0, same | 32K native (YaRN 131K) `[UNVERIFIED]` | ~2.5 GB `[ESTIMATE]` | ~3.0–3.4 GB `[ESTIMATE]` | not verified this round `[UNVERIFIED]` | same as above | off by default | **Official** `Qwen/Qwen3-4B-GGUF` | Mature | Quality tier |
| **Qwen3.5-0.8B** | 0.8B hybrid (Gated-DeltaNet + MoE) | Apache-2.0, confirmed (V1 §1) | 262,144 native | ~0.6 GB `[UNVERIFIED — R4]` | ~0.9 GB `[DERIVED]` | 52.1 non-think (V1 §1) | 201 languages claimed in aggregate; per-language TR/DE **not confirmed** (V1 §1) | non-thinking default | Community only (unsloth); **no official GGUF** (HTTP 401, V1 §1) | Arch present in `master` (`LLM_ARCH_QWEN35`) per RT §A2 new evidence; support maturity for MoE variant unclear | Below default tier |
| **Qwen3.5-2B** | 2B hybrid (Gated-DeltaNet + MoE) | Apache-2.0, confirmed | 262,144 native | **1.28 GB** confirmed (unsloth Q4_K_M, V1 §1) | ~1.6–1.9 GB `[DERIVED]` | **61.2 non-think** confirmed (V1 §1) | same aggregate-only caveat | **non-thinking default** | Community only (unsloth); no official repo | Arch confirmed present (`LLM_ARCH_QWEN35MOE` + `qwen3next`, RT §A2); hybrid recurrent layers confirmed (`qwen35moe.cpp`, RT §A3) | **Experimental (D9)** — promotable via nine-gate test |
| **Qwen3.5-4B** | 4B hybrid, same arch | Apache-2.0, confirmed | 262,144 native | **2.74 GB** confirmed | ~3.1–3.5 GB `[DERIVED]` | 89.8 **but thinking-mode default** — non-thinking number not published (V1 §1) | same | **thinking ON by default** — must be explicitly disabled | Community only | Same arch caveats as 2B | Quality-experimental, only if thinking-off is verified reliable |
| **Gemma 4 E2B** | 2.3B effective / **5.1B with embeddings** | **Apache-2.0**, confirmed via 4 independent fetches (V1 §2, https://ai.google.dev/gemma/docs/gemma_4_license) | 128K | 2.58 GB (LiteRT) | **0.74 GB (macOS) – 3.5 GB (Windows)**, platform-dependent (R4 §A.6) | not published | "140+ languages" claimed; no per-language list (R4 §A.4) | configurable, mechanism unconfirmed | Official QAT GGUF | Supported | **Excluded from default** — PLE memory tax and platform variance unresolved (D9) |
| **Gemma 4 E4B** | 4.5B effective / **8B with embeddings** | Apache-2.0, confirmed | 128K | 3.65 GB (LiteRT) | **0.89 GB (macOS) – 9.4 GB (Windows)** | not published | same | configurable | Official QAT GGUF | Supported | Excluded — 9.4 GB Windows peak disqualifies an 8 GB target |
| **LFM2.5-1.2B** | 1.2B conv+GQA hybrid | **LFM Open License v1.0** — revenue-capped, not OSI-permissive (R4 §A.2, R8 §B2) | 128K `[UNVERIFIED]` | 0.86 GB (Q4_0, official) | ~0.9–1.1 GB | **86.23**, best-in-class per CPU cycle | German listed; **Turkish explicitly absent** from the published language list (R4 §A.4) | n/a, no thinking mode | Yes, day-one | Supported | Excluded from default; opt-in download only |
| **LFM2.5-2.6B** | 2.69B hybrid | LFM1.0, same revenue cap | 131,072 | <2.5 GB | ~2.5 GB | IFStruct 85.49, Multi-IF 80.07 | German listed, **Turkish absent** | n/a | Yes | Supported | Excluded from default |
| **SmolLM3-3B** | 3B dense | Apache-2.0 | 64K (128K YARN) | ~1.9 GB `[UNVERIFIED]` | ~2.3 GB `[DERIVED]` | 76.7 no-think / 71.2 think — thinking *hurts* format compliance | German native; **Turkish absent** from language list (R4 §A.4) | on by default, `/no_think` disables | Yes | Supported | Excluded — no Turkish |
| **Ministral 3 3B** | 3.4B LM + 0.4B vision | Apache-2.0 | 256K | ~2.2 GB `[DERIVED]` | ~2.6 GB `[DERIVED]` | not published (MMLU 70.7) | German listed; Turkish not itemized ("plus dozens more") | n/a | GGUF availability not confirmed on card | not confirmed | Alternate-to-evaluate |
| **Granite 4.0 Micro 3B** | 3B dense (non-hybrid variant provided specifically for llama.cpp) | Apache-2.0 | 128K validated | 2.1 GB (Ollama) | ~2.5 GB `[DERIVED]` | family claims HELM lead | none published | n/a | Yes | Supported | Alternate-to-evaluate |
| **Phi-4-mini** | 3.8B | **MIT** | not captured this round | not captured | not captured | not published this round | none published this round | n/a | not confirmed this round | not confirmed | Not evaluated in depth this round; cleanest license among frontier-scale options (R8 §B2) |
| **Llama 3.2 / 4** | 1B–109B | Community License; **EU-domicile exclusion** for all multimodal 3.2 variants and the *entire* Llama 4 family (R8 §B2, secondary-source reconstruction, `[UNVERIFIED exact clause text]`) | — | — | — | — | — | — | Yes | Yes | **Disqualified** — OpenConvert's maintainer is EU-domiciled; the license would not validly grant the maintainer rights to distribute the model, independent of end-user location |
| **EXAONE 4.0** | — | EXAONE AI Model License 1.2-NC — **non-commercial** | — | — | — | — | — | — | — | — | **Disqualified** — incompatible with any future dual-license option (R8 §B2) |
| **Granite-Docling-258M** | 258M VLM (SigLIP2 → Granite 165M LM) | Apache-2.0 | — | — | — | n/a (not an instruction model) | English primary; JA/AR/ZH experimental; **no German, no Turkish** | n/a | n/a | n/a | Not a pipeline stage (§2.1) |
| **SmolDocling-256M** | 256M VLM | Apache-2.0 | — | — | — | n/a | English only | n/a | n/a | n/a | Superseded by Granite-Docling; not a pipeline stage |

### 2.1 Why the document-specialist tinies are not pipeline stages

Granite-Docling-258M and SmolDocling-256M are page-image-to-DocTags VLMs, not text classifiers, and two facts rule them out as a per-page stage regardless of language coverage:

- **Cost.** Granite-Docling: ~3 s/page on an RTX 4090 via llama.cpp (~403 tok/s), scaling `[DERIVED]` to roughly **15–60 s/page on laptop CPU** — 1.5–5 hours for a 300-page book. SmolDocling: 0.35 s/page on an A100 + vLLM, scaling to **~20–50 s/page on CPU** (R4 §A.7, https://huggingface.co/ibm-granite/granite-docling-258M/discussions/37).
- **Language.** Neither claims German or Turkish support; Granite-Docling is English-primary with JA/AR/ZH marked experimental.

Both remain useful as a narrow, explicitly user-triggered "re-analyze this page with vision" escape hatch (D16 lists this out of v1) for the <1% of pages where both geometry and the text LLM fail — never as a pipeline default.

---

## 3. Runtimes

### 3.1 llama.cpp / llama-server — the chosen runtime (D8)

Verified in V1 §3 (two independent fetches of `tools/server/README.md`, plus the GitHub releases page for `b10456`, dated 2026-08-17, https://github.com/ggml-org/llama.cpp/releases/tag/b10456):

- **CPU dispatch.** One binary per OS/arch (`llama-b10456-bin-win-cpu-x64.zip` 17.6 MB, `llama-b10456-bin-ubuntu-x64.tar.gz` 15.9 MB, `llama-b10456-bin-macos-arm64.tar.gz` 10.6 MB) — no separate AVX/AVX2/AVX-512 builds. `GGML_CPU_ALL_VARIANTS` (confirmed in `ggml/src/CMakeLists.txt`) compiles per-ISA CPU backends as dynamically loaded modules selected automatically at runtime from SSE4.2 through AVX-512. This is the mechanism D8 relies on for a single shipped binary per OS.
- **Flags relevant to OpenConvert**, all verbatim-confirmed:
  - `--api-key` — comma-separated key list, used per D8 for a per-run token.
  - `--cache-prompt` (default **on**) — reuses KV of a matching prompt prefix.
  - `--cache-reuse N` — "min chunk size to attempt reusing from the cache **via KV shifting**" — confirmed to require prompt caching, and confirmed by RT §A3 (reading `src/llama-memory-recurrent.cpp`) to be **architecturally unavailable for recurrent/hybrid layers**: "models like Mamba or RWKV can't have a state partially erased at the end of the sequence because their state isn't preserved for previous tokens." Applies to the Qwen3.5 experimental tier, not to the dense Qwen3 default.
  - `--context-checkpoints` / `-ctxcp` (default 32 per slot) and `--checkpoint-min-step` (default 8,192 tokens) — the mechanism that *does* work for hybrid models: snapshot state so a prefix rollback is possible, at a per-checkpoint RAM cost, with a default spacing far larger than any single per-book prompt set (RT §A3).
  - `-np` / `--parallel N` — server slots, default `-1` (auto).
  - `--grammar-file`, and the `json_schema` request field — grammar-constrained decoding.
  - `--chat-template-kwargs` — a JSON object string, the mechanism to pass `{"enable_thinking": false}`.
  - `GET /health` — returns `{"status":"ok"}` when the model is ready.
- **Binary size and packaging.** CPU-only builds are 10–18 MB per OS/arch — small enough to bundle inside the notarized macOS `.app` rather than post-install-download, closing a Gatekeeper-failure class (RT §A4).

### 3.2 Ollama

MIT-licensed (https://github.com/ollama/ollama/blob/main/LICENSE). Confirmed facts (V1 §5): `format` field carries a full JSON schema for structured output, live since 2024-12-06; **default context window is 2,048 tokens**, not 4,096 — silently truncating OpenConvert's longer geometry/inventory prompts unless overridden; default `keep_alive` is **5 minutes**; model store is global (`~/.ollama/models`), shared with every other app on the machine and not under OpenConvert's control. D10 treats a detected Ollama daemon as an `OpenAiCompatible` backend with an explicit `num_ctx` override — never a shipped dependency.

### 3.3 LM Studio and other local servers

LM Studio is closed-source `[UNVERIFIED — not confirmed against a license page, R4 §B.4]`. Supported only as a BYO OpenAI-compatible endpoint, never a dependency.

### 3.4 Other runtimes considered

| Runtime | License | Note |
|---|---|---|
| **mistral.rs** | MIT | Genuine Rust plan-B: CPU/CUDA/Metal, GGUF 2–8 bit, grammar enforcement with strict schema mode, Gemma 4 support landed. Smaller community, fewer eyes on CPU kernels than llama.cpp. |
| **ONNX Runtime GenAI** | MIT | Serious Windows-NPU alternative (constrained decoding, DirectML/OpenVINO/QNN/WebGPU), but the GGUF ecosystem's model availability and community momentum are elsewhere. |
| **MLX / mlx-lm** | MIT | Apple Silicon only, no documented grammar/constrained-decoding support; a later optimization, not a foundation. |
| **llamafile** | Apache-2.0 core | 4 GB Windows executable cap makes it a poor fit when the model is downloaded separately anyway. |
| **candle** | Apache-2.0/MIT | Minimalist; you build the serving layer yourself — not worth it while llama.cpp exists. |

### 3.5 Sidecar vs. embedded vs. require-Ollama (D8's basis)

R4 §B.5's comparison, condensed:

| Criterion | Embedded (in-process) | **Sidecar (chosen)** | Require Ollama |
|---|---|---|---|
| Crash isolation | Poor — a ggml assertion or OOM kills the whole app mid-conversion | **Excellent** — sidecar dies, app detects, falls back to heuristics | Excellent, but outside OpenConvert's control |
| Install complexity for the user | Zero, highest for the maintainer (build matrix) | Zero — one extra signed binary | **A second app the user must install** |
| Reproducibility | Full | Full — pinned binary, GGUF hash, threads, grammar, seed | Poor — global store, user-editable Modelfiles, silent `num_ctx`/version drift |
| Prefix caching / slots | Would require reimplementing against the raw C API | Exposed directly (`--cache-prompt`, `--cache-reuse`, `-np`) | Available but not under OpenConvert's control |
| Update decoupling | Coupled to app releases | **Decoupled** — swap the sidecar binary independently of app signing cadence | Uncontrolled |

The deciding factors, per D8: a book conversion is a long job over untrusted PDFs, so crash isolation is not optional; the app binary never links GGML; and Ollama plus any OpenAI-compatible endpoint speak the same wire protocol as the sidecar, so supporting them costs almost nothing once the sidecar client exists.

---

## 4. The CPU cost model, corrected

### 4.1 The round-1 "96 tok/s prefill" figure was a misreading

Round-1 research (R10 §0, §2.1) anchored its entire cost model on *"a 1B model in Q4 runs at ~96 tok/s prefill and ~16 tok/s generation"*, citing https://github.com/ggml-org/llama.cpp/discussions/13664 (Gemma-3-1B Q4_K_M, i7-12700H, 14 threads). V1 §4 read that thread directly and found this **contradicted**:

> Build b5275 (AVX2-optimized, working): 61.90 ms/token → **≈16.16 tok/s**
> Build b5276 (regressed CPU-x64 build): 1043.79 ms/token → **≈0.96 tok/s**

Two separate errors compounded: (1) the figure is **0.96 tok/s**, not 96 — a dropped decimal point, drawn from a *regressed* build; (2) even the correct reading, ~16 tok/s, is a **generation (`tg`)** number from a working build, not **prefill (`pp512`)** — no prefill number appears anywhere in that thread. This document states plainly: **there is no reliable directly-measured CPU prefill anchor for a sub-4B model in the research corpus.** Any number below is an estimate, not a fact, and is labeled as such.

### 4.2 Best available estimates (mark as estimates)

V1 §4, extrapolating from a confirmed 7B-model Apple-Silicon benchmark thread (M1 Pro: pp512 266 tok/s, tg128 36 tok/s with Metal — https://github.com/ggml-org/llama.cpp/discussions/4167) and the confirmed ~16 tok/s generation figure above:

| Hardware class | ~2B Q4_K_M dense, prefill (pp512) | ~2B Q4_K_M dense, generation (tg128) | Confidence |
|---|---|---|---|
| 8-core AVX2 x86 laptop | **~400–1,500 tok/s** (compute-bound, extrapolated) | **~15–35 tok/s** (anchored to the confirmed ~16 tok/s 1B-class figure) | Estimate — not directly measured |
| Apple base M-series, Metal on | pp512 ≈ 700–1,000 tok/s | tg128 ≈ 50–90 tok/s | Estimate, scaled from 7B Metal data |
| Apple base M-series, CPU-only | pp512 in the low hundreds | tg128 ≈ 10–25 tok/s | Estimate, weaker basis |

A second, independent anchor (R4 §A.6, Liquid AI, llama.cpp, Q4_0, AMD Ryzen AI 9 HX 370): LFM2.5-1.2B measured **2,975 tok/s prefill, 116 tok/s decode**. This is in the same order of magnitude as the extrapolated 8-core figures above and gives modest corroboration, though it is a different model and CPU.

**First item in the benchmark plan (§7): measure these directly with `llama-bench` on the reference machines before trusting any threshold derived from them** (per RT §A7, this is exactly the mistake D9's earlier draft made with a "prefill < 300 tok/s" gate — a gate has since been replaced with the nine-gate wall-clock test in §6).

### 4.3 Why per-page LLM use is still ruled out, even at the corrected numbers

A book page serialized as text + geometry costs **1,500–2,500 input tokens** (R10 §2.2, ~500–650 text tokens + ~900–1,800 tokens of per-line geometry/font features). Even at the more optimistic corrected prefill estimate (~1,000–1,500 tok/s), that is roughly 1–2.5 s of prefill *before generation begins*, per page, for a decision-only call — and the R10 §2.2 table's *decode* estimate (3–9 s/page at the original, now-superseded anchor) does not improve proportionally, because decode is memory-bandwidth-bound and scales far more slowly with prefill throughput gains than prefill itself does. Even a generous re-reading of the corrected numbers keeps a per-page call in the **tens of seconds** range; over 300 pages that is tens of minutes to hours, against a total-conversion budget measured in seconds (D13.11 targets ≤0.5 s/page deterministic). The conclusion R10 §0 reaches — *"a per-page LLM is therefore 20–70× the cost of everything else combined"* — survives the correction even though its specific anchor number does not: the ratio, not the absolute seconds, is the robust part (R10 §9.6).

### 4.4 Why once-per-book inventories are ~O(1)

A per-book call over a style inventory (6–12 heading-style clusters × up to 5 exemplar strings + counts + font stats) totals **300–800 input tokens for the entire book** — a few seconds once, rather than per page (R10 §2.3). This is not a smaller version of the per-page cost; it is a different asymptotic class: the LLM's input size is bounded by the number of *distinct styles* in a book (typically single digits), not the number of *pages* (hundreds). D13.6's decision to call the model on compressed inventories, never raw pages, is what keeps LLM wall-clock share inside the ≤25% hard stop regardless of book length.

### 4.5 Token budget for the four v1 tasks

| Task | Input tokens (§1 table) | Output tokens (cap) | Calls/book |
|---|---|---|---|
| Metadata | ~200–400 | small object, ~30 | 0–1 |
| Heading inventory | ~300–800 | `[{cluster_id, role}]`, ~1 token/cluster | 0–1 |
| Book structure | ~400–1,200 (chunked above 200 headings) | boundary indices only, not one object per heading | 0–1 (or a few, chunked) |
| Verse/quote batch | ~50–300/block × exactly 10/call | `[{id, type}]` | 0–3, capped at 30 blocks/book total |

Global caps from D13.6: ≤8 calls/book total, ≤1,500 output tokens/call, LLM share ≤25% of total wall-clock including the LLM, a hard stop.

### 4.6 The hybrid-model cache caveat

D8 originally listed `--cache-prompt`/`--cache-reuse` as "the exact levers we need." RT §A3 found this **factually wrong for the Qwen3.5 hybrid family specifically**: `--cache-reuse` performs KV shifting, which requires a partial `seq_rm`/`seq_add` on the attention KV cache — an operation that has no equivalent for the Gated-DeltaNet recurrent state, because a recurrent state is a fixed-size compression of *all* preceding tokens with no addressable "middle" to excise (confirmed by reading `src/llama-memory-recurrent.cpp`'s own comment on this exact limitation). `DECISIONS.md` D8 now states this precisely: `--cache-reuse` applies only to the dense fallback family; hybrid/recurrent architectures rely on **exact-prefix slot reuse** (a byte-identical shared system prefix, so the *whole* prefix — not a partial chunk — hits the cache) plus `--context-checkpoints`. This is why D13.6 and §8 of this document insist on **one stable, byte-identical system prefix shared by all four call types**, served from a single warm slot (`-np 1`) per book: for the experimental Qwen3.5 tier, that discipline is not an optimization, it is the only cache mechanism available at all.

---

## 5. Structured output

### 5.1 Hand-written GBNF vs. JSON-schema conversion

llama.cpp ships both hand-written GBNF grammars and a JSON-schema-to-grammar converter (`--grammar`/`--grammar-file`, `-j`/`--json-schema`, and the server's `json_schema`/`response_format` fields — https://github.com/ggml-org/llama.cpp/blob/master/grammars/README.md). V1 §3 confirms the converter's documented gaps, all of which OpenConvert's schemas must design around:

- `additionalProperties` defaults to **false** — favorable, reduces hallucinated keys.
- **Nested `$ref`s are broken**; remote `$ref`s unsupported.
- Unsupported: `uniqueItems`, `contains`, `if`/`then`/`else`, `patternProperties`.
- `minimum`/`maximum` work for integers only, not floats.
- `pattern` rules must be anchored `^…$`.

OpenConvert's four output shapes (§1) are simple enough — arrays of enum labels, small integer indices, flat objects with nullable string fields — that **hand-written GBNF, not schema conversion, is the default**: it is both more reliable against these documented gaps and more token-efficient, since arity ("exactly N labels for N input blocks") is expressible directly in the grammar. `llguidance` (merged upstream, PR #10224, https://github.com/ggml-org/llama.cpp/pull/10224) is a credible future alternative but requires Cargo/Rust at C++ build time via `ExternalProject_Add`, complicating a reproducible cross-platform build — start with stock GBNF, revisit only if grammar expressiveness or mask-computation cost becomes a limit.

### 5.2 The "format restrictions hurt" debate

Two directly conflicting findings exist in the literature:

- **Against structured output:** "Let Me Speak Freely?" (arXiv 2408.02442, https://arxiv.org/abs/2408.02442) reports *"a significant decline in LLM reasoning abilities under format restrictions"*, worse with tighter constraints.
- **For structured output:** the .txt team's rebuttal (https://blog.dottxt.ai/say-what-you-mean.html) reproduced the same tasks with matched prompts and found structured ≥ unstructured everywhere tested (GSM8K 0.78 vs. 0.77; Last Letter 0.77 vs. 0.73; Shuffle Object 0.44 vs. 0.41), attributing the original result to prompt mismatch and a flawed AI-based answer parser.

**OpenConvert's position:** the disputed tasks in both papers involve multi-step chain-of-thought reasoning; OpenConvert's four tasks involve essentially none — they are closed-set classification over a short, pre-digested input. The .txt position is the more plausible one for this workload, but **this is a claim to measure, not assume** — it is ablation #1 in the benchmark plan (§7): grammar-constrained vs. free-form-plus-repair, scored on the same corpus. In the meantime, the schema design mitigates the risk regardless of which side is right for this task: flat enums, tiny schemas (a handful of fields, never deeply nested), and arity enforced by the grammar rather than left to the model's discretion.

---

## 6. The decision (D9)

**Default: Qwen3-1.7B** — dense, Apache-2.0, official `Qwen/Qwen3-1.7B-GGUF`, Q4_K_M, thinking off via `/no_think`. **Tiers:** Qwen3-0.6B (low-RAM/fast), Qwen3-4B (quality, official GGUF). **Experimental tier:** Qwen3.5-2B / -4B (hybrid Gated-DeltaNet + MoE, community GGUF only), promotable to default *only* by passing the nine-gate test below.

### 6.1 Why the default is the dense model, not the newer hybrid one

Round-1 research (R4 §D.1) initially recommended Qwen3.5-2B as the default, on the strength of its published document-structure numbers (the only small model with OmniDocBench 1.5 scores: 79.8 at 2B, 86.2 at 4B) and its aggregate 201-language claim. RT §A2 overturned this recommendation on solo-maintainer risk grounds, and D9 now reflects RT's conclusion:

- **No official GGUF.** `Qwen/Qwen3.5-2B-GGUF` returns HTTP 401; only a third-party (unsloth) quant exists (V1 §1). Every unverified risk in the plan — community quant provenance, a brand-new hybrid architecture, a chat-template dependency for disabling thinking — concentrates in this one component.
- **The Turkish argument doesn't actually differentiate the two models.** D9's earlier framing cited Qwen3.5's "credible Turkish coverage" as a reason to prefer it over SmolLM3/LFM2.5 (which explicitly lack Turkish). But V1 §1 found Qwen3.5's 201-language claim is aggregate-only — no page fetched named Turkish specifically — whereas the dense Qwen3 fallback's coverage is **verified**: the Qwen3 release blog states 119 languages and dialects and names Turkish and German explicitly (checked 2026-09-09; the model card's own figure is the vaguer "100+ languages"). The safer model therefore has the *better*-evidenced language claim, not merely an equal one, so Turkish coverage is a reason to prefer it rather than the riskier one.
- **The task is classification over short inputs, not long-context reasoning.** D13's four tasks operate on 300–1,200-token compressed inventories. 262K context and MoE capacity are not the binding constraint; instruction-following and grammar conformance on short flat-enum outputs are — and that is the axis on which a 1.7B dense model and a 2B hybrid MoE differ least (D9).
- **Swapping the default is cheap, so default to the safe choice.** The model registry is data (`models.toml`), not code — D9's own logic ("swapping is a one-line change plus a re-benchmark") cuts toward defaulting safe and promoting the riskier model on evidence, not the reverse.

### 6.2 The nine-gate promotion test (D9, verbatim)

Two fixed reference machines: **L** = 4-core/8-thread AVX2 x86, 16 GB; **M** = base Apple M-series, 16 GB. Pinned llama.cpp release tag, pinned GGUF by repo **and commit SHA**, measured by a committed script (`eval/model_gate.py`).

| # | Gate | Pass condition |
|---|---|---|
| **G1** | Loads | `llama-server` loads the GGUF; `/health` → ok; `llama-bench` completes on both L and M |
| **G2** | Grammar | 200/200 grammar-constrained outputs parse and pass semantic assertions (id bijection, enum legality, arity); zero grammar failures |
| **G3** | Prompt formatting | Prompt renderer matches the reference tokenizer on 20 fixtures; thinking output absent in 200/200 |
| **G4** | **End-to-end wall-clock (the real gate)** | Full per-book call set ≤ **50 s on L** for the reference 300-page book, **and** LLM share of total conversion wall-clock (including the LLM) ≤ **25%**. Both conditions, not alternatives: at D13.11's 0.5 s/page budget the same book is ~150 s deterministic, so a 25 % share permits ≤ 50 s of LLM. (Ratified: this tightens D9's original 90 s, which at 37.5 % would have breached D13.6's hard stop on the very same input.) |
| **G5** | **Prefix/state reuse** | A second identical call set with prompt caching enabled costs ≤ **40%** of the first run's LLM wall-clock (directly probes the §4.6 hybrid-cache risk — failure here invalidates the cost model for this model) |
| **G6** | RAM | Peak RSS ≤ **2.5 GB** at `-c 8192 -np 1`; RSS returns to baseline within 2 s of `kill()` |
| **G7** | Quality, vs. incumbent | McNemar (§7) non-inferior on all four decision tasks vs. Qwen3-1.7B, and significantly better on ≥1. Equal is not a reason to switch |
| **G8** | DE + TR probes | ≥95% on 100-item Turkish and 100-item German flat-enum instruction-following probes; **zero** outputs containing characters outside the target language's alphabet |
| **G9** | Provenance | The exact GGUF is reproducible by OpenConvert's own quantization from the official safetensors at a pinned revision; SHA-256 matches a hash **produced by OpenConvert**, not copied from a third party |

Any single failure keeps the model at `tier = experimental`. The whole gate re-runs on every llama.cpp version bump — the hybrid architecture is new, and regressions are plausible; V1 §4 documents a real case of a CPU-backend build regressing throughput by 17× between two consecutive llama.cpp releases (Gemma-3-1B, b5275→b5276).

**Where gate results live.** The **source of truth** is the machine-readable run file, one per run, committed at `eval/results/model_gate/<model>__<build>__<machine>.json`: per-gate pass/fail, the measured value, the threshold it was checked against, the machine descriptor, the llama.cpp build tag, and the date. `docs/MODEL_GATE.md` is the human-readable table **generated** from those files by `eval/model_gate.py --render-table` — never hand-edited, and regenerated as a release-checklist item. The same rule covers the §7 benchmark plan: its numbers are read back out of the committed run files, so a claim in prose and the artifact behind it cannot drift apart.

### 6.3 Why Gemma 4, LFM2.5, and Llama are not default

| Model | Reason excluded |
|---|---|
| **Gemma 4 E2B/E4B** | Apache-2.0 is verified (§2), a legitimate alternate candidate — but the Per-Layer-Embeddings memory tax is unresolved: E2B is 2.3B "effective" but 5.1B with embeddings, and measured peak RAM ranges from 0.74 GB (macOS) to 3.5–9.4 GB (Windows) for the same nominal model, an internally inconsistent picture the research corpus could not reconcile (R4 §A.6 open question #2). Excluded until that variance is explained. |
| **LFM2.5-1.2B / -2.6B** | Best instruction-following-per-CPU-cycle in the class, but the LFM Open License v1.0 conditions commercial use on the *user's organization* staying under a revenue threshold — an obligation an OSS app cannot police downstream, and one that would flow onto every corporate user of a bundled default (R4 §A.2). Also: Turkish is explicitly absent from its published language list. Fine as an opt-in download, never bundled. |
| **Llama 3.2 (multimodal variants) / Llama 4 (entire family)** | EU-domicile exclusion in the license grant (R8 §B2, `[UNVERIFIED exact clause text, secondary source]`). OpenConvert's maintainer is EU-domiciled, so this is reported as a hard blocker on the *developer's* side, independent of any end user's location — not merely a redistribution inconvenience. |

### 6.4 Model distribution

Never shipped in the installer. The in-app model manager (Rust, `oc-net`) — or `openconvert model pull` for CLI users — downloads from a **pinned** `huggingface.co/<repo>/resolve/<commit-sha>/<file>` URL (a commit SHA, never a branch name, so the target cannot be silently swapped), verifies SHA-256 against a hash compiled into `models.toml` (shipped with the app; no remote registry in v1), and writes LICENSE + NOTICE beside the downloaded file. Downloads never happen during a conversion — the conversion path has no network code (D13.9). First-run UI shows size, RAM estimate, CPU expectation, and license before the user commits to the download.

---

## 7. Benchmark plan

Adapted from R4 §D.3, corrected per the RT findings noted throughout this document.

**Corpus.** 60 PDFs, held out from prompt development: **20 German, 20 Turkish, 20 English**, spanning fiction, an academic monograph with footnotes, a two-column journal article, and scanned OCR per language. ~25 pages/document sampled → **~1,500 gold-labelled pages**. Gold labels are human-annotated; Qwen3.5-9B (thinking on, offline, no latency budget) is used only to pre-label and to measure the small-model gap — **never as ground truth**.

**Tasks and metrics.**

| Task | Metric | Target |
|---|---|---|
| Block classification (heading level, header/footer, footnote, caption, list) | macro-F1, per-class, per-language | macro-F1 ≥0.92 on LLM-invoked blocks; must beat the heuristic baseline on the same low-confidence subset by ≥8 F1 points |
| Chapter boundary | boundary F1, ±1 block tolerance | ≥0.95 |
| Footnote/caption association | link accuracy | ≥0.90 |
| Output validity | % grammar-valid / % correct arity / % `<think>` leakage | 100% / ≥99.9% / 0% |

**Ablations.** (1) Grammar-constrained vs. free-form-plus-repair, settling the §5.2 debate for this task specifically. (2) Thinking on vs. off at equal wall-clock budget. (3) Verbose per-block JSON vs. token-minimal enum output (predicted 5–10× decode saving, R4 §B.6). (4) Prefix cache on vs. off. (5) 0.6B vs. 1.7B vs. 4B vs. Qwen3.5-2B — the accuracy/latency Pareto front. (6) Serialization variants: normalized-bbox-as-text vs. categorical geometry words vs. font-size ranks (§8's categorical-words position is a hypothesis to confirm, not an assumption). (7) A heuristics-only baseline on the full corpus — the number every other result is measured against.

**Reference machines.** L (4c/8t AVX2 x86, 16 GB) and M (base Apple M-series, 16 GB) — the same two machines as the promotion gate (§6.2), so gate results and benchmark-plan results are directly comparable.

**Acceptance rule.** Ship the LLM path for a given task only if, on the low-confidence subset, it beats the pure-heuristic baseline by ≥8 macro-F1 points at ≤25% of conversion wall-clock, verified with **McNemar's test** (χ² = (b−c)²/(b+c) over paired pass/fail outcomes; exact binomial form when b+c < 25 — R9 §C.7, https://en.wikipedia.org/wiki/McNemar%27s_test) for non-inferiority, **and** a measured **false-repair rate ≤1% per category** (R9 §C.8: the fraction of cases where deterministic-only passed and LLM-augmented failed, reported per assertion category rather than only in aggregate, since a net-positive average can hide a severe per-category harm). If a task wins on English and German but loses on Turkish, it ships **language-gated** rather than globally disabled or globally enabled.

**Default posture until this evidence exists: `ai.enabled = false`** (D17). Escalation in v1 uses structural predicates, not calibrated confidence scores — no gold set exists yet to calibrate against, and RT §A7 established that this makes the calibration gap non-blocking rather than critical-path: the first books converted with predicates *are* the calibration data.

---

## 8. Prompt design guidelines

Five rules, all derived from the evidence above, not stylistic preference:

1. **One stable, byte-identical system prefix across all four call types.** Required for `--cache-prompt` to help the dense tiers and *required at all* for the hybrid tier's exact-prefix reuse (§4.6). The taxonomy, few-shot exemplars, and instructions live in this fixed prefix; only the per-call payload varies.
2. **Categorical geometry words, never raw coordinates.** A 7B model given `[x0,y0,x1,y1]` as text scores 34.3% on layout-dependent extraction (R10 §4.1); a 0.5–4B model is worse. Geometry reaches the prompt as `indent: "deep"`, `alignment: "centered"`, `font: "body+2pt, italic"` — never numbers.
3. **Few-shot per language, and explicit permission to abstain.** Each of the four call types includes exemplars in the book's detected language, and the prompt explicitly instructs the model to answer `unsure`/`other`/`paragraph` (the safe default per task) when uncertain, rather than guessing.
4. **Never emit prose.** Every output is a label, an ID, or a boundary index — never free text, never a rewritten span. This is both the grammar's job (structurally enforced) and the prompt's job (stated as an instruction, defense in depth).
5. **IDs bijective; structure as boundary/run-length, not per-item.** Returned IDs must be exactly the input IDs, in the same count, checked by the schema and again by a semantic assertion (R9 §C.2). The book-structure task in particular asks for *transitions* (where frontmatter ends, where a part begins) rather than one object per heading — RT §A8 found that a per-item shape on a 1,200-heading reference work would blow the output-token budget by orders of magnitude and make an id-bijection failure near-certain; the boundary/run-length shape cuts output tokens roughly 50× on that pathological case.

### 8.1 Four worked examples

**Metadata (task 1).** Input (pages 1–3, style-annotated, no coordinates):

```
[LARGE][CENTERED] Die Verwandlung
[MEDIUM][CENTERED] Erzählung
[SMALL][CENTERED] von Franz Kafka
```

Prompt: *"Extract the book's title, subtitle, author(s), translator, publisher, and year from this title page. Copy strings exactly as they appear. If a field is not present, return null. Do not guess."*

Output: `{"title":"Die Verwandlung","subtitle":"Erzählung","authors":["Franz Kafka"],"translator":null,"publisher":null,"date":null}`

Validation: every non-null field must appear as a verbatim substring of the input (case- and whitespace-normalized) — the cheapest and strongest anti-hallucination check available anywhere in the pipeline (R10 §6.16).

**Heading-style inventory (task 2).** Input:

```
[{"cluster_id":0,"size_z":3.1,"weight":"bold","count":24,"starts_page_ratio":0.9,
  "examples":["Kapitel Eins","Kapitel Zwei","Kapitel Drei"]},
 {"cluster_id":1,"size_z":0.0,"weight":"regular","count":1840,"starts_page_ratio":0.02,
  "examples":["Als Gregor Samsa eines Morgens..."]}]
```

Prompt: *"Here are the distinct text styles in a book, with examples. For each, say whether it is chapter_heading, section_heading, subsection_heading, part_heading, running_head, epigraph, body, caption, or other. The book's language is German."*

Output: `[{"cluster_id":0,"role":"chapter_heading"},{"cluster_id":1,"role":"body"}]`

Validation: as §1's pre-gates (silhouette/cluster-count/body-share), plus a held-out self-consistency check — 8–10 individual runs sampled from the labelled clusters but not shown as exemplars; if per-instance labels disagree with the cluster label on >20% of them, reject the whole mapping and fall back to deterministic size-rank ordering (RT §A8).

**Book structure (task 3).** Input (flat heading list, indices only shown here for brevity):

```
[{"idx":0,"text":"Önsöz","page":3},{"idx":1,"text":"Birinci Bölüm","page":7},
 ...,{"idx":38,"text":"Dizin","page":410}]
```

Prompt: *"This is the list of headings extracted from a book, in page order. The book's language is Turkish. Identify: the index at which front matter ends, every part boundary, and the index at which back matter begins. Do not invent or reorder entries."*

Output: `{"frontmatter_end_idx":1,"part_boundaries":[],"backmatter_start_idx":37}`

Validation: indices strictly within range and increasing; front matter contiguous at the start, back matter contiguous at the end (RT §A8's boundary-shape fix).

**Verse/quote/preformatted (task 4).** Input (batched, one of exactly 10 in a call):

```
{"id":"b0412","text":"Two roads diverged in a yellow wood,\nAnd sorry I could not travel both",
 "indent":"deep","lines":4,"avg_line_words":7,"centered":false,"monospace":false}
```

Prompt: *"Classify each block as verse, blockquote, preformatted, or paragraph. Answer paragraph when unsure."*

Output: `[{"id":"b0412","type":"verse"}]`

Validation: text is byte-identical before and after (only the wrapper changes — this is the strongest instance of Gate L in the whole system, since the change is provably a CSS class and nothing else); `verse` additionally requires ≥3 lines and a short-line ratio the deterministic detector already measured, so the LLM cannot override strong counter-evidence (R10 §6.13).

---

## 9. BYO providers (D10)

Model format: **GGUF**. Inference strategy: **sidecar** (§3.5). BYO provider strategy: one `LlmProvider` trait behind an OpenAI-compatible HTTP client, two implementations in v1:

- **`LocalSidecar`** — the `llama-server` process OpenConvert spawns and owns (§3.5).
- **`OpenAiCompatible`** — base URL + optional key, for any server speaking the same wire protocol.

**Ollama** is `OpenAiCompatible` with auto-detection on `localhost:11434`, using its `format` field for JSON-schema output, and an **explicit `num_ctx` override** — Ollama's 2,048-token default would silently truncate the geometry/inventory prompts described in §4.5 (§3.2). **LM Studio** and other local servers are custom-endpoint `OpenAiCompatible` configurations.

**No cloud preset in v1.** A non-loopback endpoint requires an explicit consent toggle that names the host and states that document text leaves the machine; the conversion report records that this consent was given, when, and to which host.

**Thinking control (resolved).** RT §A3's second-order finding — that `--chat-template-kwargs` is a sidecar-specific lever depending on the GGUF's embedded Jinja template, and is not part of the OpenAI-compatible chat/completions contract — is settled by putting the difference behind one trait method. `LlmProvider` exposes **`thinking_control()`**, and each implementation returns the lever it actually has:

| Provider | How thinking is disabled |
|---|---|
| `LocalSidecar` | `chat_template_kwargs: {"enable_thinking": false}` in the `/v1/chat/completions` body |
| Ollama | `think: false` in the request body |
| Generic `OpenAiCompatible` | `/no_think` appended to the shared system prefix, for Qwen-family models |

Because the marker is appended to the **shared system prefix** (§8 rule 1) rather than to the per-call payload, the generic path keeps the prefix byte-identical across all four tasks and therefore keeps prefix reuse intact.

The lever is best-effort; the **check is not**. For *every* provider, any `<think>` content in the response, or any output that does not match the grammar, **fails gate S** and the pipeline falls back to the deterministic answer, which is recorded in the `Decision` with `fallback_used = true`. A provider that silently ignores its knob therefore degrades safely and visibly instead of leaking reasoning tokens into the IR — which is why this is a provider-shape question rather than a correctness risk.

---

## 10. Risks and open questions

1. **Turkish evidence gap.** No source found in either research round covers Turkish dehyphenation, OCR post-correction, or structural classification specifically (R4 open question §3; R10 §9.1; RT throughout). Qwen3's 119-language figure is now verified per-language (the release blog names Turkish and German explicitly — §2, §6.1), while Qwen3.5's 201-language claim remains aggregate-only; but a *named* language is still not a *measured* one — Turkish is the single most likely place the default model's accuracy breaks, and G8's 100-item Turkish probe plus the benchmark plan's language-gated shipping rule (§7) exist specifically because of this gap, not as generic caution.
2. **Calibration does not exist yet.** No paper calibrates deterministic confidence signals for PDF→EPUB (R10 §4.4; RT §A7 calls this "the largest single risk to this architecture"). D17's structural-predicate approach converts this from a blocking dependency into a side effect of the first real usage — but until `eval calibrate` runs against a real gold set, every escalation trigger is a rule, not a tuned threshold, and should be documented as such wherever it appears.
3. **llama.cpp regressions are real and have already happened once.** V1 §4 found a documented 17× throughput regression between two consecutive llama.cpp CPU-backend builds (Gemma-3-1B, b5275→b5276). The nine-gate promotion test (§6.2) re-runs on every llama.cpp version bump for exactly this reason; the same discipline should extend to the *default* model's gates (G4–G6 at minimum), not only to promotion candidates, since a llama.cpp bump can silently blow the dense default's own wall-clock budget.
4. **Community GGUF provenance for the experimental tier.** The Qwen3.5 quants OpenConvert would ship are third-party (unsloth), not official (V1 §1). G9 requires OpenConvert to reproduce the exact GGUF from official safetensors and produce its own SHA-256 before promotion — this is a real, recurring engineering cost (pull the BF16 release, run `convert_hf_to_gguf.py` + `llama-quantize`, verify against the shipped quant) that recurs on every model version and every llama.cpp bump, and is explicitly not required for the dense default, which has an official GGUF (RT §A2/§B14).

---

## Notes for the Chief Architect

Both items raised during this write-up have since been resolved by the Chief Architect and are recorded here as settled:

1. **Qwen3's language coverage is verified.** Checked directly against the Qwen3 release blog and model card on 2026-09-09: the release blog states **119 languages and dialects** and lists **Turkish and German explicitly**; the model card states "100+ languages", gives the license as **apache-2.0**, documents thinking disabled via `enable_thinking=False` or the `/no_think` soft switch, and explicitly names **llama.cpp** among supported runtimes. §6.1's "the Turkish argument doesn't differentiate the two models" reasoning therefore stands on a verified claim, and the figure is safe for user-facing copy provided it is attributed to the release blog rather than the card.
2. **Thinking control is resolved across providers** (§9). `LlmProvider` exposes `thinking_control()`; `LocalSidecar` sends `chat_template_kwargs: {"enable_thinking": false}` on `/v1/chat/completions`; Ollama sends `think: false`; generic OpenAI-compatible endpoints get `/no_think` appended to the shared system prefix for Qwen-family models. For **every** provider, any `<think>` content or non-grammar output fails gate S and the pipeline falls back deterministically — so a provider that silently ignores the knob degrades safely instead of leaking reasoning into the IR.
