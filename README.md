# OpenConvert

Open-source, local-first desktop app that converts PDF into high-quality **reflowable EPUB 3.3**.

Not a format converter: a deterministic structure-inference pipeline with a character-conservation law,
a validate→repair loop, and a small local LLM used only for four narrow, once-per-book semantic decisions.

> Deterministic first. AI where necessary. Validate everything. Repair only what is broken.

**Status:** pre-implementation. The design is complete (`docs/`); the code is being built phase by phase
per `docs/IMPLEMENTATION_PLAN.md`. Current state: `PROGRESS.md`.

- Start here: `docs/DECISIONS.md` (the ADR) → `docs/ARCHITECTURE.md` → `docs/PIPELINE.md`
- Implementing: `docs/IMPLEMENTATION_PLAN.md`, `CLAUDE.md`
- Evidence archive: `research/`

License: Apache-2.0 (see `docs/LICENSE_AND_DEPENDENCIES.md`).
