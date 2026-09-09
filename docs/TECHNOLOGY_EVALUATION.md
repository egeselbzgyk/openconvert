# OpenConvert — Technology Evaluation

**Status:** Companion to `DECISIONS.md` (source of truth) and `PROBLEM_ANALYSIS.md`. This document explains *why* each choice in `DECISIONS.md` (D1–D12) won against its alternatives, with the evidence. Where this document and `DECISIONS.md` appear to differ, `DECISIONS.md` governs — no disagreement was found; discrepancies below are flagged as evidence gaps, not architecture objections.
**Scope:** Research only. Nothing in this document is implemented.
**Date:** 2026-09-09.
**Citation convention:** `(R#, §, url)` for round-1 research, `(V#, §, url)` for round-2 verification, `(RT, §)` for the red-team review, `(D#)` for a `DECISIONS.md` decision. `[UNVERIFIED]` marks a claim the cited research could not independently confirm — treat as a risk, not a fact.

---

## 1. Evaluation criteria and weights

Every comparison in this document is scored against the same eight priorities, given by the architect and used verbatim by R6 as its scoring rubric:

| # | Priority | Weight | Why it is weighted this way |
|---|---|---|---|
| 1 | Low RAM | 10 | A document converter that eats gigabytes to convert one book is not "local-first," it is "local-only-if-you-have-a-workstation." |
| 2 | Fast startup | 9 | A desktop tool a user opens for one conversion and closes; seconds of chrome-boot time are the first impression. |
| 3 | Small binary | 8 | Distribution friction (download size, disk footprint, antivirus scan time) compounds with priorities 1–2 into "does this feel like a native app." |
| 4 | Strong PDF tooling | 7 | The entire product is PDF fidelity; a weak PDF backend cannot be compensated for later. |
| 5 | Easy local-LLM integration | 6 | The AI-assist story (D8–D10) needs a clean seam, not a fight with the host language's FFI story. |
| 6 | Cross-platform (Win/macOS/Linux) | 5 | A one-OS tool is a much smaller product. |
| 7 | Maintainability | 4 | Single-maintainer-plus-LLM-pair-programmer project; a codebase that fights refactors is a slow death. |
| 8 | Developer velocity | 3 | Real, but explicitly the lowest-weighted priority — this project is optimizing for the shipped artifact, not for how fast the first prototype appears. |

(R6, §0, `R6_desktop_stack.md`) These weights are the same rubric applied — implicitly — throughout the rest of this document: a component that costs RAM or binary size is judged harder than one that costs a few weeks of developer time.

---

## 2. Desktop stack

### 2.1 Scored matrix

R6 scored seven candidate stacks (1–5 per priority, weighted sum out of 260) (R6, §1, `R6_desktop_stack.md`):

| Priority (weight) | Rust+Tauri 2 | Electron | Python+Qt | .NET+Avalonia | Flutter | Go+Wails | Electron+Rust (napi-rs) |
|---|---|---|---|---|---|---|---|
| Low RAM (10) | **5** | 2 | 2 | 3 | 3 | 4 | 2 |
| Fast startup (9) | **5** | 2 | 2 | 4 | 4 | 5 | 2 |
| Small binary (8) | **5** | 1 | 2 | 3 | 3 | 5 | 1 |
| PDF tooling (7) | 4 | 4 | **5** | 4 | 2 | 1 | **5** |
| Local-LLM (6) | **5** | **5** | 4 | 3 | 2 | 3 | **5** |
| Cross-platform (5) | 3 | **5** | 3 | 4 | 4 | 3 | **5** |
| Maintainability (4) | 4 | 4 | 3 | 4 | 3 | 4 | 3 |
| Dev velocity (3) | 3 | **5** | **5** | 3 | 2 | 3 | 3 |
| **Weighted total / %** | **233 / 89.6%** | 160 / 61.5% | 155 / 59.6% | 181 / 69.6% | 154 / 59.2% | 190 / 73.1% | 157 / 60.4% |

**Measured, not estimated, headline numbers** (gethopp.app, 9 Apr 2025, N=1 but methodology stated, identical app in both frameworks, 6 windows): Tauri 2 **8.6 MiB** bundle / **~172 MB** RAM vs Electron **244 MiB** / **~409 MB** RAM — a 28× and 2.4× gap respectively; startup difference "negligible" (R6, §2.1, https://www.gethopp.app/blog/tauri-vs-electron). A second source (Oflight, 2026) gives directionally consistent but not independently measured ranges (5–15 MB / 50–150 MB / 0.5–1 s vs 80–120 MB / 200–500 MB / 2–4 s) (R6, §2.2). A third source's more dramatic numbers (tech-insider.org) is flagged `[UNVERIFIED — do not cite]` in R6 itself: its Tauri/Electron figures are verbatim copies of the gethopp benchmark wrapped in unfindable "benchmark suite" citations, which reads as fabricated content around a real number (R6, §2.2).

### 2.2 Per-candidate verdict

**Rust + Tauri 2.x — adopted (D1, D2).** Wins priorities 1–3 decisively and ties or wins 4–5. Its only structural loss is P6 (WebKitGTK version spread on Linux) and P8 (Rust learning curve for a Python/TS-comfortable solo maintainer) — both scored 3, both accepted as residual risk in D1/D2. The sidecar model (`externalBin`) is, per R6, "the single most important Tauri feature for this project" (R6, §3.2): it is what makes D8 (llama-server) and D4 (Tesseract) both clean, permission-scoped sidecars without linking C/C++ into the main binary.

**Electron — rejected.** Loses the top three weighted priorities simultaneously (244 MiB / 409 MB vs Tauri's 8.6 MiB / 172 MB) and only ties, not wins, on PDF tooling — `pdf.js` has the same missing-layout-algorithms gap as Rust (R6, §4.2). Its one decisive advantage — `webContents.capturePage()` works on **hidden** windows, unlike Tauri's webview (R6, §4.4) — is obtainable for free in CI via Playwright (§7.3), which additionally covers WebKit, the engine Electron cannot reach and the one that actually matters (macOS/Linux Tauri users, Apple Books, Kobo).

**Python + Qt — rejected as the shipped stack, retained for `eval/`.** The only candidate that outright wins PDF tooling, but PyInstaller packages run 30–120 MB for a bare window before any PDF/LLM dependency is added (R6, §5, https://www.pythonguis.com/faq/packaged-installer-file-sizes/), and in-process rendering QA would need QtWebEngine — a bundled Chromium, reintroducing exactly the cost Electron was rejected for. D1 keeps Python for `eval/` only, never a build or runtime dependency of the shipped app.

**.NET + Avalonia — rejected, closest runner-up.** `PdfPig` (Apache-2.0) is, by R6's own account, "the single best *layout-analysis* library in any ecosystem" (R6, §6) — Docstrum, RecursiveXYCut, whitespace-cover, Allen-interval reading order and a header/footer classifier, all pre-implemented and permissively licensed. It loses on two structural grounds: Avalonia has no first-party production WebView `[UNVERIFIED — R6 could not fetch a current status page]`, which forecloses any EPUB preview or HTML rendering path at all; and `LLamaSharp` pins a specific llama.cpp commit and visibly lags upstream between releases (R6, §6). D3's resolution — port PdfPig's Apache-2.0 algorithms into Rust — is explicitly how OpenConvert captures this candidate's one real advantage without adopting the stack.

**Flutter, Go+Wails, Kotlin/Compose, Qt/C++, pure-Rust UIs — rejected**, each on a single decisive axis: Flutter's `pdfrx` is a viewer, not an extraction toolkit (R6, §7); Go's only serious PDF text stack (UniPDF) is proprietary-or-AGPL (R6, §8); Compose Desktop inherits JVM startup/RAM cost by construction; Qt/C++ has the worst developer velocity of any option for a two-entity (human + LLM) team; pure-Rust UIs (egui/iced/Slint/GPUI) render no HTML at all, which is fatal for a tool whose product *is* HTML — Blitz (Stylo+Taffy+Vello) is the one candidate that could close this gap but is explicitly pre-alpha with maintainers discouraging production use (R6, §8, https://github.com/DioxusLabs/blitz) — see the watch list (§13).

### 2.3 What the stack implies for LLM integration and rendering QA

Two downstream architectural decisions fall directly out of the P1–P3 weighting, independent of which host language had won: **run the LLM as an HTTP sidecar, not in-process** (§8), and **never bundle a headless browser for in-app rendering QA** — Tauri cannot screenshot a hidden webview (`wry` #1358, open since Sep 2024; `tao` #289, open since Jan 2022) (R6, §3.5), so visual regression moves entirely into CI via Playwright, which is *better* than Electron's in-app capability because Playwright ships its own WebKit build and Electron cannot reach WebKit at all (R6, §11; §8 below).

---

## 3. PDF parsing backends

### 3.1 The cross-library benchmark that decided D3

The one independently reproducible cross-library benchmark found (py-pdf/benchmarks, Intel i7-6700HQ, 14 PDFs from 285 KiB–14.7 MiB) gives the single strongest empirical fact behind D3:

| Library | Avg speed | Avg quality |
|---|---|---|
| PyMuPDF | 0.1 s | 96% |
| **pypdfium2** | **0.1 s** | **97%** |
| Tika | 0.2 s | 95% |
| pdftotext (Poppler) | 0.3 s | 91% |
| pypdf | 3.5 s | 96% |
| pdfminer.six | 5.8 s | 89% |
| pdfplumber | 9.5 s | 75% |

(R2, §A.11, https://github.com/py-pdf/benchmarks) **pypdfium2 (i.e. PDFium) matches PyMuPDF's speed and beats its quality — under a BSD-3 license instead of AGPL.** That single row is the strongest evidence supporting D3, because it removes the usual argument for tolerating MuPDF's license (that it is simply better).

### 3.2 Capability matrix and license traps

| Library | License | Glyph bbox | Font name/size/flags | Render mode | Images+SMask | Tagged/marked content | Renders pages | 2025–26 activity |
|---|---|---|---|---|---|---|---|---|
| MuPDF | **AGPL/comm** | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | High |
| **PDFium** | **BSD-3** | ✅ tight+loose | ✅ | ✅ | ✅ [UNVERIFIED] | ❌ | ✅ | Weekly builds |
| Poppler | **GPL** | ✅ | ✅ | ✅ | ✅ | ⚠️ partial | ✅ | Monthly |
| pdf.js | Apache-2.0 | ❌ item-level only | ⚠️ | ⚠️ | ✅ | ⚠️ | ✅ (canvas) | High |
| pdfminer.six | MIT | ✅ | ✅ | ✅ | ✅ | ✅ (mcid) | ❌ | Moderate |
| lopdf | MIT | ❌ | ❌ | ❌ | ⚠️ raw | ✅ raw | ❌ | Moderate |
| hayro | Apache-2.0/MIT | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ | High (experimental) |
| pdf_oxide | MIT/Apache-2.0 | ✅ | ✅ | ⚠️ | ✅ | ⚠️ | ✅ optional | High |
| PdfPig | Apache-2.0 | ✅ | ✅ | ⚠️ | ✅ | ⚠️ | via Skia add-on | High |
| PDFBox | Apache-2.0 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | High |

(R2, §A.12) ✅ supported / ⚠️ partial / ❌ not available.

**The license map that matters for D15 (Apache-2.0):**

| Component | License | Effect |
|---|---|---|
| MuPDF / PyMuPDF / mupdf-rs | **AGPL-3.0 or commercial** | Forces AGPL on OpenConvert *and every fork*. Rejected. |
| Poppler | **GPL-2/3** | Acceptable only as an optional, subprocess-invoked, external CI oracle — never linked. |
| **PDFium** | **BSD-3** | Clean. |
| `pdfium-render`, `pypdfium2` | MIT/Apache-2.0, Apache-2.0/BSD-3 | Clean. |
| `lopdf` | MIT | Clean. |
| `hayro`, `pdf_oxide` | MIT/Apache-2.0 | Clean, but immature (§13). |

(R2, §D.1) D3's choice — PDFium via `pdfium-render` for glyph/render access, `lopdf` for the object model — is the only combination in this table that is simultaneously fast, high-quality, and license-clean.

### 3.3 The one genuine gap, and D3's answer to it

Rust has excellent PDF *access* through PDFium (char-level boxes, font names, render mode, `is_generated`, `is_hyphen`) but **no library that ships reading-order or page-segmentation algorithms** — that gap belongs to .NET's PdfPig and Python's pdfplumber (R2, §D.2, Rust). PdfPig is Apache-2.0, and its layout algorithms (Docstrum, RecursiveXYCut, whitespace-cover, Allen-interval reading order, a decoration/header-footer classifier) are "roughly 500–1,500 lines each, no PDF-format knowledge, no dependencies" — porting them to Rust is legally clean (attribution + NOTICE) and, in R6's assessment, "a bounded, well-specified task that Claude Code is very good at" (R6, §3.6). This is exactly what D3/Appendix A's `oc-layout` crate does, and it converts what would otherwise be a missing-capability risk into an implementation-effort item.

### 3.4 Residual risk: PDFium's own gaps

`has_unicode_map_error()` was searched for in `pdfium-render` and not found (RT, per D3's own residual-risk note) — the broken-CMap detector must fall back to a U+FFFD/PUA-share and dictionary-hit-rate heuristic instead of a direct API flag. `pdf_oxide`'s published benchmarks (claimed faster than PyMuPDF) are vendor-reported with no described corpus and are explicitly flagged as needing a hands-on bake-off before being believed (R2, §D.5). Neither gap changes the D3 decision; both are tracked as open verification items.

---

## 4. Deterministic layout algorithms

### 4.1 What is proven to work, with published numbers

| Task | Deterministic method | Published evidence | Expected quality |
|---|---|---|---|
| Word/line grouping | Nearest-neighbour (Manhattan 20% / Euclidean 40%) | PdfPig | Very high on born-digital |
| Block segmentation | Docstrum primary, whitespace-rectangle secondary | 6.0% / 9.8% line error, UW-III (scanned) | High |
| Reading order (Manhattan) | Recursive XY-cut + column detection | **100%**, arXiv 2607.01018 | Excellent |
| Reading order (complex) | XY-Cut++ (pre-mask + density-driven splits) | 98.8 BLEU, DocBench-100 | Good |
| Reading order (wrap-around) | — | XY-cut 49.7% | Poor — escalate |
| Headings | TOC/outline match > font-size+weight clustering | TOC P_ED ≥ 0.9; GROBID section-title F1 76.43% | Good with TOC, moderate without |
| Headers/footers/page numbers | Cross-page repetition + edit distance + parity + arithmetic progression | PdfPig `DecorationTextBlockClassifier`; Lin 2003 | High |
| Paragraphs | Indent + short-final-line + leading | MuPDF `paragraph-break` option | High on justified text |
| Dehyphenation | Same-doc evidence → dictionary → tiny CRF | 92.38% balanced acc (CRF) vs 66.87% (dict only) | High |
| Footnotes | Bottom band + small font + superscript marker + rule | — [UNVERIFIED] | Moderate |
| Captions | Proximity to image + "Figure N" regex | GROBID fig-title F1 69.03% | Moderate |
| Lists | Marker regex + hanging indent | DocLayNet List-item easiest class | High |
| Tables | Ruling lines primary, whitespace fallback | Camelot lattice F1 0.778 | Moderate (ruled) / Poor (borderless) |

(R2, §B.10) The single most load-bearing result in this table is the reading-order row: on **Manhattan layouts — a novel, a textbook, a technical manual** — plain recursive XY-cut scores **100%**, and a learned method (LayoutReader) scores **worse** at 96.0% (R2, §B.2, arXiv 2607.01018). This is the strongest evidence for OpenConvert's deterministic-first thesis (D13's Conservation-law framing, D17's structural-predicate escalation): the exact place where a general-purpose book converter spends most of its time is also the place where the cheapest method is already at ceiling.

### 4.2 Where deterministic methods are known to fail

The same table's weak rows are equally informative: wrap-around/non-Manhattan layouts (magazines, glosses) drop XY-cut to 49.7% (R2, §B.2); dehyphenation's naive dictionary baseline gets 98.8% raw accuracy but only 32% recall on the metric that matters (keep-hyphen recall) because ~98% of hyphens should simply be removed (R2, §B.7, Hernaes 2019) — a kilobyte CRF classifier lifts that to 86% recall / 92.38% balanced accuracy at effectively no cost, which is why D13.6 drops LLM-based dehyphenation from v1 entirely in favor of the CRF. Table detection tops out around 0.78 F1 / 0.79 TEDS even on *ruled* tables (Camelot, ICDAR-2013) (R2, §B.9) — which is why D13.11 treats a table as "ruled → HTML, else image" rather than chasing structure-recovery fidelity a reflowable EPUB cannot usefully render anyway.

---

## 5. ML layout models — and why they are post-v1 (D16)

### 5.1 The Apache-2.0 option and its packaging cost

Docling's `egret`/`heron` family (arXiv 2509.11720) is the best-documented, best-licensed option surveyed:

| Model | Params | CPU latency (AMD EPYC 7763, 4 threads) | DocLayNet mAP |
|---|---|---|---|
| egret-m | 19.5M | 0.334 s/page | — |
| heron | 42.9M | 0.643 s/page | 0.699 |
| heron-101 | 76.7M | 0.988 s/page | 0.696–0.780 |

(R2, §C.1, arXiv 2509.11720) Both `docling-layout-egret-medium` and `docling-layout-heron` are confirmed **Apache-2.0** on their Hugging Face model cards — but **weights-only, no `.onnx` file** (V2, §5, https://huggingface.co/docling-project/docling-layout-heron): `config.json` + `model.safetensors` (78.3 MB egret-m, 172 MB heron) only. Shipping either as an ONNX Runtime model means OpenConvert would own a safetensors→ONNX export step, not a download. The Docling *system*, not just the layout model, costs **~6.2 GB peak memory** end-to-end on Apple M3 Max / Intel Xeon E5-2690 for a 225-page document (R2, §C.1, arXiv 2408.09869) — the number R2 recommends showing the architect directly, because it illustrates how fast an ML-first pipeline violates priority 1 even with a permissively licensed, well-documented model family.

### 5.2 The AGPL minefield

DocLayNet-trained YOLO checkpoints (`FreeOCR-AI/yolo-doclaynet`, `opendatalab/DocLayout-YOLO`) reach 0.718–0.82 mAP50-95 and would run an order of magnitude cheaper than heron (extrapolated 50–200 ms/page at 2.3–3.2M params) `[UNVERIFIED — extrapolated, not measured]` — but both repositories are **AGPL-3.0**, inherited from Ultralytics' training code, with weight licensing not separately stated (assume AGPL until proven otherwise) (R2, §C.2). This is, in R2's own words, "the single easiest license mistake to make in this space" — a fast, accurate, seemingly-drop-in model that AGPLs the whole application the moment its weights ship.

### 5.3 HURIDOCS — the strongest existence proof for OpenConvert's architecture

The one comparison point that most directly validates D13/D17's deterministic-plus-tiny-classifier design:

| Mode | Approach | CPU speed | Accuracy |
|---|---|---|---|
| LightGBM "fast" | Gradient-boosted trees over Poppler-XML token features, **no visual input** | **0.42 s/page** | "slightly lower" |
| VGT (Vision Grid Transformer) | Full visual model, DocLayNet-trained | 13.5 s/page CPU | F1 0.962 (PubLayNet) |

(R2, §C.5, https://github.com/huridocs/pdf-document-layout-analysis) A **32× CPU speedup for "slightly lower" accuracy**, from a tree ensemble over exactly the geometric/font features PDFium already exposes — kilobytes-to-megabytes, no ONNX runtime dependency, no rasterization cost. D13's `oc-layout` architecture (heuristics primary, optional tabular classifier over PDFium features as the one ML addition, per R2 §D.4) is this pattern.

### 5.4 The DocLayNet human ceiling

DocLayNet's own paper reports per-class **human inter-annotator agreement** alongside model mAP (R2, §C.7, arXiv 2206.01062):

| Class | Human | Best model (YOLOv5x6) |
|---|---|---|
| Page-footer | **93–94** | 61.1 |
| List-item | 87–88 | 86.2 |
| **All classes** | **82–83** | 76.8 |

Two conclusions shape D16/D17: (1) on **page-footer** — exactly the class deterministic cross-page-repetition detection nails almost perfectly — the best model scores dramatically *worse* than humans (61.1 vs 93–94 mAP); don't use an ML model where a cheap heuristic already beats every published model. (2) Overall human agreement is only 82–83 mAP — the models surveyed (heron 0.699–0.780, DocLayout-YOLO 0.797) are approaching a ceiling that is itself noisy, meaning there is limited headroom left to buy with a heavier model even where one would be legally and computationally acceptable.

**Conclusion (matches D16):** ship heuristics-only for v1; hold the ONNX-exported egret/heron path and the LightGBM tabular classifier as the two ranked post-v1 escalation options, invoked only on pages heuristics flag as low-confidence (R2, §D.4).

---

## 6. OCR engines

### 6.1 Comparison

| Engine | License | DE/TR | Model size | CPU speed/page | Bboxes+conf | Verdict |
|---|---|---|---|---|---|---|
| **Tesseract 5** | **Apache-2.0** | Yes/Yes (100+ langs) | `tessdata_fast` ~1.5 MB/lang | 1.96 s (fast) / 3.46 s (best), i7-10750H | hOCR/TSV/ALTO, word bbox+conf | **Default v1 engine** |
| PP-OCRv4/v5 (ONNX/RapidOCR) | Apache-2.0 | Yes/Yes (37 langs) | Det 4.6–113 MB, Rec 10–90 MB | ~0.38–0.57 s/page (OnnxTR proxy) | Yes | Optional accuracy upgrade |
| EasyOCR | Apache-2.0 | Yes/Latin | Not published, PyTorch-heavy | ~3.2 s/page (community) | Yes | Not recommended (PyTorch weight) |
| docTR/OnnxTR | Apache-2.0 | Multilingual, DE unconfirmed | Not disclosed | 0.38–0.57 s/page | Yes | Possible alt backend |
| Surya OCR | Code Apache-2.0, **weights modified OpenRAIL-M** | 91 langs incl. DE | 650M params | GPU 5.35 pages/s; **Apple CPU/Metal 0.108 pages/s (~9.3 s/page)** | Yes, rich | **Excluded** — license + CPU speed |
| Apple Vision | Proprietary, OS-bundled | DE confirmed since 2020; TR unconfirmed | 0 MB | ~150–300 ms/page | Yes | Optional macOS-only |
| Windows.Media.Ocr | Proprietary, OS-bundled | Varies by pack | 0 MB | Not found | Yes, no explicit conf | Optional Windows-only |
| ocrs (Rust) | [UNVERIFIED] | **Latin only** | Small (ONNX/RTen) | Not found | Confidence unconfirmed | Watch (§13) |
| Calamari | GPL-3.0 | Fraktur specialist | — | — | — | **Excluded** — GPL |

(R3, §2) Tesseract is the only engine that is simultaneously CPU-only, Apache-2.0, small, TSV-with-confidence, and confirmed for both German and Turkish — the four axes that matter most for D4.

### 6.2 The packaging decision behind D4

Tesseract's own GitHub releases ship source tarballs, not prebuilt cross-platform binaries `[PARTIALLY VERIFIED — V2 could not enumerate the actual asset filenames for 5.5.3]` (V2, §7). The practical per-OS path is: Linux distro packages, macOS Homebrew, Windows UB-Mannheim installer (V2, §7, https://github.com/UB-Mannheim/tesseract/wiki) — none of which is directly sidecar-shippable as-is; each is an installer or package-manager artifact, not a standalone relocatable binary. Building Tesseract once per target OS in CI (vcpkg static triplets) and shipping the result as a `externalBin` sidecar is the more predictable path, and matches Tauri's sidecar model exactly, but it is real, non-trivial CI-recipe and code-signing work for a single maintainer.

D4's resolution — **v1 detects and uses the user's own system-installed Tesseract first, with a copy-pasteable install hint on scanned-PDF detection; a packaged, signed "OCR pack" follows once the CI build recipe and signing story are proven** — directly reflects this packaging cost, and is a red-team-forced change (RT) from an earlier draft that assumed Tesseract would simply be bundled from day one. Calibre's refusal to process image-only PDFs at all (PROBLEM_ANALYSIS §6) remains a differentiation opportunity either way — even "detect and prompt" beats "silently produce an empty chapter."

### 6.3 Rejected and watched alternatives

PP-OCR via ONNX is the accuracy upgrade path — better published accuracy/size than Tesseract, but only community Rust ports exist (`oar-ocr`, `rust-paddle-ocr`, `paddle-ocr-rs`), none official, adding real build complexity for no clean win over Tesseract today (R3, §2). `ocrs` (robertknight, Rust-native, ONNX via `rten`) is explicitly a preview and Latin-alphabet-only — disqualifying for Turkish and irrelevant to German only by accident of alphabet coverage; it is the most promising long-term pure-Rust OCR path and belongs on the watch list. Surya's 650M-param VLM and its revenue-gated modified-OpenRAIL-M weight license disqualify it outright — both on size ("not a small CPU model") and on license (incompatible with unrestricted redistribution) (R3, §3.6). Apple Vision and Windows.Media.Ocr are legitimate zero-footprint optional backends, gated on confirming Turkish-language-pack availability, which the research could not confirm for Apple Vision `[UNVERIFIED]` (R3, §2).

---

## 7. EPUB generation, validation, and rendering

### 7.1 Generation: why hand-rolled (D5)

The Rust ecosystem's one real EPUB-writing crate, `epub-builder` (MPL-2.0), targets **EPUB 3.0.1, not 3.3**, and its own maintainers list real gaps: no default CSS/XHTML templates, weak multi-author/multi-language metadata, no encoding of the HTML content model (R5, §C1, C5). Since the output *is* the product and needs byte-level control — `epub:type` semantics, correct manifest `properties` (OPF-014 is a top converter error class), deterministic zip with `mimetype` stored-first (PKG-007), page-list navigation — hand-rolling the OPF/NAV/XHTML with `quick-xml` + `zip` trades a coarse third-party abstraction for direct control over exactly the failure classes that matter, without giving up safety: the risk surface is well-documented (§7.2 below) and directly testable by the Tier-1 internal validator (R5, §C5). `epub-builder` remains worth reading/adapting from (it is MPL-2.0, permissive enough) even though it is not a dependency.

### 7.2 Validation: the JVM problem and the AGPL near-miss

EPUBCheck (BSD-3, v5.3.0, 2025-09-01) is the authoritative validator but ships **only as a JAR** — no GraalVM native-image build or jlink minimal-runtime distribution was found in its release notes (R5, §B1). This is a direct conflict with "low RAM, fast startup, small binary": bundling EPUBCheck in the base install means bundling or requiring a JVM. EPUBCheck's own minimum runtime JRE version could not be confirmed — the only Java-version text found ("JDK 1.7 or above") is explicitly a *build*-time requirement in a "build from source" section, not a stated runtime minimum `[COULD NOT VERIFY — V2, §8]`.

**The pure-Rust near-miss: `epubveri`.** A JVM-free, embeddable alternative, architecturally exactly what a Rust-based converter wants — RELAX NG/Schematron engines, a WASM binding, measured **98.8% of EPUBCheck's own test-suite invalid files correctly flagged with matching error codes, 1.1% false-positive rate on valid files** (R5, §B3, https://github.com/veripublica/epubveri). It is disqualified for v1 on two independent grounds: pre-1.0 (v0.4.4, "not yet a drop-in replacement" per its own maintainers) and, decisively, **dual-licensed AGPL-3.0/commercial** — adopting it would force OpenConvert to be AGPL. D6 explicitly puts it on the watch list rather than adopting it, and R5 recommends reaching out to the maintainers about a permissive-licensing path, since an embeddable Rust validator is precisely what this project needs long-term (R5, §B3).

**D6's three-tier resolution**, directly reflecting this cost/coverage tradeoff:

| Tier | What runs | Cost |
|---|---|---|
| **Tier 1** (always on) | Hand-written Rust validator: OCF/zip structure, OPF required metadata, manifest↔spine referential integrity, `properties` completeness, XHTML well-formedness — targeting exactly RSC-005/RSC-012/OPF-014/PKG-007, the four classes most common in *converter-generated* (not hand-authored) EPUB (R5, §B6, §B7) | Milliseconds, ships in the core binary |
| **Tier 2** (optional, red-team-forced [RT]) | EPUBCheck via an optional "validation pack" — a jlink-minimized JRE bundled with `epubcheck.jar`, ~40–50 MB, same download mechanism as the LLM model | User-initiated download; the red team's objection was that refusing a 45 MB JRE pack while offering a 1 GB model download was an inconsistent risk posture (D6) |
| **Tier 3** (CI only) | Real EPUBCheck (a JDK in GitHub Actions costs nothing) plus Ace by DAISY, run against a regression corpus on every release | Never shipped to end users |

(R5, §B7; D6) The four converter-common error codes Tier 1 targets:

| Code | Meaning | Typical converter cause |
|---|---|---|
| RSC-005 | Malformed XML/HTML | Bad markup from PDF text/layout extraction |
| RSC-012 | Fragment identifier not defined | TOC/nav or `noteref`↔`footnote` id mismatch |
| OPF-014 | Manifest `properties` not declared | SVG/MathML/scripted content unflagged in manifest |
| PKG-007 | `mimetype` not first/not stored | A generic zip library mishandling the special first entry |

(R5, §B6)

### 7.3 Rendering QA: Playwright over Electron capture, and why

Playwright's bundled browser sizes are stated directly in its docs: **Chromium ~281 MB, WebKit ~180 MB, Firefox ~187 MB** (R5, §D1, https://playwright.dev/docs/browsers) — incompatible with "small binary" as an in-app dependency, and why D7 never bundles any of them. Electron's `webContents.capturePage()` genuinely does work on hidden windows, which Tauri's webview cannot (`wry` #1358, `tao` #289, both open) — Electron's one real advantage over Tauri (R5, §D2; R6, §3.5). D7 accepts the constraint rather than fighting it: **structural, render-free checks ship in the app** (heading-hierarchy sanity, image/manifest parity, internal-link resolution, text-density heuristics — pre-empting EPUBCheck's own RSC-012 for free, in milliseconds, with no rendering engine at all) (R5, §D7), **CI runs Playwright with both Chromium and WebKit** at fixed viewports against golden images (R5, §D1; R6, §11), and cross-webview pixel comparison is abandoned as a reliability strategy — WebView2/WKWebView/WebKitGTK are three different engines whose font rasterization genuinely differs, so a screenshot only means something same-OS-to-same-OS (R5, §D2). The one place Electron's capability would matter — Chromium — is also the one place OpenConvert's users don't read: Apple Books, Kobo, and Tauri's own macOS/Linux webviews are all WebKit, and Playwright reaches WebKit; Electron cannot (R6, §11).

For engine-agnostic layout QA that does need real rendering, D7 uses DOM measurement inside the app's resident webview — `element.scrollWidth > element.clientWidth` — rather than pixels: "any overflow at all" is a stable cross-engine signal even though the exact amount varies by engine (R5, §D9, https://developer.mozilla.org/en-US/docs/Web/API/Element/scrollWidth).

Comparing a PDF page render against an EPUB render at the pixel level is, per R5, the wrong tool: reflow means identical content can span very different vertical extents depending on font size and viewport — there is no stable expected layout to diff against. The correct comparison is text-content and image-count parity, computed structurally (R5, §D9) — the same idea behind D13.4's conservation law.

---

## 8. LLM runtimes

### 8.1 Sidecar vs. in-process vs. Ollama

| Criterion | Embedded `llama-cpp-2` (in-process) | **`llama-server` sidecar** | Require Ollama |
|---|---|---|---|
| RAM | Best in theory, but unloading a model does not reliably return memory to the OS (allocator fragmentation) | +30–80 MB process overhead `[DERIVED]`; `kill()` returns **100%** of model RAM instantly | Worst — daemon holds up to 3 models resident, 5-min keep-alive, unaware of app memory pressure |
| Crash isolation | **Poor** — a ggml assertion or OOM takes down the whole app mid-conversion | **Excellent** — sidecar dies, app detects, falls back to heuristics | Excellent, but outside the app's control |
| Packaging | Hardest: native lib per OS×arch, hardened runtime, notarization, CPU-feature dispatch, needs `clang` for bindgen | Easier: one extra signed executable, lazily downloadable | Easiest for the app, but the user must install a **second application** |
| Update strategy | Coupled to app releases — a llama.cpp CVE forces a full signed app release | **Decoupled** — swap the sidecar binary independently | Uncontrolled — Ollama can silently change defaults under the app |
| Determinism | Full control, but the bindings' own authors warn of UB | Full control | Poor — shared global model store, user-editable Modelfiles, silent version drift, default `num_ctx` **2048** truncates long geometry prompts |
| Testability | Requires mocking a C API | **HTTP + JSON — trivially mockable in `cargo test`** | Same protocol, same testability, once detected |

(R4, §B.5) The recommendation and D8's decision agree exactly: **sidecar `llama-server`**, with Ollama supported as an auto-detected optional backend and any OpenAI-compatible endpoint as a bring-your-own option (D10), because both speak the sidecar's own wire protocol at near-zero marginal cost.

### 8.2 Verified llama.cpp release assets

V1's independent verification of llama.cpp's release page (build `b10456`, 2026-08-17) confirms the packaging story D8 relies on:

| Asset | Size |
|---|---|
| `llama-b10456-bin-win-cpu-x64.zip` | 17.6 MB |
| `llama-b10456-bin-win-cpu-arm64.zip` | 11.7 MB |
| `llama-b10456-bin-macos-arm64.tar.gz` | 10.6 MB |
| `llama-b10456-bin-macos-x64.tar.gz` | 10.9 MB |
| `llama-b10456-bin-ubuntu-x64.tar.gz` | 15.9 MB |
| `llama-b10456-bin-ubuntu-arm64.tar.gz` | 12.9 MB |

(V1, §3, https://github.com/ggml-org/llama.cpp/releases/tag/b10456) Critically for D8's "single CPU binary with runtime ISA dispatch" claim: **there is only one Windows/Linux CPU binary per architecture** — no separate AVX/AVX2/AVX512 zips, unlike pre-2025 releases. This is verified against the build system directly: `GGML_CPU_ALL_VARIANTS` in `ggml/src/CMakeLists.txt` requires `GGML_BACKEND_DL` and produces per-ISA CPU backend variants as dynamically-loadable modules selected automatically at runtime (V1, §3, raw CMakeLists.txt fetch). V1 could not confirm each individual archive's *internal* contents (that `llama-server` specifically is inside every zip) without downloading — `[COULD NOT VERIFY, but consistent with long-standing llama.cpp release convention]`. Server flags D8 depends on (`--api-key`, `--cache-prompt`, `--cache-reuse N`, `-np`, `--chat-template-kwargs`, `/health`) were independently confirmed verbatim against two fetches of `tools/server/README.md` (V1, §3).

### 8.3 The `--cache-reuse` limitation for hybrid models (red-team A3)

D8's original draft listed `--cache-prompt`/`--cache-reuse` as "the exact levers" for amortizing the shared system prefix across a book's LLM calls. The red team found this wrong for a **hybrid recurrent** model — the architecture of D9's excluded-by-default, promotable Qwen3.5 tier:

- `--cache-reuse N` works via **KV shifting** — partial `seq_rm`/`seq_add` on the attention KV cache (V1, §3, verbatim from `tools/server/README.md`).
- llama.cpp's own recurrent-memory source carries the comment that "models like Mamba or RWKV can't have a state partially erased at the end of the sequence because their state isn't preserved for previous tokens" (RT, A3, citing `src/llama-memory-recurrent.cpp`) — a recurrent state is a fixed-size compression of everything before it; there is no "middle" to excise via KV shifting.
- Qwen3.5-MoE's own architecture file builds its gated-delta-net layers with `llm_graph_input_rs`, i.e. it genuinely uses hybrid (attention + recurrent) memory, so the recurrent half of every hybrid layer stack cannot be KV-shifted at all (RT, A3).
- The mechanism that *does* work for recurrent models is **context checkpoints** (`--context-checkpoints`/`-ctxcp`, `--checkpoint-min-step`, default spacing 8192 tokens) — snapshot-based rollback, not chunk-level reuse, costing RAM per checkpoint and spaced far larger than OpenConvert's entire per-book prompt set (RT, A3).

**Consequence, reflected in D9 and D13.6:** `--cache-reuse` is qualified, not deleted, in D8 — it applies to the dense fallback family (Qwen3-1.7B, the actual v1 default) but not to any hybrid model in the experimental tier. D9's own promotion gate for Qwen3.5 (G5: "second identical call set ≤ 40% of the first") is precisely a test of whether prefix reuse works at all for a candidate model before it can become the default — this gate exists *because of* the A3 finding. D13.6's requirement of one byte-identical system prefix shared across all call types, with `-np 1` and a single warm slot per book, is the design response: since a hybrid model cannot partially reuse a cache, the only lever left is making sure the *whole* prefix is either an exact match (fast) or not (full reprocessing) — never a partial match that KV-shifting could have salvaged for a dense model.

### 8.4 Structured output technique

llama.cpp's JSON-schema-to-grammar path has real, documented gaps that matter for a strict-output pipeline: `additionalProperties` defaults to false (a benefit — reduces hallucinated keys), but nested `$ref`s are broken, `uniqueItems`/`contains`/`if-then-else`/`patternProperties` are unsupported, and `minimum`/`maximum` work only for integers (R4, §B.1). D13.6's four call types therefore rely on **hand-written GBNF grammars**, not JSON-schema conversion — more reliable and more token-efficient given OpenConvert's output shape is simple (arrays of enum labels, small integer indices) (R4, §B.1, B.6). Token-minimal output shapes (e.g. `{"l":["h2","p","fn"],"o":[0,1,2]}` rather than a verbose per-block object array) are estimated at a 5–10× output-token reduction, translating almost linearly into CPU wall-clock savings since decode, not prefill, is the bottleneck `[DERIVED, R4 §B.6]`.

---

## 9. Language and dictionary tooling

| Component | Choice | License | Verified facts | Open risk |
|---|---|---|---|---|
| Language ID | **`whatlang`** | MIT (V2, §4) | Version 0.18.0 confirmed | Turkish/German coverage `[COULD NOT VERIFY — V2 ran out of fetch budget before confirming the supported-language list]` |
| Language ID (rejected) | `lingua-rs` | Apache-2.0 | Confirmed | Downloads **~300 MB by default for all 75 languages** (V2, §4, https://github.com/pemistahl/lingua-rs); per-language Cargo features exist and would need to be scoped to just DE/EN/TR to be usable at all |
| Word-frequency lists (DE/EN/TR) | **Self-generated from CC0/PD text** (D15) | CC0 (our own generation) | English hunspell dictionary independently confirmed **permissive, SCOWL-derived, NOT GPL** (V2, §4, raw LibreOffice README fetch) | German (igerman98) license **could not be confirmed this session** — widely reported elsewhere as GPL-family but no successfully fetched source confirmed it (V2, §4) — D15's decision to generate frequency lists from Standard Ebooks/DTA/Wikisource-TR text ourselves sidesteps this uncertainty entirely rather than resolving it |
| Hyphenation | `hyphenation` crate | Apache-2.0/MIT (crate itself, confirmed) | v0.8.4, patterns bundled as `.bincode` | The crate's own license is clean, but the **underlying TeX/hyph-utf8 pattern licenses were not individually confirmed**, nor was the exact language list (DE/TR/EN presence) `[COULD NOT VERIFY, V2 §4]` — flagged for a follow-up fetch before shipping |
| Spellcheck engine | `zspell` | **"Non-standard"** per crates.io's own license field | v0.5.5, Hunspell `.aff`/`.dic`-compatible | License string unmapped to a known SPDX id — requires a manual check of the crate's actual `LICENSE` file before depending on it (V2, §4) |
| German compound splitting | Dictionary-based split-and-check (no ML) | — | No Rust-native CharSplit equivalent found (a negative result from absence, not confirmed non-existence) (V2, §4) | — |

**The compound-splitting decision, explained.** CharSplit (the standard Python tool for German compound decomposition) has no Rust port. V2's endorsed fallback — try splitting a token at each internal position, accept a split where both halves (accounting for German's `-s-`/`-n-`/`-es-` Fugenlaute) are found in the dictionary/word-frequency list — is dependency-free and sufficient for the spellcheck/hyphenation use case OpenConvert actually needs (word-boundary sanity for dehyphenation and language-detection confidence, not full morphological analysis). It deliberately does not attempt to match an ML compound splitter's recall; that tradeoff is consistent with D13.6's broader "no ML where a cheap deterministic method covers the actual need" pattern.

**Why self-generated word-frequency lists (D15), not a bundled hunspell pack, for v1 core data.** Two independent facts converge: the German hunspell dictionary's license could not be verified and is widely believed to be GPL-family, which would be a direct violation of D15's Apache-2.0 posture if bundled without resolution; and English's own hunspell dictionary, while confirmed permissive, is one language of three. Generating frequency lists from CC0/public-domain text the project already controls (Standard Ebooks CC0 XHTML for English, DTA plain text for German, Wikisource-TR for Turkish) removes the licensing question for the core data entirely, at the cost of the generation tooling being a new, small piece of `eval/`-side infrastructure. Hunspell packs remain a legitimate optional *later* addition once their licenses are individually confirmed.

---

## 10. Synthetic PDF tooling

| Tool | License | Role | Fit |
|---|---|---|---|
| **Typst** | **Apache-2.0** (V2, §3, confirmed crate + repo) | Primary generator of valid, complex fixtures | Embeddable in-process via the `typst` crate (`compile()` + a `World` trait); `typst-pdf` exposes `PdfOptions`/`PdfStandard`, including a confirmed **`Ua_1` variant for PDF/UA-1** conformance (V2, §3, https://docs.rs/typst-pdf/latest/typst_pdf/enum.PdfStandard.html); tagged-PDF-by-default landed in v0.14; current published crate is **0.15.1** — though V2 flags a real discrepancy between the newest GitHub release tag visible (0.14.2) and the crates.io-published version (0.15.1), recommending a direct re-check before pinning (V2, §3) |
| **WeasyPrint** | BSD-3 | XHTML→PDF ground truth pairing (Standard Ebooks source shares the same HTML/CSS family) | Python; CI-only, never a runtime dependency |
| **ReportLab** | BSD | Deliberate defect injection via canvas-level content-stream ordering control | Python; the tool of choice for "wrong reading order," overlapping text, missing/substituted fonts |
| **lopdf** | MIT | Mutating an already-valid fixture — strip ToUnicode maps, shuffle operators, break xrefs, damage structure trees | Rust-native; pairs with Typst-generated fixtures for a same-language toolchain |
| **qpdf** | Apache-2.0 | QDF mode converts a PDF to a human-readable, line-diffable text form for reviewable, git-diffable defect authoring | C++ CLI; "what exactly is broken about fixture #17" becomes an auditable code-review diff, not a binary patch |
| **pdf-writer** | MIT/Apache-2.0 `[UNVERIFIED — R7 could not confirm this session, standard knowledge]` | Rust-native equivalent of ReportLab's canvas trick — raw content-stream operator ordering | Same defect class as ReportLab, without leaving Rust |
| **krilla** | Own license `[UNVERIFIED — top-level license not confirmed, only its bundled-dependency NOTICE]` | Typst's actual production PDF backend since a 2025 switch | Worth prototyping directly; credible modern option since Typst itself now depends on it |
| mutool (MuPDF CLI) | **AGPLv3/commercial** | Rasterization/format conversion for scanned-PDF simulation | Restricted to an arms-length, offline, CI-only, never-shipped, never-linked utility if used at all — explicit legal sign-off recommended even for that limited use (R7, §C.1) |

(R7, §C.1, C.2) The two-stage pipeline this implies — generate valid-but-complex fixtures with Typst/WeasyPrint, then corrupt them on purpose with ReportLab/lopdf/qpdf — matches D18's corpus-honesty requirement directly: Typst tags PDFs by default while only 12.6% of real-world PDFs are tagged (PROBLEM_ANALYSIS §1), so D18 strips struct trees from synthetic fixtures by default and caps `ours(*)`-generated content at ≤40% of the corpus, with a frozen ≥100-file real-world holdout never used to fit thresholds.

---

## 11. Testing tooling

Versions confirmed via the crates.io JSON API (reliable this session; crates.io's HTML pages themselves 404'd for the verifier) (V2, §9):

| Crate | Version | License |
|---|---|---|
| `quick-xml` | 0.42.0 | MIT |
| `zip` | **8.6.0** (flagged: the brief expected "2.x/4.x" — a bigger jump than expected, worth a manual spot-check before pinning) | MIT |
| `insta` | 1.48.0 | Apache-2.0 |
| `proptest` | 1.11.0 | MIT OR Apache-2.0 |
| `cargo-nextest` | 0.9.143 | Apache-2.0 OR MIT |
| `cargo-deny` | 0.20.2 | MIT OR Apache-2.0 |
| `criterion` | 0.8.2 | Apache-2.0 OR MIT |

(V2, §9) D11's testing stack (`cargo nextest` + `insta` + `proptest` + `criterion` + `cargo-fuzz` + `cargo-deny`; Vitest; Playwright; pytest) is built on tools with real, current, permissively-licensed releases — no gaps found in this pass.

**The testing-strategy shape behind D11**, drawn from R9's synthesis of established patterns (insta/syrupy/Vitest snapshot conventions; proptest/Hypothesis/fast-check property testing; Chen et al. 1998 and Segura et al. 2016 on metamorphic testing) (R9, §B.1–B.4):

- **Canonical-JSON IR snapshots**, sorted keys, floats rounded to 2 decimals at serialization time, volatile fields (timestamps, UUIDs) redacted rather than mocked away — directly matches `oc-model`'s own canonical-JSON rules (IR_SKETCH.md).
- **Property tests** for exactly the invariants D13.4's conservation law needs machine-checked: idempotence of normalization, no-text-loss as a subset relation, monotonic reading order within a column, and crash-safety on arbitrary byte input (fuzzing-adjacent).
- **Metamorphic relations** purpose-built for a PDF pipeline where no independent ground truth exists for an arbitrary real-world file: page-order invariance, `/Rotate` invariance, and content-stream-operator-reordering invariance (glyphs drawn out of visual order must still extract correctly) — the last one directly targets a common real PDF-producer quirk.
- **A four-tier test pyramid** (fast-unit <1s/test with cassette-replayed LLM calls only; integration <2min with real external-oracle binaries; nightly with the full corpus, mutation testing, and a bounded number of live LLM calls to refresh cassettes) matches D11's CI-cost shape and D13.8's cassette/cache determinism contract.
- **Per-OS visual baselines, never shared cross-OS**, because font rasterization differs by platform text-shaping engine (FreeType/Linux, CoreText/macOS, DirectWrite/Windows) in ways no amount of code-level fixing resolves — this is Playwright's own documented guidance (R9, §B.10) and is why D7's visual QA is scoped to same-OS regression, never cross-OS pixel comparison.

---

## 12. Final stack

| Component | Choice | License | Why | Rejected alternatives |
|---|---|---|---|---|
| Core language | **Rust** | — | Low RAM/fast startup/small binary without GC; best permissive glyph-level PDF access; `insta`/`proptest` maturity (D1) | Python+Qt (AGPL PyMuPDF, 30–120 MB packages), Electron/JS (item-level PDF geometry only), Go (no permissive glyph-level PDF lib), .NET (no production WebView) |
| UI language | TypeScript | — | Existing maintainer skill; keeps the Rust surface a well-defined command API (D1) | — |
| Offline tooling | Python (`eval/` only, never shipped) | — | Best corpus/ground-truth/metrics ecosystem; never a build or runtime dependency (D1) | — |
| Desktop shell | **Tauri 2.x** | MIT/Apache-2.0 | 8.6 MiB / ~172 MB RAM vs Electron's 244 MiB / ~409 MB (measured); first-class `externalBin` sidecars; mandatory Ed25519-signed updater (D2) | Electron (fails P1–P3), pure-Rust UIs (no HTML renderer) |
| PDF parse/render | **PDFium via `pdfium-render` 0.9.x** + `bblanchon/pdfium-binaries` (SHA-256 pinned) | BSD-3 (engine), MIT/Apache-2.0 (binding) | Matches PyMuPDF on speed, beats it on quality, under a permissive license (D3) | MuPDF (AGPL), Poppler (GPL), pdf.js (item-level only) |
| PDF object model | `lopdf` | MIT | Outlines, `/StructTreeRoot`, XMP, `/Producer` (D3) | — |
| Layout algorithms | Ported from PdfPig (Docstrum, XY-cut, nearest-neighbour words, Allen-interval reading order) | Apache-2.0, attributed | Best open classical DLA reference implementation in any ecosystem (D3, Appendix A) | ML layout models (post-v1, D16) |
| OCR | **Tesseract 5**, system-detected first; optional signed pack later | Apache-2.0 | Only engine that is CPU-only, small, TSV+confidence, DE+TR confirmed (D4) | PP-OCR (community Rust ports only), Surya (OpenRAIL-M + 650M params), ocrs (Latin-only preview) |
| EPUB generation | Hand-rolled EPUB 3.3 (`quick-xml`+`zip`), typed XHTML builder | — | Byte-level control of the actual product; illegal markup made unrepresentable by the type system (D5) | `epub-builder` (targets 3.0.1, no content model), `ebooklib` (Python, AGPL) |
| EPUB validation | Tier-1 internal validator (always on) + optional EPUBCheck pack (jlink JRE, ~40–50 MB) + CI EPUBCheck/Ace | Internal: —; EPUBCheck: BSD-3 | JVM conflicts with size/startup priorities as a hard runtime dep; the four converter-common error classes are checkable without one (D6) | `epubveri` (98.8% parity but pre-1.0, AGPL — watch, don't adopt) |
| Visual/structural QA | Render-free structural checks (ships) + Playwright Chromium/WebKit in CI (not shipped) | — | Tauri cannot screenshot hidden windows; pixel PDF-vs-EPUB comparison is meaningless under reflow (D7) | Bundled headless Chromium (~281 MB), Blitz (pre-alpha) |
| LLM runtime | **`llama-server` sidecar**, official release binary, `externalBin`, lazy spawn, idle-kill | MIT (llama.cpp) | Crash isolation; `kill()` returns 100% of model RAM; HTTP makes every call mockable (D8) | In-process `llama-cpp-2` (UB-prone), require Ollama (second app, `num_ctx` 2048 truncation) |
| Default model | **Qwen3-1.7B**, dense, Q4_K_M, thinking off | Apache-2.0 | Official GGUF exists, `--cache-reuse` actually works, 119 languages per the Qwen3 release blog with DE/TR listed explicitly (model card: "100+", and it names llama.cpp) | Qwen3.5-2B (no official GGUF, hybrid arch defeats `--cache-reuse`) — promotable via a 9-gate test |
| Model format / BYO | GGUF; `LlmProvider` trait (`LocalSidecar`, `OpenAiCompatible`) | — | One HTTP client covers the sidecar, Ollama (auto-detected), and any OpenAI-compatible endpoint (D10) | — |
| Language ID | `whatlang` | MIT | Small, no 300 MB model download | `lingua-rs` (300 MB default, all 75 languages) |
| Word-frequency data | Self-generated from CC0/PD text (Standard Ebooks, DTA, Wikisource-TR) | CC0 (our generation) | Avoids the German hunspell dictionary's unverified, likely-GPL license (D15) | Bundling igerman98 directly |
| Hyphenation | `hyphenation` crate | Apache-2.0/MIT (crate); pattern licenses unverified | Knuth-Liang patterns, small bundled data | — |
| Testing | `cargo nextest` + `insta` + `proptest` + `criterion` + `cargo-fuzz` + `cargo-deny`; Vitest; Playwright; pytest | Per-crate, all permissive | Snapshot/property/fuzz coverage matched to a deterministic pipeline (D11) | — |
| Packaging | Tauri bundler: Windows NSIS+MSI, macOS .dmg (notarized), **Linux AppImage primary** + Flatpak + best-effort .deb/.rpm; Ed25519 updater | — | AppImage is covered by Tauri's updater; base install ≈35–45 MB (D12) | — |
| Project license | **Apache-2.0** | — | Patent grant; matches PDFium bindings, PdfPig (ported), Tesseract, Typst, and the permissive model tier (D15) | AGPL (would unlock mupdf-rs/epubveri but forecloses adoption) |

---

## 13. Watch list

Components deliberately not adopted for v1, tracked for re-evaluation:

- **`hayro`** (pure-Rust PDF interpreter, Apache-2.0/MIT) — "currently the most feature-complete" pure-Rust option, passes 1,400+ PDFs from established test suites, but self-describes as "still in a very development stage" with performance "explicitly not yet a priority" and no encrypted-PDF support (R2, §D.2). If it matures, it eliminates PDFium's native-sidecar/`libloading` requirement entirely — a strategic win for binary size and supply-chain simplicity. Already used as a differential-testing oracle in D3's residual-risk plan even before any adoption decision.
- **`pdf_oxide`** — real, permissively licensed, right API shape (`extract_chars`, `extract_spans`), but its published benchmarks (claimed faster than PyMuPDF) are vendor-reported with no described corpus; needs a controlled bake-off before being believed (R2, §D.5).
- **`epubveri`** — 98.8%/1.1% accuracy against EPUBCheck's own test suite, architecturally exactly right for a Rust-based converter, blocked purely by license (AGPL-3.0/commercial) and pre-1.0 maturity. Re-evaluate at 1.0, and consider contacting the maintainers about permissive licensing (R5, §B3).
- **Blitz** (DioxusLabs: Stylo + Taffy + Vello/Skia) — the only Rust-native HTML/CSS renderer that could close the "no in-app pixel QA" gap, but the maintainers themselves state it is "beta" with "still many bugs and missing features" (R5, §D6; R6, §8). Revisit in 2027.
- **`ocrs`** — the most promising pure-Rust OCR path, but an early preview and Latin-alphabet-only today, disqualifying it for Turkish (R3, §3.10).
- **PP-OCR Rust ports** (`oar-ocr`, `rust-paddle-ocr`, `paddle-ocr-rs`) — real community projects, none official, adding build complexity without a clean win over Tesseract today; PP-DocLayout-S's 14.5 ms/page CPU claim (if it survives verification, and its weights license is confirmed) would also be the cheapest layout-model option surveyed by a wide margin (R2, §D.4; R3, §2).
- **Gemma 4** — confirmed genuinely Apache-2.0 as of this research round (a change from Gemma 1–3's custom terms) (V1, §2), making it a legitimate *alternate* candidate model — but its Per-Layer-Embeddings footprint is measured at 0.74–3.5 GB peak RAM depending on platform for the E2B tier, and no OmniDocBench-class document-task evidence was published for it (D9). Watch for evidence, not adopted.
- **LFM2.5** — excluded outright, not merely watched: its license is revenue-capped (LFM Open License v1.0), and no Turkish-language evidence was found (D9). Included on the watch list only in case its license terms change.
- **Qwen3.5 promotion** — the default-model question is not closed, it is gated. D9's nine-condition promotion test (G1–G9: health, grammar-constrained correctness, tokenizer/template fidelity, per-book latency budget, **prefix-reuse ratio specifically addressing the §8.3 finding**, peak RSS, McNemar non-inferiority against the incumbent, DE/TR flat-enum accuracy, and GGUF reproducibility from a pinned official revision) is the explicit mechanism by which Qwen3.5-2B/4B could become the shipped default without another full architecture review. Re-run on every llama.cpp version bump (D9).

---

## 14. Notes for the Chief Architect

No disagreement with `DECISIONS.md` D1–D12 was found in this research pass; every choice traced above matches its corresponding decision, including the [RT]-marked ones, which this document treats as the current, already-corrected state rather than as an open dispute. Three items are worth the architect's attention as *evidence-quality* gaps rather than decision-quality ones:

1. **The `zip` crate version jump (0.42→ the confirmed 8.6.0)** is exactly the kind of single-source, higher-than-expected number V2 itself flags for a manual spot-check before pinning in `Cargo.toml` (V2, §9). Low risk, cheap to verify, worth doing before Phase 0.
2. **German and Turkish dictionary/hyphenation-pattern licensing remains genuinely unresolved**, not merely under-researched — two independent verification attempts (V2, §4) hit dead ends (404s, robots.txt blocks, binary-data fetch failures) rather than finding a permissive answer. D15's self-generated-word-list strategy correctly routes around this for the core data, but the `hyphenation` crate's underlying TeX pattern licenses were never confirmed and should be checked directly before the hyphenation pipeline ships, since that dependency was not rerouted the same way.
3. **PDFium's SMask/vector-path/bookmark coverage was never verified at the API-signature level** (R2, §D.5) — this is the one PDFium gap that could materially affect figure-extraction quality (a stated open uncertainty in D3's own residual-risk note) and would be cheap to close with a short spike against a handful of real PDFs before committing to it as load-bearing for figure anchoring.

None of the three changes any decision in `DECISIONS.md`; all three are verification debt. **They are now tracked as an explicit Phase 0 task list** — `IMPLEMENTATION_PLAN.md` Phase 0, "Verification debt (must close before the phase that depends on it)", owner = maintainer — together with the four items carried from `LICENSE_AND_DEPENDENCIES.md`: `zspell`'s licence text and the German/Turkish hunspell licences (both only if the optional dictionary pack is built), the validation pack's per-vendor JRE licence (Temurin), and the UB-Mannheim Windows Tesseract path and version check. Each closes with a dated note in `docs/DECISIONS_LOG.md`, and an open row past its blocking phase is a release blocker.
