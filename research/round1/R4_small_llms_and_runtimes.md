# R4 — Small Local LLMs & Runtimes for OpenConvert (PDF → reflowable EPUB)

**Research round 1 · Compiled 2026-09-09 · Research only, nothing implemented.**

Scope: a local, CPU-only LLM used *sparingly* for narrow structural decisions (block
classification, reading-order repair, chapter boundaries, paragraph merge/split,
footnote/caption association, suspicious-output detection) on a typical 8–16 GB laptop.
Inputs = compact serialized text + geometry JSON; outputs = strict JSON.

**Evidence conventions.** Every non-obvious claim carries a source link (full list at the
end). Claims I could not verify against a primary source are tagged `[UNVERIFIED]`.
Numbers I computed from other cited numbers are tagged `[DERIVED]` with the derivation shown.

> **Headline caveat that shapes everything below.** On laptop *CPU*, decode is roughly
> 10–30× more expensive per token than prefill. Google's own on-device numbers for a 4B-class
> model on CPU are ~173–277 tok/s prefill and ~17–27 tok/s decode
> ([LiteRT-LM Gemma 4 benchmarks](https://developers.google.com/edge/litert-lm/models/gemma-4)).
> This means the architecture question is not "which model is smartest" but "how few output
> tokens can we get away with, and on how few pages can we afford to call the model at all."

---

## Part A — Candidate models

### A.0 What actually exists in the 0.3B–4B class as of September 2026

The 2026 landscape shifted twice since the 2025 generation:

1. **Qwen3.5 small series** (0.8B / 2B / 4B / 9B, Apache-2.0) landed Feb–Mar 2026 and is the
   first small family that is *natively multimodal* and explicitly benchmarked on document
   parsing (OmniDocBench 1.5). Later Qwen releases (3.6 in April 2026, 3.8 in August 2026)
   **did not ship small models** — the smallest open Qwen3.8 is 27B
   ([Qwen3.8 repo](https://github.com/QwenLM/Qwen3.8), [Wikipedia: Qwen](https://en.wikipedia.org/wiki/Qwen)).
   So Qwen3.5 *is* the current Qwen small tier, not a stale one.
2. **Gemma 4** (Mar 31 / Apr 2 2026) is the first Gemma released under **Apache-2.0** rather
   than the Gemma Terms of Use
   ([Google blog](https://blog.google/innovation-and-ai/technology/developers-tools/gemma-4/),
   [ai.google.dev](https://ai.google.dev/gemma/docs/core),
   [the-decoder](https://the-decoder.com/googles-gemma-4-is-now-available-with-apache-2-0-licensing-for-the-first-time/)).
   That removes the single biggest legal blocker to bundling Gemma in an OSS app.
3. **Liquid AI LFM2.5** (1.2B Jan 2026, 2.6B Aug 2026) is the strongest *instruction-following-
   per-CPU-cycle* option, but ships under a **non-OSI revenue-capped license**, which is a
   problem for an open-source project (details in A.4).

Explicitly checked and **ruled out of the 0.3–4B tier**:

| Family | Why it is out |
|---|---|
| **Llama 4** | Smallest open variant is Scout 17B-16E (109B total). No 1B/3B in the Llama 4 line. ([Llama 4 Scout card](https://www.prompthub.us/models/llama-4-scout)) |
| **gpt-oss** | Smallest is gpt-oss-20b (Apache-2.0). Useful *larger* reference only. ([OpenAI model card](https://openai.com/index/gpt-oss-model-card/)) |
| **Nemotron Nano** | 9B v2 is the small one — larger tier, NVIDIA Open Model License. ([HF card](https://huggingface.co/nvidia/NVIDIA-Nemotron-Nano-9B-v2)) |
| **Phi** | Phi-4-mini 3.8B (MIT) still the small entry; the 2026 release was Phi-4-reasoning-vision-**15B**. No credible "Phi-5-mini" found. ([Wikipedia: Phi](https://en.wikipedia.org/wiki/Phi_(language_model)), [Phi-4-mini card](https://huggingface.co/microsoft/Phi-4-mini-instruct)) |
| **Mistral Large 3** | 675B total / 41B active — server class. ([Mistral 3 announcement](https://mistral.ai/news/mistral-3)) |
| **EXAONE** | Could not verify a current 2026 small release within budget. `[UNVERIFIED]` |

### A.1 Comparison table — primary candidates

Sizes are Q4_K_M GGUF unless noted. "RAM @ Q4, 8k ctx" = weights + KV + runtime overhead.

| Model | Params (active) | Released | License | Ctx | Q4_K_M file | RAM @Q4/8k | IFEval | Doc evidence | Vision | Thinking toggle | GGUF | Ollama |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| **Qwen3.5-2B** | 2B, hybrid DeltaNet + sparse MoE (active params not published) | Feb–Mar 2026 | Apache-2.0 | 262,144 | **1.28 GB** | ~1.6–1.9 GB `[DERIVED]` | 61.2 (non-think) | **OmniDocBench1.5 79.8**, OCRBench 84.5, MMLongBench-Doc 45.4 | yes (+668 MB mmproj) | non-think default, `enable_thinking` | yes | `qwen3.5:2b` 2.7 GB |
| **Qwen3.5-4B** | 4B, same hybrid | Feb–Mar 2026 | Apache-2.0 | 262,144 | **2.74 GB** | ~3.1–3.5 GB `[DERIVED]` | **89.8** (think default) | **OmniDocBench1.5 86.2**, OCRBench 85.0, MMLongBench-Doc 54.2 | yes | think ON by default → must disable | yes | `qwen3.5:4b` 3.4 GB |
| **Qwen3.5-0.8B** | 0.8B hybrid | Feb–Mar 2026 | Apache-2.0 | 262,144 | ~0.6 GB `[UNVERIFIED]` | ~0.9 GB `[DERIVED]` | 52.1 | MMMU 49 (no OmniDocBench published) | yes | non-think default | yes | `qwen3.5:0.8b` 1.0 GB |
| **Gemma 4 E2B** | 2.3B effective / **5.1B with embeddings** | 2026-03-31 | **Apache-2.0** | 128K | 2.58 GB (LiteRT) | 0.74 GB (macOS) – **3.5 GB (Windows)** measured | not published | none published | text+image+audio | configurable | official **QAT GGUF** | `gemma4:e2b` **7.2 GB** |
| **Gemma 4 E4B** | 4.5B effective / **8B with embeddings** | 2026-03-31 | **Apache-2.0** | 128K | 3.65 GB (LiteRT) | 0.89 GB (macOS) – **9.4 GB (Windows)** measured | not published | none published | text+image+audio | configurable | official QAT GGUF | `gemma4:e4b` **9.6 GB** |
| **LFM2.5-1.2B** | 1.2B, conv+GQA hybrid | 2026-01-05 | **LFM Open License v1.0** (revenue-capped) | 128K `[UNVERIFIED]` | 0.86 GB (Q4_0, official) | ~0.9–1.1 GB | **86.23** | none published | no (VL variant separate) | n/a | yes, day-one | community |
| **LFM2.5-2.6B** | 2.69B (22 conv + 8 GQA blocks) | 2026-08-06 | **LFM1.0** (revenue-capped) | 131,072 | <2.5 GB total (official) | ~2.5 GB | IFStruct 85.49, Multi-IF 80.07, IFBench 59.17 | none published | no | n/a | yes, day-one | community |
| **Ministral 3 3B** | 3.4B LM + 0.4B vision = 4B | 2025-12-02 | Apache-2.0 | 256K | ~2.2 GB `[DERIVED]` | ~2.6 GB `[DERIVED]` | not published (MMLU 70.7) | none published | yes | n/a | not confirmed on card | not confirmed |
| **Granite 4.0 Micro 3B** | 3B dense (non-hybrid variant exists specifically for llama.cpp) | 2025-10-02 | Apache-2.0 | 128K validated | 2.1 GB (Ollama) | ~2.5 GB `[DERIVED]` | family claims HELM IFEval lead (H-Small) | none published | no | n/a | yes | `granite4:3b` 2.1 GB |
| **SmolLM3-3B** | 3B dense | 2025 | Apache-2.0 | 64K (128K YARN) | ~1.9 GB `[UNVERIFIED]` | ~2.3 GB `[DERIVED]` | **76.7 no-think** / 71.2 think | none published | no | `/no_think` flag | yes (ggml-org) | yes |
| **Falcon-H1 1.5B / 3B** | 1.5B / 3B hybrid | 2025 | TII Falcon LLM License `[UNVERIFIED]` | — | — | — | — | none | no | — | some GGUF | — |

**Larger comparison tier (7–9B), for quality ceiling reference only:**

| Model | Params | License | IFEval | Doc evidence | Q4 size | Notes |
|---|---|---|---|---|---|---|
| Qwen3.5-9B | 9B hybrid MoE | Apache-2.0 | **91.5** | OCRBench 89.2, MMLU-Redux 91.1, MMMLU 81.2 | ~6.6 GB (Ollama) | thinking on by default |
| Gemma 4 12B "unified" | 12B | Apache-2.0 | — | — | 7.6 GB (Ollama) | encoder-free multimodal |
| gpt-oss-20b | 20B / 3.6B active | Apache-2.0 | — | — | ~12 GB `[UNVERIFIED]` | too big for 8 GB laptops |
| Nemotron-Nano-9B-v2 | 9B | NVIDIA Open Model License | — | — | — | toggleable reasoning |

### A.2 Licensing / bundling analysis (the part that constrains an OSS app)

| License | Bundle in installer? | Auto-download from app? | Attribution | Verdict for OpenConvert |
|---|---|---|---|---|
| **Apache-2.0** (Qwen3.5, Gemma 4, Granite 4, SmolLM3, Ministral 3, gpt-oss, Granite-Docling, SmolDocling) | Yes | Yes | Retain NOTICE / license file, state modifications | **Clean.** Ship the license text next to the GGUF; note the file is a "Derivative Work" if you re-quantize. |
| **LFM Open License v1.0 / LFM1.0** (LFM2, LFM2.5) | Legally yes, but | commercial rights are **conditioned on the user's org being under a $10M annual-revenue threshold** | attribution + NOTICE + mark modified files | **Avoid as the shipped default.** An OSS app cannot police downstream users' revenue; bundling would push a licensing condition onto every corporate user. Fine as an *opt-in* download. |
| **Gemma Terms of Use** (Gemma ≤3) | conditional | conditional | prohibited-use policy must flow down | Superseded — Gemma 4 is Apache-2.0, so this no longer applies to the current generation. |
| **Llama Community License** | conditional | conditional | "Built with Llama", 700M MAU cap | Moot: no Llama 4 model in our size class. |
| **NVIDIA Open Model License** (Nemotron) | conditional | conditional | — | Larger tier only; not needed. |

**Practical bundling recommendation:** do **not** put weights in the installer. Ship a small
model-manager that downloads a pinned GGUF (with SHA-256) from Hugging Face on first use,
displays the license, and writes the license + NOTICE into the app's model directory. This
keeps the installer small, keeps signing simple, and makes license compliance auditable.

### A.3 Instruction following and strict-JSON reliability

Direct evidence, best first:

- **Qwen3.5-4B IFEval 89.8**, MMLU-Redux 88.8 ([card](https://huggingface.co/Qwen/Qwen3.5-4B)).
  Caveat: the 4B card documents thinking mode as the default and does not publish a separate
  non-thinking table, so **89.8 should be read as a thinking-mode number**. Our latency budget
  forbids thinking mode, so treat the usable figure as materially lower. `[DERIVED caution]`
- **Qwen3.5-2B IFEval 61.2, explicitly non-thinking** ([card](https://huggingface.co/Qwen/Qwen3.5-2B)).
  This is the honest apples-to-apples number for our deployment mode, and it is *mediocre*.
- **LFM2.5-1.2B IFEval 86.23** vs Llama-3.2-1B's 52.37
  ([Liquid AI](https://www.liquid.ai/blog/introducing-lfm2-5-the-next-generation-of-on-device-ai)) —
  the best instruction-following-per-byte in the class by a wide margin, and LFM2.5-2.6B adds
  **IFStruct 85.49** and **Multi-IF 80.07**
  ([MarkTechPost](https://www.marktechpost.com/2026/08/06/liquid-ai-lfm2-5-2-6b-on-device-agentic-model/)).
- **SmolLM3-3B IFEval 76.7 without thinking** — notably *better* than with thinking (71.2)
  ([card](https://huggingface.co/HuggingFaceTB/SmolLM3-3B)). Useful datapoint: reasoning modes
  do not automatically help format compliance.
- **Ministral 3 3B** and **Gemma 4** both advertise native function calling / structured JSON
  output ([Ministral 3 card](https://huggingface.co/mistralai/Ministral-3-3B-Instruct-2512),
  [ai.google.dev](https://ai.google.dev/gemma/docs/core)) but neither publishes IFEval.

**The key mitigation is that we should not rely on the model's unaided JSON discipline at all.**
With grammar-constrained decoding, *syntactic* validity is guaranteed by construction (Part B),
so IFEval mainly predicts *semantic* obedience — did it label the right blocks, did it emit
one label per input block. Design the schema so that arity is enforced by the grammar too
(fixed-length arrays are expressible in GBNF), and the remaining failure mode is a wrong label,
not malformed output.

Evidence on whether constraining hurts quality is genuinely contested:

- **Against:** "Let Me Speak Freely?" (arXiv [2408.02442](https://arxiv.org/abs/2408.02442))
  reports "a significant decline in LLM reasoning abilities under format restrictions," worse
  with tighter constraints.
- **For:** the .txt team's [rebuttal](https://blog.dottxt.ai/say-what-you-mean.html) reproduced
  those tasks with matched prompts and found structured ≥ unstructured everywhere
  (GSM8K 0.78 vs 0.77; Last Letter 0.77 vs 0.73; Shuffle Object 0.44 vs 0.41), attributing the
  original result to prompt mismatch and a flawed AI-based answer parser.
- **Tooling maturity:** JSONSchemaBench (arXiv [2501.10868](https://arxiv.org/abs/2501.10868))
  evaluates Guidance, Outlines, llama.cpp, XGrammar, OpenAI and Gemini across 10K real schemas
  for coverage, efficiency and quality — the right benchmark to consult before finalizing a
  schema, since llama.cpp's own JSON-schema subset has documented gaps (Part B).

For our workload the classification tasks involve little chain-of-thought, so the .txt position
is likely the operative one — but this should be measured, not assumed (see Part D).

### A.4 Multilingual: specific German and Turkish evidence

This is where candidates separate sharply. Turkish is the harder constraint.

| Model | German | Turkish | Evidence |
|---|---|---|---|
| **Qwen3.5 (all sizes)** | covered | **covered** | "expanded support to **201 languages and dialects**"; 4B MMMLU 76.1, MMLU-ProX 71.5, WMT24++ 66.6; 2B MMMLU 56.9, WMT24++ 45.8; 9B MMMLU 81.2 (29 langs). No per-language TR/DE breakdown published. ([4B](https://huggingface.co/Qwen/Qwen3.5-4B), [2B](https://huggingface.co/Qwen/Qwen3.5-2B), [9B](https://huggingface.co/Qwen/Qwen3.5-9B)) |
| **Gemma 4** | covered | likely covered | "140+ languages"; E4B **MMMLU 76.6**. No language list published on the card. ([E4B card](https://huggingface.co/google/gemma-4-E4B)) |
| **LFM2.5-2.6B** | **explicitly listed** | **explicitly absent** | Language list: EN, AR, ZH, FR, DE, IT, JA, KO, PT, ES, VI, TH, ID, HI, RU, PL — Turkish is not in it. ([card](https://huggingface.co/LiquidAI/LFM2.5-2.6B)) |
| **SmolLM3-3B** | **native (1 of 6)** | **absent** | Native: EN, FR, ES, DE, IT, PT; plus AR/ZH/RU with fewer tokens. Global MMLU 53.5. ([card](https://huggingface.co/HuggingFaceTB/SmolLM3-3B)) |
| **Ministral 3 3B** | **explicitly listed** | not listed (card says "plus dozens more") | Multilingual MMLU 0.652. ([card](https://huggingface.co/mistralai/Ministral-3-3B-Instruct-2512)) |
| **Granite-Docling-258M** | **absent** | **absent** | English primary; experimental JA/AR/ZH only. ([card](https://huggingface.co/ibm-granite/granite-docling-258M)) |

There is a Turkish-specific benchmark, **TR-MMLU** (arXiv [2501.00593](https://arxiv.org/abs/2501.00593),
6,200 MCQs / 62 sections / 67 disciplines), which explicitly flags **tokenization** as a critical
factor for Turkish — relevant because Turkish agglutination inflates token counts and therefore
CPU prefill cost. I could not retrieve small-model TR-MMLU scores within budget. `[UNVERIFIED]`

**Important nuance for our tasks:** we are not asking the model to *generate* German or Turkish.
We ask it to classify blocks whose text is German or Turkish. The requirement is therefore
mostly (a) a tokenizer that doesn't explode on Turkish, and (b) enough lexical familiarity to
recognize "Inhaltsverzeichnis", "Fußnote", "Kapitel", "İçindekiler", "Kaynakça", "Bölüm".
Qwen3.5's 248,320-token vocabulary and 201-language claim make it the safest bet;
SmolLM3 and LFM2.5 are the riskiest for Turkish.

### A.5 Reasoning / "thinking" modes and whether they can be disabled

Latency-critical. Thinking mode multiplies output tokens, and output tokens are the expensive
thing on CPU.

| Model | Default | Disable mechanism | Notes |
|---|---|---|---|
| Qwen3.5-4B / 9B | **thinking ON** | `enable_thinking: False` (chat-template flag); `/no_think` in some deployments | Must be disabled explicitly. Artificial Analysis notes Qwen3.5 small models "use significantly more output tokens (230–390M)" than larger siblings to reach their scores — i.e. they are token-hungry reasoners. ([AA](https://artificialanalysis.ai/articles/qwen3-5-small-models)) |
| Qwen3.5-0.8B / 2B | **non-thinking** | already off | Good default for us. |
| Gemma 4 | "configurable thinking modes" | per ai.google.dev; exact flag not documented on the card `[UNVERIFIED]` | |
| SmolLM3 | thinking on | `/no_think` in system prompt, or `enable_thinking=False` | IFEval is *better* with thinking off (76.7 vs 71.2). |
| LFM2.5 | no thinking mode | n/a | Straight instruction model. |

**Design consequence:** the runtime must let us pin the chat template's thinking flag *and*
verify it, because a silently-enabled `<think>` block would blow the latency budget by 5–20×.
This argues for direct llama.cpp control over the template rather than an opaque Modelfile.

### A.6 Measured CPU throughput (prefill and generation)

I could not find published llama.cpp CPU `pp`/`tg` numbers for Qwen3.5 or Gemma 4 specifically.
What I do have are two trustworthy first-party anchor sets plus the structure to extrapolate.

**Anchor 1 — Google, LiteRT-LM, CPU backend** ([source](https://developers.google.com/edge/litert-lm/models/gemma-4)).
These are CPU-only rows, prefill / decode in tok/s, with peak CPU memory:

| Device (CPU backend) | E2B prefill | E2B decode | E2B TTFT | E2B peak RAM | E4B prefill | E4B decode | E4B TTFT | E4B peak RAM |
|---|---|---|---|---|---|---|---|---|
| MacBook Pro M4 (/M4 Max for E4B) | 901 | 42 | 1.1 s | 736 MB | 277 | 27 | 3.7 s | 890 MB |
| Windows, Intel Lunar Lake | 435 | 30 | 2.4 s | 3,505 MB | 173 | 17 | 6.0 s | **9,372 MB** |
| Android S26 Ultra | 557 | 47 | 1.8 s | 1,733 MB | 195 | 18 | 5.3 s | 3,283 MB |
| Raspberry Pi 5 (16 GB) | 133 | 8 | 7.8 s | 1,546 MB | 51 | 3 | 20.5 s | 3,069 MB |

Two things jump out. First, **E4B decode on a Windows laptop CPU is 17 tok/s** — a 300-token
answer takes 18 seconds. Second, the **Windows peak CPU memory of 9.4 GB for E4B** is
disqualifying for an 8 GB machine, and the E2B figure of 3.5 GB on Windows vs 0.74 GB on macOS
shows this varies enormously by platform. The Ollama download sizes corroborate that Gemma 4's
"effective" parameter counts understate memory: `gemma4:e2b` is **7.2 GB** and `gemma4:e4b` is
**9.6 GB**, both *larger* than `gemma4:12b` at 7.6 GB ([Ollama library](https://ollama.com/library/gemma4)).
That is the Per-Layer-Embeddings tax (E2B is 2.3B effective but **5.1B with embeddings**;
E4B is 4.5B effective but **8B with embeddings** — [E2B card](https://huggingface.co/google/gemma-4-E2B),
[E4B card](https://huggingface.co/google/gemma-4-E4B)).

**Anchor 2 — Liquid AI, llama.cpp, Q4_0, AMD Ryzen AI 9 HX 370**
([source](https://www.liquid.ai/blog/introducing-lfm2-5-the-next-generation-of-on-device-ai)):

| Model | Prefill | Decode | Memory |
|---|---|---|---|
| LFM2.5-1.2B (Q4_0, llama.cpp) | **2,975 tok/s** | **116 tok/s** | 856 MB |
| LFM2.5-2.6B | — | 113 tok/s (Ryzen AI Max+ 395); 220 tok/s (Apple M5 Max) | <2.5 GB |

The gap between anchor 1 and anchor 2 is instructive: llama.cpp's CPU prefill (2,975 tok/s for
1.2B) is far better than LiteRT-LM's (901 tok/s for a 2.3B-effective model on a comparable
class of CPU). Some of that is model size, but **llama.cpp is the better-optimized CPU path**,
and Liquid's older LFM2 post also claims LFM2 is "2× faster decode and prefill than Qwen3 on CPU"
([LFM2 blog](https://www.liquid.ai/blog/liquid-foundation-models-v2-our-second-series-of-generative-ai-models)).

**`[DERIVED]` working estimates for llama.cpp on an 8-core AVX2 x86 laptop / Apple M-series,
Q4_K_M, 8k context** — derive from anchor 2 by inverse-scaling with parameter count (decode is
memory-bandwidth-bound, so ≈ linear in weight bytes; prefill is compute-bound, also ≈ linear):

| Model | Est. prefill tok/s | Est. decode tok/s | Est. time for 1,200-token prompt + 250-token answer |
|---|---|---|---|
| Qwen3.5-0.8B | 3,000–4,500 | 100–160 | ~2.0–2.9 s |
| Qwen3.5-2B / LFM2.5-2.6B | 1,200–2,000 | 45–90 | ~3.6–7.4 s |
| Qwen3.5-4B / Ministral 3 3B | 600–1,100 | 22–45 | ~7.6–12.4 s |
| Qwen3.5-9B | 250–500 | 10–20 | ~15–37 s |

These are **estimates, not measurements**, and validating them is the first item in the
benchmark plan (Part D). But they are directionally consistent with both anchors, and they are
enough to make the architectural call: **a 4B model called once per page is not viable for a
300-page book; a 2B model called on 10–15% of pages is.**

### A.7 Document-specialized tiny models

**IBM Granite-Docling-258M** ([announcement](https://www.ibm.com/new/announcements/granite-docling-end-to-end-document-conversion),
[card](https://huggingface.co/ibm-granite/granite-docling-258M)):

- **It is a VLM, not a text model.** Architecture: SigLIP2 vision encoder → pixel-shuffle
  projector → Granite 165M language model. Input = **page images**. Output = **DocTags**, IBM's
  markup format that encodes every page element with spatial context, convertible to
  Markdown/JSON/HTML via Docling.
- Apache-2.0, released 2025-09-17. It is the "product-ready evolution" of SmolDocling-256M.
- Benchmarks: table structure TEDS **0.97**, code recognition F1 0.988, equation F1 0.968,
  full-page OCR F1 0.84, **layout detection mAP only 0.27**, OCRBench 500.
- **Languages: English primary; Japanese, Arabic, Chinese experimental. No German, no Turkish.**
  This alone disqualifies it as a core component for OpenConvert.
- **CPU feasibility: poor.** Community reports on the model's own HF discussions describe
  "multiple minutes per page" on an RTX 4070 via Transformers, versus **~3 s/page on an RTX 4090
  via llama.cpp** at ~403 tok/s generation — roughly a 100× runtime gap. Root causes given by
  IBM devs: an image-tiling strategy that produces very many input tokens, plus preprocessing
  overhead ([discussion #37](https://huggingface.co/ibm-granite/granite-docling-258M/discussions/37)).
  `[DERIVED]` Scaling 403 tok/s on a 4090 down to a laptop CPU (realistically 40–80 tok/s for a
  258M model at Q4) puts a DocTags page at **roughly 15–60 s/page** on CPU, plus image-token
  prefill. For a 300-page book that is 1.5–5 hours. Not viable as a per-page step.

**SmolDocling-256M-preview** ([card](https://huggingface.co/ds4sd/SmolDocling-256M-preview),
arXiv [2503.11576](https://arxiv.org/abs/2503.11576)):

- Idefics3-based vision-to-sequence model, Apache-2.0, outputs DocTags. Claims parity with VLMs
  "up to 27× larger."
- **0.35 s/page on an A100 with vLLM.** English only. No multi-page batch inference. `[DERIVED]`
  A100→laptop-CPU is conservatively 50–150×, i.e. **~20–50 s/page**.
- Superseded by Granite-Docling; use only as a research reference.

**Layout / reading-order specialist models:** the relevant lineage is **LayoutReader**
(arXiv [2108.11591](https://arxiv.org/abs/2108.11591)), a seq2seq model trained on **ReadingBank**
(500K document images with reading-order, text and layout annotations harvested from Word XML).
It "performs almost perfectly in reading order detection and significantly improves both
open-source and commercial OCR engines in ordering text lines." A 2024 EMNLP follow-up models
reading order as **ordering relations** rather than a sequence
(DOI [10.18653/v1/2024.emnlp-main.540](https://doi.org/10.18653/v1/2024.emnlp-main.540)).
**These are ~100–300M encoder models, not LLMs, and they run on CPU in milliseconds.** For the
reading-order sub-task specifically, a LayoutReader-class model is a stronger and far cheaper
option than any generative LLM. This deserves its own research round.

**Small VLM on CPU per page — is it remotely feasible?**

| Model | Best evidence | `[DERIVED]` laptop-CPU s/page |
|---|---|---|
| SmolDocling-256M | 0.35 s/page on A100 + vLLM | ~20–50 s |
| Granite-Docling-258M | ~3 s/page on RTX 4090 + llama.cpp | ~15–60 s |
| SmolVLM-2B | 5.02 GB min GPU RAM; ~1.2k tokens per image vs Qwen2-VL's 16k; "runs efficiently on-device, such as a laptop" ([HF blog](https://huggingface.co/blog/smolvlm)) | ~30–90 s |
| Qwen3.5-2B (vision) | +668 MB mmproj; OCRBench 84.5 | ~30–120 s |
| Gemma 4 E4B (vision) | CPU decode 17–27 tok/s | ~60–200 s |

**Conclusion: no, a small VLM per page on CPU is not feasible for whole-book conversion.**
It *is* feasible as a **rare escape hatch** — e.g. 1–3 pages out of 300 where geometry-based
heuristics and the text LLM both fail (a complex multi-column figure spread, a scanned insert).
At ~30 s/page and <1% of pages, that costs ~1.5 minutes on a 300-page book, which is acceptable.
Budget the VLM path as an explicitly user-triggered or confidence-gated last resort, never as a
pipeline stage.

---

## Part B — Runtimes

### B.1 llama.cpp — capabilities that matter to us

**Structured output.** llama.cpp has first-class GBNF grammars plus a JSON-schema→grammar
converter, exposed as `--grammar` / `--grammar-file`, `--json-schema` / `-j`, and on the server
as the `grammar`, `json_schema`, and OpenAI-style `response_format` fields
([grammars README](https://github.com/ggml-org/llama.cpp/blob/master/grammars/README.md),
[server README](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md)).
Documented **limitations of the JSON-schema subset**, all of which we must design around:

- `additionalProperties` defaults to **false** (good for us — reduces hallucinated keys)
- **nested `$ref`s are broken**; remote `$ref`s unsupported in the C++ implementation
- unsupported: `uniqueItems`, `contains`, `if`/`then`/`else`, `patternProperties`
- `minimum`/`maximum` work for integers only, not floats
- `pattern` rules must be anchored `^…$`

**Practical implication:** author the grammar as **hand-written GBNF**, not as a JSON schema we
hope converts cleanly. Our output shape is simple enough (arrays of enum labels, small integer
indices) that a hand-written GBNF is both more reliable and more token-efficient.

**llguidance** is merged upstream (PR [#10224](https://github.com/ggml-org/llama.cpp/pull/10224),
merged 2025-02-02) behind the `LLAMA_LLGUIDANCE` CMake flag, invoked with an `llg:` prefix
(`llg:regex:…`, Lark CFGs, JSON schemas). It uses lexer/parser with regex derivatives + Earley
parsing for efficient mask computation. **Packaging catch: it is built via `ExternalProject_Add`
and requires Cargo/Rust at compile time**, which complicates a reproducible cross-platform build.
Recommendation: start with stock GBNF; adopt llguidance only if we hit grammar expressiveness
or mask-computation cost limits.

**Prompt caching / KV reuse — the single biggest lever for our workload.** llama-server exposes:

- `--cache-prompt` (**enabled by default**) — reuses the KV of a matching prompt prefix
- `--cache-reuse N` — minimum chunk size for KV shifting, enabling reuse of *partially* matching
  prefixes
- `--kv-unified` — unified KV buffer across sequences
- `--cache-idle-slots` — keep idle slots' caches warm
- `--context-shift` — automatic context management for long generation

Since every one of our calls shares an identical multi-hundred-token system prompt +
label taxonomy + few-shot examples, prefix caching converts that fixed cost from *per call* to
*per session*. On a 1,200-token prompt where 700 tokens are shared preamble, this is a ~58%
prefill reduction. `[DERIVED]`

**Batching.** `-np`/`--parallel N` (defaults to auto) creates server slots; `-cb`/`--cont-batching`
enables continuous batching. On CPU, batching many small classification prompts raises aggregate
throughput because prefill is compute-bound and benefits from larger GEMMs, but it also
multiplies KV memory by the slot count — on an 8 GB machine, 2–4 slots is the realistic ceiling
for a 2B model. `[DERIVED]`

**Thread and CPU control.** `-t`/`--threads` (generation), `-tb`/`--threads-batch` (prefill),
plus `--cpu-mask` and `--cpu-range` for affinity. Setting `-tb` higher than `-t` is the right
shape for us (prompt-heavy, output-light), and pinning matters on hybrid P/E-core laptops.

**Bindings maturity:**

| Binding | Latest | License | Maturity notes |
|---|---|---|---|
| **llama-cpp-2** (Rust) | **0.1.156, 2026-09-03** | MIT/Apache-2.0 | Actively maintained, tracks upstream closely. Exposes `sampling`, `llama_batch`, `context`, `json_schema_to_grammar`, speculative decoding. Authors warn the bindings "are not safe" and misuse yields UB; requires clang for bindgen. Deliberately mirrors the C API rather than offering an ergonomic abstraction. ([docs.rs](https://docs.rs/llama-cpp-2/latest/llama_cpp_2/)) |
| **llama-cpp-python** | **0.3.35, 2026-08-17** | MIT | Most mature. Prebuilt wheels for CPU, CUDA 11.8–13.2, Metal, ROCm, Vulkan; Python 3.10–3.12. Ships an OpenAI-compatible server, JSON + JSON-schema constrained modes, multimodal support. ([PyPI](https://pypi.org/project/llama-cpp-python/)) |
| **node-llama-cpp** | current | MIT | Prebuilt binaries for macOS/Linux/Windows with auto-detected Metal/CUDA/Vulkan and always-on Accelerate on macOS; falls back to CMake source build. `createGrammarForJsonSchema()` supports "a small subset of the JSON schema spec". Has a dedicated Electron packaging guide. ([docs](https://node-llama-cpp.withcat.ai/guide/)) |

**Packaging pitfalls (llama.cpp, all bindings):**

- **CPU feature dispatch.** A single binary must handle machines without AVX2/AVX-512.
  llama.cpp supports runtime CPU-feature dispatch, but naive builds bake in `-march=native` and
  crash with SIGILL on older hardware. Build with explicit baseline + runtime dispatch, and test
  on a non-AVX-512 machine. `[UNVERIFIED — verify current build-flag names against upstream CMake]`
- **macOS code signing / notarization.** Metal shader libraries and any dylibs must be signed
  with hardened runtime; embedded Rust/Cargo build products from llguidance would each need
  signing. Prefer statically linking the inference core.
- **Windows:** MSVC vs MinGW ABI, and `llamafile`-style single-file packaging hits a **4 GB
  executable limit on Windows** ([llamafile README](https://github.com/Mozilla-Ocho/llamafile)).
- **Binary size:** a CPU-only llama.cpp static build is on the order of 5–15 MB; adding CUDA or
  ROCm balloons it by hundreds of MB. For OpenConvert, ship **CPU + Metal only** and treat GPU
  as an optional plugin. `[UNVERIFIED — measure]`

### B.2 llama-server as a sidecar

`llama-server` is a self-contained binary exposing OpenAI-compatible chat/completions/embeddings
*and* Anthropic Messages-compatible endpoints, with all the caching/batching/grammar flags above.
Shipping it as a **sidecar process** is entirely practical: spawn on demand, bind to
`127.0.0.1` on an ephemeral port with a per-session token, and kill on idle.

### B.3 Ollama

- **MIT licensed** ([repo](https://github.com/ollama/ollama)), REST API on `localhost:11434`,
  Python and JS SDKs, installers for macOS/Windows/Linux.
- **Structured outputs supported since 2024-12-06** via a `format` field carrying a JSON schema;
  documented as "more reliability and consistency than JSON mode," with Pydantic
  (`model_json_schema()`) and Zod integration ([Ollama blog](https://ollama.com/blog/structured-outputs)).
- **Runs as a persistent background daemon.** Models stay resident for **5 minutes by default**
  (`keep_alive`, settable to negative for forever or 0 for immediate unload);
  `OLLAMA_MAX_LOADED_MODELS` defaults to 3, `OLLAMA_NUM_PARALLEL` auto-selects 4 or 1,
  `OLLAMA_MAX_QUEUE` 512, and "parallel requests increase context size proportionally."
  Default `num_ctx` is documented as **2048** — silently truncating our long geometry prompts
  unless we set it explicitly ([FAQ](https://github.com/ollama/ollama/blob/main/docs/faq.md)).
- **Model store is global and user-wide** (`~/.ollama/models`), shared with every other app on
  the machine. Good for disk dedup, bad for reproducibility: another app can `ollama rm` our model.
- **Packaging implication:** Ollama is a *separate application*, not a library. We cannot bundle
  the daemon inside our app bundle without effectively shipping and managing a second product
  (its own updater, its own service registration, its own port). Requiring users to install
  Ollama is a real adoption tax for a "local-first desktop app" whose selling point is that it
  just works.

### B.4 Other runtimes

| Runtime | License | Verdict |
|---|---|---|
| **LM Studio** | closed-source desktop app `[UNVERIFIED — not confirmed against a license page]` | Support as an optional **BYO OpenAI-compatible endpoint** only. Never a dependency. |
| **mistral.rs** | MIT, v0.8.2 | Genuine contender for a Rust app: CPU/CUDA/Metal, GGUF 2–8 bit + in-situ quantization, **grammar enforcement with strict schema mode**, Rust crate + Python SDK, Gemma 4 support landed. Smaller community than llama.cpp; fewer eyes on CPU kernels. ([repo](https://github.com/EricLBuehler/mistral.rs)) |
| **candle** | Apache-2.0/MIT | Minimalist Rust ML framework; you build the serving layer yourself. Not worth it when llama.cpp exists. |
| **MLX / mlx-lm** | MIT | **Apple Silicon only**, macOS 15+. No documented grammar/constrained-decoding support. Would mean maintaining a second inference path for one platform. Note Ollama and Qwen both ship `-mlx` variants ([Ollama qwen3.5](https://ollama.com/library/qwen3.5)), so it's a viable *later* optimization, not a foundation. ([mlx-lm](https://github.com/ml-explore/mlx-lm)) |
| **ONNX Runtime GenAI** | MIT, **v0.14.0 (2026-05-29)** | Serious alternative: Windows/Linux/macOS/Android, x86/x64/**arm64**, CPU + DirectML + OpenVINO + QNN + WebGPU, **constrained decoding**, C/C++/C#/Python bindings, NuGet/pip. Best-in-class on Windows-with-NPU. Weakness: fewer models converted, and GGUF ecosystem momentum is elsewhere. ([repo](https://github.com/microsoft/onnxruntime-genai)) |
| **vLLM / SGLang** | Apache-2.0 | Server-class, GPU-first. Skip. |
| **llamafile** | Apache-2.0 (MIT for llama.cpp deltas), v0.10.x | Charming single-file distribution, but the **4 GB Windows executable cap** and the fact that we want the model downloaded separately anyway make it a poor fit. ([repo](https://github.com/Mozilla-Ocho/llamafile)) |
| **KoboldCpp** | AGPL `[UNVERIFIED]` | Consumer chat UI, not an embedding target. Skip. |

### B.5 Integration strategy: embedded vs sidecar vs "require Ollama"

| Criterion | **Embedded llama.cpp (in-process)** | **llama-server sidecar** | **Require Ollama** |
|---|---|---|---|
| RAM | Best — one process, no IPC copies, direct control of `n_ctx`/KV | +30–80 MB process overhead, plus request/response JSON copies `[DERIVED]` | Worst — daemon holds up to 3 models resident by default, 5-min keep-alive, unaware of our memory pressure |
| Startup latency | Model load only (~0.5–3 s from page cache for a 1–3 GB GGUF) `[DERIVED]` | + process spawn + HTTP readiness poll (~0.3–1 s) `[DERIVED]` | + daemon cold start + possible model pull (minutes on first run) |
| Install complexity | Zero extra for the user; highest for us (build matrix) | Zero extra for the user; one extra signed binary | **User must install a second app.** Unacceptable for a "just works" desktop tool |
| Crash isolation | **Poor** — an OOM or SIGILL in ggml kills the whole app mid-conversion | **Excellent** — sidecar dies, app detects, restarts or falls back to heuristics | Excellent, but failures are outside our control and hard to diagnose |
| Packaging / signing | Hardest: native lib per OS×arch, hardened runtime, notarization, CPU-feature dispatch | Easier: one extra executable to sign; can be lazily downloaded | Easiest for us, hardest for the user |
| Cross-platform | Full control, most work | Same binary story, but decoupled from app build | Ollama's platform support, not ours |
| Update strategy | Coupled to app releases — a llama.cpp CVE means a full app release | **Decoupled** — swap the sidecar binary independently | Uncontrolled: Ollama can change defaults (e.g. `num_ctx`) under us |
| Determinism / reproducibility | Full: we pin the llama.cpp commit, the GGUF hash, threads, seed, grammar | Full | **Poor**: shared global model store, user-modifiable Modelfiles, silent version drift |

**Recommendation: sidecar `llama-server`, with an embedded-library escape hatch.** Reasoning:

1. **Crash isolation is not optional here.** A book conversion is a long job over untrusted PDFs.
   A ggml assertion or an OOM must degrade to "heuristics-only for this chunk," not lose the job.
2. **Reproducibility matters more than 50 MB of RAM.** We pin the binary, the GGUF hash, thread
   count, grammar and seed. Ollama's shared global store makes bug reports unreproducible.
3. **Prefix caching and slots are exactly what llama-server exposes** (`--cache-prompt`,
   `--cache-reuse`, `-np`, `-cb`). Re-implementing that against the raw C API is work we would
   only be redoing.
4. **Update decoupling** lets us ship llama.cpp security fixes without a full signed app release.
5. Ollama remains supported as an **optional detected backend** (it is MIT and its `format`
   field gives us JSON-schema outputs), and LM Studio / any OpenAI-compatible endpoint as
   BYO — because both speak the same protocol as our sidecar, this costs almost nothing.

The one real cost is startup latency and IPC overhead, both of which are small next to a
multi-second CPU inference call.

### B.6 Structured output, caching and batching — recommended technique stack

1. **Hand-written GBNF, not JSON-schema conversion.** Avoids every documented gap in llama.cpp's
   schema subset, and lets us encode arity ("exactly N labels for N input blocks") directly.
2. **Token-minimal output shape.** Decode is the bottleneck, so emit
   `{"l":["h2","p","p","fn","cap"],"o":[0,1,3,2,4]}` — enum tokens and small ints — not verbose
   per-block objects. `[DERIVED]` This is plausibly a 5–10× reduction in output tokens versus a
   naive `[{"block_id":..., "label":..., "confidence":...}]` shape, which translates almost
   linearly into wall-clock savings on CPU.
3. **Stable shared prefix.** Freeze the system prompt + taxonomy + few-shot block so it is
   byte-identical across all calls; put only the variable page payload at the end. Then
   `--cache-prompt` + `--cache-reuse` amortize it.
4. **Batch by page, not by block.** One call per ambiguous page region, containing all its
   blocks, rather than one call per block. Reduces per-call fixed overhead by ~40×. `[DERIVED]`
5. **2–4 server slots with continuous batching** on ≥8-core machines; 1 slot on 8 GB machines.
6. **Never enable thinking mode.** Pin `enable_thinking: false` and assert no `<think>` token
   appears in output; treat its appearance as a config regression.

---

## Part C — Is there real evidence small LLMs are good at these tasks?

**Honest answer: the evidence is thin and mostly indirect.** There is no paper I found that
evaluates a sub-4B LLM on "classify text blocks given text + geometry, output strict JSON" for
German/Turkish books. What exists falls into four buckets.

**1. Reading order is a solved-ish problem *without* LLMs, by smaller specialist models.**
LayoutReader (arXiv [2108.11591](https://arxiv.org/abs/2108.11591)) built ReadingBank (500K
annotated document images, harvested from Word XML) and reports its seq2seq model "performs
almost perfectly in reading order detection and significantly improves both open-source and
commercial OCR engines in ordering text lines." A 2024 EMNLP paper reformulates reading order as
**ordering relations** rather than sequence generation
(DOI [10.18653/v1/2024.emnlp-main.540](https://doi.org/10.18653/v1/2024.emnlp-main.540)). A 2026
preprint goes the other direction entirely — **deterministic** hierarchical zone decomposition
("True Human Reading", DOI [10.2139/ssrn.7013519](https://doi.org/10.2139/ssrn.7013519)).
**This is the most important finding in Part C: for reading order specifically, an LLM is
probably the wrong tool.** A ~100M encoder or a well-tuned XY-cut/zone decomposition is faster,
cheaper, and better-evidenced.

**2. Document-parsing benchmarks exist, and Qwen3.5 small models are scored on them.**
OmniDocBench (arXiv [2412.07626](https://arxiv.org/abs/2412.07626)) covers 9 document types with
19 layout categories and 15 attribute labels, and explicitly "evaluat[es] both pipeline-based
methods and end-to-end vision-language models, revealing their strengths and weaknesses."
Qwen3.5 publishes **OmniDocBench 1.5 scores of 79.8 (2B) and 86.2 (4B)** — the only direct
structural-parsing numbers I found for models in our size class. Caveat: OmniDocBench measures
*full-page parsing from images*, which is not our task (we feed text + geometry, not pixels), so
this is suggestive rather than dispositive.

**3. Layout-encoded prompting of LLMs is an active but early 2025–2026 literature.** Closest to
our design:
- *"Spatial-Aware Financial Document Understanding: A Region-Segmented OCR-LLM Pipeline with
  **Layout-Encoded Prompting** for Key-Value Extraction"*, IEEE eIT 2026
  (DOI [10.1109/eit68936.2026.11670412](https://doi.org/10.1109/eit68936.2026.11670412)) —
  serializes geometry into the prompt, exactly our pattern, but for KV extraction not structure.
- *"HiPS: Hierarchical PDF Segmentation of Doctrinal Legal Books"* (2025) — recovers
  document-wide **section hierarchies** in books using OCR cues, XML typography and an
  "LLM-refined pipeline". This is the single closest published analogue to our chapter-boundary
  and heading-level tasks. `[UNVERIFIED — retrieved via Semantic Scholar index; full text not read]`
- *"Research on Document Layout Detection and Description Method for RAG"*, ICRCA 2025
  (DOI [10.1109/icrca64997.2025.11011072](https://doi.org/10.1109/icrca64997.2025.11011072)).
- *"NovaLAD: A Fast, CPU-Optimized Document Extraction Pipeline"* (2026) — concurrent YOLO models
  with *optional* VLM enhancement. Notable because it independently arrives at our architecture:
  cheap deterministic models by default, expensive model only when needed. `[UNVERIFIED]`
- The **post-OCR correction with LLMs** literature is more mature and directionally supportive
  (AAAI 2025 DOI [10.1609/aaai.v39i27.35012](https://doi.org/10.1609/aaai.v39i27.35012);
  ACM DocEng 2026 *"Cost-Aware Human-LLM Collaboration for Post-OCR Corrections in Swiss
  Historical Newspapers"*, DOI [10.1145/3820755.3832797](https://doi.org/10.1145/3820755.3832797)).
  That last title is worth reading for its **cost-aware** framing — same economics as ours.

**4. The strongest production evidence is that the leading OSS pipelines do *not* use LLMs for
this.** Docling (MIT, arXiv [2408.09869](https://arxiv.org/abs/2408.09869) and
[2501.17887](https://arxiv.org/abs/2501.17887)) does layout with **DocLayNet** and tables with
**TableFormer** — purpose-built CV models, no LLM — and claims it "runs efficiently on commodity
hardware in a small resource budget." IBM's own answer to end-to-end document AI, Granite-Docling,
is a **258M VLM**, not a general LLM.

**What this means for OpenConvert.** The brief's instinct — use the LLM *only* where
deterministic heuristics have low confidence — is well-supported by the literature, but for a
slightly different reason than assumed: not merely that the LLM is expensive, but that for
several sub-tasks (reading order especially) **specialist non-LLM models are demonstrably
better**. The tasks where a small LLM plausibly adds unique value are the *semantic* ones that
geometry cannot settle:

| Task | LLM value | Better alternative? |
|---|---|---|
| Heading level (h1/h2/h3) disambiguation | **High** — needs text semantics + numbering conventions across languages | none obvious |
| Chapter boundary decisions | **High** — "Kapitel 3" vs a running header requires reading | none obvious |
| Header/footer vs body | Low | geometry + cross-page repetition detection is near-perfect |
| Footnote/caption **association** | **Medium-high** — marker matching is semantic | partly heuristic (marker regex + proximity) |
| Paragraph merge/split across pages | **Medium** — hyphenation + sentence continuation | strong heuristics exist; LLM as tiebreaker |
| Reading order correction | **Low** | LayoutReader-class model, or deterministic zone decomposition |
| Suspicious-output detection | **High** — "does this EPUB chapter read like prose?" is exactly an LM task; perplexity alone may suffice | LM scoring without generation is cheaper |

Note the last row: **for suspicious-output detection you may not need generation at all.**
Computing token log-probs over the produced text is prefill-only — 10–30× cheaper than decoding
on CPU — and llama.cpp exposes logprobs. That is a strong, cheap use of the model.

---

## Part D — Tentative recommendation

### D.1 Default model: **Qwen3.5-2B, Q4_K_M GGUF, text-only (no mmproj), thinking disabled**

Rationale:

- **Apache-2.0** — bundle-safe, redistribute-safe, no revenue clause, no MAU cap, no
  acceptable-use policy to flow down.
- **1.28 GB at Q4_K_M**, ~1.6–1.9 GB resident at 8k context `[DERIVED]` — comfortably inside an
  8 GB laptop alongside a PDF renderer and the app itself. The vision projector is a separate
  668 MB file we simply do not download for the default profile.
- **The only small model with published document-structure numbers**: OmniDocBench 1.5 **79.8**,
  OCRBench 84.5, MMLongBench-Doc 45.4.
- **201 languages** with a 248K vocabulary — the only candidate where Turkish is credibly
  covered. SmolLM3 and LFM2.5 both omit Turkish from their language lists.
- **Non-thinking by default** — no latency trap, unlike Qwen3.5-4B/9B.
- 262K native context means long-document prompts are never a structural problem (though we
  will use ~2–4k for cost reasons).
- Excellent ecosystem: GGUF from multiple publishers, `qwen3.5:2b` in Ollama, llama.cpp DeltaNet
  support fixed and merged (PRs #19139 / #20416, March 2026).

**Its honest weakness: IFEval 61.2 non-thinking.** That is not great, and it is why grammar-
constrained decoding is mandatory rather than optional, and why the benchmark plan (D.3) leads
with instruction-obedience measurement rather than assuming it.

**Fallback tiers** (user-selectable, same prompt/grammar, same code path):

| Tier | Model | When |
|---|---|---|
| **Tiny** (≤6 GB RAM, or "fast" mode) | **Qwen3.5-0.8B** Q4_K_M, ~0.6 GB | Netbooks, battery mode; expect materially worse heading-level accuracy (IFEval 52.1) |
| **Default** | **Qwen3.5-2B** Q4_K_M | 8–16 GB laptops |
| **Quality** (≥16 GB, user opts in) | **Qwen3.5-4B** Q4_K_M, 2.74 GB, thinking OFF | IFEval 89.8, OmniDocBench 86.2; ~2–3× slower per call `[DERIVED]` |
| **Ceiling / offline eval only** | **Qwen3.5-9B** | Generating gold labels to score the small models against; not a shipping default |
| **Alternates to evaluate, not ship** | Gemma 4 E2B (Apache-2.0, official QAT GGUF, but the PLE memory tax and 7.2 GB Ollama blob need investigating), Ministral 3 3B (Apache-2.0, 256K, native JSON), Granite 4.0 Micro 3B (Apache-2.0, non-hybrid variant explicitly provided for llama.cpp), LFM2.5-2.6B (fastest + best IFStruct, **but revenue-capped license and no Turkish**) | |

Staying inside one model family for all tiers is deliberate: one prompt, one grammar, one
tokenizer's quirks to learn, one chat template to pin.

**Explicitly not recommended as core components:** Granite-Docling-258M and SmolDocling-256M
(English-only, 15–60 s/page on CPU), and any per-page VLM. Keep a VLM behind a
"re-analyze this page with vision" button, budgeted at <1% of pages.

### D.2 Runtime: **bundled `llama-server` sidecar**, with optional Ollama / BYO-endpoint backends

- Ship a signed, pinned `llama-server` binary per OS×arch (CPU + Metal only; no CUDA in the
  default installer).
- Launch on demand on `127.0.0.1:<ephemeral>` with a per-session bearer token; idle-timeout and
  kill.
- Flags: `--cache-prompt` (default on), `--cache-reuse 256`, `-np 2..4` (1 on 8 GB machines),
  `-cb`, `-t <physical cores − 2>`, `-tb <physical cores>`, `--ctx-size 4096`, and an explicit
  chat-template override that pins thinking off.
- Constrain every call with **hand-written GBNF**; never rely on `--json-schema` conversion
  given its documented gaps.
- Model manager downloads the pinned GGUF by SHA-256 on first use, writes LICENSE + NOTICE
  beside it, and shows the license to the user.
- Detect an existing Ollama daemon and offer it as an alternate backend (its `format` field
  gives JSON-schema outputs), plus a "custom OpenAI-compatible endpoint" field for LM Studio and
  friends. All three speak the same wire protocol, so this is one client implementation.
- If the app is Rust: `llama-cpp-2` (v0.1.156, actively maintained) is the fallback for a future
  in-process mode, and **mistral.rs** (MIT, grammar + strict schema mode, CPU/Metal) is the
  credible plan-B engine. If the app is Electron/Node: `node-llama-cpp` has prebuilt binaries and
  an Electron guide. Neither changes the sidecar-first recommendation.

### D.3 Benchmark plan to validate (or kill) this choice

**Corpus.** 60 PDFs, held out from any prompt development:
- 20 German (fiction, academic monograph with footnotes, two-column journal article, scanned OCR)
- 20 Turkish (same spread) — Turkish is the highest-risk language and must be first-class
- 20 English (control)
- Sampling: ~25 pages per document → **~1,500 gold-labelled pages**, ~60,000 text blocks.
- Deliberately over-sample the hard cases: multi-column, sidebars, footnote-heavy, running
  headers that look like headings, chapter openers with drop caps, tables of contents.
- Gold labels: human-annotated, with **Qwen3.5-9B (thinking on, offline, no latency budget)**
  used only to pre-label and to measure the small-model gap — never as ground truth.

**Tasks and metrics.**

| Task | Metric | Target |
|---|---|---|
| Block classification (heading level / body / header-footer / footnote / caption / list item) | macro-F1, per-class F1, per-language F1 | macro-F1 ≥ 0.92 on LLM-invoked blocks; **must beat the heuristic baseline on the same low-confidence subset by ≥8 F1 points, or the LLM is not earning its place** |
| Chapter boundary | boundary F1 with ±1 block tolerance | ≥0.95 |
| Paragraph merge/split | pairwise merge accuracy | ≥0.97 |
| Footnote/caption association | link accuracy | ≥0.90 |
| Reading-order correction | Kendall's tau + exact-sequence accuracy vs gold, **head-to-head against a LayoutReader-class model and against deterministic zone decomposition** | LLM must win, or we drop the LLM from this task |
| Suspicious-output detection | AUC on injected corruptions (shuffled blocks, dropped footnotes, merged chapters) | AUC ≥0.85; **also test the prefill-only logprob variant** |
| Output validity | % grammar-valid, % correct arity, % `<think>` leakage | 100% / ≥99.9% / 0% |

**Ablations that must run.**
1. Grammar-constrained vs free-form + repair — settles the
   [2408.02442](https://arxiv.org/abs/2408.02442) vs [dottxt](https://blog.dottxt.ai/say-what-you-mean.html)
   question *for our task*.
2. Thinking on vs off at equal wall-clock budget.
3. Verbose JSON vs token-minimal enum output (predicted 5–10× decode saving).
4. Prefix cache on vs off (predicted ~58% prefill saving).
5. 0.8B vs 2B vs 4B vs 9B — the accuracy/latency Pareto front.
6. Text+geometry serialization variants: normalized bbox ints vs relative descriptors vs
   font-size ranks. This is likely worth more accuracy than a model upgrade.
7. **Heuristics-only baseline** on the full corpus — the number every LLM result is measured against.

**CPU targets** (three machines, all runs CPU-only, `llama-bench` for raw pp/tg plus end-to-end):
- 8-core x86 AVX2, 16 GB, DDR5 (mainstream Windows/Linux laptop)
- 4-core x86 AVX2, 8 GB, DDR4 (floor case — must not OOM)
- Apple M-series base chip, 16 GB (Metal off for the CPU baseline, on for the shipped path)

Report **prefill and generation tok/s separately** — the openbenchmarking llama.cpp profile does
exactly this (prompt processing at 512/1024/2048, generation at 128) and is a reasonable template
([openbenchmarking](https://openbenchmarking.org/test/pts/llama-cpp)).

**Latency budget (the acceptance gate).** For a 300-page book on the 8-core/16 GB target:

| Budget item | Target |
|---|---|
| Fraction of pages that invoke the LLM at all | **≤15%** (≈45 pages) |
| LLM calls per invoked page | ≤2 |
| Prompt tokens per call (of which shared, cached prefix) | ≤1,400 (≥700 shared) |
| Output tokens per call | **≤120** |
| Wall-clock per call, Qwen3.5-2B Q4_K_M | **≤2.5 s** `[DERIVED from A.6]` |
| **Total LLM time for the book** | **≤4 minutes** |
| **LLM share of total conversion time** | **≤35%** |
| Worst-case single-page stall (user-visible) | ≤5 s |
| Peak process RSS including model | **≤3.0 GB** |

If measured numbers cannot hit ≤4 minutes of LLM time on a 300-page book, the correct response
is **not** a smaller model — it is a stricter confidence gate that invokes the LLM on fewer pages.

**Decision rule.** Ship the LLM path only if, on the low-confidence subset, it beats the pure-
heuristic baseline by ≥8 macro-F1 points at ≤35% of conversion wall-clock, on all three languages
independently. If it wins on English and German but loses on Turkish, ship it language-gated
rather than globally.

---

## Sources

**Models — official cards, repos and announcements**
- Qwen3.5-4B — https://huggingface.co/Qwen/Qwen3.5-4B and https://huggingface.co/Qwen/Qwen3.5-4B/blob/main/README.md
- Qwen3.5-2B — https://huggingface.co/Qwen/Qwen3.5-2B and https://huggingface.co/Qwen/Qwen3.5-2B/blob/main/README.md
- Qwen3.5-0.8B — https://huggingface.co/Qwen/Qwen3.5-0.8B
- Qwen3.5-9B — https://huggingface.co/Qwen/Qwen3.5-9B
- Qwen3.8 repo (confirms no small models in 3.6/3.8) — https://github.com/QwenLM/Qwen3.8
- Qwen release timeline — https://en.wikipedia.org/wiki/Qwen
- Qwen3.5 small models analysis — https://artificialanalysis.ai/articles/qwen3-5-small-models
- Qwen3.5 GGUF sizes (4B) — https://huggingface.co/unsloth/Qwen3.5-4B-GGUF/tree/main
- Qwen3.5 GGUF sizes (2B) + mmproj — https://huggingface.co/unsloth/Qwen3.5-2B-GGUF/tree/main
- Qwen3.5 in Ollama — https://ollama.com/library/qwen3.5
- llama.cpp DeltaNet offloading fixes — https://huggingface.co/AesSedai/Qwen3.5-35B-A3B-GGUF/discussions/6
- Gemma 4 announcement — https://blog.google/innovation-and-ai/technology/developers-tools/gemma-4/
- Gemma 4 model overview — https://ai.google.dev/gemma/docs/core
- Gemma 4 E2B card — https://huggingface.co/google/gemma-4-E2B
- Gemma 4 E4B card — https://huggingface.co/google/gemma-4-E4B
- Gemma 4 Apache-2.0 reporting — https://the-decoder.com/googles-gemma-4-is-now-available-with-apache-2-0-licensing-for-the-first-time/
- **Gemma 4 on-device CPU benchmarks** — https://developers.google.com/edge/litert-lm/models/gemma-4
- Gemma 4 in Ollama — https://ollama.com/library/gemma4
- LFM2.5 announcement (CPU prefill/decode) — https://www.liquid.ai/blog/introducing-lfm2-5-the-next-generation-of-on-device-ai
- LFM2.5-2.6B card — https://huggingface.co/LiquidAI/LFM2.5-2.6B
- LFM Open License v1.0 — https://huggingface.co/LiquidAI/LFM2.5-2.6B/blob/main/LICENSE
- LFM2.5-2.6B coverage — https://www.marktechpost.com/2026/08/06/liquid-ai-lfm2-5-2-6b-on-device-agentic-model/
- LFM2 original blog (CPU comparison vs Qwen3) — https://www.liquid.ai/blog/liquid-foundation-models-v2-our-second-series-of-generative-ai-models
- Mistral 3 / Ministral 3 announcement — https://mistral.ai/news/mistral-3
- Ministral-3-3B-Instruct card — https://huggingface.co/mistralai/Ministral-3-3B-Instruct-2512
- Granite 4.0 announcement — https://www.ibm.com/new/announcements/ibm-granite-4-0-hyper-efficient-high-performance-hybrid-models
- Granite 4 in Ollama — https://ollama.com/library/granite4
- SmolLM3-3B card — https://huggingface.co/HuggingFaceTB/SmolLM3-3B
- Falcon-H1 variants listing — https://huggingface.co/models?search=falcon-h1
- gpt-oss model card — https://openai.com/index/gpt-oss-model-card/
- Nemotron-Nano-9B-v2 — https://huggingface.co/nvidia/NVIDIA-Nemotron-Nano-9B-v2
- Llama 4 Scout (size floor) — https://www.prompthub.us/models/llama-4-scout
- Phi timeline — https://en.wikipedia.org/wiki/Phi_(language_model) ; Phi-4-mini — https://huggingface.co/microsoft/Phi-4-mini-instruct

**Document-specialized models**
- Granite-Docling announcement — https://www.ibm.com/new/announcements/granite-docling-end-to-end-document-conversion
- Granite-Docling-258M card — https://huggingface.co/ibm-granite/granite-docling-258M
- Granite-Docling CPU/GPU speed reports — https://huggingface.co/ibm-granite/granite-docling-258M/discussions/37
- SmolDocling-256M card — https://huggingface.co/ds4sd/SmolDocling-256M-preview
- SmolDocling paper — https://arxiv.org/abs/2503.11576
- SmolVLM blog (on-device memory/throughput) — https://huggingface.co/blog/smolvlm

**Runtimes**
- llama.cpp GBNF / JSON-schema grammars — https://github.com/ggml-org/llama.cpp/blob/master/grammars/README.md
- llama-server README (caching, batching, threads, endpoints) — https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md
- llguidance integration PR (merged 2025-02-02) — https://github.com/ggml-org/llama.cpp/pull/10224
- Apple Silicon llama.cpp perf thread (Metal, not CPU) — https://github.com/ggml-org/llama.cpp/discussions/4167
- llama-cpp-2 (Rust) — https://docs.rs/llama-cpp-2/latest/llama_cpp_2/
- llama-cpp-python — https://pypi.org/project/llama-cpp-python/
- node-llama-cpp — https://node-llama-cpp.withcat.ai/guide/
- Ollama repo (MIT, distribution) — https://github.com/ollama/ollama
- Ollama FAQ (keep_alive, num_ctx, parallelism, model store) — https://github.com/ollama/ollama/blob/main/docs/faq.md
- Ollama structured outputs — https://ollama.com/blog/structured-outputs
- mistral.rs — https://github.com/EricLBuehler/mistral.rs
- mlx-lm — https://github.com/ml-explore/mlx-lm
- ONNX Runtime GenAI — https://github.com/microsoft/onnxruntime-genai
- llamafile — https://github.com/Mozilla-Ocho/llamafile
- llama.cpp CPU benchmark profile (pp/tg separated) — https://openbenchmarking.org/test/pts/llama-cpp

**Structured output evidence**
- "Let Me Speak Freely?" — https://arxiv.org/abs/2408.02442
- .txt rebuttal — https://blog.dottxt.ai/say-what-you-mean.html
- JSONSchemaBench — https://arxiv.org/abs/2501.10868

**Document structure / reading order literature**
- LayoutReader + ReadingBank — https://arxiv.org/abs/2108.11591
- Modeling Layout Reading Order as Ordering Relations (EMNLP 2024) — https://doi.org/10.18653/v1/2024.emnlp-main.540
- True Human Reading: Deterministic Hierarchical Zone Decomposition (2026) — https://doi.org/10.2139/ssrn.7013519
- OmniDocBench — https://arxiv.org/abs/2412.07626
- Docling technical report — https://arxiv.org/abs/2408.09869
- Docling toolkit paper — https://arxiv.org/abs/2501.17887
- Spatial-Aware Financial Document Understanding (layout-encoded prompting, IEEE eIT 2026) — https://doi.org/10.1109/eit68936.2026.11670412
- Document Layout Detection and Description for RAG (ICRCA 2025) — https://doi.org/10.1109/icrca64997.2025.11011072
- Reference-Based Post-OCR Processing with LLM (AAAI 2025) — https://doi.org/10.1609/aaai.v39i27.35012
- Cost-Aware Human-LLM Collaboration for Post-OCR Corrections (ACM DocEng 2026) — https://doi.org/10.1145/3820755.3832797

**Multilingual**
- TR-MMLU (Turkish benchmark) — https://arxiv.org/abs/2501.00593

---

## Open questions for round 2

1. **Measure, don't estimate.** Every llama.cpp CPU tok/s figure for Qwen3.5 in this report is
   derived. Run `llama-bench` on the three target machines before committing.
2. **Gemma 4 E2B's real memory footprint.** The 7.2 GB Ollama blob vs 2.58 GB LiteRT file vs
   3.5 GB Windows peak RAM cannot all be describing the same thing. If E2B's GGUF footprint is
   actually ~2 GB, it becomes a serious rival to Qwen3.5-2B.
3. **Turkish, concretely.** No per-language TR score exists for any candidate. Build a small
   Turkish structural-classification probe set early — this is the likeliest place the default
   choice breaks.
4. **LayoutReader-class models deserve their own round.** If a 100–300M encoder solves reading
   order at millisecond cost, the LLM's job shrinks to headings, chapters, and footnote linking —
   which may justify dropping to the 0.8B tier.
5. **Prefill-only suspicious-output detection.** Scoring text with logprobs instead of generating
   a verdict could be 10–30× cheaper. Worth prototyping before building a generative checker.
6. **Qwen3.5-4B non-thinking IFEval** is not published. Measure it; the 89.8 figure is almost
   certainly thinking-mode and should not drive the tier decision.
