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

Turns a PDF into the Stage-1 extraction layer: glyphs with all thirteen verified signals,
images decoded and composited, outlines, metadata, encryption, resource limits, `C_raw` as the
first conservation baseline, and `dump-stage ingest` to see all of it.

All twenty named tests (1.1–1.20) pass, plus twenty-odd more that each exist because something
was measured and turned out not to be what the plan assumed. **VD-d is closed.** The five
findings worth knowing before touching this code are in `docs/DECISIONS_LOG.md`: PDFium's glyph
order is not rotation-invariant; it reports both hard and soft hyphens as U+0002; a CID font
stripped of `/ToUnicode` yields control characters that the broken-text detector originally
missed; image extraction was O(n²) in page count; and the `pypdfium2` reference the plan
proposed for VD-d could not have answered VD-d's question.

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

### Cancellation (`oc-core`, `openconvert`)

- New: `oc_core::cancel::{Cancel, Outcome}` and `oc_core::progress::{Progress, Silent}`.
- New: `openconvert::control` — NDJSON on stdin, `{"t":"cancel"}` and `{"t":"ping"}` (§2.3).
  Unknown messages are ignored so the protocol stays forward-compatible.
- `dump-stage` polls the flag at the stage boundary and before every page, and exits **3**
  with `done{"status":"cancelled"}`.

### Images decoded (`oc-pdf`, `oc-testkit`) — VD-d closed

- New: `oc_pdf::images::DecodedImage` and `PdfDoc::image_bytes`, which composite through
  `get_processed_image()` and check `max_image_pixels` before decoding.
- New fixtures `h14_stencil_mask` and `h15_indexed_colour`.
- **VD-d answered:** PDFium composites soft masks, stencil masks, indexed palettes and
  DeviceGray correctly, so the image policy uses `get_processed_image()` and writes no
  compositing of its own. CMYK JPEG, 1-bit CCITT and JPX remain uncovered — no encoder exists
  to build the fixtures honestly — and are deferred to real samples in the Phase 7 corpus.
- The plan's proposed `pypdfium2` reference was **not** used: it wraps the same PDFium, so the
  comparison could not have answered the question. Known-answer fixtures were used instead.
  See `docs/DECISIONS_LOG.md`.

## Phase 2 — Text assembly and normalization

Glyphs become runs and lines, normalisation `N` is applied exactly once, page furniture is
removed before segmentation, and every stage runs under the conservation law. All twenty-two
rows of the Phase 2 table are green, plus about forty additions.

### The conservation law, first (`oc-core`, `oc-model`)

- New: `oc_core::ledger_check::check_invariants` — I-1 (balance), I-2 (declared reasons),
  I-3 (conserving stages), I-4 (cumulative budgets) — plus `oc_core::stages`, where each
  stage's kind and closed reason set are declared next to the checker that enforces them.
  Built before any transformation, so every transformation is born under it.
- New in `oc-model`: `LedgerDelta`, `StageKind`, `StageCheck`, `c_of`.
- **Corrected:** `Reason::adds()` was `matches!(self, Ocr)`. `LigatureExpand` is on both sides
  of the ledger at once — the case I-1 exists for — so writing it tripped a debug assertion.
  Replaced with `may_add` / `may_remove`.
- **Decided:** I-4 is charged on *net* loss per reason. Read literally, a book that sets `ﬁ`
  blows the 0.001 "other" budget while losing nothing. See `docs/DECISIONS_LOG.md`.

### Normalisation `N` (`oc-text`)

- New: `normalize`, `NFC ∘ strip(U+00AD) ∘ expand_ligatures ∘ NFC`, with a paired
  Removed + Added entry per ligature and a `SoftHyphen` removal per soft hyphen.
- **Two departures from the letter of the spec**, both recorded: `N` composes again after
  stripping, without which its output is not NFC and it is not idempotent; and U+FB05 expands
  to `st`, not the plan's `ft`, which is the long-s misreading and would turn `beſt` into
  `beft`.
- New: `fold_key`, the only place in the pipeline where case changes. Turkish and Azerbaijani
  pair the dotted and dotless i their own way. A property test forbids case folding in
  emitted text for any input.
- New in `oc-model`: `LangTag`.

### The hyphen marker (`oc-pdf`) — half of Phase 1's largest carried debt

- **Fixed:** PDFium reports a hyphen drawn at a line break as U+0002. Extraction now decodes
  it to U+002D before `C_raw` is counted, so no run text carries a control character. What it
  cannot recover — whether the source wrote U+002D or U+00AD — needs the content stream and
  moves to Phase 3 with the `OverdrawDedup` count that needs the same mechanism.

### Word and line assembly (`oc-text`, `oc-model`)

- New: `oc_model::text::{Run, Line, RunId, TextProvenance, FurnitureKind}`, and
  `oc_text::{words, lines}`.
- Spaces are inferred from the gap distribution: 2-means per (font, size), thresholded at the
  midpoint of the centroids, only when the clusters separate. Letter-spaced display text has
  no separation and falls back to a metric width.
- Baseline clustering takes its tolerance from the *larger* of the two sizes, so a 7 pt marker
  raised 3 pt joins its 12 pt line instead of becoming one.
- **Found by test 2.10:** justified text stretches its spaces with `TJ` offsets, so assembly
  was inventing a second space beside every real one. A space is now only inferred between two
  non-space glyphs.
- **Found by the test 2.21 snapshot, and worse:** with space-adjacent gaps excluded, 2-means
  fits intra-word kerning alone and always returns two clusters — `except at o ccasional
  inter vals`. Every other assertion in the phase stayed green, because a space is whitespace
  and outside `C`. `words.min_space_ratio` (0.15 em) is now an absolute floor under any
  inferred space.

### Furniture (`oc-layout`)

- New: `detect_furniture` / `apply_furniture`. Bands, digit-masked and locale-folded keys,
  normalised edit distance, y-clustering, and repetition measured over four scopes — global,
  each parity, and the best sliding window — with the ratio *and* the repeat requirement
  computed inside the scope they apply to.
- Five rules that refuse before anything is deleted: a page's only line stays; a body-typed
  unpunctuated line followed by lower case stays; a ratio in the grey zone abstains and is
  marked `uncertain`; an all-numeric line that forms no arithmetic progression is a chapter
  number and stays; and no page is ever emptied.
- Page numbers, arabic and roman, move to `PageRef.label` — outside `C`, which is what makes
  removing them a clean `PageNumber` entry rather than a paradox.

### Statistics, dictionaries and language (`oc-text`)

- New: `quality_stats` — the nine Gopher/MassiveText numbers with datatrove's thresholds, and
  the `ok`/`suspicious`/`broken` verdict. They route and never fix.
- New: `oc_text::freq` — a sorted string table with a `u32` offset index, and `dict_hit_rate`,
  the second `broken_text` signal. Tokens are classified before they are counted, because a
  letters-only tokenizer finds nothing at all in a page of glyph indices.
- New: `eval/src/oc_eval/generate/wordfreq.py`, which refuses any source outside CC0/PD and
  writes a manifest naming every file it read.
- **English ships, German and Turkish do not.** The plan calls DTA and Wikisource-TR
  "CC0/PD"; their transcriptions are CC-BY-SA, which D15 does not allow in a shipped
  artefact. `dict_hit_rate` returns `None` for both, and `None` is not zero.
- New: `oc_text::lang` — `dc:language` over the body, `xml:lang` per block behind three
  guards, and `W_LANG_UNSTABLE` when more than a fifth of the book disagrees.
- **Found:** `whatlang` returns ISO 639-3 and `dc:language` is BCP-47, so `eng` would have
  failed EPUBCheck at the end of a conversion. A 70-entry table maps them and a test over
  `Lang::all()` keeps it complete.

### Fixtures and wiring

- New hand-made fixtures h16–h21: superscript marker, letter-spaced text, mixed sizes, a
  constant band number, recto/verso heads, and a page whose only line is its running head.
  The plan names h07–h12; Phase 1 had already spent those.
- **Found:** `h04_ligature_fi` never contained a ligature. `to_winansi` mapped U+FB01 to `?`,
  so the page drew `?n` and every test that carried it through unchanged passed on a question
  mark. Fixed with a `/ToUnicode` CMap — whereupon PDFium expanded the ligature itself, which
  is the opposite of what R2 §B.8 states.
- New Typst fixtures f04 (German) and f05 (Turkish), written for the fixtures rather than
  quoted.
- New: `openconvert` grew a library target holding `pipeline` and `dump_text`. The
  orchestrator cannot live in `oc-core`, which owns `thresholds` and is therefore a dependency
  of every stage crate.
- New: `dump-stage text`.

## Phase 3 — Layout

The document stops being a page of boxes and becomes an ordered sequence of paragraphs.
Blocks, columns, reading order, paragraph reconstruction and dehyphenation. All seventeen rows
of the Phase 3 table are green, plus about sixty additions. VD-b is closed.

### Blocks, cross-checked (`oc-layout::blocks`)

- New: `segment_blocks` — Docstrum's between-line rule as the primary segmenter, Breuel's
  maximal-white-rectangle cover as an independent second reading, and a per-block IoU between
  the two. Disagreement below `layout.block.agreement_iou_min` flags the block; Docstrum's
  answer stands either way.
- New in `oc-model`: `Block`, `BlockKindHint`, and `Serialize` for `BlockId`. Block identity
  is minted here and is the document's from now on (D13.3).
- **Corrected:** the between-line vector is measured between line *boxes*, not centroids. A
  paragraph's short last line has its centroid far to the left of the line above it, and the
  centroid reading cut it into a block of its own on every paragraph of `f01` — best IoU
  0.038, which is exactly what the cross-check is for. Docstrum's neighbours are characters,
  and a centroid is a property of a line.
- **Three faults in the cover**, all found by dumping it: the forty-rectangle budget was being
  spent on rectangles that separate nothing, so the search now runs per column; a result is
  grown to maximality and kept only if it reaches both edges of its region; and two columns
  are separated by the page's column hypothesis rather than by a rectangle neither column's
  search could hold. Ten flags on `f02` fell to one, and the one that remains has a named
  cause in `docs/DECISIONS_LOG.md`.

### Columns and reading order (`oc-layout::columns`, `::reading_order`)

- New: `detect_columns` over the x-projection of run coverage, with the condition PIPELINE
  leaves implicit — **a valley is a gutter only if there is text on both sides of it** within
  the vertical span that qualified it. Without it the blank lower half of a short column is a
  252 pt gutter, which is what `f02` reported.
- New: `reading_order` — recursive XY-cut with pre-masking, the split direction from the
  region's own valleys, and masked elements put back before the first block they sit above
  and share a column with. No learned model, and the evidence says there should not be one:
  XY-Cut++ 0.988 BLEU-4 against LayoutReader's 0.788, and 100 % against 96.0 % on the
  Manhattan layouts a book is made of (R2 §B.2).
- **Decided:** columns are detected *before* segmentation, from run boxes rather than block
  boxes, because PIPELINE §6 step 2 projects glyph coverage and because a column hypothesis
  that can be withdrawn has to be able to withdraw the line splits it implied.

### Cross-page continuity (`oc-layout::continuity`)

- New: the check R10 §6.5 calls the highest-value deterministic signal in the pipeline. A
  wrong column count reorders a page and a reordered page stops flowing into the next, so the
  document is laid out, the break rate measured, and laid out again with one column fewer.
- **Decided:** the re-run has to read *better*. `f02` is a genuine two-column document whose
  single page boundary falls at the end of a sentence; the literal rule downgrades it on that
  one sample and interleaves the page test 3.2 exists to protect.

### Paragraphs (`oc-layout::paragraphs`)

- New: the book-level convention by mode, line grouping by leading, the short-last-line cue,
  and the merge across column and page boundaries. New in `oc-model`: `Para`,
  `ParagraphConvention`.
- New stage `paragraphs`, Budgeted over `Dehyphenate` and nothing else, and **I-5 is now
  checked in the ledger**: an entry under that reason is exactly one U+002D or U+2010 leaving,
  never an addition, never two characters.
- **Corrected:** `paragraph.line_unwrap_factor` 0.4 → 0.45. PIPELINE names 0.45 as the PDF
  path's value and 0.4 as the generic HTML one, and PIPELINE outranks the plan.

### Dehyphenation (`oc-text::dehyphen`, `::compound_de`)

- New: the four tiers in PIPELINE's order, with the in-document lexicon first among the
  evidence — a book about pipelines contains the word `pipeline`, and its own vocabulary costs
  nothing and beats any dictionary. Fail closed throughout: nothing decided means keep.
- New: the German rules. An upper-case continuation means the hyphen is real — German
  capitalises a noun at its first letter and nowhere else — and that keeps `Nord-Süd-Achse`
  whole without consulting anything. The Fugenlaut-aware compound acceptor takes an
  attestation predicate rather than a word list, because D15's German list does not exist yet
  and the document's own vocabulary does.
- New: the tiny classifier — 8,192 hashed character features, FNV-1a, 32 KB of float32,
  trained by `eval/src/oc_eval/train/hyphen_clf.py` from the same twelve CC0 Standard Ebooks
  the word list comes from, and committed with its training manifest and its holdout.
  **Holdout keep-recall 0.912, join-recall 0.930**, against R2 §B.7's 85.8 % for this kind of
  model and 31.7 % for the dictionary-only baseline it replaces.
- **Measured, and honest about it:** `f01`'s `pipeline` is *not* rejoined. The classifier reads
  `pipe-line` as a real compound, which it was in the nineteenth-century register the training
  corpus is written in. It fails in the safe direction — a visible hyphen rather than a
  corrupted word — and the fix is a corpus with a modern register, which is Phase 7's.

### Anchoring, drop caps, and the dump (`oc-layout::anchor`, `openconvert::dump_layout`)

- New: images anchored before the first block below them, and nothing dropped — an image on a
  page with no text at all still reaches the flow.
- New: drop-cap detection. A single oversized glyph *with text beside it*; a lone one is a
  display initial on a title page.
- New: `dump-stage layout`, which writes what the stage decided rather than what it measured —
  including the per-block disagreement and the column retries, which are what a wrong
  conversion is diagnosed from.

### Fixtures and supply chain

- **`f02_two_column` rewritten.** It had never had two columns: its page was tall enough to
  hold every line in the first one, so every "two column" assertion over it passed vacuously.
- New: `h22_false_gutter` (five pages with a valley that is not a gutter) and
  `h23_paragraph_across_pages` (a paragraph and a word broken at the same page break), and
  `f06_hyphenation_de`.
- **VD-b closed: `hyphenation` is banned, not depended on.** The crate ships the `hyph-utf8`
  pattern files with their licence headers stripped and disclaims them in its own README;
  upstream, Turkish is LPPL-1.0+ and `en-us` carries a bespoke non-SPDX notice, and the
  compiled dictionaries fold in GPL/LGPL/MPL extended data. None of that is visible to a
  licence scanner, so the ban is where it is enforced.

## Phase 4 — Structure

Ordered paragraphs become a document tree: headings and their levels, the section skeleton,
notes, figures and captions, lists, quotes and verse, tables, images and metadata.

### New crate surface

- New: `oc-model::doc` — the semantic layer of `IR_SKETCH`: `Span`, `SpanStyle`, `LinkTarget`,
  `Align`, `Heading`, `List`, `ListItem`, `Verse`, `Pre`, `Note`, `NoteKind`, `Figure`, `Cell`,
  `Table`, `PageBreak`, `Content`, `Section`, `SectionRole`, `FrontMatterKind`,
  `BackMatterKind`, `Zone`, `Metadata`, `MetaSource`, `Warning`, `Severity`.
  `Document` itself belongs to the `document` stage (PIPELINE §9) and is not here yet.
- New: `oc-model::ids` — `NoteId`, `FigureId`, `TableId`, `ClusterId`, `PageBreakId`.
- New IR fields on `layout::Para`: `spans`, `drop_cap`, `align`, `lang`, `confidence`. Optional
  additions, so `IR_VERSION` stays 1.
- New: `oc-model::extract::{VecId, VectorRegion}` and `PdfDoc::page_vectors`.
- New: `PdfDoc::xmp`, returning the packet's Dublin Core fields.
- New: `oc-pdf::images::perceptual_hash` — the 64-bit average hash the ornament rule compares.
- New: `oc-text::similarity::normalised_edit_distance`, moved out of `oc-layout::furniture`,
  which now has a second consumer.
- New: `oc-structure` — `view`, `build`, `headings::{cluster, candidate, numbering,
  outline/levels, toc_page, runin}`, `notes`, `figures`, `lists`, `quotes`, `tables`, `images`,
  `meta`, `book`, `stage`.
- New: `openconvert::{input, structure_input, dump_structure}` and
  `pipeline::{structure_stage, body_runs}`.

### New CLI surface

- New: `dump-stage structure` — a header carrying the stage check, the structural digest, the
  metadata, the note match rate and whether escalation is allowed, then one line per top-level
  section.

### New warning codes

`W_STYLE_INVENTORY_INVALID`, `W_NOTE_UNMATCHED`, `W_CAPTION_AMBIGUOUS`,
`W_LIST_NUMBERING_GAP`, `W_TABLE_AS_IMAGE`, `W_ORNAMENT_DROPPED`, `W_ZONES_OUT_OF_ORDER`,
`W_SECTION_PAGES_NOT_MONOTONE`.

### New `thresholds.toml` entries

`vector.{rule_max_thickness_pt, rule_min_aspect}`;
`headings.{size_quantum_pt, bold_weight_min, centered_tolerance_ratio, space_above_min_em,
cluster_char_share_min, outline_match_ned_max, runin_max_words}`;
`inventory.max_examples`; `toc.{min_entries, min_line_share, max_front_pages, min_leader_chars}`;
`layout.block.size_barrier_ratio`;
`footnote.{zone_band_min, rule_max_width_ratio, rule_gap_max_em}`;
`caption.{max_gap_em, max_width_ratio}`; `list.{min_siblings, max_depth,
indent_step_tolerance_pt}`; `table.{min_row_rules, min_column_rules, grid_snap_pt,
rule_overlap_min}`; `verse.{short_line_fill_max, min_lines}`;
`quote.{indent_min_em, centered_max_lines}`; `images.ornament_min_pages`.

### New fixtures

Typst `f07_verse_and_quote`, `f08_footnotes`, `f09_novel_structure`, `f10_lists_and_table`
(the plan's `f06`–`f08`, shifted because Phases 2 and 3 spent those numbers; `f07` is the
verse fixture Phase 3 deferred). Hand-made `h24_footnote_symbol_cycle`,
`h25_two_figures_one_caption`, `h26_borderless_table`, `h27_repeated_ornament`,
`h28_xmp_over_boilerplate`, `h29_drop_cap`. The hand-made builder gained filled rules, placed
images, an Info dictionary and an XMP packet.

### Fixed in earlier phases

- `oc-pdf::outline::read_outline` expanded every `/Next` chain at every node, so a chain of
  five came back with thirty-two entries. The outline is heading ground truth, so every
  duplicate would have become a heading.
- Docstrum merged a heading into the paragraph beneath it whenever a producer set the two one
  leading apart, which Typst and most book designers do. `layout` now splits a block at a
  material size change, and `paragraphs` closes an open paragraph at one.
- A superscript set with an OpenType `sups` glyph — drawn on the baseline at the body size —
  was read as ordinary text, so footnote markers were swallowed into the middle of body runs.
