# Round 1 → Round 2: contradictions, unverified load-bearing claims, gaps

Compiled by the Chief Architect after reading all 10 round-1 reports (R1–R10).

## A. Contradictions between reports

| # | Topic | Report A says | Report B says | Why it matters | Resolution plan |
|---|---|---|---|---|---|
| 1 | Gemma 4 license | R4: Gemma 4 (Mar 2026) is **Apache-2.0**, first Gemma without Gemma Terms | R8: Gemma 3/3n/4 under **Gemma Terms of Use** (R8 only fetched the generic terms page) | Affects whether Gemma 4 can be an alternate model; does not affect default (Qwen3.5) | Fetch HF `google/gemma-4-E2B` card + `ai.google.dev/gemma/terms` (V1) |
| 2 | Small-LLM CPU prefill throughput | R10: ~96 tok/s prefill for Gemma-3-1B Q4 (i7-12700H, llama.cpp #13664) → 16–26 s prefill/page | R4: ~1,200–2,000 tok/s prefill for a 2B (derived from LFM2.5-1.2B at 2,975 tok/s on Ryzen AI 9 HX 370) | ~15× gap changes how many LLM calls are affordable; but both agree per-page LLM is unaffordable and per-book calls are the pattern | Find independent llama-bench pp512/tg128 numbers for 1–3B Q4 on x86 laptops (V1). Architecture is made robust to either: per-book O(1) calls + hard per-block budget |
| 3 | Tauri offscreen rendering for QA | R5: "webview screenshot workable via community tooling" | R6: hidden-window screenshots **not supported** (wry #1358, tao #289 open); use Playwright in CI | Decides in-app visual QA design | Trust R6 (primary sources). Check only whether a hidden Tauri window can run JS and return DOM layout metrics via IPC (V2). Default design: in-app = render-free structural checks + visible preview; pixel/DOM checks in CI |
| 4 | LLM page budget | R4: "LLM on ≤15% of pages, ≤2 calls/page" | R10: "never per page; ~4–8 per-book calls + ≤3% of blocks batched" | Determines cost model & UX | Adopt R10's design; R4's budget table is reinterpreted as an upper bound for the *block-batch* path |
| 5 | Qwen license verification | R4: Qwen3.5 small models Apache-2.0 (model cards) | R8: verified only Qwen3 (Qwen3-32B LICENSE) | Default model decision | Fetch Qwen/Qwen3.5-2B, -0.8B, -4B cards (V1) |
| 6 | MinerU license | R1: "MinerU Open Source License (Apache-2.0-based) since v3.1.0" | R8: "Apache-2.0 + additional commercial terms (MAU/revenue thresholds)" | None — MinerU not a dependency | No action; both agree it is not pure Apache-2.0 |
| 7 | OCR integration | R3: `tesseract-rs` (compile from source, FFI) as most self-contained | R6: Tesseract as **sidecar binary** via `externalBin` | Packaging plan | Decision: sidecar (consistent with llama-server pattern, no FFI, optional download). Verify availability of static tesseract builds / build recipe (V2) |

## B. Load-bearing claims marked [UNVERIFIED] that must be checked before decisions

1. Qwen3.5-2B: exists, Apache-2.0, non-thinking default, GGUF available, llama.cpp supports its architecture (hybrid DeltaNet?), IFEval 61.2 non-thinking. (V1)
2. Gemma 4 E2B license + real GGUF footprint (V1) — alternate model only.
3. llama.cpp release assets: does `ggml-org/llama.cpp` publish per-OS binaries including `llama-server` (win-avx2/avx512, macOS arm64, linux x64)? (V1) — packaging plan depends on it.
4. `docling-layout-egret` / `heron` model cards: Apache-2.0, ONNX export availability, input size. (V2) — optional ML escalation only.
5. Dictionary/word-list licenses for EN/DE/TR (hunspell de_DE = igerman98 GPL?; en_US SCOWL; tr_TR) and Rust crates for spell/dictionary lookup and hyphenation (`zspell`, `hyphenation`, `lingua-rs`, `whatlang`) with sizes and licenses. (V2) — dehyphenation & language detection design.
6. `pdfium-render` char API: font weight, font flags, `is_generated`, `is_hyphen`, `has_unicode_map_error`, text render mode; page object API for images (SMask) and paths. (V2)
7. Typst as a Rust crate: can it compile a document in-process and emit tagged PDF; API stability. (V2)
8. EPUBCheck 5.3 Java version requirement (Java 11+?) for CI. (V2, minor)
9. Tesseract prebuilt/static binaries per OS or a known CI build recipe (vcpkg/conda-forge). (V2)

## C. Gaps (no evidence found in round 1; decisions will be made with explicit uncertainty)

- Turkish: no published numbers for dehyphenation/OCR/LLM on Turkish. Plan: deterministic locale-aware casing; dictionary-based dehyphenation with a Turkish word list; LLM gated per language after a Turkish probe set exists.
- Confidence calibration: no published calibration of geometric confidence signals for PDF→EPUB. Plan: build a labelled gold set (Standard Ebooks-derived + hand labels) and fit thresholds to a false-repair-rate target (selective prediction).
- Footnote/caption/drop-cap/verse detection accuracy: no isolated published numbers. Plan: self-calibrate on the corpus; report per-category.
- "% of PDFs that are scanned" — unknown; treat OCR as optional download in v1.
- No public benchmark measures PDF→reflowable-EPUB quality on trade books. Plan: build one (assertion suite in olmOCR-bench style + Standard-Ebooks ground-truth pairs).
