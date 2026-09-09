# R10 — Deterministic vs LLM Decision Matrix for PDF → EPUB

**Project:** OpenConvert (open-source, local-first, CPU-only PDF → reflowable EPUB)
**Track:** R10 — where is a small (0.5–4B, CPU) LLM actually better than deterministic algorithms, and at what cost
**Date:** 2026-09-09
**Status:** Research only. No implementation. All claims carry URLs; unverified claims are marked `[UNVERIFIED]`.

---

## 0. TL;DR for the Chief Architect

**The headline number.** On a laptop CPU, a 1B model in Q4 runs at **~96 tok/s prefill and ~16 tok/s generation** ([llama.cpp #13664](https://github.com/ggml-org/llama.cpp/discussions/13664), Gemma 3 1B Q4_K_M, i7-12700H, 14 threads). A book page serialized as *text + geometry + font features in JSON* is **1500–2500 input tokens**. That is **16–26 s of prefill per page** before the model emits a single token. Docling's *entire* deterministic+ML pipeline (RT-DETR layout + TableFormer + assembly) runs at **0.41–1.06 s/page** on comparable CPUs ([Docling Technical Report, Table 1](https://arxiv.org/html/2408.09869v5)).

**A per-page LLM is therefore 20–70× the cost of everything else combined.** A 300-page book: ~4 minutes deterministic, vs. **2–6 hours** with a per-page LLM. This single fact determines the architecture.

**The architectural consequence — the most important recommendation in this report:**

> **Do not invoke the LLM per page. Invoke it per *book*, over a compressed, deduplicated inventory of evidence.**

Books are stylistically homogeneous: 300 pages contain perhaps 4–8 distinct heading styles, one header/footer pattern, one body font. You do not need 300 decisions about "is this a heading?"; you need **one** decision about "what do these 6 style-clusters mean?", applied uniformly. This turns LLM cost from `O(pages)` to `O(1)`, and it *improves* quality, because 300 independent per-page decisions disagree with each other while one global decision cannot.

**Verdicts at a glance** (full matrix in §5):

| # | Sub-problem | Verdict |
|---|---|---|
| 1 | Text extraction (glyph→text, ToUnicode, ligatures) | **Deterministic** |
| 2 | Word/line reconstruction & spacing | **Deterministic** |
| 3 | Dehyphenation (DE compounds, TR) | **Deterministic** (+ narrow batched LLM escape hatch) |
| 4 | Paragraph reconstruction | **Deterministic + ML layout model** |
| 5 | Column detection & reading order | **Deterministic + ML layout model** |
| 6 | Running header/footer & page numbers | **Deterministic** (beats ML *and* VLM) |
| 7 | Heading detection & level assignment | **Hybrid** — detection deterministic, *semantics* per-book LLM |
| 8 | Chapter boundary / book structure / TOC | **Hybrid** — top-2 LLM value |
| 9 | Footnote detection & association | **Deterministic + ML layout model** |
| 10 | Caption detection & figure association | **Deterministic + ML layout model** |
| 11 | List detection (bullets/numbers/nesting) | **Deterministic** |
| 12 | Table detection & structure; image-vs-HTML | **Deterministic + ML** (TableFormer-class) |
| 13 | Block quotes / poetry / verse / preformatted | **Hybrid** — best *per-block* LLM case |
| 14 | OCR error post-correction | **Deterministic detect; LLM correction OFF by default** |
| 15 | Image placement / anchoring | **Deterministic** |
| 16 | Metadata extraction (title/author) | **Hybrid** — highest LLM value per CPU-second |
| 17 | Language detection | **Deterministic** (explicitly no LLM) |
| 18 | "Suspicious output" detection | **Deterministic** (Gopher-style statistics) |
| 19 | EPUBCheck message → repair action | **Deterministic** (finite ~180-ID mapping table) |
| 20 | User-visible warnings / explanations | **Deterministic templates** (architect is right) |

**Net: 5 of 20 sub-problems get an LLM, and 3 of those 5 are once-per-book calls.** Total LLM budget for a 300-page book: **~4–8 calls, ~15–60 CPU-seconds**, i.e. under 20% of conversion time.

---

## 1. Method, and what this evidence base can and cannot support

**What I fetched.** arXiv abstracts and (where available) full HTML, ACL Anthology papers, official docs (Docling, Marker/Datalab, MinerU, GROBID, PyMuPDF, Unstructured, Calibre, EPUBCheck, DAISY), model cards (Granite-Docling), and benchmark repositories (OmniDocBench, olmOCR-Bench, datatrove).

**Constraint to be transparent about.** The web-search budget for this session was exhausted before I began, so I could not run keyword discovery. Every source below was reached by fetching a URL directly. This biases the corpus toward *well-known* work and means **absence of evidence here is weak evidence of absence.** Two areas are genuinely under-covered and are flagged as gaps: **Turkish-language** document processing (§6.3, §6.14) and **calibration studies of deterministic confidence signals** (§4.4).

**Benchmark caveat that matters for a book converter.** OmniDocBench's headline "Overall" score is `((1 − text_edit)*100 + table_TEDS + formula_CDM) / 3` ([OmniDocBench repo](https://github.com/opendatalab/OmniDocBench)). Two of three terms are **tables and formulas**. Trade books, novels, history, and essays contain almost none of either. Likewise Nougat's 0.071-vs-0.255 edit-distance win over the PDF text layer ([arXiv 2308.13418](https://arxiv.org/html/2308.13418v1)) is dominated by LaTeX math markup that a born-digital novel simply does not have. **Do not import scientific-PDF leaderboard rankings into a book-conversion architecture.** For prose with a clean embedded text layer, the text layer is near-perfect and every generative model is a downgrade risk.

---

## 2. The CPU cost model (the constraint that decides everything)

### 2.1 Measured anchors

| Component | Measurement | Hardware | Source |
|---|---|---|---|
| Docling full pipeline (layout + TableFormer + assembly, OCR off) | **1.57 pages/s** (16 thr) / **0.94 pages/s** (4 thr) | Intel Xeon E5-2690 | [arXiv 2408.09869 §Table 1](https://arxiv.org/html/2408.09869v5) |
| Docling full pipeline, same config | **2.45 pages/s** (16 thr) / **2.18 pages/s** (4 thr) | Apple M3 Max | ibid. |
| Docling memory footprint | 2.42 GB (pypdfium) – 6.20 GB (native) | — | ibid. |
| Docling layout model (RT-DETR @ 72 dpi) | "**sub-second latency**" | single CPU | ibid. |
| TableFormer (per table) | "**between 2 and 6 seconds**" | standard CPU | ibid. |
| XY-Cut reading order (deterministic) | **289–685 FPS** (~1.5–3.5 ms/page) | not stated | [arXiv 2504.10258 Table 5](https://arxiv.org/html/2504.10258v2) |
| XY-Cut++ (deterministic + masks) | **248–781 FPS** (mean 514) | not stated | ibid. |
| LayoutReader (learned reading order) | **17–27 FPS** (mean 22) | not stated | ibid. |
| SmolDocling-256M VLM, per page | **102.21 s** (Transformers) / **6.15 s** (MLX) | Apple M3 Max | [Docling VLM docs](https://docling-project.github.io/docling/usage/vision_models/) |
| **Gemma 3 1B Q4_K_M, text-only** | **95.61 tok/s prefill; 16.16 tok/s generation** | Intel i7-12700H, 14 thr, AVX2 | [llama.cpp #13664](https://github.com/ggml-org/llama.cpp/discussions/13664) |

### 2.2 Derived per-page LLM cost `[ESTIMATE — my arithmetic on the anchors above]`

A typical trade-book page: ~350–450 words ≈ **500–650 text tokens**. Serializing geometry + font features per line (bbox ×4, font name, size, weight/italic flags, indent) costs **~25–40 tokens per line**; at 35–45 lines/page that is **+900–1800 tokens**.

**Input ≈ 1500–2500 tokens/page.**

| Mode | Output tokens | Prefill (s) | Decode (s) | **Total/page** | **300-page book** |
|---|---|---|---|---|---|
| Decision-only (emit a small JSON verdict) | 50–150 | 16–26 | 3–9 | **19–35 s** | **1.6–2.9 h** |
| Rewrite text (emit corrected prose) | 500–700 | 16–26 | 31–43 | **47–69 s** | **3.9–5.8 h** |
| Same, on a fast 8–16 core desktop (assume 4–5× the laptop) | — | — | — | **4–8 s** (decision-only) | **20–40 min** |
| **Docling deterministic+ML pipeline, for comparison** | — | — | — | **0.41–1.06 s** | **2–5 min** |

**Two rules fall out of this table.**

1. **Never make the LLM re-emit prose.** Generation is 6× more expensive per token than prefill and is exactly where hallucination lives. The LLM must emit *decisions and spans*, never text.
2. **Budget the LLM at ≤20–25% of wall-clock.** With deterministic at ~0.6 s/page, that is ~0.15 s/page of LLM budget. At 4–8 s per LLM call, **you can afford an LLM call on ~2–4% of pages** — or, far better, **a handful of once-per-book calls.**

### 2.3 Why the "once-per-book" pattern wins arithmetically

A per-book call over a *style inventory* (6 heading-style clusters × 3 exemplar strings + counts + font stats) is **300–800 input tokens total** for the whole book: **3–8 s prefill, ~2–5 s decode ≈ 10 s once**, vs. 1.6–2.9 h for per-page. That is a **~1000× cost reduction** for the decisions that actually need semantics.

---

## 3. How Docling / Marker / MinerU / Unstructured actually decide structure

*(Research question (i).)*

### 3.1 Docling — heuristic sandwich around two small ML models

Docling's own technical report is explicit about the split ([arXiv 2408.09869](https://arxiv.org/html/2408.09869v5)):

1. **PDF backend — heuristic.** "retrieves the programmatic text tokens, consisting of string content and its coordinates on the page, and also renders a bitmap image."
2. **Model pipeline — ML.** Layout = an **RT-DETR-derived object detector retrained on DocLayNet**, run at **72 dpi**, "sub-second latency" on one CPU. Tables = **TableFormer**, a vision transformer, 2–6 s/table on CPU.
3. **Post-processing — heuristic.** "augments metadata, **detects the document language, infers reading-order** and eventually assembles a typed document object."

**Reading order in Docling is heuristic, not learned.** So is language detection. So is assembly. The learned parts are exactly two: *where are the boxes and what class are they*, and *what is the grid inside a table*. That is a good template for OpenConvert.

Docling later added a **VLM pipeline** (SmolDocling-256M, then Granite-Docling-258M) as an *alternative*, not a replacement ([VLM docs](https://docling-project.github.io/docling/usage/vision_models/)). What it gained, per the [Granite-Docling model card](https://huggingface.co/ibm-granite/granite-docling-258M) (vs SmolDocling):

| Task | SmolDocling-256M | Granite-Docling-258M |
|---|---|---|
| Layout (mAP / F1) | 0.23 / 0.85 | **0.27 / 0.86** |
| Code recognition (edit dist / F1) | 0.114 / 0.915 | **0.013 / 0.988** |
| Equations (edit dist / F1) | 0.119 / — | **0.073 / 0.968** |
| Table TEDS (structure / with content) | 0.82 / 0.76 | **0.97 / 0.96** |
| **Full-page OCR (edit dist / F1)** | 0.48 / 0.80 | **0.45 / 0.84** |
| OCRBench | 338 | **500** |

**Read the last-but-one row carefully.** A purpose-built 258M document VLM has **0.45 edit distance on full-page OCR**. That is not a text extractor; it is a *structure* extractor. Its wins are concentrated in code, equations and tables — content a novel does not contain. IBM's own announcement adds the crucial caveat that Granite-Docling exists partly to fix SmolDocling's "occasional tendency to get **stuck in loops of repeating the same token**" ([IBM announcement](https://www.ibm.com/new/announcements/granite-docling-end-to-end-document-conversion)).

### 3.2 Marker — explicitly a confidence cascade, and its authors say so

Marker's architecture is the closest published thing to what OpenConvert should build ([Marker repo](https://github.com/datalab-to/marker)):

- **Text:** `pdftext` — heuristic, reads the embedded layer in reading order.
- **Layout:** lightweight **rf-detr** (learned, CPU) in *fast* mode; the **Surya VLM** in *balanced* mode.
- **Quality gate:** heuristic logic decides whether embedded text is usable; garbled/scanned pages trigger OCR.
- **Tables:** "reconstructed from the text layer with **CPU heuristics**; **low-confidence reconstructions fall back to the VLM**."
- Design principle, in their words: *"only calls the VLM where necessary, which improves speed while keeping accuracy."*

Marker's own olmocr-bench table (from the repo):

| Mode | Overall | Digital-only | arXiv math | Tables | Multi-column | **Headers & footers** |
|---|---|---|---|---|---|---|
| Balanced (GPU, VLM) | 76.0 | 83.5 | 83.9 | 73.4 | 76.6 | **95.9** |
| Fast (GPU) | 66.6 | 71.6 | 23.4 | 69.0 | 76.0 | **93.2** |
| **Fast, no OCR (CPU)** | 43.6 | 55.8 | **0.0** | 46.1 | 67.0 | **92.8** |

This table is a gift to a book converter. Stripping *all* neural text recognition costs you everything on math (83.9 → 0.0) and a lot on tables (73.4 → 46.1), but **headers/footers barely move (95.9 → 92.8)** and multi-column loses only ~10 points. **The purely deterministic CPU path is already near-ceiling on exactly the sub-problems a novel cares about.**

**On `--use_llm`:** the docs say it handles *"merge tables across pages, handle inline math, format tables properly, and extract values from forms."* Note what is **not** in that list: reading order, headings, paragraphs, headers/footers, dehyphenation, chapters. Marker's LLM mode is scoped to **tables, math, and forms** — the three things that are hardest for geometry and least common in books. Backends: Gemini (default), Vertex, Claude, OpenAI-compatible, Azure, OpenRouter, **Ollama (local)**.

**Critically: I found no published ablation isolating the `--use_llm` contribution.** The benchmark tables compare *balanced/fast/no-OCR*, not *LLM on/off*. Treat any claimed `--use_llm` improvement as `[UNVERIFIED]`. Inspecting the LLM processor base class ([marker/processors/llm/__init__.py](https://github.com/VikParuchuri/marker/blob/master/marker/processors/llm/__init__.py)) shows `BaseLLMProcessor` / `BaseLLMSimpleBlockProcessor` / `BaseLLMComplexBlockProcessor`, block selection by `block_types`, and a `handle_rewrites()` that **try/except-parses the LLM response and logs on failure without an explicit fallback** — i.e. the validation layer is thin. OpenConvert should do better (§4.3).

### 3.3 MinerU — pipeline (2.x) → decoupled VLM (2.5)

MinerU 1.x/2.x was "PDF-Extract-Kit models" plus "**finely-tuned preprocessing and postprocessing rules**" ([arXiv 2409.18839](https://arxiv.org/abs/2409.18839)). MinerU2.5 is a **1.2B decoupled VLM** doing two-stage parsing: layout on a downsampled image, then recognition on **high-resolution crops** ([arXiv 2509.22186](https://arxiv.org/abs/2509.22186)). The decoupling is the interesting engineering idea — it is the same "cheap global pass, expensive local pass" pattern this report recommends, just realized in vision.

MinerU 2.5.4 scores **96.6 on olmOCR-Bench headers/footers** ([olmOCR repo](https://github.com/allenai/olmocr)) — but note it is a GPU-class model.

### 3.4 Unstructured — strategy switch, and an honest admission

`partition_pdf` offers `auto` / `fast` / `hi_res` / `ocr_only` ([Unstructured docs](https://docs.unstructured.io/open-source/core-functionality/partitioning)):

- `fast` = pdfminer + **rule-based heuristics** on the embedded text layer.
- `hi_res` = **detectron2_onnx** ML layout model.
- `ocr_only` = Tesseract + text partitioning.

The guidance contains a striking line: use `hi_res` when classification accuracy matters, but it **"acknowledges its difficulty with multi-column layouts"**, and `ocr_only` **"handles ... multi-column documents better than hi_res."** An ML layout detector is not automatically better than geometry at the thing geometry is best at.

### 3.5 GROBID — the counter-example that proves ML is worth it *when* the signal is semantic

GROBID is a **cascade of CRF sequence labellers** over "Layout Tokens" carrying Unicode text + font size/name + bold/italic/superscript + bbox, with `pdfalto` handling "the recovery of text order at block level, the detection of columns" ([GROBID principles](https://grobid.readthedocs.io/en/latest/Principles/)). Deep-learning variants buy "a few additional F1-score points" at "slower runtime"; CRF is the default "to maintain the ability to process PDF quickly, with commodity hardware, with low memory usage."

Its measured ceiling on 2000 bioRxiv PDFs, v0.8.1 ([GROBID benchmark](https://grobid.readthedocs.io/en/latest/Benchmarking-biorxiv/)):

| Field | F1 (strict) | F1 (soft) |
|---|---|---|
| Title | 77.26 | 79.47 |
| Authors | 82.84 | 83.35 |
| Abstract | 2.18 | **59.12** |
| **Section title** | — | **74.86** |
| Reference citation | — | 83.21 |
| DOI | 76.78 | — |

**This is the single most useful "how good is deterministic+CRF, really?" datapoint in the report.** Title at 77–79% and section-title at 75% — on the *most studied document genre in existence*, with a model trained on it — is the honest ceiling for feature-engineered structure inference. That gap is where an LLM has room (§6.7, §6.16). It is also why the LLM must be *validated*: 25% of section titles are already wrong before any model touches them.

---

## 4. Cross-cutting evidence

### 4.1 Can a small text-only LLM reason over coordinates? — No.

*(Research question (ii). This is the most decision-relevant evidence in the report.)*

**LayTextLLM** ([arXiv 2407.01976](https://arxiv.org/html/2407.01976v3)) ran the exact experiment the architect is asking about: feed a text LLM bounding boxes **as plain-text numbers** in the prompt.

| Setup | KIE avg F-score | VQA (ANLS/CIDEr) |
|---|---|---|
| **Llama2-7B-chat + coordinates as text tokens** | **34.3%** | 20.1 / 28.0 |
| LayTextLLM (learned coordinate → embedding projection) | **78.1%** | 147.9 / 229.1 |

**A 7B model given coordinates as numbers scores 34.3%.** OpenConvert's budget is 0.5–4B — strictly worse. And the token cost is brutal: coordinate-as-text produces **4085.7 tokens vs 664.3** on DocVQA, a **6.2× blow-up** — which, at 96 tok/s prefill, is the difference between 7 s and 43 s per page.

Corroboration from three directions:

- **DocLLM** ([arXiv 2401.00908](https://arxiv.org/abs/2401.00908)) does not put boxes in the prompt at all; it **decomposes the attention mechanism into disentangled matrices** to inject spatial structure. If coordinates-in-prompt worked, nobody would build this.
- **LayoutLLM** ([arXiv 2404.05225](https://arxiv.org/abs/2404.05225)) needs a dedicated **"layout instruction tuning"** stage — i.e. layout comprehension must be *trained in*, not prompted in.
- **SpatialEval** ([arXiv 2406.14852](https://arxiv.org/abs/2406.14852)) is more nuanced and worth reading honestly: text-only spatial input actually **beats** vision-only (Mistral-7B 62.1% text vs LLaVA-1.6-Mistral-7B 47.1% vision on Spatial-Grid; vision-only is near the 25% chance baseline on Spatial-Map/Maze-Nav). So textual geometry is *not useless*. But **62–72% at 7B is nowhere near production reliability**, and gains with scale were "modest" — which means shrinking to 1–4B does not recover it.

**Conclusion for OpenConvert.** Do not hand a 0.5–4B model raw `[x0,y0,x1,y1]` and expect spatial reasoning. If geometry must reach the LLM, **pre-digest it into categorical linguistic features** the model can actually use — `indent: "deep"`, `alignment: "centered"`, `gap_above: "large"`, `font: "body+2pt, italic"`, `column: 1` — never numbers. This is cheap, cuts tokens ~5×, and moves the task from spatial reasoning (which fails) to classification (which works).

### 4.2 Reading order: the deterministic algorithm *beats* the learned model

**XY-Cut++** ([arXiv 2504.10258](https://arxiv.org/html/2504.10258v2)) — a rule-based method with pre-masking, multi-granularity segmentation, and cross-modal matching:

**DocBench-100, BLEU-4 ↑**

| Method | Complex | Regular | Mean |
|---|---|---|---|
| XY-Cut (plain) | 0.749 | 0.818 | 0.797 |
| **LayoutReader (learned)** | 0.656 | 0.844 | **0.788** |
| MinerU | 0.701 | 0.946 | 0.873 |
| **XY-Cut++ (rule-based)** | **0.986** | **0.989** | **0.988** |

**OmniDocBench by layout type, BLEU-4 ↑**

| Method | Single | Double | Three-col | Complex | Mean |
|---|---|---|---|---|---|
| XY-Cut | 0.895 | 0.695 | 0.702 | 0.717 | 0.753 |
| LayoutReader | 0.988 | 0.831 | **0.595** | 0.716 | 0.783 |
| MinerU | 0.961 | 0.933 | 0.923 | 0.887 | 0.926 |
| **XY-Cut++** | **0.993** | **0.951** | **0.967** | **0.901** | **0.953** |

**Speed (FPS ↑):** XY-Cut 487 mean, **XY-Cut++ 514**, LayoutReader **22**, MinerU **11**.

Three findings, all load-bearing:

1. **A pure-geometry algorithm reaches 0.988 BLEU-4 — beating the learned model by 0.20** and beating MinerU's VLM-assisted ordering.
2. **LayoutReader *collapses* on three-column (0.595) — worse than naive XY-Cut (0.702).** The learned model generalizes *worse* than the rule outside its training distribution. This is the canonical warning against "the model will handle it."
3. **It is 23× faster.** 1.9 ms/page vs 45 ms/page.

Reading order is the sub-problem where hype is loudest and evidence is most decisively against it. Note also the current OmniDocBench leaderboard reading-order edit distances — PaddleOCR-VL-1.6 **0.1278**, MinerU2.5-Pro **0.120**, Marker **0.243** ([OmniDocBench repo](https://github.com/opendatalab/OmniDocBench)) — i.e. even the best GPU VLMs still get ~12% of reading order wrong, while a rule tuned for the layout family gets ~1%.

### 4.3 The false-repair risk, and the cascade that contains it

*(Research question (iii).)*

The danger is not that the LLM fails to help; it is that it **changes correct output into incorrect output**. Direct evidence:

- **Boros et al. 2024** ([LaTeCH-CLfL 2024](https://aclanthology.org/2024.latechclfl-1.14/)): 14 models (GPT-2/3/3.5/4, BLOOM 560M–7.1B, OPT 350M–6.7B, LLaMA/LLaMA-2-7B) across 8 benchmarks in EN/FR/**DE**/PL/CS/BG/SL/EL. Verdict, verbatim: **"LLMs mostly degrade the input text, occasionally leave it unchanged, and rarely improve it."** Their error taxonomy includes *"C3.5: Invention of a new text (context inconsistency) — the input text disappears and the model hallucinates by inventing a completely different story."*
- **Huang et al. 2024** ([arXiv 2310.01798](https://arxiv.org/abs/2310.01798), "LLMs Cannot Self-Correct Reasoning Yet"): *"LLMs struggle to self-correct their responses without external feedback, and at times, their performance even degrades after self-correction."* → **an LLM asked to review its own or a pipeline's output, with no external oracle, is a net-negative operation.**
- **Nougat** ([arXiv 2308.13418](https://arxiv.org/html/2308.13418v1)): degenerates into repetition on **~1.5% of test pages**, "increas[ing] significantly for documents outside the training domain," requiring a bespoke logit-variance stopping heuristic (window B=15, threshold 6.75). Generative decoding introduces a failure mode that does not exist in deterministic code.
- **Granite-Docling** was built partly to fix SmolDocling "getting stuck in loops of repeating the same token" ([IBM](https://www.ibm.com/new/announcements/granite-docling-end-to-end-document-conversion)).
- **Format constraints make it worse, not better.** "Let Me Speak Freely?" ([arXiv 2408.02442](https://arxiv.org/abs/2408.02442)) finds **"stricter format constraints generally lead to greater performance degradation in reasoning tasks."** So the JSON schema you add for safety **costs you accuracy** — a real trade, not a free lunch. Mitigation: keep schemas *flat and short* (enums and spans, not nested objects), and prefer many tiny schemas over one big one.

**The four-gate rule.** An LLM edit is applied **only if all four hold**:

1. **Gate D — deterministic uncertainty.** The deterministic confidence signal for this step is below its calibrated threshold. If deterministic is confident, the LLM is *never consulted*. (This alone eliminates >95% of false-repair opportunities, because false repairs overwhelmingly happen on cases that were already right.)
2. **Gate S — schema & self-consistency.** Output parses against the schema; enum values are legal; referenced span IDs exist; counts are consistent.
3. **Gate L — locality & conservation.** The edit is bounded: it may **relabel** a block, **merge/split at an existing boundary**, or **reorder within a page**. It may **never** alter character content except in the one whitelisted case (§6.3 dehyphenation, where the edit is "delete one hyphen"). Enforce a hard invariant: **the multiset of non-whitespace characters is preserved** across every structural LLM edit. Any violation → discard.
4. **Gate V — post-edit validation improves.** Re-run the deterministic quality statistics (§6.18) on the edited region. If any degrade, **revert**. This is the "external feedback" whose absence Huang et al. identified as fatal.

**Cascade economics** are well established — FrugalGPT ([arXiv 2305.05176](https://arxiv.org/abs/2305.05176)) reports matching GPT-4 with up to **98% cost reduction** via cascades. And Marker already ships this pattern: tables use "CPU heuristics; **low-confidence** reconstructions fall back to the VLM," because it "only calls the VLM where necessary."

**Default posture: fail-closed.** If the LLM is unavailable, times out, or fails any gate, the deterministic result stands and a warning is emitted. The LLM is *never* on the critical path for producing a valid EPUB.

### 4.4 Confidence signals — concrete, cheap, and (honestly) uncalibrated

*(Research question (iv).)*

Per-step signals that are measurable in O(1) or O(n) with no model:

| Step | Signal | Cheap? | Fires when |
|---|---|---|---|
| 1 Text extraction | fraction of glyphs mapping to U+FFFD / PUA / no ToUnicode; ratio of dictionary-hit words to total; char-class entropy vs. language prior | ✔ | garbled encoding |
| 2 Words/spacing | distribution of inter-glyph gaps: bimodality (Hartigan dip / simple 2-means separation) of intra-word vs inter-word gap; count of 1-char "words" | ✔ | no clean gap threshold |
| 3 Dehyphenation | lexicon hit for `A+B` vs `A-B`; in-document co-occurrence of the unhyphenated form (Calibre's exact trick) | ✔ | neither form attested |
| 4 Paragraphs | line-length regularity: σ/μ of justified line widths; first-line indent consistency (mode share); leading (baseline-gap) variance | ✔ | mixed/absent indent convention |
| 5 Columns | **gutter clarity**: width and emptiness of the whitespace valley in the x-projection profile; ratio of gutter width to modal word-space | ✔ | shallow/interrupted valley |
| 6 Headers/footers | **repetition ratio across pages** of normalized band text (digits → `#`); band y-position stability (σ of y across pages) | ✔ | ratio in 0.3–0.7 grey zone |
| 7 Headings | **font-size z-score separation** from body mode; number of distinct (size, weight, family) clusters; within-cluster consistency; silhouette score of the clustering | ✔ | clusters overlap / >6 clusters |
| 8 Chapters | agreement between PDF outline, TOC-page parse, page-break positions, and heading clusters (3-way vote) | ✔ | sources disagree |
| 9 Footnotes | presence of a superscript marker in body + matching marker at bottom band; font-size ratio to body; separator rule detection | ✔ | markers unmatched |
| 10 Captions | distance to nearest figure/table bbox; caption-prefix regex hit (`Fig.`, `Abb.`, `Şekil`, `Table`, `Tabelle`, `Tablo`); ambiguity = ≥2 candidate figures within 1.5× the min distance | ✔ | ambiguous association |
| 11 Lists | marker-regex consistency down the run; hanging-indent regularity; numbering monotonicity | ✔ | numbering gaps |
| 12 Tables | ruling-line count; cell-grid alignment residual; TableFormer/heuristic structure confidence; row/col count disagreement between methods | ✔ | grid residual high |
| 13 Quotes/verse | indent-delta z-score; short-line ratio; line-length σ; presence of quotation glyphs / attribution dash | ✔ | indent present but ambiguous |
| 14 OCR errors | dictionary hit rate; language-model perplexity from a **tiny n-gram model** (not an LLM); non-word rate vs. corpus prior | ✔ | rate above language prior |
| 16 Metadata | XMP/DocInfo present and non-boilerplate (`Microsoft Word - doc1.docx` → reject) | ✔ | absent/boilerplate |
| 18 Suspicion | **Gopher-family statistics** (see §6.18) | ✔ | thresholds breached |

**Honest gap.** I found **no paper that calibrates these specific signals against error rates for PDF→EPUB.** DocLayNet reports inter-annotator agreement ([arXiv 2206.01062](https://ar5iv.labs.arxiv.org/html/2206.01062)) which bounds achievable accuracy, and Nougat's variance heuristic is a worked precedent for a statistic-as-detector, but nobody has published reliability diagrams for "font-size z-score separation predicts heading error." **Recommendation:** build a 200–400 page internal gold set spanning EN/DE/TR, single/double column, novel/textbook/scanned, and fit each threshold to a target precision — e.g. *"escalate to LLM only where deterministic precision drops below 0.9."* Without this, thresholds are guesses and the cascade's central premise is unverified. **This should be a funded work item, not an afterthought — the whole architecture rests on it.**

### 4.5 Tiny document VLM vs general 1–4B text LLM, on low-confidence pages only

*(Research question (v).)*

**Verdict: neither, as a general page-level helper — but the tiny document VLM is the better of the two, and only for a narrow set of page types.**

| | Granite-Docling-258M (doc VLM) | General 1–4B text LLM |
|---|---|---|
| Cost/page on CPU | **6.15 s** (M3 Max, MLX) — **102 s** in Transformers | 19–35 s (1B, decision-only, laptop) |
| Sees actual glyph shapes | ✔ | ✘ |
| Full-page OCR quality | **edit dist 0.45** — poor | n/a |
| Table TEDS | **0.97 struct / 0.96 content** — excellent | n/a |
| Layout mAP | 0.27 (low) | n/a |
| Repetition/loop failure | documented; partly fixed vs SmolDocling | documented |
| Languages | EN; JA/AR/ZH experimental. **No German or Turkish claim** | broad |
| Reads long prose reliably | ✘ | ✔ (it is the input) |

**Where the tiny doc VLM earns its keep:** a page whose *text layer is broken* (no ToUnicode, garbled encoding) or whose *table structure* the geometry heuristics could not recover. On both, the text LLM has literally nothing to work with — it cannot see glyphs — while the VLM can. Note MinerU 2.5.4's **96.6** and PaddleOCR-VL's **97.0** on olmOCR-Bench headers/footers vs **Nanonets-OCR2-3B's 32.1** ([olmOCR repo](https://github.com/allenai/olmocr)): among small VLMs, *document-specialized training is everything*; general multimodal capability at 3B is worthless here.

**Where the text LLM earns its keep:** semantic judgements over *already-correct text* — is this heading a chapter or a section, is this indented block a quotation or a continuation, is this string the author's name. The VLM is bad at these (Granite-Docling layout mAP 0.27) and the text LLM is natively suited.

**They are complementary, not competing, and both should be optional, off by default, and rate-limited.** Practical policy: doc-VLM on `≤2%` of pages (broken-text-layer pages only), text LLM on `O(1)` per-book calls plus `≤3%` of blocks. Also note SmolDocling's headline claim — 256M "competes with VLMs up to **27× larger**" ([arXiv 2503.11576](https://arxiv.org/abs/2503.11576)) — cuts *both* ways: specialization beats scale, so a general 4B text model is the worst of both worlds for document structure.

---

## 5. The matrix

Legend — **Cost/page**: deterministic steps are μs–ms unless noted; LLM costs use §2.2. **Signal**: the escalation trigger from §4.4.

| # | Problem | Deterministic solution | LLM needed? | Hybrid? | Why (evidence) | Confidence signal | Cost |
|---|---|---|---|---|---|---|---|
| 1 | Text extraction (glyph→text, ToUnicode, ligatures) | PDF font `ToUnicode`/CMap via MuPDF/pdfium; ligature expansion (`TEXT_PRESERVE_LIGATURES`); CID fallback; **OCR** when broken | **No** | Only *OCR* fallback (not LLM) | LLM cannot see glyphs — any "fix" is invention. Calibre: ligatures ll/ff/fi "may convert incorrectly"; non-Unicode fonts → "garbled non-English output" ([Calibre](https://manual.calibre-ebook.com/conversion.html)). PyMuPDF exposes exactly these flags ([app1](https://pymupdf.readthedocs.io/en/latest/app1.html)) | U+FFFD/PUA share; dictionary hit rate | <10 ms |
| 2 | Word/line reconstruction & spacing | Gap-threshold clustering on glyph advances; baseline clustering into lines; `TEXT_INHIBIT_SPACES` for letter-spaced text | **No** | No | Solved for 20+ yr; pdftext/pdfalto/pdfact all ship it ([pdfact](https://github.com/ad-freiburg/pdfact)). Costs 6.2× tokens to even *state* to an LLM ([LayTextLLM](https://arxiv.org/html/2407.01976v3)) | gap-distribution bimodality | <10 ms |
| 3 | Dehyphenation (DE compounds, TR) | Soft-hyphen strip; line-end hyphen + lexicon lookup of joined form; **in-document unhyphenated-form dictionary** (Calibre); DE compound splitter (**CharSplit ~95%** head-detection on GermaNet, [repo](https://github.com/dtuggener/CharSplit)) | **No** (default) | **Yes, narrow** — batch residuals once per book | Deterministic resolves the vast majority; residual is genuinely lexical. TR is an evidence gap (§6.3) | lexicon miss on **both** joined and split forms | <5 ms; LLM ~1 batched call/book |
| 4 | Paragraph reconstruction | Line grouping by leading + indent mode + justification; Calibre's `line-unwrap-factor` (default **0.4**); merge across column/page breaks (pdfact) | **No** | Rarely | Calibre and pdfact both do this deterministically; a layout model adds block boundaries but not the merge logic | σ/μ of line widths; indent-mode share | <20 ms |
| 5 | Columns & reading order | **XY-Cut++**-class: projection-profile gutters + pre-mask + multi-granularity segmentation | **No** | No | **0.988 BLEU-4 vs LayoutReader 0.788; 514 vs 22 FPS**; LayoutReader *worse than naive XY-Cut* on 3-column ([arXiv 2504.10258](https://arxiv.org/html/2504.10258v2)). Coordinates-as-text at 7B = 34.3% | gutter width/emptiness | ~2 ms |
| 6 | Header/footer & page numbers | **Cross-page repetition** of normalized band text + y-stability; page-number regex (arabic/roman/DE/TR) | **No** | No | Marker CPU-only, no-OCR still scores **92.8** here ([Marker](https://github.com/datalab-to/marker)); a 3B VLM scores **32.1** ([olmOCR](https://github.com/allenai/olmocr)). Layout models see one page; repetition is inherently cross-page | repetition ratio (grey zone 0.3–0.7) | ~1 ms |
| 7 | Heading detection & level | Font-size/weight/family clustering + z-score vs body; numbering regex; ML layout `Section-header` class | Detection: **No**. **Level/semantics: yes** | **Yes — once per book** | DocLayNet `Title` **human agreement only 60–72%** — intrinsically semantic ([DocLayNet](https://ar5iv.labs.arxiv.org/html/2206.01062)); GROBID section-title **74.86 F1** soft. "Chapter 3" vs bold run-in vs large-font epigraph is *meaning*, not geometry | cluster silhouette; #clusters; z-gap | det. ~5 ms; **1 LLM call/book (~10 s)** |
| 8 | Chapter boundary / book structure / TOC | PDF outline via `get_toc()` ([PyMuPDF](https://pymupdf.readthedocs.io/en/latest/document.html)); TOC-page parse; Calibre chapter XPath + `chapter\|book\|section\|part` regex | **No** if outline exists | **Yes — once per book** | `get_toc()` returns **empty when no outline** — very common in scanned/older books. Front/back-matter ordering is world knowledge (Preface→Part I→Ch.1→Appendix→Index) | 3-way agreement of outline/TOC/clusters | det. ~50 ms/book; **1 LLM call/book** |
| 9 | Footnotes & reference association | Bottom-band + smaller-font + separator rule; superscript marker matching (digits/†‡*/DE-TR conventions) | **No** | Rarely | DocLayNet `Footnote` **77.2 mAP** (YOLOv5x6), human 83–91 — geometry-solvable; marker matching is exact string work | unmatched-marker count | ~5 ms |
| 10 | Caption detection & figure association | Prefix regex (`Fig./Abb./Şekil/Tab./Tabelle/Tablo`) + nearest-bbox + reading-order adjacency | **No** | No | DocLayNet `Caption` **77.7 mAP**, human 84–89. Association is a nearest-neighbour problem | ≥2 candidates within 1.5× min distance | ~2 ms |
| 11 | Lists (bullets/numbers/nesting/continuation) | Marker regex + hanging-indent + x-offset levels; numbering monotonicity across pages | **No** | No | DocLayNet `List-item` **86.2 mAP** — the *best-detected* structural class. Continuation is numbering arithmetic | numbering gaps; indent-level jumps | ~3 ms |
| 12 | Tables: detection, structure, image-vs-HTML | Ruling-line + alignment-grid heuristics; **TableFormer**-class model for structure; render-as-image only when structure confidence fails | **No** | **VLM** (not text LLM) on low confidence | TableFormer PubTabNet TEDS **98.5 simple / 95.0 complex** ([arXiv 2203.01017](https://arxiv.org/abs/2203.01017)). Marker: heuristics first, "low-confidence → VLM". **Accessibility forbids image-by-default**: DAISY — images of tables "take[] the content away from anyone who cannot see it" ([DAISY](https://kb.daisy.org/publishing/docs/html/tables-basics.html)) | grid residual; row/col disagreement | **2–6 s/table** (TableFormer, CPU) |
| 13 | Block quotes / poetry / verse / preformatted | Indent-delta + short-line ratio + line-length σ + quote glyphs + monospace-font test | **No** (default) | **Yes — best per-block LLM case** | Geometry cannot separate *verse* from *a narrow column of prose* from *a long indented quotation*: identical signatures, different semantics. No layout dataset even has these classes — DocLayNet has 11 classes, none of them "verse" | indent present + short-line ratio in grey zone | det. ~2 ms; LLM on **~1–3% of blocks** |
| 14 | OCR error post-correction | Dictionary + tiny char n-gram LM; confusion-pair rules (rn→m, l↔1, İ/I/ı); **re-OCR with different params** beats guessing | **No — actively harmful** | Only a **fine-tuned, language-specific** corrector, off by default | Boros et al.: **"LLMs mostly degrade the input text ... and rarely improve it"** across EN/**DE**/FR/PL/CS/BG/SL/EL, incl. 350M–7B models ([LaTeCH-CLfL 2024](https://aclanthology.org/2024.latechclfl-1.14/)). Contrast: *instruction-tuned* Llama-2 got **54.51% CER reduction** — but fine-tuned, English-only, 7B ([LT4HALA 2024](https://aclanthology.org/2024.lt4hala-1.14/)) | non-word rate vs language prior | det. ~20 ms |
| 15 | Image placement / anchoring in flow | Anchor at nearest reading-order boundary; keep with caption; float→inline conversion | **No** | No | Pure geometry + reading order (§5 already solved). EPUB reflow makes exact placement meaningless anyway | overlap with text columns | ~2 ms |
| 16 | Metadata (title/author) | XMP + DocInfo ([PyMuPDF](https://pymupdf.readthedocs.io/en/latest/document.html)); largest-font-on-p1 heuristic; filename parse | **No** (when XMP good) | **Yes — highest value/CPU-second** | XMP is frequently absent or boilerplate (`Microsoft Word - doc1.docx`). GROBID, purpose-built and trained: **title 77.26 F1 strict, authors 82.84** ([GROBID](https://grobid.readthedocs.io/en/latest/Benchmarking-biorxiv/)) — i.e. even the best deterministic system leaves 20% on the table, and this is *one short decision per book* | XMP absent/boilerplate; multiple large-font candidates on p1–3 | det. ~5 ms; **1 LLM call/book (~2–5 s)** |
| 17 | Language detection | **Lingua / CLD3 / fastText** | **No — never** | No | Lingua: German avg **89.27%** (99.70% on sentences); "a few dozen MB" RAM; **8.65 s (CLD2) / 21.13 s (Lingua)** for 3000 texts/lang vs 10m44s for langdetect ([lingua-py](https://github.com/pemistahl/lingua-py)). Book-length input → effectively 100%. An LLM is ~10⁴× more expensive for a solved problem | prediction margin between top-2 languages | ~1 ms/book |
| 18 | "Suspicious output" detection | **Gopher/MassiveText statistics** — exact thresholds in [datatrove](https://github.com/huggingface/datatrove/blob/main/src/datatrove/pipeline/filters/gopher_quality_filter.py): mean word length 3–10, symbol/word ≤0.1, non-alpha-word ≤0.8, ≥2 stop words; **repetition** ([datatrove](https://github.com/huggingface/datatrove/blob/main/src/datatrove/pipeline/filters/gopher_repetition_filter.py)): dup-line 0.30, dup-para 0.30, dup-line-char 0.20, dup-para-char 0.20, top-2/3/4-gram 0.20/0.18/0.16, dup-5..10-gram 0.15→0.10 | **No — LLM-as-judge is worse** | No | Battle-tested at web scale; O(n); zero false-repair risk. Huang et al.: self-correction **without external feedback degrades** ([arXiv 2310.01798](https://arxiv.org/abs/2310.01798)) — an LLM judge *is* the missing-oracle case. Bonus: the repetition thresholds **also catch LLM/VLM degeneration loops** (Nougat's 1.5% failure mode) | the statistics *are* the signal | ~5 ms/page |
| 19 | EPUBCheck message → repair action | **Static mapping table** over the ~**180+** documented IDs (RSC/OPF/HTM/PKG/CSS/MED/ACC/NAV/NCX/CHK/SCP/INF × 5 severities) ([EPUBCheck messages](https://w3c.github.io/epubcheck/docs/messages/)) | **No** | No | The message set is **finite, versioned, documented and stable**. A lookup table is 100% accurate, instant, testable, and auditable. An LLM here can only introduce nondeterminism into your correctness layer | unmapped message ID → log for the maintainer, do not guess | ~1 ms |
| 20 | User-visible warnings / explanations | Localized message templates (EN/DE/TR) with slot filling | **No** | No | Architect is right. Templates are translatable, reviewable, consistent, testable, and cost nothing. LLM phrasing is unauditable, unlocalizable without re-review, and can misdescribe what the tool did — a *trust* bug, not a style bug | n/a | ~0 |

---

## 6. Per-sub-problem notes and the Input → Detection → Decision → Repair → Validation sketches

Format per item: **(g)** the sketch; **(h)** LLM design where applicable.

### 6.1 Text extraction — Deterministic
**Best deterministic:** MuPDF/pdfium glyph→Unicode via `ToUnicode` CMap, falling back to font `Encoding`, then to the CID with `use CID instead of U+FFFD` ([PyMuPDF app1](https://pymupdf.readthedocs.io/en/latest/app1.html)). Ligatures expanded via `TEXT_PRESERVE_LIGATURES` (on by default). **Failure cases** (Calibre is blunt): "Special glyphs (ll, ff, fi) may convert incorrectly"; "Embedded non-Unicode fonts cause garbled non-English output"; "OCR'd PDFs may have inaccurate text layers" ([Calibre](https://manual.calibre-ebook.com/conversion.html)).
**(b) Small ML layout model:** irrelevant — operates on boxes, not glyphs.
**(c) Text-only LLM:** **no evidence, and structurally impossible** — the LLM's input *is* the broken text; it can only invent a plausible replacement. This is precisely the Boros "invention of a new text" failure.
**(d) VLM:** genuinely relevant — it re-reads the rendered pixels. But Granite-Docling's full-page OCR edit distance is **0.45**; a real OCR engine (Tesseract/Surya) is the correct fallback, not a doc-VLM.
**(e) Cost:** <10 ms/page deterministic; OCR fallback 0.5–3 s/page CPU.
**(f) Verdict: Deterministic**, with OCR (not LLM) fallback.
**(g)** Input: font dict + glyph stream. Detection: per-page U+FFFD/PUA rate, dictionary hit rate, char-class entropy vs. language prior. Decision: if garbled-rate > θ → mark page `text_layer_broken`. Repair: **re-render at 300 dpi and OCR**; never patch characters. Validation: Gopher stats on the OCR output must beat the original; else keep original and warn.
**(h)** No LLM.

### 6.2 Word/line reconstruction & spacing — Deterministic
**Best deterministic:** cluster glyphs into lines by baseline y (with tolerance = 0.3× font size); within a line, insert a space when the advance gap exceeds a threshold derived from the *observed bimodal gap distribution* for that font/size, not a fixed constant. Handle letter-spaced display text with `TEXT_INHIBIT_SPACES` ("H a l l o !" → "Hallo!"). **Failure cases:** justified text with extreme word-spacing; kerned display type; superscripts pulling the baseline; RTL/BiDi.
**(b)** A layout model gives block boundaries, not intra-line spacing — no help.
**(c)/(d)** No evidence of benefit; catastrophic token cost (§4.1).
**(e)** <10 ms/page. **(f) Verdict: Deterministic.**
**(g)** Input: glyphs with advances. Detection: fit 2-means to the gap distribution; confidence = separation / within-cluster spread. Decision: if separated → threshold at the midpoint; if not → fall back to font-metric default space width. Repair: none needed. Validation: resulting token stream passes dictionary hit-rate check.
**(h)** No LLM.

### 6.3 Dehyphenation — Deterministic (+ narrow batched escape hatch)
**Best deterministic (four tiers, in order):**
1. Strip U+00AD soft hyphens unconditionally.
2. Line-final `-` + next-line-initial lowercase → candidate join.
3. **In-document dictionary** (Calibre's "Remove unnecessary hyphens ... uses document content as a dictionary to remove hyphens from hyphenated words appearing elsewhere unhyphenated"). This is excellent because it is *self-calibrating to the book's own vocabulary* — proper nouns, neologisms, and domain terms included.
4. Language lexicon; for German, a compound splitter (**CharSplit: ~95% head-detection accuracy on the GermaNet compound test set**, trained on 1M Wikipedia nouns).

**Language-specific failure cases:**
- **German:** the hard case is a *genuinely hyphenated* compound (`Nord-Süd-Achse`, `E-Mail-Adresse`, `Goethe-Institut`) broken at its real hyphen — joining it is wrong. Signal: if the joined form is absent from both lexicon and in-document dictionary *and* both halves are independently attested capitalized nouns, **keep the hyphen**. Also: pre-1996 orthography splits `ck→k-k` and `ß/ss` alternation at line breaks in old scans.
- **Turkish:** `[UNVERIFIED — I found no published study on Turkish dehyphenation or post-OCR correction; this is reasoning from language structure, not from a fetched source.]` Turkish hyphenates at syllable boundaries and is agglutinative, so the joined form is very often a *productively inflected* type absent from any finite lexicon (`kitaplarımızdan`) — **dictionary hit rate is a much weaker signal than in German or English**. The right deterministic tool is a **morphological analyzer** (Zemberek/TRmorph class) used as an *acceptor*: "is `A+B` a valid surface form?" rather than "is it in a word list." Additional Turkish hazard: **dotted/dotless i** (`İ/i` vs `I/ı`) is simultaneously an encoding hazard, an OCR confusion pair, and a case-folding trap — normalize with Turkish-locale casing rules, never invariant `toLowerCase()`.
- **All languages:** hyphen at a page/column break where the continuation is far away in the stream.

**(b)** No. **(c)/(d)** No published numbers for LLM dehyphenation that I could reach.
**(e)** <5 ms/page. **(f) Verdict: Deterministic**, with an optional batched LLM pass on residuals.
**(g)** Input: line-final hyphen candidates + both halves + surrounding sentence. Detection: 4-tier lookup above. Decision: join / keep / **undecided**. Repair: join or keep. Validation: resulting token is not a new non-word; character multiset changes by exactly −1 hyphen.
**(h) LLM design (the one place a text LLM may touch characters):**
- **Why:** the residual set is genuinely lexical-semantic and unresolvable by lookup.
- **Input format:** one batched call per book with **all** unresolved candidates (typically 5–40 for a 300-page book), each as `{id, left, right, joined, context_sentence}`. **No coordinates.**
- **Prompt strategy:** few-shot, per-language, explicitly asking for `join | keep_hyphen | unsure`, with instruction to answer `unsure` when uncertain. Never ask it to produce the word — only to choose.
- **Output schema:** flat array `[{id: string, action: "join"|"keep"|"unsure"}]`. Deliberately minimal (§4.3: strict schemas cost accuracy — keep it flat).
- **Validation:** every `id` exists and appears once; `unsure` → deterministic default (keep hyphen). Applying the action must change the character multiset by exactly the removal of one `-` and nothing else.
- **Fallback:** LLM unavailable/invalid → keep hyphen (safe: a spurious hyphen is visible and fixable; a wrong join corrupts a word silently).
- **Cacheability: excellent.** Key = `(language, left, right)`. Vocabulary repeats heavily across a corpus; a persistent local cache makes this free after the first few books. Ship a precomputed cache for common DE/TR/EN pairs.
- **Failure handling:** on parse failure, fall back wholesale; log the candidate list for the user's review pane.

### 6.4 Paragraph reconstruction — Deterministic + ML layout model
**Best deterministic:** group lines into blocks by leading; detect the paragraph convention (first-line indent vs. blank-line separation) by taking the **mode** across the whole book; break a paragraph when a line is "short" relative to the justified measure AND the next line begins at the indent. Calibre parameterizes exactly this as `line-unwrap-factor` (default **0.4** = "remove breaks from lines shorter than 40% of the document's line length"). Merge across column and page boundaries (pdfact lists this explicitly as one of its four hard problems).
**Failure cases:** last line of a paragraph that happens to be full-measure; dialogue in novels (many genuinely short lines); poetry misread as prose; a paragraph interrupted by a floating figure.
**(b)** Yes, materially: DocLayNet `Text` **88.1 mAP** (YOLOv5x6) — the block boundaries a layout model gives are a strong prior for where paragraphs may not merge.
**(c)** No evidence. Note the token cost is at its worst here (the LLM must see every line's geometry).
**(e)** <20 ms/page + shared layout-model cost. **(f) Verdict: Deterministic + ML layout model.**
**(g)** Input: lines with x-extent, indent, leading. Detection: book-level convention inference; per-break confidence from (line-length z-score, indent match, leading z-score). Decision: merge/break. Repair: n/a. Validation: paragraph length distribution is plausible (no 1-word paragraphs en masse; no 3000-word paragraphs); this feeds §6.18.
**(h)** No LLM. *If ever added:* only as a per-book question about the paragraph convention (2 exemplars, 1 call) — not per break.

### 6.5 Columns & reading order — Deterministic + ML layout model
**Best deterministic:** XY-Cut++-class recursive projection-profile cutting with pre-masking of full-width elements (figures, tables, rules) before cutting. **Numbers are in §4.2 and they are decisive: 0.988 vs 0.788 vs the learned model, at 23× the speed, with the learned model *degrading below naive XY-Cut* on three-column layouts.**
**Failure cases:** sidebars that span partial height; pull-quotes crossing the gutter; rotated text; newspaper layouts with L-shaped articles.
**(b)** Yes — the layout model's *classes* (Picture/Table/Caption) are what you pre-mask with. This is the synergy: the ML model supplies the mask, the deterministic algorithm supplies the order.
**(c)** **Strong negative evidence** — LayTextLLM's 34.3% for a 7B model with coordinates-as-text.
**(d)** VLMs are better than they used to be (best OmniDocBench reading-order edit **0.120–0.128**) but still ~10× the error of a tuned rule, at GPU cost.
**(e)** ~2 ms/page + layout model. **(f) Verdict: Deterministic + ML layout model.**
**(g)** Input: blocks with bboxes + classes. Detection: x-projection valley analysis → gutter width/emptiness score. Decision: column count k and cut positions; if gutter score low → single-column fallback. Repair: recursive cut. Validation: **cross-page continuity** — does the last sentence of page *n* continue grammatically into the first of page *n+1*? A cheap deterministic proxy: page-final line ends without terminal punctuation AND page-initial line starts lowercase. If continuity breaks on many pages, the column hypothesis is wrong → re-run with k−1.
**(h)** No LLM. Note the validation check above is the highest-value deterministic signal in the whole pipeline and costs nothing.

### 6.6 Header/footer & page-number removal — Deterministic (beats everything)
**Best deterministic:** define top and bottom bands (e.g. outer 8% of the text area). Normalize band text (digits→`#`, case-fold, strip punctuation). Compute the **repetition ratio** across all pages, separately for odd/even pages (verso/recto headers commonly differ: book title on one side, chapter title on the other). Remove bands whose ratio exceeds θ and whose y-position σ is small. Page numbers: regex over arabic, roman (front matter!), and localized forms.
**Failure cases:** chapter-title running heads that change every chapter (repetition ratio is high *within* a chapter, low across the book — handle by computing repetition within sliding windows); a page number that is also body content; first-page-of-chapter suppression.
**(b)** **Actively worse.** DocLayNet YOLOv5x6 gets **Page-footer 61.1 / Page-header 67.9 mAP** while humans get **93–94 / 85–89** — the two worst model-vs-human gaps in the table. (Fair caveat: mAP@0.5–0.95 punishes thin boxes on strict localization, so this overstates the gap somewhat.) The deeper point stands: **a layout model looks at one page; repetition is definitionally cross-page.** The model cannot access the signal that solves the problem.
**(c)** No. **(d)** Mixed and unreliable: MinerU2.5.4 **96.6**, PaddleOCR-VL **97.0**, olmOCR **96.1** — but **Nanonets-OCR2-3B 32.1** on the same category. A small VLM may catastrophically fail here.
**(e)** ~1 ms/page. **(f) Verdict: Deterministic. Highest confidence verdict in this report.**
**(g)** Input: all pages' band text. Detection: repetition ratio (windowed + global), y-stability, odd/even split. Decision: remove / keep / uncertain. Repair: delete band blocks; **retain page numbers in a `page-list` nav for print-fidelity readers** rather than discarding. Validation: removed text must not contain sentence-continuing content (no lowercase start + no terminal punctuation before it).
**(h)** No LLM.

### 6.7 Heading detection & level assignment — **Hybrid (per-book LLM)**
**Best deterministic:** cluster all text runs by `(font family, size, weight, italic, color, alignment)`. Compute z-score of each cluster's size vs. the body-text mode. Candidate headings = clusters with z > θ, low token count, and high "starts a block" rate. Level = rank order of cluster size, plus numbering-regex evidence (`Chapter N`, `N.M`, `Kapitel N`, `Bölüm N`, roman numerals).
**Failure cases — exactly the three the architect names:**
- *"Chapter 3"* — large font, but is it a chapter head or a running head? (repetition check disambiguates)
- *bold run-in heading* — same size as body, bold, **inline with the paragraph it heads**. Font-size z-score is **zero**. Deterministic detection fails here almost completely.
- *large-font epigraph* — larger than body, centered, italic, **not a heading at all**. Geometrically identical to a subtitle.
- Also: drop caps (huge first glyph), small-caps chapter openers, decorative part-title pages.

**(b)** Helps but does not solve: DocLayNet `Section-header` **74.6 mAP** and — the key number — **`Title` inter-annotator agreement is only 60–72%**. When *humans* agree only 60–72% of the time, the task is not perceptual; it is interpretive. GROBID's trained CRF reaches **74.86 F1** on section titles. **~25% error is the deterministic/learned ceiling.**
**(c)** No direct benchmark I could reach. But this is the archetypal case where a text LLM has the right kind of knowledge: *"Chapter Three"*, *"Part II: The Long Road"*, *"§4.2 Methods"*, *"Bölüm 5"* are recognizable **as strings**, with no geometry needed.
**(d)** Not needed — text suffices.
**(e)** Deterministic ~5 ms/page. **One LLM call per book ≈ 10 s** (§2.3).
**(f) Verdict: Hybrid** — deterministic detection and clustering, LLM for cluster→role/level mapping, once per book.
**(g)** Input: all text runs with style features. Detection: style clustering + z-scores + numbering regex; confidence = silhouette score + z-gap + numbering coverage. Decision: if clusters are well-separated *and* numbering is consistent → assign levels deterministically, **no LLM**. Else escalate the *cluster inventory* (not the pages). Repair: apply the returned mapping to every member of each cluster. Validation: (i) h1 count is plausible (2–60 for a book); (ii) no level skips (h1→h3); (iii) heading order is monotone with page order; (iv) headings assigned `h1` correlate with page-break positions. Any failure → revert to deterministic ranking.
**(h) LLM design:**
- **Why:** the residual 25% is semantic, and the DocLayNet human-agreement number proves geometry cannot close it.
- **Input format:** **one call per book.** A compact style inventory: for each cluster, `{cluster_id, size_z, weight, italic, alignment, is_centered, count, starts_page_ratio, examples: [up to 5 verbatim strings]}`. **No bounding boxes, no page images.** Typically 6–12 clusters ⇒ **300–800 tokens.**
- **Prompt strategy:** "Here are the distinct text styles in a book, with examples. For each, say whether it is `chapter_heading`, `section_heading`, `subsection_heading`, `part_heading`, `running_head`, `epigraph`, `body`, `caption`, `other`." Provide the book's language. Ask for one label per cluster and nothing else. Few-shot with 2 exemplar inventories.
- **Output schema:** flat `[{cluster_id: int, role: <enum>}]`. Flat and enum-constrained, per §4.3.
- **Validation:** as in (g). Additionally, `chapter_heading` clusters must have count ≥ 2 and ≤ 200; `epigraph` must not be the most frequent large-font cluster.
- **Fallback:** deterministic size-rank ordering.
- **Cacheability: excellent** — key on a hash of the style inventory. Books from the same publisher/series share inventories exactly.
- **Failure handling:** if validation fails, keep deterministic and surface a "heading levels may be imprecise" warning (§6.20 template).
- **Run-in headings specifically:** these need a *separate*, cheap deterministic detector (bold/italic run at paragraph start, ≤8 words, ends with `.`/`—`/`:`) whose *candidates* can be batched into the same per-book call as a second question. Do **not** spend a per-page call on them.

### 6.8 Chapter boundary / book structure / TOC — **Hybrid (per-book LLM)**
**Best deterministic (three independent sources, then vote):**
1. **PDF outline** via `get_toc()` — returns `[lvl, title, page, dest]` from the embedded bookmark chain ([PyMuPDF](https://pymupdf.readthedocs.io/en/latest/document.html)). **When present, this is ground truth and no further work is needed.** Its documented limitation is decisive: it "does not generate a TOC from content analysis — it only reads existing bookmark structures", returning an **empty list** when absent.
2. **TOC-page parsing** — locate front-matter pages that are dominated by `title … dotted leader … page-number` lines; parse them.
3. **Heading clusters + page breaks** from §6.7.

Front/back matter: front matter is conventionally **roman-numeral paginated**; the arabic-1 reset is a hard, reliable boundary signal. Back matter is keyword-detectable (`Appendix/Anhang/Ek`, `Notes/Anmerkungen/Notlar`, `Bibliography/Literatur/Kaynakça`, `Index/Register/Dizin`, `Glossary`, `Acknowledg(e)ments/Danksagung/Teşekkür`).
**Failure cases:** unnumbered chapters; parts vs chapters conflated; a "Prologue"/"Epilogue" that is structurally a chapter; multi-volume works; front matter with no roman numerals.
**(b)** No — this is a document-level problem, not a page-level one.
**(c)** **Yes, plausibly the second-best LLM case.** Deciding that `["Preface", "Introduction", "Part One", "1. The Beginning", ..., "Epilogue", "Notes", "Index"]` maps to `frontmatter / frontmatter / part / chapter / ... / backmatter` requires exactly the world knowledge an LLM has and a regex does not — *and the entire input is a list of ~40 short strings.*
**(e)** Deterministic ~50 ms/book. **1 LLM call/book ≈ 5–10 s.**
**(f) Verdict: Hybrid.** Deterministic when a PDF outline exists (skip the LLM entirely); LLM on the heading list otherwise.
**(g)** Input: heading candidates (text, level, page), pagination style per page, PDF outline if any. Detection: 3-way agreement score. Decision: outline present → use it. Agreement high → deterministic. Else → LLM on the flat heading list. Repair: assign `frontmatter/part/chapter/backmatter` roles; build `nav.xhtml` + `toc.ncx`; set spine order and file splits. Validation: chapters are contiguous and non-overlapping; page numbers monotonically increase; every chapter has ≥1 page; front matter precedes body precedes back matter; the resulting nav passes EPUBCheck NAV_* checks.
**(h) LLM design:**
- **Input format:** `[{idx, text, page, style_cluster}]` — a flat list, typically 20–80 entries, **400–1200 tokens.**
- **Prompt strategy:** "This is the list of headings extracted from a book, in page order. Classify each as `frontmatter`, `part`, `chapter`, `section`, `backmatter`. Do not invent or reorder entries." Give the language.
- **Output schema:** `[{idx: int, role: <enum>}]`, same length as input.
- **Validation:** length equality; `idx` bijection; roles form a legal sequence (front matter contiguous at the start, back matter contiguous at the end, chapters monotone). **Reject the whole response** on any violation.
- **Fallback:** deterministic level-ranking + keyword rules.
- **Cacheability:** good — key on the heading-list hash.
- **Failure handling:** revert, warn, and expose the TOC in the user's review UI (a TOC is the *one* thing users reliably fix by hand — Calibre ships a dedicated ToC Editor for exactly this reason).

### 6.9 Footnotes & reference association — Deterministic + ML layout
**Best deterministic:** footnote zone = bottom band with font size < 0.9× body, often preceded by a short horizontal rule. Markers: superscript digits/symbols in body, matched to line-initial markers in the zone. Match by (a) exact symbol equality, (b) order, (c) page identity. Endnotes: collect from back-matter sections and match by chapter + number.
**Failure cases:** footnotes continuing onto the next page; markers rendered as regular-size text in brackets `[1]`; a footnote whose marker is inside a table; symbol cycles (`*, †, ‡, §`) that reset per page.
**(b)** Yes: DocLayNet `Footnote` **77.2 mAP** (human 83–91) — the model reliably finds the *zone*; matching is then exact string work.
**(c)/(d)** No evidence, no need.
**(e)** ~5 ms/page. **(f) Verdict: Deterministic + ML layout model.**
**(g)** Input: blocks + font sizes + superscript flags. Detection: zone identification + marker extraction; confidence = fraction of body markers with a zone match. Decision: if match rate > 0.9 → associate; else flag. Repair: build EPUB `<a epub:type="noteref">` ↔ `<aside epub:type="footnote">` pairs. Validation: **every noteref resolves to exactly one footnote and vice versa** (a bijection check — cheap, total, and it directly prevents the EPUBCheck `RSC_007` broken-reference error, §6.19).
**(h)** No LLM.

### 6.10 Caption detection & figure association — Deterministic + ML layout
**Best deterministic:** caption candidates = short blocks matching a localized prefix regex (`Fig(ure)?\.?\s*\d`, `Abb(ildung)?\.?\s*\d`, `Şekil\s*\d`, `Table|Tabelle|Tablo`) **or** blocks immediately above/below a figure bbox with smaller/italic font. Associate to the nearest figure by edge distance, preferring below-then-above (typographic convention), constrained to the same column.
**Failure cases:** two figures side by side with one caption between them; captions in the margin; multi-paragraph captions; a caption on the facing page.
**(b)** Yes: DocLayNet `Caption` **77.7 mAP** (human 84–89), and `Picture` **77.1**.
**(e)** ~2 ms/page. **(f) Verdict: Deterministic + ML layout model.**
**(g)** Input: figure bboxes + text blocks. Detection: prefix regex + distance ranking; confidence = ratio of best to second-best distance. Decision: associate if ratio > 1.5; else leave unassociated. Repair: wrap as `<figure><img/><figcaption/></figure>`. Validation: no figure has >1 caption; no caption is attached to a figure on another page.
**(h)** No LLM. *Distinct feature, out of scope here:* generating **alt text** for accessibility genuinely requires a VLM and is a legitimate optional feature — but that is content *creation*, not structure recovery, and should be a separate, explicitly user-invoked mode.

### 6.11 Lists — Deterministic
**Best deterministic:** marker regex (`•·–—*‣▪`, `\d+[.)]`, `[a-z][.)]`, roman) at line start followed by consistent hanging indent. Nesting level from the x-offset of the marker, quantized to observed indent steps. Continuation across pages/columns: a list continues if the next block's marker continues the numbering or repeats the bullet glyph at the same indent.
**Failure cases:** a paragraph beginning with a year (`1984 was...`) misread as an ordered item; dialogue with em-dashes; nested lists whose indent steps are not uniform.
**(b)** Yes, and it is the strongest class: DocLayNet `List-item` **86.2 mAP**, human 87–88 — **the model is essentially at the human ceiling here.**
**(e)** ~3 ms/page. **(f) Verdict: Deterministic** (ML layout as a cross-check).
**(g)** Input: lines + markers + indents. Detection: marker-consistency run-length; numbering monotonicity. Decision: open/continue/close list; nesting from quantized indent. Repair: emit `<ul>/<ol>/<li>`. Validation: ordered-list numbers are contiguous; nesting depth ≤ 5; no `<li>` outside a list.
**(h)** No LLM. Continuation across pages is arithmetic on the numbering — an LLM adds cost and risk for nothing.

### 6.12 Tables: detection, structure, and the image-vs-HTML decision — Deterministic + ML
**Best deterministic:** ruling-line extraction from vector graphics → grid; where rules are absent, whitespace-column alignment across rows. **Structure model:** TableFormer-class ViT — **PubTabNet TEDS 98.5 (simple) / 95.0 (complex)**; Granite-Docling reaches **TEDS-struct 0.97**. Cost: **2–6 s per table on CPU** — the single most expensive deterministic component.
**Failure cases:** borderless tables with ragged columns; spanning cells; nested tables; rotated tables; tables split across pages (this is exactly what Marker's `--use_llm` targets: *"merge tables across pages"*).
**The image-vs-HTML decision is a policy question, and accessibility settles it.** DAISY: *"including images of tables instead of the actual data takes the content away from anyone who cannot see it"*; tabular data must use `table`/`th`/`scope`/`caption`/`headers`; styling `td` to look like a header is called out as "a common bad practice" ([DAISY](https://kb.daisy.org/publishing/docs/html/tables-basics.html)). **Therefore: HTML by default; image only as an explicit, warned fallback when structure confidence fails — and when you do fall back, DAISY's own advice is to also link to the real data.**
**(b)** Required, not optional. **(c)** A text-only LLM sees the cell strings but not the grid — reconstructing a grid from coordinates is precisely the §4.1 failure. **(d)** Yes, VLMs are strong here and this is Marker's documented fallback ("low-confidence reconstructions fall back to the VLM").
**(e)** 2–6 s/table CPU. Budget-wise: a novel has ~0 tables, a textbook may have 50 — cap total table time and degrade to image beyond it.
**(f) Verdict: Deterministic + ML (TableFormer-class); optional VLM (not text LLM) on low confidence.**
**(g)** Input: table bbox + cells + rules. Detection: grid-alignment residual; row/col count agreement between the rule-based and model-based reconstructions. Decision: agreement → HTML; disagreement → VLM if enabled → else **image + warning + extracted text in a `<details>` fallback**. Repair: emit `<table>` with `th`/`scope`. Validation: every row has the same cell count after span expansion; no empty table; cell text multiset equals the source text multiset (**conservation check** — catches VLM hallucination in tables directly).
**(h)** No text LLM. If a VLM is used, gate it on the conservation check above; any invented or dropped cell text → discard and fall back to image.

### 6.13 Block quotes / poetry / verse / preformatted — **Hybrid (best per-block LLM case)**
**Best deterministic:** indent-delta z-score vs. body left margin; short-line ratio; line-length variance; presence of opening/closing quotation glyphs; a trailing attribution line (`— Author`); monospace font family ⇒ preformatted; centered short lines with high line-count ⇒ possibly verse.
**Failure cases — and they are severe:** verse, a long indented quotation, a narrow inset column, and an epigraph are **geometrically indistinguishable**. All four are: indented, short lines, non-justified. **No public layout dataset even has these classes** — DocLayNet's 11 classes contain none of `quote`, `verse`, `epigraph`. So there is no ML layout model to buy, and no benchmark to measure against.
**(b)** **No** — the classes do not exist in the training data.
**(c)** **This is the strongest per-block case for a text-only LLM in the entire matrix**, because: the distinguishing evidence is *linguistic* (metre, rhyme, line-break placement, quotation framing, tense/voice shift); the input is short (a single block, 50–300 tokens); geometry contributes almost nothing so you need not serialize it; the output is a single enum; validation is trivial; and the change is purely a **CSS class**, so a wrong answer degrades presentation but **cannot corrupt text**. `[No published benchmark exists for this task that I could reach — the argument is structural, not empirical.]`
**(d)** No advantage.
**(e)** Deterministic ~2 ms/page; LLM only on **~1–3% of blocks**. With ~15 blocks/page × 300 pages = 4500 blocks, 2% = **90 blocks**. Batched 10 per call at ~1500 tokens/call ⇒ 9 calls ≈ **2–4 minutes** on a laptop CPU. **Cap this at a hard budget** (e.g. 30 blocks/book) or make it opt-in for poetry/literary presets.
**(f) Verdict: Hybrid** — deterministic first, LLM on low-confidence blocks only, budget-capped.
**(g)** Input: indented/short-line blocks. Detection: indent z-score, short-line ratio, line-length σ, quote glyphs, monospace test. Decision: confident → assign `blockquote`/`pre`/`verse`/`paragraph`. Ambiguous **and** budget remains → LLM. Repair: assign the semantic wrapper + EPUB CSS class. Validation: **text is byte-identical before and after** (only the wrapper changes); verse blocks preserve original line breaks (`<br/>` or `poem` line divs) rather than being reflowed.
**(h) LLM design:**
- **Why:** purely semantic; no dataset or heuristic exists; risk is bounded to CSS.
- **Input format:** batched, 5–10 blocks per call, each `{id, text (verbatim, up to ~60 words), indent: "shallow"|"deep", lines: N, avg_line_words: N, centered: bool, monospace: bool}`. **Categorical geometry, no numbers** (§4.1).
- **Prompt strategy:** "Classify each block as `verse`, `blockquote`, `preformatted`, or `paragraph`. Answer `paragraph` when unsure." Few-shot with one example of each in the book's language.
- **Output schema:** `[{id, type: <enum>}]`.
- **Validation:** ids bijective; **text unchanged** (hard invariant); `preformatted` requires the monospace flag; `verse` requires ≥3 lines and short-line ratio > 0.6 (i.e. the LLM cannot override strong deterministic counter-evidence).
- **Fallback:** `paragraph` (the safe default — a missed blockquote is a cosmetic loss; a wrongly-declared `<pre>` breaks reflow on a phone).
- **Cacheability:** moderate — key on block-text hash. Low reuse across books, high reuse across re-runs of the same book (which matters a lot during user iteration).
- **Failure handling:** budget exhausted or LLM absent → all remaining ambiguous blocks become `blockquote` if indented, else `paragraph`.

### 6.14 OCR error post-correction — Deterministic detection; LLM correction OFF by default
**Best deterministic:** dictionary + character n-gram LM to *locate* suspect tokens; confusion-pair rules (`rn→m`, `l↔1↔I`, `0↔O`, `cl→d`, and for Turkish `İ↔I↔l↔1`, `ş↔s`, `ğ↔g`, for German `ß↔B`, `ä↔ii`); **and, most importantly, re-OCR the region with different engine parameters/PSM and compare** — agreement between two independent OCR passes is far more trustworthy than any single generative guess.
**(c) Evidence — this is where the hype is most dangerous and the evidence most direct:**
- **Against:** Boros et al. tested **14 models** (including 350M–7B, i.e. exactly OpenConvert's size class) on **8 benchmarks in EN/FR/DE/PL/CS/BG/SL/EL** and concluded: *"LLMs are not good at correcting transcriptions of historical documents of any kind ... they usually degrade them"* and *"LLMs mostly degrade the input text, occasionally leave it unchanged, and rarely improve it."* Their error taxonomy includes wholesale invention of new text.
- **For, with heavy caveats:** Thomas, Gaizauskas & Lu got **54.51% CER reduction** with an **instruction-tuned Llama-2** (vs 23.30% for fine-tuned BART) on BLN600, 19th-c British newspapers. But: *fine-tuned on in-domain parallel data*, *English only*, *7B*. This is not "an off-the-shelf 1B model helps"; it is "a task-specific fine-tune on matched data helps."
- **German:** covered in Boros et al.'s negative result. **Turkish: no evidence found — this is a genuine gap** `[UNVERIFIED]`. Turkish's agglutinative morphology means a corrector must respect vowel harmony and suffix legality; a general LLM at 1–4B has weak Turkish coverage and will confidently produce non-words that *look* Turkish. Risk here is higher than for German, not lower.
**(d)** A VLM re-reading pixels is a legitimate second opinion; a text LLM guessing from corrupted text is not.
**(e)** Deterministic ~20 ms/page. LLM correction requires *generation* of prose = the 47–69 s/page mode = **the worst cost profile in the matrix**, paired with the worst evidence.
**(f) Verdict: Deterministic detection. LLM correction disabled by default.** Ship it, if at all, as an explicitly labelled experimental toggle with a diff view.
**(g)** Input: OCR text + per-token confidence (Tesseract/Surya expose these). Detection: non-word rate vs. language prior; low-confidence token clusters. Decision: if non-word rate > θ → **re-OCR with alternate parameters**; if two passes agree, accept; if they disagree, mark uncertain. Repair: only apply confusion-pair rules where the corrected form is in-lexicon **and** the original is not. Validation: edit distance per token ≤ 2; corrected token is in-lexicon; **total character count change < 1%**.
**(h)** If the experimental toggle is enabled: input = single suspect token + 10 words of context; output = the corrected token **only** (never a sentence); reject any output with edit distance > 2 from the original or containing characters outside the language's alphabet; require the result to be in-lexicon; show every change in a user-facing diff. Cacheability high (token+context hash). **Never enable by default, and never for Turkish until an evaluation exists.**

### 6.15 Image placement / anchoring — Deterministic
**Best deterministic:** anchor each image at the nearest block boundary in reading order; keep figure+caption together; convert page-floats to inline at the anchor; preserve relative order. **Failure cases:** full-page plates with no textual anchor; images that belong to a facing-page spread; decorative rules and ornaments (should be dropped, not anchored).
**(b)** Yes — `Picture` **77.1 mAP** (human 69–71, i.e. **the model actually exceeds human agreement's lower bound here**, because "what counts as one picture" is genuinely ambiguous to humans).
**(e)** ~2 ms/page. **(f) Verdict: Deterministic.**
**(g)** Input: image XObjects + bboxes + reading order. Detection: overlap with text columns; size ratio (tiny + repeated ⇒ ornament/logo, drop). Decision: inline / float / drop. Repair: emit `<figure>` at the anchor. Validation: no image emitted twice; every image referenced from the spine; image count matches extraction count (an EPUBCheck `RSC_007`/`OPF` guard).
**(h)** No LLM. In a reflowable EPUB the reading system controls placement anyway — precision beyond "correct position in the flow" is unobservable.

### 6.16 Metadata extraction — **Hybrid (highest LLM value per CPU-second)**
**Best deterministic:** read XMP (`get_xml_metadata()`) and DocInfo (`Document.metadata`); if `dc:title`/`dc:creator` are present and not boilerplate, done. Else: largest-font text block on pages 1–3, plus a `by|von|yazan` pattern, plus filename parsing.
**Failure cases:** DocInfo containing the *converter's* output (`Microsoft Word - Document1.docx`, `untitled.indd`) — extremely common and **worse than nothing, because it looks valid**; title split across two font sizes; subtitle mistaken for title; author vs. translator vs. editor vs. series editor on a title page listing all four; "A Novel" absorbed into the title.
**(b)** No. This is not a layout problem.
**(c)** **Best LLM case in the matrix, purely on economics.** Input is a title page: **~100–200 tokens**. Output is **~30 tokens**. **One call per book ≈ 2–5 s** — under 2% of a 300-page conversion. And the deterministic ceiling is demonstrably low: GROBID, purpose-built and CRF-trained on exactly this task, achieves **title F1 77.26 strict / 79.47 soft, authors 82.84** — leaving ~20% of the most user-visible field in the book wrong.
**(e)** **2–5 s per book.** **(f) Verdict: Hybrid** — XMP/DocInfo first, LLM on the title page when metadata is absent or boilerplate.
**(g)** Input: XMP + DocInfo + text of pages 1–3 with style features. Detection: is metadata present and non-boilerplate (blocklist of known converter strings)? Decision: present → use it. Absent/boilerplate → LLM on title-page text. Repair: populate OPF `dc:title`, `dc:creator` (with `role` refinement), `dc:language`, `dc:publisher`, `dc:date`, `dc:identifier`. Validation: **every returned string must appear verbatim as a substring of the input pages** (the strongest and cheapest anti-hallucination check available anywhere in this pipeline); title length 1–200 chars; author name shape is plausible; language matches §6.17's detection.
**(h) LLM design:**
- **Why:** cheapest possible LLM invocation with the highest user-visible payoff; deterministic ceiling is provably ~20% short.
- **Input format:** verbatim text of pages 1–3 with `[LARGE]`/`[MEDIUM]`/`[SMALL]`/`[CENTERED]` inline style annotations. **No coordinates.** ~200–400 tokens.
- **Prompt strategy:** "Extract the book's title, subtitle, author(s), translator, publisher, and year from this title page. Copy strings exactly as they appear. If a field is not present, return null. Do not guess."
- **Output schema:** flat object of nullable strings + `authors: [string]`.
- **Validation:** **verbatim-substring check on every field** (case- and whitespace-normalized); nulls accepted; reject the whole response if any field fails.
- **Fallback:** largest-font-block heuristic, then filename.
- **Cacheability: excellent** — key on the page-1–3 text hash; identical for every copy of the same edition.
- **Failure handling:** revert to heuristic and surface the fields in the user's editable metadata pane. Metadata is the one thing users *expect* to review before export, so a visible, editable form is better UX than any amount of model accuracy.

### 6.17 Language detection — Deterministic, no LLM
**Best deterministic:** Lingua (or CLD3/fastText) over the concatenated body text. Lingua: German **89.27% average**, **99.70% on full sentences**; "a few dozen megabytes" RAM; **21.13 s for 3000 texts per language** (high-accuracy multithreaded), vs CLD2 at 8.65 s and langdetect at 10m44s.
With a whole book as input, single-language accuracy is **effectively 100%**. The real problems are *multilingual* books (quotations in Latin/French/Greek, bilingual editions), which is a **per-block** detection task where Lingua's word-pair accuracy (93.90% DE) matters — and where short-input accuracy (74.20% single word) means you should require ≥5 words before assigning a `lang` attribute.
**(b)/(c)/(d)** No. An LLM is roughly **10⁴× more expensive** for a fully solved problem, and would be *less* accurate on short spans than a character-n-gram model built for it.
**(e)** ~1 ms/book. **(f) Verdict: Deterministic. This is a hard "no LLM", with no caveats.**
**(g)** Input: body text. Detection: Lingua over the whole book (primary) and over each block ≥5 words (secondary). Decision: assign `dc:language`; set `xml:lang` on blocks whose detected language differs from the primary **and** whose top-2 confidence margin exceeds θ. Repair: none. Validation: primary language must match the dominant script; per-block overrides must be < 20% of blocks (else the primary detection was wrong).
**(h)** No LLM.

### 6.18 "Suspicious output" detection — Deterministic statistics
**Best deterministic:** the Gopher/MassiveText quality heuristics, whose exact production thresholds are public in HuggingFace's `datatrove` implementation:

*Quality* ([gopher_quality_filter.py](https://github.com/huggingface/datatrove/blob/main/src/datatrove/pipeline/filters/gopher_quality_filter.py)): `min_doc_words 50`, `max_doc_words 100000`, `min_avg_word_length 3`, `max_avg_word_length 10`, `max_symbol_word_ratio 0.1`, `max_bullet_lines_ratio 0.9`, `max_ellipsis_lines_ratio 0.3`, `max_non_alpha_words_ratio 0.8`, `min_stop_words 2`.

*Repetition* ([gopher_repetition_filter.py](https://github.com/huggingface/datatrove/blob/main/src/datatrove/pipeline/filters/gopher_repetition_filter.py)): `dup_line_frac 0.3`, `dup_para_frac 0.3`, `dup_line_char_frac 0.2`, `dup_para_char_frac 0.2`, `top_n_grams ((2,0.2),(3,0.18),(4,0.16))`, `dup_n_grams ((5,0.15),(6,0.14),(7,0.13),(8,0.12),(9,0.11),(10,0.10))`.

Add domain-specific ones: U+FFFD/PUA share; dictionary hit rate vs. the language prior; character-class entropy; **and the duplicate-paragraph check the architect names, which the `dup_para_frac`/`dup_para_char_frac` pair covers exactly.**

**Two properties make this the keystone of the architecture:**
1. **It costs nothing** (O(n) string statistics, ~5 ms/page) and has **zero false-repair risk** — it only *flags*, never edits.
2. **The same repetition statistics detect LLM and VLM degeneration loops.** Nougat degenerates on ~1.5% of pages; SmolDocling gets "stuck in loops of repeating the same token." `dup_n_grams` catches both. So the deterministic validator that guards your input **also guards your LLM's output**, for free.

**(c) LLM-as-judge: no.** Huang et al. is directly on point — self-correction without external feedback degrades performance; an LLM judge over your own pipeline's output *is* the no-external-feedback case. Plus it inverts the economics: you would pay the most expensive component to evaluate the cheapest.
**(e)** ~5 ms/page. **(f) Verdict: Deterministic.**
**(g)** Input: final text per page and per book. Detection: the statistics above. Decision: page-level `ok / suspicious / broken`. Repair: **route, don't fix** — `broken` → re-extract via OCR; `suspicious` → surface in the review pane with the specific failing statistic named. Validation: this *is* the validation layer; it runs before and after every optional model stage, and any stage that worsens a statistic is reverted (Gate V, §4.3).
**(h)** No LLM.

### 6.19 EPUBCheck feedback → repair actions — Deterministic mapping table
**Best deterministic:** a static table over EPUBCheck's documented message IDs. The catalogue is **~180+ IDs** with structured prefixes (`RSC`, `OPF`, `HTM`, `PKG`, `CSS`, `MED`, `ACC`, `NAV`, `NCX`, `CHK`, `SCP`, `INF`) and five severities (Fatal / Error / Warning / Info / Usage) ([EPUBCheck messages reference](https://w3c.github.io/epubcheck/docs/messages/)).
**Why deterministic wins decisively:** the message set is **finite, enumerable, versioned, officially documented, and stable**. A lookup table is 100% accurate on covered IDs, instant, unit-testable, and reviewable in a PR. An LLM here would inject nondeterminism into the *correctness-verification layer* — the one layer that must be trustworthy — and would occasionally "repair" a valid EPUB into an invalid one. Since OpenConvert generates the EPUB itself, it knows exactly which of its own emitters produced each construct, so the mapping is `message_id → emitter → fix`, which is even more tractable than generic repair.
**Practical design:** cover the ~30 IDs your own generator can plausibly trigger (`RSC_007` broken reference, `OPF_003` resource not declared, `HTM_*` markup, `NAV_*` nav structure, `PKG_007` mimetype, `ACC_001` missing alt) with real fixes. For any **unmapped** ID: **do not guess.** Log it verbatim, surface it to the user, and open a maintainer issue. An unmapped message is a bug in your table, and the correct response is to fix the table — not to ask a model to improvise. That is a *feature*: the table's coverage is a measurable quality metric.
**(e)** ~1 ms. **(f) Verdict: Deterministic.**
**(g)** Input: EPUBCheck JSON output. Detection: parse `id`, `severity`, `location`. Decision: table lookup → `auto_fix` / `warn_user` / `unmapped`. Repair: apply the mapped transform. Validation: **re-run EPUBCheck**; the fix must reduce the error count and introduce no new IDs, else revert. (Bounded at 3 iterations to prevent oscillation.)
**(h)** No LLM.

### 6.20 User-visible warnings & explanations — Deterministic templates
**The architect's instinct is correct, and the reasons are stronger than "it's cheap":**
- **Localization.** OpenConvert targets EN/DE/TR. Templates are translated once and reviewed by a human. LLM-generated German or Turkish from a 1–4B model is unreviewable at scale and will be visibly poor — a small model's Turkish is *worse* than its English, so the users most in need of clarity get the worst text.
- **Auditability.** A warning is a factual claim about what the software did ("3 tables were rendered as images because their structure could not be recovered"). If the model paraphrases, it can misstate the fact. That is a **trust bug**, not a style issue.
- **Testability.** You can assert on template output. You cannot assert on generated prose.
- **Determinism.** The same input must produce the same warning, or bug reports become irreproducible.
- **Cost.** Zero vs. seconds.
**(f) Verdict: Deterministic templates with slot filling.** No LLM, not even optionally.
**(g)** Input: the structured warning record from any stage. Detection: n/a. Decision: select template by `warning_code` + locale. Repair: fill slots (counts, page numbers, block ids). Validation: every `warning_code` has a template in every supported locale (a CI check).
**(h)** No LLM.

---

## 7. Ranked lists

### 7.1 Where the LLM adds the most value per CPU-second

| Rank | Sub-problem | LLM cost per book | Why it wins | Risk if wrong |
|---|---|---|---|---|
| **1** | **#16 Metadata (title/author)** | **1 call, 2–5 s** | ~300 input tokens; deterministic ceiling provably ~20% short (GROBID title F1 77.26); **verbatim-substring validation makes hallucination detectable with certainty**; most user-visible field in the whole book | Low — user reviews it in an editable form anyway |
| **2** | **#8 Book structure / TOC** | **1 call, 5–10 s** (skipped entirely when the PDF has an outline) | Input is ~40 short strings; requires world knowledge (`Prologue` is a chapter, `Index` is back matter) that no regex has; output is order- and count-checkable | Low–moderate — a bad TOC is visible and hand-fixable |
| **3** | **#7 Heading level assignment** | **1 call, ~10 s** | DocLayNet `Title` **human agreement 60–72%** proves the residual is interpretive, not perceptual; the per-book style-inventory framing costs ~600 tokens instead of ~600k | Moderate — validated by level-skip and monotonicity checks |
| **4** | **#13 Quote / verse / preformatted** | ~9 batched calls, **2–4 min** (cap it) | No dataset, no heuristic, no layout class exists — genuinely unserved; **the edit is a CSS class, so text cannot be corrupted** | Very low by construction |
| **5** | **#3 Dehyphenation residuals** | 1 batched call, **2–5 s** | Small residual set; lexical judgement; heavily cacheable across a corpus | Low — bounded to removing one hyphen, and validated |

**Everything above the line totals ~4 LLM calls and ~20–30 CPU-seconds per book** (excluding the optional, capped #13). Against a ~4-minute deterministic conversion that is **~10%** overhead for meaningful gains on metadata, TOC and headings — the three things a reader notices first when opening an EPUB.

### 7.2 Where the LLM adds nothing, or actively harms

| Rank | Sub-problem | Why it fails |
|---|---|---|
| **1** | **#14 OCR post-correction** | **Measured degradation.** Boros et al., 14 models incl. 350M–7B, 8 languages incl. German: *"LLMs mostly degrade the input text ... and rarely improve it"*, with documented invention of entirely new text. Also the most expensive mode (must generate prose, 47–69 s/page). Worst evidence + worst cost. |
| **2** | **#1 Text extraction / encoding repair** | Structurally impossible: the LLM's only input is the corrupted text. Any output is invention, and it will be *fluent* invention — undetectable by the user. The correct fallback is OCR. |
| **3** | **#5 Reading order from coordinates** | **XY-Cut++ 0.988 vs LayoutReader 0.788, at 23× the speed.** A 7B model with coordinates-as-text scores **34.3%** on layout-dependent extraction, at 6.2× the tokens. A 1–4B model is strictly worse. |
| **4** | **#18 "LLM as judge" for suspicious output** | Huang et al.: self-correction without external feedback degrades. Inverts the economics — most expensive component evaluating the cheapest. Gopher statistics do it in 5 ms with zero false-repair risk. |
| **5** | **#17 Language detection** | Solved to 99.7% on sentences by a few-dozen-MB FST at ~1 ms/book. An LLM is ~10⁴× the cost and *less* accurate on short spans. |
| **6** | **#19 EPUBCheck → repair mapping** | ~180 documented, versioned, stable IDs. A table is 100% accurate and testable. An LLM injects nondeterminism into the correctness layer. |
| **7** | **#20 Warning phrasing** | Unlocalizable without human re-review (and small-model Turkish/German is poor), unauditable, untestable, non-reproducible — for zero benefit. |
| **8** | **#6 Header/footer removal** | The solving signal is **cross-page repetition**, which no per-page model can access. Deterministic scores **92.8** even in Marker's CPU-only no-OCR mode; a 3B VLM scores **32.1**. |
| **9** | **#2 Word/line reconstruction** | 20-year-solved geometry; costs 6.2× tokens merely to *describe* to a model. |
| **10** | **#12 Table structure from a text LLM** | The grid is spatial; §4.1 says that is exactly what fails. Use TableFormer (TEDS 98.5) or a VLM. |
| **11** | **#11 List detection** | DocLayNet `List-item` **86.2 mAP** vs human 87–88 — already at the ceiling. Continuation is arithmetic. |
| **12** | **#10 Caption association** | Nearest-neighbour on bboxes. `Caption` 77.7 mAP; ambiguity is rare and detectable. |
| **13** | **#15 Image anchoring** | Reflowable EPUB makes fine placement unobservable. |
| **14** | **#4 Paragraph reconstruction (per break)** | Worst token-cost profile (every line's geometry) for a problem Calibre solves with one scalar (`line-unwrap-factor 0.4`). |
| **15** | **#9 Footnote association** | A bijection check on markers. Exact, total, free. |

---

## 8. Answers to the specific research questions

**(i) How Docling / Marker / MinerU / Unstructured decide structure.** §3. All four are **heuristic sandwiches around 1–2 small learned models**. In every case the learned component answers *"where are the boxes and what class are they"* and *"what is the grid inside this table"*; **everything else — reading order, language, assembly, headers, paragraphs — is rules.** Docling states this explicitly ("Post-Processing ... infers reading-order"), Marker states its cascade principle explicitly ("only calls the VLM where necessary"), Unstructured exposes it as a user-facing strategy switch and even admits `hi_res` is *worse* than OCR-based partitioning on multi-column. On LLM post-processing: **Marker's `--use_llm` is scoped to tables-across-pages, inline math, table formatting, and form values — not to structure — and ships with no published ablation** `[UNVERIFIED improvement]`. Docling's VLM route gained tables (TEDS 0.82→0.97), code (0.114→0.013) and equations (0.119→0.073), but full-page OCR stayed poor (edit distance **0.45**) and IBM's own announcement cites fixing **token-repetition loops** as a motivation. MinerU 2.x moved to a **1.2B decoupled VLM** with the same cheap-global/expensive-local structure this report recommends.

**(ii) Papers on LLM reading-order correction / layout post-processing / text-only LLM with boxes.** §4.1–4.2. The literature does **not** support prompting coordinates to a text LLM. LayTextLLM's own baseline — **Llama2-7B-chat with coordinates as text = 34.3% vs 78.1%** with learned projections, at **6.2× the tokens** — is the cleanest single number. DocLLM rebuilds attention rather than prompting boxes; LayoutLLM requires dedicated layout instruction tuning; SpatialEval shows 7B models at 62–72% on textual spatial tasks with only modest gains from scale. And the task itself does not need a model: **XY-Cut++ beats LayoutReader by 0.20 BLEU-4 at 23× the speed, with LayoutReader falling below naive XY-Cut on three-column pages.** **A 1–4B model will not do spatial reasoning from numbers. If geometry must reach an LLM, discretize it into words first.**

**(iii) False-repair risk and cascade mitigation.** §4.3. Documented from four directions: measured degradation on post-correction (Boros et al.), degradation from self-correction without an external oracle (Huang et al.), generative degeneration loops (Nougat 1.5%; SmolDocling; both requiring bespoke detectors), and **accuracy loss caused by the JSON schema you add for safety** ("Let Me Speak Freely?"). Mitigation is the **four-gate rule**: act only when (D) deterministic confidence is low, (S) the output parses and is self-consistent, (L) the change is local and **conserves the character multiset**, and (V) post-edit deterministic statistics do not worsen. Gate D alone removes most of the risk, because false repairs concentrate on cases that were already correct. Gate V is the "external feedback" whose absence Huang et al. identified as fatal — **and §6.18's Gopher statistics supply it for free.**

**(iv) Concrete confidence signals and their calibration.** §4.4 gives a per-step signal table, all O(1)–O(n) and model-free. **Honest finding: no published calibration of these signals for PDF→EPUB exists that I could reach.** DocLayNet's inter-annotator agreement bounds what is achievable; Nougat's logit-variance detector is a worked precedent for statistic-as-detector; Gopher's thresholds are production-proven at web scale. **Recommendation: treat threshold calibration as a funded work item** — a 200–400 page gold set across EN/DE/TR and layout families, with each threshold fitted to a target precision. The cascade's entire premise ("escalate only where deterministic is unreliable") is unverified until this exists.

**(v) Tiny document VLM vs general 1–4B text LLM on low-confidence pages.** §4.5. **Neither as a general page-level helper.** Granite-Docling-258M costs **6.15 s/page even on optimized Apple-silicon MLX (102 s in plain Transformers)** and has **0.45 full-page-OCR edit distance** with no German or Turkish support claimed — so it is a *structure* tool, not a text tool. A general 1–4B text LLM costs 19–35 s/page and cannot see glyphs at all. They are **complementary and both narrow**: the doc-VLM for pages whose *text layer is broken* or whose *table grid* failed (≤2% of pages); the text LLM for *semantic* judgements over already-correct text, overwhelmingly as **once-per-book calls**. SmolDocling's "competes with models 27× larger" cuts both ways — **specialization beats scale, so a general 4B text model is the worst of both worlds for document structure.**

---

## 9. Residual risks and evidence gaps

1. **Turkish is unevidenced.** No fetched source covers Turkish dehyphenation, OCR post-correction, or layout. Agglutinative morphology weakens the dictionary-lookup signal that anchors §6.3; the `İ/I/ı/l/1` family is simultaneously an encoding, OCR, and case-folding hazard. **Do not ship LLM-based Turkish text correction without an evaluation set.** Build one.
2. **Confidence thresholds are uncalibrated** (§4.4). The largest single risk to this architecture.
3. **Marker's `--use_llm` benefit is unquantified** in public docs — do not cite it as evidence for anything.
4. **Benchmark transfer is unproven.** Every number here comes from scientific papers, forms, newspapers, or slides. **No public benchmark measures PDF→reflowable-EPUB quality on trade books.** OmniDocBench's headline metric is 2/3 tables-and-formulas. Consider publishing one — it would be a real contribution and would let OpenConvert measure its own regressions.
5. **The character-conservation invariant (Gate L) is the cheapest safety property available and should be enforced globally**, not per-stage. Every transformation that is not explicitly a text edit must preserve the non-whitespace character multiset. This one assertion makes hallucination *structurally impossible* everywhere except the two whitelisted places (§6.3, §6.14).
6. **CPU cost figures for text LLMs derive from one measurement** (Gemma 3 1B Q4_K_M on an i7-12700H). Server CPUs with AVX-512 and more cores will be several times faster; older laptops slower. The *ratio* to the deterministic pipeline (20–70×) is the robust part, not the absolute seconds.

---

## 10. Sources

**Systems, official docs and repositories**
- Docling Technical Report — https://arxiv.org/abs/2408.09869 · full text: https://arxiv.org/html/2408.09869v5 (pipeline stages, RT-DETR layout, TableFormer, CPU throughput Table 1)
- Docling: An Efficient Open-Source Toolkit for AI-driven Document Conversion — https://arxiv.org/abs/2501.17887
- Docling VLM pipeline docs — https://docling-project.github.io/docling/usage/vision_models/ (SmolDocling 102.21 s vs MLX 6.15 s per page)
- Docling architecture docs — https://docling-project.github.io/docling/concepts/architecture/
- docling-eval — https://github.com/docling-project/docling-eval
- Granite-Docling-258M model card — https://huggingface.co/ibm-granite/granite-docling-258M (all benchmark deltas vs SmolDocling)
- IBM Granite-Docling announcement — https://www.ibm.com/new/announcements/granite-docling-end-to-end-document-conversion (token-repetition loops)
- SmolDocling — https://arxiv.org/abs/2503.11576
- Marker — https://github.com/datalab-to/marker (`--use_llm` scope, olmocr-bench table, CPU/GPU modes, cascade principle)
- Marker LLM processor base classes — https://github.com/VikParuchuri/marker/blob/master/marker/processors/llm/__init__.py
- Datalab documentation index — https://documentation.datalab.to/llms.txt
- Surya — https://github.com/datalab-to/surya (650M model, 83.3 olmOCR-bench, OCR error detection model, CPU via llama.cpp)
- MinerU — https://arxiv.org/abs/2409.18839 · MinerU2.5 (1.2B decoupled VLM) — https://arxiv.org/abs/2509.22186
- Unstructured — https://github.com/Unstructured-IO/unstructured · partitioning strategies — https://docs.unstructured.io/open-source/core-functionality/partitioning
- GROBID principles — https://grobid.readthedocs.io/en/latest/Principles/ · bioRxiv benchmark — https://grobid.readthedocs.io/en/latest/Benchmarking-biorxiv/
- PyMuPDF text-extraction appendix — https://pymupdf.readthedocs.io/en/latest/app1.html · Document API (get_toc, metadata, XMP) — https://pymupdf.readthedocs.io/en/latest/document.html · recipes — https://pymupdf.readthedocs.io/en/latest/recipes-text.html
- pdfact — https://github.com/ad-freiburg/pdfact
- Calibre conversion manual — https://manual.calibre-ebook.com/conversion.html (heuristic processing, line-unwrap-factor, chapter XPath, PDF caveats)
- EPUBCheck getting started — https://w3c.github.io/epubcheck/docs/getting-started/ · messages reference — https://w3c.github.io/epubcheck/docs/messages/
- DAISY Accessible Publishing KB, tables — https://kb.daisy.org/publishing/docs/html/tables-basics.html
- lingua-py — https://github.com/pemistahl/lingua-py
- CharSplit (German compound splitter) — https://github.com/dtuggener/CharSplit
- datatrove Gopher quality filter — https://github.com/huggingface/datatrove/blob/main/src/datatrove/pipeline/filters/gopher_quality_filter.py · repetition filter — https://github.com/huggingface/datatrove/blob/main/src/datatrove/pipeline/filters/gopher_repetition_filter.py

**Benchmarks**
- OmniDocBench — https://arxiv.org/abs/2412.07626 · full text https://arxiv.org/html/2412.07626 · repo & live leaderboard https://github.com/opendatalab/OmniDocBench
- olmOCR & olmOCR-Bench — https://arxiv.org/abs/2502.18443 · https://github.com/allenai/olmocr
- DocLayNet — https://arxiv.org/abs/2206.01062 · full text https://ar5iv.labs.arxiv.org/html/2206.01062 (inter-annotator agreement + per-class mAP)
- DocLayout-YOLO — https://arxiv.org/abs/2410.12628 · https://arxiv.org/html/2410.12628v1
- TableFormer — https://arxiv.org/abs/2203.01017
- Nougat — https://arxiv.org/abs/2308.13418 · full text https://arxiv.org/html/2308.13418v1 (repetition degeneration, variance stopping criterion)

**Layout / reading order / LLM-with-geometry**
- XY-Cut++ — https://arxiv.org/abs/2504.10258 · full text https://arxiv.org/html/2504.10258v2 (all reading-order tables + FPS)
- LayoutReader / ReadingBank — https://arxiv.org/abs/2108.11591
- LayTextLLM — https://arxiv.org/abs/2407.01976 · full text https://arxiv.org/html/2407.01976v3 (coordinates-as-text 34.3% vs 78.1%; 6.2× token blow-up)
- DocLLM — https://arxiv.org/abs/2401.00908
- LayoutLLM — https://arxiv.org/abs/2404.05225
- LMDX — https://arxiv.org/abs/2309.10952
- SpatialEval — https://arxiv.org/abs/2406.14852
- Reading Order Matters (Token Path Prediction) — https://arxiv.org/abs/2310.11016
- Document parsing survey — https://arxiv.org/abs/2410.21169

**LLM reliability, cost and post-correction**
- Boros et al., *Post-Correction of Historical Text Transcripts with LLMs: An Exploratory Study*, LaTeCH-CLfL 2024 — https://aclanthology.org/2024.latechclfl-1.14/ (**"LLMs mostly degrade the input text"**; 14 models; EN/DE/FR/PL/CS/BG/SL/EL)
- Thomas, Gaizauskas & Lu, *Leveraging LLMs for Post-OCR Correction of Historical Newspapers*, LT4HALA 2024 — https://aclanthology.org/2024.lt4hala-1.14/ (54.51% CER reduction, fine-tuned Llama-2, English)
- Huang et al., *LLMs Cannot Self-Correct Reasoning Yet* — https://arxiv.org/abs/2310.01798
- Tam et al., *Let Me Speak Freely?* (format restrictions degrade performance) — https://arxiv.org/abs/2408.02442
- Chen, Zaharia & Zou, *FrugalGPT* (cascades) — https://arxiv.org/abs/2305.05176
- Belcak et al., *Small Language Models are the Future of Agentic AI* — https://arxiv.org/abs/2506.02153
- Gopher / MassiveText — https://arxiv.org/abs/2112.11446
- SmolLM — https://huggingface.co/blog/smollm
- llama.cpp CPU inference benchmark thread — https://github.com/ggml-org/llama.cpp/discussions/13664 (Gemma 3 1B Q4_K_M: 95.61 tok/s prefill, 16.16 tok/s generation, i7-12700H) · Apple Silicon thread — https://github.com/ggml-org/llama.cpp/discussions/4167

**Marked `[UNVERIFIED]` in this report:** Turkish dehyphenation/morphology and Turkish OCR post-correction claims (§6.3, §6.14); Zemberek/TRmorph as acceptors; the magnitude of Marker's `--use_llm` improvement; the absence of any published PDF→EPUB benchmark or confidence-signal calibration study (absence of evidence, given the search constraint in §1). Per-page LLM cost figures in §2.2 are marked `[ESTIMATE]` — arithmetic on the measured anchors in §2.1.
