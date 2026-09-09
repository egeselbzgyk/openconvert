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

### Desktop shell (`apps/desktop`)

- New: the Tauri 2 app, its capability (one program: the sidecar) and CSP (`connect-src 'none'`).
- New: `apps/desktop/ui/src/engine.ts` - `parseEvents`, `handshake`, `describeEngine`, and the
  version handshake that refuses a stale staged sidecar (A0.7).
- New: `cargo run -p xtask -- stage-sidecars`, which writes a `STAGE_STAMP` beside the staged engine.
- New: `LICENSE` (Apache-2.0, D15) and the app icons.

### Test tooling (`oc-testkit`, `xtask`)

- New: `oc-testkit::assertions` - the eleven-kind closed `Assertion` enum, `AssertionDocument`,
  `parse`, `evaluate`, and a three-valued `Outcome` where `Pending` is never a pass.
- New: `cargo run -p xtask -- ci-lint` (no skipped tests, no unnumbered markers,
  `--release-branch` also rejects `models.toml` placeholders) and
  `cargo run -p xtask -- thresholds-lint` (D17 provenance, UTC).
- `xtask` now has a library target so its binary and its tests share one implementation of
  every rule.

### CLI and events (`openconvert`, `oc-core`)

- New CLI: `openconvert inspect <INPUT.pdf> [--json] [--pages <RANGE>] [--password <STRING>]
  [--progress none|json]`, plus `--help` and `--version`.
- New env var: `OC_PDF_PASSWORD`.
- New: `oc-core::events::EventSink` (NDJSON on stderr, `hello`/`done`/`fatal`, 8 KiB line cap,
  `PROTOCOL_VERSION = 1`) and `oc-core::exit::ExitCode`.
- New error codes: `E_USAGE`, `E_INPUT`, `E_PDF`, `E_PDFIUM_ABI`.

### Inspection (`oc-pdf`)

- New: `inspect::inspect`, `inspect::InspectReport` (`openconvert.inspect/1`), `inspect::PdfDoc`,
  `inspect::PdfOpen`, `inspect::InspectOptions`, `pdfium::PdfiumDoc`.
- New warning codes: `W_IMAGE_ONLY_PAGES`, `W_BROKEN_TEXT_PAGES`.

### Fixtures (`xtask`, `corpus/`)

- New: `cargo run -p xtask -- fixtures` compiles the three Typst sources in-process to
  `target/fixtures/` and records them in `corpus/manifest.json` with `producer_stratum = "ours(Typst)"`.
  `--keep-structtree` emits the `__tagged` variants instead (D18).
- New: `corpus/fixtures/typst/{f01_prose_single_column,f02_two_column,f03_image_only}.typ` and
  `corpus/fixtures/assets/scan_page_01.png`, plus its generator
  `eval/src/oc_eval/generate/scan_sim.py`.

### Supply-chain policy

- Changed: `deny.toml` now excludes `xtask` and audits only what ships, with every rule absolute.
- New: `deny.tools.toml` audits the developer tooling with the same licence, ban and source policy;
  its advisories are reported by a non-blocking CI step rather than enforced. `docs/SECURITY.md` 9
  states the scope of each gate.

### Producer detection (`oc-pdf`)

- New: `producer::ProducerFamily` (nine variants; `PdfTeX` serialises as `pdfTeX` to match D18's
  stratum spelling) and `producer::producer_family`.

### Page classification (`oc-pdf`)

- New: `classify::PageClass`, `classify::PageCharStats`, `classify::PageImageStats`,
  `classify::classify_page` (D13.10).
- New thresholds: `pageclass.confidence.{ocr_sandwich, image_only, blank, broken_text, mixed, text,
  fallback_blank}`, all `provisional`.

### PDF backend (`oc-pdf`, `xtask`)

- New: `cargo run -p xtask -- vendor-pdfium` — fetches the PDFium binary pinned in `xtask/pdfium.lock`
  (`chromium/7881`, 151.0.7881.0) for the host triple, verifies its SHA-256, and unpacks it to
  `vendor/pdfium/<triple>/`.
- New: `backend::PdfBackend`, `backend::BackendVersion`, `pdfium::PdfiumBackend::bind`,
  `pdfium::EXPECTED_PDFIUM_BUILD`, `error::PdfError`.
- New env var: `OC_PDFIUM_PATH` — an authoritative override for PDFium discovery.
- Changed: `pdfium-render` selects the exact `pdfium_7881` feature instead of `pdfium_latest`.

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

## Phase 1 — PDF inspection and ingestion

*(in progress)*

### IR (`oc-model`)

- New: `extract::{Glyph, FontInfo, FontId, CharHistogram}` and
  `ledger::{Reason, LedgerEntry}` — the closed fifteen-variant `Reason` of D13.4.

### Extraction (`oc-pdf`)

- New: `glyphs::{PageGlyphs, family_key}`, `PdfDoc::page_glyphs`, and
  `pdfium::PdfiumBackend::resolve_library`.
- Changed: `PdfiumBackend::bind` is idempotent — PDFium can only be bound once per process.
- Changed: `PageGeometry::normalise`'s debug assertion is now conditional on the source rect
  lying inside the crop box; content outside it is `ClippedOffPage`, not a bug.

### Metamorphic invariants (`oc-pdf`, `oc-testkit`, `xtask`)

- New: `oc_testkit::mutate::{rotate, cropbox_offset}` — the mutation recipes, over `lopdf`.
- New: `oc_testkit::handmade::line_of_glyphs`, one line of glyphs in any operator order.
- New: `cargo xtask mutations`, writing `corpus/fixtures/mutations/h01__cropbox_offset.pdf`.
- Measured: PDFium's character order is not invariant under `/Rotate 90`. No stage may
  treat the backend's glyph order as reading order; see `docs/DECISIONS_LOG.md`.

### Broken-text detection (`oc-pdf`, `oc-testkit`, `xtask`)

- New: `oc_testkit::mutate::strip_tounicode`, and `f01__strip_tounicode.pdf` under
  `corpus/fixtures/mutations/`.
- **Fixed:** a page whose CID fonts have no `/ToUnicode` classified as `text` with full
  confidence. PDFium returns the raw glyph indices, which are control characters and so were
  counted by neither the U+FFFD nor the private-use counter. `PageCharStats` gains `control`
  and `is_broken_text` sums all three over the unchanged threshold.
- New: `PageInfo.control_chars` in the `inspect` report, so a `broken_text` verdict says which
  of the three undecodable kinds reached it. The three snapshots change by that field only.
- Measured: PDFium marks a line-break hyphen with U+0002 and `is_hyphen()`; those are excluded
  from the control count. See `docs/DECISIONS_LOG.md` for what it means for `N` and Phase 3.

### Images (`oc-model`, `oc-pdf`, `oc-testkit`)

- New: `oc_model::extract::{ImageRef, ImageKind, ImageId, PageRef}` per `IR_SKETCH`.
- New: `oc_pdf::images::{classify_image, effective_dpi}` and `PdfDoc::page_images`.
- New: `has_smask` and `is_inline` are read from the file with `lopdf` — PDFium exposes
  neither — and matched to PDFium's image objects by draw order, used only when the two
  agree on the count. `PdfiumDoc` now keeps the parsed object tree alongside.
- New fixtures `h09_image_smask` and `h10_inline_image`, the only ones for which those two
  flags are true.
- New thresholds `images.{full_page_area_ratio, strip_aspect_ratio, ornament_max_side_pt}`.

### Resource limits (`oc-core`, `oc-pdf`, `openconvert`, `oc-testkit`)

- New: `oc_core::limits::{Limits, LimitExceeded}` — the shipped budget, from `thresholds.toml`.
- New: `oc_pdf::limits::{check_image, read_page_content}` and `PdfError::LimitExceeded`.
- New: `PdfOpen::open_with_limits`, which is where the page-count guard runs — before any
  page is touched. `open` keeps the shipped defaults.
- New: `openconvert inspect --max-pages <N>`. A limit refusal exits **2** with
  `E_LIMIT_EXCEEDED`: it is a configuration outcome, not a conversion failure.
- New fixtures `h11_pixel_bomb` and `h12_decompression_bomb`.

### Encryption (`oc-pdf`, `oc-testkit`, `openconvert`)

- New: `oc_pdf::encrypt::Permissions`, reported on `document.permissions` and **never
  enforced** (D13.11). The three `inspect` snapshots change by that object only.
- New: `PdfError::PasswordRequired`, split out of `PdfError::Open` — a supervisor has to be
  able to tell "needs a password" from "not a PDF". The CLI maps it to exit **2** with
  `E_PASSWORD_REQUIRED`.
- New: `oc_testkit::mutate::encrypt` (AES-128 via `lopdf`), and three encrypted fixtures
  under `corpus/fixtures/mutations/`.

### Outlines and metadata (`oc-model`, `oc-pdf`, `oc-testkit`)

- New: `oc_model::extract::OutlineEntry`, `oc_pdf::outline::read_outline` and
  `PdfDoc::outline` — walked depth-first, because `PdfBookmarks::iter()` drops the depth.
- New: `oc_pdf::meta::{is_encrypted, has_struct_tree, xmp_packet, read_xmp, XmpMeta}`.
- **Fixed:** `encrypted` and `has_struct_tree` were byte searches over the raw file, so any
  document containing the *text* `/StructTreeRoot` reported a structure tree. Both now come
  from the object tree, with the byte search kept only as a fallback.
- New fixture `h13_outline` (six entries, three levels, two roots) and threshold
  `limits.max_outline_entries`.

### Fuzz-lite (`oc-pdf`)

- New: `tests/fuzz_lite.rs` — ~22 200 inputs per run across three generators, asserting only
  that nothing panics or hangs.
- Measured: truncation reaches an extraction path in 2–3 % of cases (PDFium needs the trailer
  and xref at the end of the file); single-byte corruption reaches one in 93–98 %. Both
  generators are kept and the reach figure is asserted by its own test, so it cannot decay
  silently. See `docs/DECISIONS_LOG.md`.

### `dump-stage ingest` (`oc-model`, `oc-pdf`, `openconvert`)

- New: `oc_pdf::dump::{DumpHeader, DumpPage, header, page}` and
  `openconvert dump-stage <STAGE> <INPUT.pdf>` — one canonical-JSON object per line, a header
  then one per page, streamed so peak memory is one page (RT B4).
- **Changed:** `CharHistogram` serialises as a character-to-count map instead of its internal
  128-slot ASCII array, which was putting 128 mostly-zero entries into every page of every
  dump.
- An unimplemented stage is `E_UNKNOWN_STAGE` and exit 2.

### Differential oracle (`oc-pdf`, CI)

- New: `tests/oracle.rs` behind the `poppler-oracle` cargo feature, and a CI job that installs
  `poppler-utils` and turns it on. The feature is the gate in place of a skip attribute, which
  CLAUDE.md bans.
- **Found:** PDFium reports both a hard hyphen (U+002D) and a soft hyphen (U+00AD) as U+0002,
  and `is_hyphen()` does not distinguish them. D13.4's `SoftHyphen` reason cannot fire from a
  PDFium stream, and PIPELINE §369's compound-word rule loses its cheapest signal. Recovering
  it needs content-stream access — the same mechanism the `OverdrawDedup` gap needs. See
  `docs/DECISIONS_LOG.md`.

### Performance (`oc-pdf`)

- **Fixed:** image extraction was O(n²) in page count — `page_images` rebuilt the document's
  page map on every page. Measured at 78 µs/page over 50 pages and 564 µs/page over 400; now
  flat at ~16 µs/page. `PdfiumDoc` reads the ordered page ids once at open.
- New: `tests/scaling.rs`, which asserts the *ratio* of per-page cost between 100 and 800
  pages rather than any absolute duration, so it detects a complexity change and not a slow
  CI runner.
