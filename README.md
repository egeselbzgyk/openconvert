# OpenConvert

Open-source, local-first desktop app that converts PDF into high-quality **reflowable EPUB 3.3**.

Not a format converter: a deterministic structure-inference pipeline with a character-conservation law,
a validate→repair loop, and a small local LLM used only for four narrow, once-per-book semantic decisions.

> Deterministic first. AI where necessary. Validate everything. Repair only what is broken.

**Status:** the engine converts. Phases 0–5 are complete: a PDF goes in and a reflowable EPUB 3.3
comes out, and EPUBCheck reports zero errors and zero warnings on every fixture. What is not there
yet is the repair loop, the conversion report, OCR, the LLM path and the desktop UI — the design is
complete (`docs/`) and the code is built phase by phase per `docs/IMPLEMENTATION_PLAN.md`. Current
state: `PROGRESS.md`.

```
cargo run -p xtask -- vendor-pdfium        # the pinned PDFium binary
cargo run -p xtask -- fixtures            # compile the Typst fixtures
cargo run -p openconvert -- convert target/fixtures/f09_novel_structure.pdf
cargo run -p openconvert -- validate target/fixtures/f09_novel_structure.epub
```

- Start here: `docs/DECISIONS.md` (the ADR) → `docs/ARCHITECTURE.md` → `docs/PIPELINE.md`
- Implementing: `docs/IMPLEMENTATION_PLAN.md`, `CLAUDE.md`
- Evidence archive: `research/`

License: Apache-2.0 (see `docs/LICENSE_AND_DEPENDENCIES.md`).
