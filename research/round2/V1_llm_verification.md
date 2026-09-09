# V1 — LLM/model-runtime claim verification

Verifier: V1 (second-agent check). Method: WebFetch only (WebSearch quota exhausted); direct `curl` to huggingface.co and github.com is blocked by the environment's egress proxy ("organization policy"), so all fetches went through the WebFetch tool, which renders the page and summarizes with a small model — every number below was cross-checked with **at least two independent fetches** (different URL and/or different page section) before being marked VERIFIED, specifically to guard against that summarizer inventing plausible-sounding numbers on a failed/empty render. Today is 2026-09-09; my training cutoff is Jan 2026, so releases after that date are outside my prior knowledge and were evaluated only on fetched evidence + internal consistency.

---

## 1. Qwen3.5 small models — **VERIFIED (exists), with one correction**

Qwen3.5-2B, -0.8B and -4B all **exist** on Hugging Face and are self-consistent across three independent fetch paths (rendered HTML page, raw `README.md`, and the `huggingface.co/api/models/...` JSON endpoint), which returned matching numbers (architecture class `Qwen3_5ForConditionalGeneration`, 201-language claim, IFEval scores) each time — strong evidence this is real page content, not a hallucination.

Caveat: the Qwen **org listing page** (https://huggingface.co/Qwen) did *not* show any Qwen3.5-named repo in its (truncated, trending-sorted) view — it showed newer "Qwen3.8" repos instead. This is not a contradiction of existence, just evidence the org page only surfaces a subset of repos; the individual model-page/README/API fetches for Qwen3.5-2B/0.8B/4B all succeeded and agree with each other.

| Fact | Qwen3.5-0.8B | Qwen3.5-2B | Qwen3.5-4B | Source |
|---|---|---|---|---|
| (a) Exists / release | Yes | Yes, last updated "March 2, 2026" per API, 2.6M+ downloads | Yes | https://huggingface.co/Qwen/Qwen3.5-2B , https://huggingface.co/api/models/Qwen/Qwen3.5-2B , https://huggingface.co/Qwen/Qwen3.5-0.8B , https://huggingface.co/Qwen/Qwen3.5-4B |
| (b) License | Apache-2.0 | Apache-2.0 (confirmed via raw `LICENSE` file: standard Apache License 2.0 text, "Copyright 2026 Alibaba Cloud") | Apache-2.0 | https://huggingface.co/Qwen/Qwen3.5-2B/raw/main/LICENSE |
| (c) Architecture | Causal LM w/ vision encoder; **hybrid Gated Delta Networks + sparse MoE** | same | same | https://huggingface.co/Qwen/Qwen3.5-2B/raw/main/README.md |
| Params (total) | 0.8B | 2B | 4B (active/total split not stated on page) | model cards |
| (d) Thinking-mode default | **Non-thinking** by default | **Non-thinking** by default | **Thinking enabled** by default (`enable_thinking=True` default; disable via `chat_template_kwargs: {"enable_thinking": false}`) | model cards |
| (e) IFEval | non-think 52.1 / think 44.0 | non-think 61.2 / think 78.6 | non-think/instruct 89.8 (no separate think number surfaced) | model cards |
| Multilingual | 201 languages/dialects claimed (no per-language list surfaced; Turkish/German not individually confirmed on the pages fetched) | same | "expanded... to 201 languages and dialects" | model cards |
| (f) Context length | 262,144 native | 262,144 native | 262,144 native, extensible to ~1,010,000 via RoPE scaling | model cards |
| (g) Official GGUF | — | **`Qwen/Qwen3.5-2B-GGUF` returns HTTP 401** (not publicly listed/does not exist as an open repo) — Qwen does **not** appear to publish an official GGUF repo for this model | — | https://huggingface.co/Qwen/Qwen3.5-2B-GGUF (401) |
| GGUF from community | — | `unsloth/Qwen3.5-2B-GGUF` exists; **Q4_K_M = 1.28 GB** (also Q8_0 2.01GB, BF16 3.78GB, UD-Q4_K_XL 1.34GB) | — | https://huggingface.co/unsloth/Qwen3.5-2B-GGUF/tree/main |
| (h) llama.cpp note | Searched full README text for "llama.cpp" / "GGUF" — **no mention found**; deployment section only lists SGLang, vLLM, KTransformers, Transformers | | | https://huggingface.co/Qwen/Qwen3.5-2B/raw/main/README.md |

**Verdict: VERIFIED that Qwen3.5-2B/0.8B/4B exist, are Apache-2.0, and match the architecture/context/thinking-default claims.** One important correction to earlier research: the model cards make **no reference to llama.cpp support at all**, and Qwen does not publish official GGUFs for these — only third-party (unsloth) GGUFs exist. Any claim of "Qwen confirms/recommends llama.cpp" or "official GGUF" for Qwen3.5 should be treated as **CONTRADICTED**; llama.cpp support depends on the community having added the `Qwen3_5ForConditionalGeneration` (hybrid Gated-DeltaNet+MoE) architecture, which is a nontrivial new arch, not automatically supported. **This llama.cpp-support gap is itself unverified** — I could not find a changelog entry confirming llama.cpp merged support for the Qwen3.5 architecture — flag as **[COULD NOT VERIFY — llama.cpp Qwen3.5 arch support]**.

Multilingual per-language claims (b): the "Turkish/German named" sub-claim is **[COULD NOT VERIFY]** — pages only gave the aggregate "201 languages/dialects" figure; no enumerated language list was surfaced by any fetch.

---

## 2. Gemma 4 license — **VERIFIED: Apache-2.0 (changed from Gemma Terms)**

Four independent fetches agree:

1. `huggingface.co/google/gemma-4-E2B` rendered page → license metadata shown as `apache-2.0`.
2. Raw YAML frontmatter of that model's `README.md`:
   ```yaml
   library_name: transformers
   license: apache-2.0
   license_link: https://ai.google.dev/gemma/docs/gemma_4_license
   pipeline_tag: any-to-any
   ```
   (https://huggingface.co/google/gemma-4-E2B/raw/main/README.md)
3. `ai.google.dev/gemma/terms` — banner: *"Gemma 4 released with text, audio and image input and long up to 256K context window!"* and explicit note: *"For Gemma 4 terms, see the Gemma 4 license."* (https://ai.google.dev/gemma/terms)
4. `ai.google.dev/gemma/docs/gemma_4_license` — page title **"Apache License 2.0 | Gemma | Google AI for Developers"**, body is the standard Apache-2.0 text (Jan 2004), with separate (non-license) "Prohibited use policy" and "Intended use statement" pages linked from the nav, same pattern as other Apache-licensed models that carry usage guidelines outside the license itself. (https://ai.google.dev/gemma/docs/gemma_4_license)

Contrast/calibration check: `huggingface.co/google/gemma-3-1b-it` license metadata is `license: gemma` (the older custom Gemma Terms of Use), confirming the tool correctly distinguishes the two and this isn't a copy-paste artifact. (https://huggingface.co/google/gemma-3-1b-it)

**Verdict: VERIFIED — Gemma 4 (per gemma-4-E2B card) is licensed Apache-2.0, a change from Gemma 1–3's custom "Gemma Terms of Use."** This resolves round-1 contradiction #1 in favor of R4's claim; R8's claim ("Gemma 3/3n/4 all under Gemma Terms") is **CONTRADICTED** for Gemma 4 specifically — R8 likely only checked the generic `/gemma/terms` page and didn't follow through to the Gemma-4-specific license doc or the model card. Note the license file is still hosted at a Google-branded URL and additional "Prohibited use policy"/"Intended use" documents exist alongside it — worth a follow-up read of those two policy pages before treating Gemma 4 as unencumbered Apache-2.0 for commercial redistribution, since Google could in principle fold restrictions into the "intended use" doc even while the license text itself is Apache-2.0. **[COULD NOT VERIFY — content of the separate Prohibited-Use/Intended-Use pages]**, not fetched (out of scope of the literal license question).

---

## 3. llama.cpp release binaries — **VERIFIED**

Latest release at fetch time: **`b10456`**, dated 2026-08-17 (llama.cpp uses sequential `b<N>` build tags, not semver; the 5 most recent releases at fetch time were b10456, b10455, b10453, b10452, b10451, all mid-August 2026 — consistent with the project's continuous near-daily release cadence). Source: https://github.com/ggml-org/llama.cpp/releases and https://github.com/ggml-org/llama.cpp/releases/tag/b10456 (both fetches independently listed the same tag/assets).

Full asset list for b10456 (from `.../releases/expanded_assets/b10456`, with sizes), confirming OS/arch/backend variants:

| Asset | Size |
|---|---|
| llama-b10456-bin-win-cpu-x64.zip | 17.6 MB |
| llama-b10456-bin-win-cpu-arm64.zip | 11.7 MB |
| llama-b10456-bin-win-cuda-12.4-x64.zip (+ cudart) | 239 MB |
| llama-b10456-bin-win-cuda-13.3-x64.zip (+ cudart) | 140 MB |
| llama-b10456-bin-win-cuda-13.4-arm64.zip (+ cudart) | 134 MB |
| llama-b10456-bin-win-vulkan-x64.zip | 33.2 MB |
| llama-b10456-bin-win-opencl-adreno-arm64.zip | 12.3 MB |
| llama-b10456-bin-win-openvino-2026.2.1-x64.zip | 77 MB |
| llama-b10456-bin-win-sycl-x64.zip | 114 MB |
| llama-b10456-bin-win-rocm-7.14-x64.zip | 188 MB |
| llama-b10456-bin-macos-arm64.tar.gz | 10.6 MB |
| llama-b10456-bin-macos-x64.tar.gz | 10.9 MB |
| llama-b10456-xcframework.zip | 273 MB |
| llama-b10456-bin-ubuntu-x64.tar.gz | 15.9 MB |
| llama-b10456-bin-ubuntu-arm64.tar.gz | 12.9 MB |
| llama-b10456-bin-ubuntu-s390x.tar.gz | 15.1 MB |
| llama-b10456-bin-ubuntu-vulkan-x64.tar.gz | 31.7 MB |
| llama-b10456-bin-ubuntu-vulkan-arm64.tar.gz | 26 MB |
| llama-b10456-bin-ubuntu-openvino-2026.2.1-x64.tar.gz | 97.3 MB |
| llama-b10456-bin-ubuntu-sycl-fp32/fp16-x64.tar.gz | ~51 MB |
| llama-b10456-bin-android-arm64.tar.gz | 74.2 MB |
| llama-b10456-ui.tar.gz | 2.94 MB |

**Note (important for packaging plans): there is only ONE Windows/Linux CPU binary per arch** (`win-cpu-x64`, `ubuntu-x64`) — **no separate avx/avx2/avx512 variants** in this release, unlike older llama.cpp releases circa 2023–2024 which shipped avx/avx2/avx512 as separate zips.

This matches `docs/build.md`'s / `ggml/src/CMakeLists.txt`'s **`GGML_CPU_ALL_VARIANTS`** option, confirmed present verbatim in the build system:
```cmake
if (GGML_CPU_ALL_VARIANTS)
    if (NOT GGML_BACKEND_DL)
        message(FATAL_ERROR "GGML_CPU_ALL_VARIANTS requires GGML_BACKEND_DL")
```
(https://raw.githubusercontent.com/ggml-org/llama.cpp/master/ggml/src/CMakeLists.txt). When enabled, the build produces per-ISA CPU backend variants (SSE4.2 through AVX-512, Sandy-Bridge-through-Sapphire-Rapids tuned, plus ARM/PowerPC/s390x/RISC-V equivalents) as **dynamically-loadable modules selected automatically at runtime** based on detected CPU features — i.e. one official `win-cpu-x64` / `ubuntu-x64` binary does handle AVX/AVX2/AVX512 dispatch at runtime, without the user picking a build. `docs/build.md` itself (rendered + raw) confirmed `GGML_BACKEND_DL` ("Backends can be built as dynamic libraries loaded at runtime... use the same llama.cpp binary on different machines with different GPUs") but **did not itself contain the string "GGML_CPU_ALL_VARIANTS"** in either of two fetches — that detail lives in the CMake source, not the prose doc. **Verdict: VERIFIED via CMakeLists.txt (primary source), not via build.md prose.**

I could not directly confirm each individual archive's *internal contents* (i.e., that `llama-server` is inside every zip) — GitHub's asset list only exposes filenames/sizes, not archive contents, without downloading. **[COULD NOT VERIFY — llama-server presence per-archive]**, though this matches long-standing, well-established llama.cpp release conventions (every `bin-*` archive ships `llama-cli`, `llama-server`, `llama-bench`, etc. together) and is consistent with prior knowledge.

Server flags — **VERIFIED**, confirmed via two independent fetches (rendered GitHub blob + raw.githubusercontent.com) of `tools/server/README.md`, both returning matching text:

| Flag | Description (verbatim) |
|---|---|
| `--api-key` | "API key to use for authentication, multiple keys can be provided as a comma-separated list (default: none)" |
| `--cache-prompt` / `--no-cache-prompt` | "whether to enable prompt caching (default: enabled)" |
| `--cache-reuse N` | "min chunk size to attempt reusing from the cache via KV shifting, requires prompt caching to be enabled (default: 0)" |
| `-np, --parallel N` | "number of server slots (default: -1, -1 = auto)" |
| `--grammar-file FNAME` | "file to read grammar from" |
| `json_schema` (request field) | "Set a JSON schema for grammar-based sampling" — passed in the `/completion` (and by extension chat/completions) request body |
| `--chat-template-kwargs` | "sets additional params for the json template parser, must be a valid json object string, e.g. `{"key1":"value1"}`" — this is the mechanism to pass `enable_thinking=false` (as `{"enable_thinking": false}`), matching the Qwen3.5-4B model card's own documented invocation |
| `--host` | "ip address to listen, or bind to a UNIX socket if the address ends with .sock (default: 127.0.0.1)" |
| `--port` | "port to listen (default: 8080)" |
| `GET /health` | Returns HTTP 200 with `{"status":"ok"}` when the model is ready |

---

## 4. CPU throughput anchors — **CONTRADICTED / clarified: the "~96 tok/s prefill" claim is a misreading**

The two discussion URLs supplied did **not** contain the needed pp512/tg128 tables directly, but discussion **#13664 is in fact the exact source of the "~96 tok/s Gemma-3-1B" claim**, and reading it precisely overturns that claim:

> Model: **gemma-3-1b-it-Q4_K_M.gguf**, CPU: **Intel Core i7-12700H**, 14 threads.
> - Build b5275 (AVX2-optimized): **61.90 ms/token → ≈16.16 tok/s**
> - Build b5276 (regressed CPU-x64 build): **1043.79 ms/token → ≈0.96 tok/s**
> - Build b5432 (still regressed): 1132.96 ms/token → ≈0.88 tok/s

(https://github.com/ggml-org/llama.cpp/discussions/13664)

Two things went wrong in the earlier "~96 tok/s prefill" claim: (1) the figure is **0.96 tok/s**, not 96 — almost certainly a dropped decimal point; and (2) even the correct reading is a **token-generation (`tg`, eval) number**, not **prefill/prompt-processing (`pp512`)** — the discussion is entirely about `ms/token` during generation on a regressed (non-SIMD) CPU backend build, not batched prompt processing. The working, non-regressed AVX2 build's generation speed for this exact model/CPU is **~16 tok/s**, and no prefill number appears in that thread at all. **Verdict on the "~96 tok/s prefill" claim: CONTRADICTED** — it conflates a mis-transcribed decimal, a bugged/regressed build, and generation-vs-prefill.

I could not find pp512/tg128 llama-bench tables for 1–4B Q4 models on x86 CPUs in the two given discussions, on openbenchmarking.org's `pts/llama-cpp` test page (which stated results exist but weren't surfaced as concrete numbers on the page I fetched — **[COULD NOT VERIFY — direct pp512/tg128 numbers for small models]**), or via a GitHub Apple-Silicon benchmark megathread (discussion #4167), which **only contains 7B-model data**:

| Chip | Model | Quant | pp512 t/s | tg128 t/s | Source |
|---|---|---|---|---|---|
| M1 Pro (16-core GPU) | LLaMA 7B | Q4_0 | 266.25 | 36.41 | github.com/ggml-org/llama.cpp/discussions/4167 |
| M2 Ultra (76-core GPU) | LLaMA 7B | Q4_0 | 1238.48 | 94.27 | same |
| M4 Max (40-core GPU) | LLaMA 7B | Q4_0 | 885.68 | 83.06 | same |
| M5 Pro (20-core GPU) | LLaMA 7B | Q4_0 | 1620.64 | 66.33 | same |

Also confirmed the **general pp-vs-tg magnitude gap** from llama.cpp's own `llama-bench` README examples (7B Q4_0 on CUDA: pp512 ≈2,369–2,400 t/s vs tg128 ≈131 t/s — a ~18–20x gap), which supports the general shape of the reconciliation below but is GPU, not CPU. (https://raw.githubusercontent.com/ggml-org/llama.cpp/master/tools/llama-bench/README.md)

**Best evidence-backed estimate (extrapolated, not directly sourced — mark as estimate, not verified fact):**

- **(a) 8-core AVX2 x86 laptop, ~2B Q4_K_M dense transformer:** pp512 in the **~400–1,500 tok/s** range is plausible (compute-bound, scales roughly with core count × AVX2 throughput ÷ active params; a 1B-class model on a similar-generation Intel laptop chip with a *working, non-regressed* AVX2 build was measured at ~16 tok/s for **generation**, and pp512 for small dense models is routinely one to two orders of magnitude above tg on CPU per the CUDA example's ~18–20x pp:tg ratio pattern, though the ratio is typically smaller on CPU than GPU because CPU token-generation is even more memory-bandwidth-starved relative to its own compute ceiling). tg128 for the same setup: **~15–35 tok/s** (anchored directly to the confirmed ~16 tok/s figure for a same-class 1B model on i7-12700H with a correct AVX2 build).
- **(b) Apple M-series base chip (e.g., M1/M2/M3, CPU-only vs. Metal), ~2B Q4_K_M:** scaling the confirmed 7B-model M1 Pro numbers (pp512 266 t/s, tg128 36 t/s with Metal/GPU) down by roughly the 7B→2B parameter ratio (~3.5x, since pp is compute/FLOPs-bound and roughly inverse-proportional to active params) gives an estimated **pp512 ≈ 700–1,000 tok/s and tg128 ≈ 50–90 tok/s on Metal** for a base M-series chip; CPU-only (no Metal) figures would be substantially lower for pp (Metal GPU pp/tg dominates CPU on Apple Silicon), plausibly **pp512 in the low hundreds and tg128 in the 10–25 tok/s** range on CPU-only. **These (b) figures are extrapolations from 7B-model data, not directly measured for a 2B model — treat as estimate, not verified.**

This reconciles the two conflicting round-1 claims: **"~96 tok/s prefill for Gemma-3-1B"** is simply wrong (should be ~16 tok/s *generation*, not prefill, on a working build), while **"~1,200–2,000 tok/s prefill for a 2B model"** is in the right order of magnitude for prompt-processing specifically (compute-bound, high-throughput), particularly on stronger/newer CPUs (e.g., Ryzen AI 9 HX 370 cited in round 1) — prefill (pp) and generation (tg) are simply very different regimes and should not be quoted interchangeably.

---

## 5. Ollama structured outputs + defaults — **VERIFIED**

- **`format` JSON-schema support:** confirmed. Ollama's blog post (published **2024-12-06**) states: *"Ollama now supports structured outputs making it possible to constrain a model's output to a specific format defined by a JSON schema,"* with a working `curl` example passing a full JSON-schema object as the `format` field to `/api/chat`. (https://ollama.com/blog/structured-outputs)
- **Default context window:** *"By default, Ollama uses a context window size of 2048 tokens"* — **confirmed 2048, not 4096.** (https://github.com/ollama/ollama/blob/main/docs/faq.md)
- **Default `keep_alive`:** *"By default models are kept in memory for 5 minutes before being unloaded."* — **confirmed 5 minutes.** (same URL)

---

## Summary table

| # | Claim | Verdict |
|---|---|---|
| 1a–c | Qwen3.5-2B/0.8B/4B exist, Apache-2.0, hybrid Gated-DeltaNet+MoE dense-ish arch | VERIFIED |
| 1d–g | thinking defaults, IFEval, 262K context, 201 languages | VERIFIED (Turkish/German not individually named — COULD NOT VERIFY that sub-point) |
| 1g | Official Qwen GGUF exists | CONTRADICTED — no official GGUF repo (401); only unsloth community GGUF (Q4_K_M 1.28GB) |
| 1h | llama.cpp support / required version noted on model card | CONTRADICTED — README never mentions llama.cpp/GGUF at all; whether llama.cpp code supports the arch is COULD NOT VERIFY |
| 2 | Gemma 4 = Apache-2.0 | VERIFIED (4 independent sources); resolves round-1 contradiction in favor of R4 |
| 3 | llama.cpp official multi-OS binaries incl. server, with runtime CPU-ISA dispatch (GGML_CPU_ALL_VARIANTS) | VERIFIED |
| 3 | Server flags (--api-key, --cache-reuse, --cache-prompt, -np, --grammar-file, json_schema, --chat-template-kwargs, --host/--port, /health) | VERIFIED |
| 4 | "~96 tok/s prefill for Gemma-3-1B Q4 on i7-12700H" | CONTRADICTED — actually ~0.96 tok/s (regressed build) or ~16 tok/s (fixed build), and it's a generation (tg) number, not prefill (pp) |
| 4 | "~1,200–2,000 tok/s prefill for a 2B model" | Plausible / right order of magnitude for pp512 on strong CPUs, but not directly re-verified for a 2B model — estimate only |
| 5 | Ollama `format` JSON-schema, num_ctx=2048 default, keep_alive=5min | VERIFIED |
