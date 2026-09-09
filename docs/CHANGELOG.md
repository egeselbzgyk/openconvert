# Changelog

One section per completed phase, listing new CLI flags, new IR fields, new warning codes and new
`thresholds.toml` entries. Required by the Definition of Done (`IMPLEMENTATION_PLAN.md` §0.3 item 8).

## Phase 0 — Repo, workspace, CI, thresholds, hello-Tauri, first Typst fixtures

*(in progress)*

### Workspace bootstrap (§1.1–§1.9)

- New: Cargo workspace with 12 `oc-*`/`openconvert` crates plus `xtask` and `apps/desktop/src-tauri`.
- New: `thresholds.toml` — 79 entries, every one carrying `{value, source, evidence, owner, review_by}` (D17).
- New: `models.toml` — four model entries (default `qwen3-1.7b-q4_k_m`), `TODO_` fields filled at Phase 9 (D9).
- New: `deny.toml` allow-list per D15; `cargo deny check` clean.
- New: `.config/nextest.toml` (`default` and `ci` profiles, retries banned), `.github/workflows/{ci,nightly}.yml`.
- New: `schemas/job-spec.v1.json` (§2.2), `corpus/manifest.json` (empty, schema v1), `eval/` Python skeleton.
- New: `docs/TEST_MATRIX.md`.
- Changed vs the plan, each recorded in `docs/DECISIONS_LOG.md`: toolchain pin 1.85.0 → 1.98.1;
  `pdfium-render` features `["image","libloading"]` → `["pdfium_latest","image_025","thread_safe"]`;
  internal path dependencies carry explicit versions; `ureq` deferred to Phase 9.
- Verification debt: **VD-a closed** (`zip` 8.6.0 is the current stable major).

### IR (`oc-model`)

- New: `IR_VERSION = 1`.
- New: `geom::Rect` — the one normalised page space (top-left origin, y down, points, after `/Rotate`
  and CropBox offset).
- New: `ids::BlockId` with `derive` / `with_collision_suffix` / `as_str` (D13.3).
- New: `canonical::to_canonical_json` and `canonical::CanonError` — sorted keys, `ir_version` first,
  NFC strings, `f32` at two decimals, non-finite floats rejected instead of written as `null`.

### PDF geometry (`oc-pdf`)

- New: `geom::PdfRect` (PDF user space, y up), `geom::Rotate`, `geom::PageGeometry` with
  `normalise` / `normalise_point` / `width_pt` / `height_pt`, and `geom::INSIDE_PAGE_TOLERANCE_PT`.

### Thresholds (`oc-core`)

- New: `crates/oc-core/build.rs` generates the typed `thresholds::T` and the `thresholds::PROVENANCE`
  table from `thresholds.toml`, and fails the build on a malformed entry.
- New: `thresholds::lint`, `thresholds::LintFinding`, `thresholds::LintProblem` — D17's owner/expiry
  rule, shared by the test and (later) `xtask thresholds-lint`.

### New thresholds

All 79 initial `thresholds.toml` entries (see the file; §1.5 of the plan is the reference list).
`model_gate.g4_max_seconds_on_L` is spelled `model_gate.g4_max_seconds_on_l`: a threshold key becomes a
Rust field name, and a capital letter there trips `non_snake_case` under `-D warnings`.

### New CLI flags

*(none yet)*

### New warning codes

*(none yet)*
