# OpenConvert — Architecture, v1

**Status:** Final for v1 build-out. Derived from `docs/DECISIONS.md` (ADR v2) and `docs/IR_SKETCH.md`; adopts sections C1, C2 and C4 of `research/round2/RED_TEAM_REVIEW.md` verbatim in spirit.
**Date:** 2026-09-09
**Scope:** what the system is made of, who owns which process, which invariants hold, and where the seams are. Stage-by-stage algorithms live in `docs/PIPELINE.md`. Phase ordering lives in `docs/IMPLEMENTATION_PLAN.md`.
**Convention:** every numeric constant in this document is a `thresholds.toml` entry. Numbers marked `(provisional)` have `source = provisional` and are unanchored guesses awaiting calibration (D17). Research citations use the form `R2 §B.4`, `RT C1`, `D13.4`.

---

## 1. Executive overview

OpenConvert converts a PDF into a reflowable EPUB 3.3 on the user's own machine, with no network access on the conversion path. It is a Rust workspace (`crates/oc-*`) compiled into one library (`oc-core`), one CLI binary (`openconvert`), and one Tauri 2 desktop shell (`apps/desktop`) that drives the CLI as a supervised subprocess. PDF access is PDFium behind an internal `PdfBackend` trait; EPUB emission is hand-rolled through a typed XHTML builder; optional local inference is `llama-server` over an OpenAI-compatible HTTP wire; optional OCR is the user's Tesseract.

The product is not the extractor. Every tool in this space has an extractor, and every tool in this space silently loses content: Marker drops ~14 % of images with no log line, deletes body text on non-zero CropBox origins, and has fabricated 58 words on a clean page; Docling drops text inside figures; MinerU has open issues for missing block text (R1 §C.3). **Not one of them checks its own output against its own input.** OpenConvert's differentiator is that it does — through a ledger-balanced conservation law enforced after every stage (§5), a confidence model that abstains rather than guesses (§6), and a validate→repair loop whose target repair rate is zero (§7).

The conversion is deterministic by default. `ai.enabled = false` ships as the v1 default (D17). When the user opts in, a small local LLM is consulted **four times per book** — never per page — over compressed inventories with geometry pre-digested into categorical words, because a 7B model given raw coordinates as text scores 34.3 % on layout-dependent extraction versus 78.1 % with learned projections, at 6.2× the tokens (LayTextLLM, R10 §4.1). The LLM never emits prose, never deletes text, and never stands unvalidated: it proposes labels; deterministic code decides what that means and whether to keep it.

### The guiding principle

> **Deterministic first. AI where necessary. Validate everything. Repair only what is broken.**

Four operational readings of that sentence, each of which decides a real design question later in this document:

1. *Deterministic first* means the deterministic path must produce a valid EPUB alone, on every input, forever. The AI path is an opt-in improvement with a measured effect (McNemar, R9 §C.7), never a dependency. A missing or corrupt sidecar degrades to a banner and a report note, not a failure (D13.11).
2. *AI where necessary* means the escalation predicate is part of the architecture, not a tuning knob. If deterministic evidence is sufficient, the model is not consulted at all — Gate D removes most false-repair opportunities before they exist, because false repairs concentrate on cases that were already correct (R10 §4.3).
3. *Validate everything* means invariants over data, not assertions in prose. The conservation law (§5) is checked after every stage in every build; the end-to-end form I-7 is a release gate.
4. *Repair only what is broken* means the repair loop is instrumented as a bug detector for our own emitter. Every repair that fires is an emitter defect, and the corpus-wide repair-fire rate is a release metric with target zero (D13.7, RT A10).

---

## 2. System context and components

Three processes at most, plus files. The engine is the only process that reads the PDF; `oc-net` is the only crate that opens a socket; the conversion path never touches the network.

```
                       ╔═══════════════ USER'S MACHINE ═════════════════════════════╗
                       ║                                                            ║
  ┌────────────────────╨──────────────────┐                                         ║
  │  apps/desktop  —  Tauri 2 shell       │   PRIVACY BOUNDARY                      ║
  │  ┌──────────────┐  ┌────────────────┐ │   ───────────────                       ║
  │  │ TS/Svelte UI │  │ thin Rust side │ │   Only two components may open a        ║
  │  │ CSP:         │  │  · job-spec    │ │   socket, both above the line and       ║
  │  │ connect-src  │  │    writer      │ │   both outside the conversion path:     ║
  │  │   'none'     │  │  · supervisor  │ │                                         ║
  │  │ no http perm │  │  · oc-net      │ │    (a) oc-net in the desktop app        ║
  │  └──────┬───────┘  └───────┬────────┘ │        — model / pack downloads,        ║
  └─────────┼──────────────────┼──────────┘          pinned host allowlist,         ║
            │ IPC              │ owns                SHA-256 verified               ║
            │                  │                                                    ║
            │            ┌─────▼────────────────┐  (b) oc-net as the Transport impl ║
            │            │  llama-server        │      behind LlmProvider, and only ║
            │            │  externalBin,        │      to a loopback endpoint unless║
            │            │  127.0.0.1:<ephem>   │      the user has toggled explicit║
            │            │  --api-key <csprng>  │      non-loopback consent (D10)   ║
            │            │  -np 1, idle-kill    │                                   ║
            │            └─────▲────────────────┘                                   ║
            │                  │ HTTP /v1/chat/completions                          ║
  ┌─────────▼──────────────────┴─────────────────────────────────┐                  ║
  │  openconvert  (CLI = the engine; one argument: job-spec path) │                  ║
  │                                                              │                  ║
  │   stdin  ← control  NDJSON  {cancel} {ping}                  │                  ║
  │   stderr → events   NDJSON  hello/job/stage/progress/warning │                  ║
  │                              /llm/heartbeat/done/fatal       │                  ║
  │   stdout → data only (--dump-stage -), empty in GUI mode     │                  ║
  │                                                              │                  ║
  │   oc-core orchestrates: oc-pdf → oc-text → oc-layout →       │                  ║
  │   oc-structure → oc-epub → oc-validate,  with oc-ai gated    │                  ║
  │   ── NO HTTP CLIENT LINKED INTO ANY OF THESE ──              │                  ║
  └───┬──────────────┬───────────────┬───────────────┬───────────┘                  ║
      │ libloading   │ spawn         │ read/write    │                              ║
      │              │               │               │                              ║
 ┌────▼───────┐ ┌────▼──────────┐ ┌──▼───────────────▼────────────────────────────┐ ║
 │ libpdfium  │ │ tesseract     │ │ FILES                                         │ ║
 │ vendored,  │ │ system-       │ │  in.pdf (read-only)                           │ ║
 │ SHA-pinned,│ │ installed,    │ │  <out>.oc-tmp-<rand> → atomic rename → .epub  │ ║
 │ Dev-ID     │ │ optional;     │ │  report.json                                  │ ║
 │ signed on  │ │ TSV word      │ │  cache/llm/<hash>.json      (content-addressed)│ ║
 │ macOS      │ │ boxes + conf  │ │  models/*.gguf + LICENSE + NOTICE             │ ║
 └────────────┘ └───────────────┘ │  packs/{ocr,validation}/                      │ ║
                                  │  overrides.json (user corrections)            │ ║
                                  │  thresholds.toml, models.toml (shipped)       │ ║
                                  └───────────────────────────────────────────────┘ ║
                       ╚════════════════════════════════════════════════════════════╝
                                  no telemetry · no crash reporting · no remote registry
```

**Privacy boundary, stated as enforceable rules (D13.9).**

| Rule | Enforcement |
|---|---|
| Core crates have no HTTP client dependency | `cargo-deny` `bans.deny` with `wrappers = ["oc-net"]` (§3); CI runs `cargo tree -e no-dev -i <net crate>` and fails if any path avoids `oc-net` |
| The conversion path never opens a socket | The whole conversion suite runs under `unshare -n` in CI |
| The webview cannot reach the network | Tauri capabilities grant no `http` permission; CSP `connect-src 'none'` |
| Model/pack downloads never happen during a conversion | Downloads live in `oc-net`, driven by the app's model manager or `openconvert model pull`; the engine takes `--model-path` / `--llm-endpoint` and has no download code |
| A non-loopback LLM endpoint is a consented act | Explicit toggle naming the host, stating that document text leaves the machine; `Transport::endpoint()` returns `EndpointKind::Remote{host}` and the conversion report records it (D10) |
| Bug reports are user-driven | "Report a problem" exports a diagnostic bundle the user reviews and sends themselves. No telemetry, no crash reporting |

**Sidecar ownership (RT C2, D8).** The engine always speaks HTTP to an endpoint. Given `--llm-endpoint URL --llm-api-key-file PATH` it uses that and spawns nothing. Absent those flags it spawns and owns a `llama-server` on `127.0.0.1:<ephemeral>` with a per-run CSPRNG `--api-key`, `-np 1`, idle-kill at 120 s, torn down on exit. The desktop app always supplies a long-lived, app-owned server, so a 1.28 GB model loads once per batch rather than once per book. One code path; no reload-per-book; no orphaned grandchild.

---

## 3. Crate map, dependency rules, public traits

### 3.1 Crate map

Reproduced from DECISIONS Appendix A, with the dependency *rule* made explicit alongside each edge.

| Crate | Responsibility | May depend on | Must not depend on |
|---|---|---|---|
| `oc-model` | IR types, `BlockId` derivation, ledger, canonical JSON, `ir_version`, overrides schema | — (serde, blake3, base32 only) | anything in the workspace |
| `oc-pdf` | `PdfBackend` trait; PDFium impl (chars, images, paths, render, bookmarks); `lopdf` object access (XMP, StructTree hints, `/Producer`); page classification; encryption | `oc-model` | `oc-text`, `oc-layout`, `oc-ai`, `oc-net` |
| `oc-text` | Normalization `N`, Turkish-aware folding keys, word/line assembly, dehyphenation (4 tiers + tiny classifier), ligatures, Gopher-style statistics, `whatlang`, word-frequency lists | `oc-model` | `oc-pdf`, `oc-ai`, `oc-net` |
| `oc-layout` | Furniture detection, block segmentation, column detection + XY-cut with pre-masking, paragraph reconstruction, image anchoring, drop caps | `oc-model`, `oc-text` | `oc-pdf`, `oc-ai`, `oc-net` |
| `oc-structure` | Heading clustering + outline/TOC matching, book structure, lists, footnotes/endnotes, captions/figures, quotes/verse, tables, metadata | `oc-model`, `oc-text`, `oc-layout` | `oc-pdf`, `oc-ai`, `oc-net` |
| `oc-epub` | Typed XHTML builder, OPF/nav/NCX/`page-list`, CSS, splitting, deterministic zip, image encoding | `oc-model` | everything else |
| `oc-validate` | Tier-1 validator, structural validator, EPUBCheck/Ace runners, message→repair table | `oc-model`, `oc-epub` | `oc-ai`, `oc-net` |
| `oc-ai` | `LlmProvider` trait, OpenAI-compatible client over a `Transport`, versioned prompts + GBNF grammars, cache, four gates, cassettes | `oc-model` | **any network crate** |
| `oc-net` | The only crate allowed to open sockets: model/pack downloader (host allowlist, SHA-256), `Transport` impl | `oc-ai` | `oc-core` |
| `oc-core` | Pipeline orchestrator: stages, ledger checks, escalation, repair loop, report, progress/cancel, sidecar supervision | all above | — |
| `openconvert` (bin) | CLI: `convert`, `inspect`, `dump-stage`, `validate`, `model pull\|list\|remove`, `bench`; NDJSON events; job-spec | `oc-core`, `oc-net` | — |
| `apps/desktop` | Tauri 2 app: spawns the CLI, owns `llama-server`, model manager, preview, report | `oc-core` (types only), `oc-net` | `oc-pdf` |

Three structural rules fall out and are worth stating because they are the ones a refactor will try to break:

- **`oc-pdf` and `oc-ai` never meet.** Neither depends on the other, and neither is a transitive dependency of the other. The LLM cannot reach a PDF; the PDF backend cannot reach a model.
- **`oc-ai` has no network crate.** It takes a `Transport` (§3.2) implemented by `oc-net` or by a test double. This is what makes every LLM call mockable in `cargo test` and what keeps the socket ban a compile-time property rather than a code-review convention.
- **`oc-epub` depends only on `oc-model`.** The emitter cannot consult layout heuristics, so a rendering decision cannot silently leak into the byte stream.

### 3.2 `cargo-deny` bans that enforce the rules

`deny.toml` at the workspace root. The `wrappers` mechanism is what turns "only `oc-net` opens sockets" from prose into a build failure.

```toml
[licenses]
allow = ["MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception", "BSD-2-Clause",
         "BSD-3-Clause", "ISC", "MPL-2.0", "Unicode-3.0", "Unicode-DFS-2016", "Zlib", "CC0-1.0"]
confidence-threshold = 0.93
# AGPL/GPL/LGPL are absent from the allow-list and therefore denied in shipped crates (D15).
# Rejected by name, with reasons, so a future contributor sees why:
#   mupdf-*, pymupdf  → AGPL-3.0 (forces AGPL on OpenConvert and every fork)
#   poppler-*         → GPL (external CI oracle only, never linked)
#   epubveri          → AGPL-3.0 (watch, do not adopt — D6)

[bans]
multiple-versions = "warn"
wildcards = "deny"

# --- the privacy ban: sockets only in oc-net -------------------------------
[[bans.deny]]
name = "reqwest"
wrappers = ["oc-net"]
[[bans.deny]]
name = "ureq"
wrappers = ["oc-net"]
[[bans.deny]]
name = "hyper"
wrappers = ["oc-net"]
[[bans.deny]]
name = "socket2"
wrappers = ["oc-net"]
[[bans.deny]]
name = "curl-sys"
wrappers = ["oc-net"]

# --- no async runtime in the pipeline core (D1) ----------------------------
[[bans.deny]]
name = "tokio"
wrappers = ["oc-net", "tauri"]
[[bans.deny]]
name = "async-std"

# --- TLS policy: rustls only, no system OpenSSL ----------------------------
[[bans.deny]]
name = "openssl-sys"
[[bans.deny]]
name = "native-tls"

# --- data-size traps flagged in V2 -----------------------------------------
[[bans.deny]]
name = "lingua"              # default build pulls ~300 MB of language models (RT B11);
                             # whatlang is the chosen detector (D13.11)

[sources]
unknown-registry = "deny"
unknown-git = "deny"

[advisories]
yanked = "deny"
```

Two further CI gates carry the rules that `cargo-deny` cannot express: `cargo tree -e no-dev -i reqwest` (and each banned crate) must show `oc-net` on every path, and the whole conversion test suite runs under `unshare -n` so that any accidental socket is a test failure rather than a privacy incident. `#![forbid(unsafe_code)]` is set in every crate; the single exemption is the thin PDFium wrapper module in `oc-pdf`, which carries `#![allow(unsafe_code)]` with a module-level comment naming the FFI contract.

### 3.3 Public API sketch

No lifetimes in public signatures; the document model owns its data and uses arena indices (D1). No `async` anywhere in the pipeline core.

```rust
// ─── oc-pdf ────────────────────────────────────────────────────────────────
pub trait PdfBackend: Send + Sync {
    fn backend_id(&self) -> BackendId;                     // ("pdfium", "<abi version>") → report
    fn doc_info(&self) -> Result<PdfDocInfo>;              // producer, outline, XMP, struct-tree flag
    fn page_count(&self) -> u32;
    fn page_info(&self, page: u32) -> Result<PageInfo>;    // boxes, /Rotate, class, class_conf
    fn glyphs(&self, page: u32) -> Result<Vec<Glyph>>;     // already in normalized page space
    fn images(&self, page: u32) -> Result<Vec<ImageRef>>;
    fn image_bytes(&self, id: ImageId) -> Result<DecodedImage>;   // SMask composited to RGBA
    fn vectors(&self, page: u32) -> Result<Vec<VectorRegion>>;
    fn render(&self, page: u32, dpi: f32, clip: Option<Rect>) -> Result<Rgba8>;
    fn outline(&self) -> Result<Vec<OutlineEntry>>;
}
pub fn open_pdfium(lib: &Path, doc: &Path, password: Option<&str>) -> Result<Box<dyn PdfBackend>>;

// ─── oc-core ───────────────────────────────────────────────────────────────
pub trait Stage: Send {
    fn name(&self) -> StageName;                  // inspect | ingest | text | … | report
    fn kind(&self) -> StageKind;                  // Conserving | Budgeted        (§5, I-3)
    fn reasons(&self) -> &'static [Reason];       // closed set this stage may cite (§5, I-2)
    fn run(&mut self, doc: &mut Doc, ctx: &Ctx) -> Result<StageOutcome>;
}

pub struct StageOutcome {
    pub ledger:    StageLedger,        // Removed / Added spans, this stage only
    pub decisions: Vec<Decision>,      // what was chosen, over what alternatives, by which method
    pub warnings:  Vec<Warning>,       // code + args; the GUI localizes
    pub checks:    StageCheck,         // I-1..I-6 results, recorded in Ledger::per_stage_checks
    pub elapsed:   Duration,
}

pub struct Ctx {
    pub cfg:        Config,               // resolved: CLI > job-spec > user > preset > defaults
    pub thresholds: Thresholds,           // parsed thresholds.toml, provenance retained
    pub cancel:     CancelToken,
    pub progress:   Arc<dyn ProgressSink>,
    pub backend:    Arc<dyn PdfBackend>,
    pub ai:         Option<Arc<dyn LlmProvider>>,   // None ⇒ Gate D can never open
    pub budgets:    Budgets,              // LLM calls, blocks, wall-clock share; interior mutability
    pub workdir:    PathBuf,              // temp files; deleted on cancel and on failure
}

// ─── oc-ai ─────────────────────────────────────────────────────────────────
pub trait LlmProvider: Send + Sync {
    fn info(&self) -> ProviderInfo;       // model_id, ctx window, grammar support, endpoint kind
    fn complete(&self, req: &LlmRequest) -> Result<LlmResponse, LlmError>;
}
pub struct LlmRequest {
    pub task: TaskId,                     // Metadata | HeadingRoles | BookStructure | VerseQuote
    pub prompt_version: PromptVersion,    // bumping this invalidates cache and cassettes
    pub system_prefix: Arc<str>,          // ONE byte-identical prefix for all tasks (D8)
    pub user: String,                     // the only part that varies
    pub grammar: Grammar,                 // GBNF source + its hash; also rendered as json_schema
    pub max_output_tokens: u32,
    pub deadline: Duration,
}
pub struct LlmResponse { pub text: String, pub cached: bool,
                         pub tokens_in: u32, pub tokens_out: u32, pub ms: u32, pub model_id: String }

/// Implemented by oc-net (real) and by cassette/stub doubles (tests).
/// This trait is why oc-ai links no network crate.
pub trait Transport: Send + Sync {
    fn endpoint(&self) -> EndpointKind;   // Loopback | Remote { host: String }  → consent + report
    fn post_json(&self, path: &str, body: &[u8], deadline: Duration)
        -> Result<Vec<u8>, TransportError>;
    fn get(&self, path: &str, deadline: Duration) -> Result<Vec<u8>, TransportError>;
}

// ─── progress & cancellation (sync, D13.11) ────────────────────────────────
pub trait ProgressSink: Send + Sync {
    fn stage_begin(&self, s: StageName);
    fn stage_end(&self, s: StageName, elapsed: Duration);
    fn progress(&self, s: StageName, done: u64, total: u64, unit: Unit);   // coalesced ≤10/s
    fn warning(&self, w: &Warning);
    fn llm(&self, t: &LlmTrace);
}

#[derive(Clone)]
pub struct CancelToken(Arc<AtomicBool>);
impl CancelToken {
    pub fn cancel(&self);
    pub fn is_cancelled(&self) -> bool;
    /// Called at every stage boundary and inside every per-page loop (RT C2).
    pub fn check(&self) -> Result<(), Cancelled>;
}
```

`Doc` is the mutable pipeline state: `PdfDocInfo` + `Vec<PageInfo>` + arenas of `Glyph`/`Run`/`Line`/`Block`, then `Option<Document>` once the semantic layer materializes, then `Option<EpubDraft>`. Stages narrow it monotonically; nothing is discarded until `report` has run, because the report cites block ids from every layer.

---

## 4. The intermediate representation

`docs/IR_SKETCH.md` is authoritative for the types; this section states the rules that govern them.

### 4.1 Why a typed tree and not Markdown

Markdown cannot express footnote references, `<figure>/<figcaption>`, blockquote nesting, verse, small caps, drop caps, `epub:type` semantics, or `page-list` — roughly half of the failure taxonomy (R1 §D.4). Adopting a Markdown intermediate representation would cap output quality at "slightly better Marker". The IR is a document tree with roles, per-block provenance (page, bbox, source run ids), per-decision `Confidence`, a span-based ledger, and a decisions log (D13.3).

### 4.2 Three layers, one geometry

The IR materializes in three layers: the **extraction layer** (`PdfDocInfo`, `PageInfo`, `Glyph`, `FontInfo`, `ImageRef`, `VectorRegion`), the **text/layout layer** (`Run`, `Line`, `Block`), and the **semantic layer** (`Document` and everything under it). All of them share **one normalized geometry space, fixed at extraction**: origin top-left, y down, PDF points, after `/Rotate` and CropBox offset. This is an asserted invariant, not a convention, because mixing CropBox and image space silently deleted body text in a shipping 2026 tool (R1 §D.6 #1, D13.3). Every `Rect` entering the IR passes through one conversion function; a debug assertion checks that no rect lies outside the CropBox by more than a tolerance.

### 4.3 Canonical JSON

Keys sorted; `ir_version` first; arrays in document order; strings NFC; no NaN/Inf (a serialization error, not a silent `null`); `f32` geometry printed with **2 decimals at serialization only** — in memory geometry stays at full `f32` precision, so the rounding is a snapshot concern and never a stored value (RT B5). The JSON text itself is NFC and UTF-8 without BOM.

### 4.4 Block id derivation

```
BlockId = base32( blake3( page_index ‖ bbox rounded to 1 pt ‖ first 64 NFC chars ) )[..10]
          + collision suffix on the (rare) duplicate
```

Block ids are a **public interface** the moment `overrides.json` references one. They are also the LLM cache key component and the anchor set for report and UI deep links. The rule follows: **changing the derivation is a breaking change that bumps `ir_version`, invalidates every cache entry, and invalidates every stored override** (RT D2). Rounding bbox to 1 pt (not 0.01) is deliberate: it makes ids stable against sub-point jitter from a re-render or a PDFium version bump, at the cost of a slightly higher collision rate that the suffix absorbs.

### 4.5 Versioning and migration policy

`ir_version` is a single monotonic integer, currently `1`, present as the first key of every serialized IR and of `overrides.json`.

| Change | Version effect | Migration |
|---|---|---|
| Adding an optional field | none | old readers ignore it; `serde` default |
| Adding a `Content` / `Reason` / `WarningCode` variant | none for readers that treat unknown variants as errors — so they must not; unknown variants are a hard error in the engine and a warning in tooling | n/a |
| Changing `BlockId` derivation | **bump** | caches and overrides invalidated; the engine refuses stale `overrides.json` with a named warning |
| Removing or retyping a field | **bump** | a `migrate_v{n}_to_v{n+1}` function in `oc-model`, unit-tested against a committed fixture of the old version |
| Changing normalization `N` or the geometry space | **bump** | full re-conversion; conservation baselines are not comparable across the bump |

The engine hard-errors on an `ir_version` it does not know, on both read and IPC handshake (`hello{ir_version}`, §8). It never silently coerces.

### 4.6 Structural digest

Full canonical-JSON snapshots of real books are tens of megabytes and unreviewable in a pull request (RT B4). `insta` snapshots therefore cover only tiny hand-made fixtures. Corpus files are snapshotted as a **structural digest**: counts per `Content` variant; the heading tree as a list of `(level, text[..40])`; ledger totals per `Reason`; the per-page class histogram; and the first and last 200 characters of every section. A digest diff is a reviewable artifact — it says "three headings became paragraphs and 1,200 characters left the ledger under `RunningHeader`" rather than showing 40,000 changed lines.

### 4.7 Overrides

`overrides.json` carries `ir_version` and `source_sha256` and is applied after `structure` and before `document`. v1's UI writes only `metadata` and `toc` patches; the `blocks` array (role, level, `merge_with_next`) exists in the schema from day one so that the post-v1 block-level correction UI is a UI change and not an IR change (D16). Any applied override is recorded as a `Decision` with `method = User` and generates a ledger entry with `reason = UserOverride` when it removes text — the one place a human may overrule the conservation budgets.

---

## 5. The conservation law and the ledger

This section adopts RT C1. It replaces the earlier "LLM edits conserve the non-whitespace character multiset" formulation, which was both mis-scoped (it guarded the most constrained operations in the system while the deletions all happen on the deterministic path) and false as written (ligature expansion turns one scalar into two; furniture removal deletes by design; OCR creates characters from nothing).

### 5.1 Normalization `N`, applied exactly once

```
N = strip(U+00AD) ∘ expand_ligatures(U+FB00..U+FB06 → ASCII sequences) ∘ NFC
```

- **NFKC is forbidden anywhere in the pipeline.** It maps `¹` → `1` and destroys the superscript footnote signal in the same pass (R2 §B.8).
- **Superscript/subscript status is captured from geometry before `N`** and stored as a `Run` attribute. `N` never alters it.
- **Text is never case-folded.** Turkish-locale-aware folding builds comparison keys for lookup only, never emitted text (R1 §A.11 #10).
- PDFium does not expand ligatures, so U+FB00–FB06 arrive in the glyph stream and must be mapped explicitly (R2 §B.8).

### 5.2 Two baselines

```
C_raw = multiset of non-whitespace Unicode scalars immediately after extraction
C_0   = same, after N + overdraw-dedup + OCR-layer-dedup      ← the retention denominator
```

`C(D)` is the multiset of Unicode scalars in all **content-document text** of state `D` **after canonical decomposition (NFD)**, excluding scalars with the Unicode `White_Space` property. The canonical form is part of the definition and not a transform applied to it: two canonically equivalent encodings are the same text, so `C` must not distinguish them. It is *de*composition rather than composition because composition depends on adjacency and a multiset does not — so `C` is the same however a stage happens to cut its text. See the D13.4 amendment of 2026-09-20. Text that reaches the output as an *attribute value* — `alt`, `title`, `page-list` labels — and all nav/OPF metadata text is **outside `C`**. This is what makes "remove the page number from the flow but keep it as a `page-list` label" a clean `Removed{PageNumber}` rather than a paradox, and it is why generated spaces (whitespace by construction) never appear in the ledger at all.

Dedup is folded into `C_0` rather than recorded against `C_raw` so that deleting a duplicate copy of a page does not inflate the retention ratio.

### 5.3 Stage taxonomy and the ledger

Every stage declares statically: `kind(s) ∈ {Conserving, Budgeted}`, `reasons(s) ⊆ Reason` (a closed enum), and `budget(s): Reason → f32` as a fraction of `|C_0|`. The declarations live in the `Stage` trait (§3.3) and are checked against what the stage actually did.

```rust
pub enum Reason { SoftHyphen, LigatureExpand, GeneratedSpace, RunningHeader, RunningFooter,
                  PageNumber, OverdrawDedup, OcrLayerDuplicate, Dehyphenate, Ocr,
                  DecorativeGlyph, Watermark, ClippedOffPage, HiddenText, UserOverride }
//                                            ^^^^^^^^^^^^^^  ^^^^^^^^^^
// ClippedOffPage = geometrically absent (outside CropBox, or removed by a clipping path).
// HiddenText     = rendered but not visible: render mode 3 on a page that is NOT an OCR
//                  sandwich, or a fill colour within the delta-E tolerance of the local
//                  background (white-on-white).  Added per ratified note N-3.

pub struct LedgerEntry { stage: StageName, reason: Reason, block: Option<BlockId>,
                         page: PageRef, span: (u32, u32), text: String, added: bool }
```

Entries are **spans, not characters**. Read literally as "every character removed is recorded", the ledger would be quadratic nonsense; as spans it is a few thousand entries per book.

### 5.4 Invariants I-1 … I-7

Checked after **every** stage. Debug and CI builds check all of them fully; release builds check the count form (histogram equality) and defer span-level reconstruction to `--verify` (D13.4).

| # | Invariant | Statement |
|---|---|---|
| **I-1** | Conservation | `C(D_i) ⊎ chars(Added_s) == C(D_{i+1}) ⊎ chars(Removed_s)` |
| **I-2** | Declared reasons | `∀ e ∈ Removed_s ∪ Added_s : e.reason ∈ reasons(s)` |
| **I-3** | Conserving stages | `kind(s) = Conserving ⇒ Removed_s = Added_s = ∅`, hence plain multiset equality |
| **I-4** | Budgets | per reason, cumulative chars ≤ `budget(s)(r) · |C_0|`; plus a global cap on non-OCR removal |
| **I-5** | Dehyphenation | `reason = Dehyphenate` removes exactly one scalar ∈ `{U+002D, U+2010}`, adds nothing, and the resulting token differs from the concatenation of the two source tokens by exactly that character |
| **I-6** | OCR | `reason = Ocr` is Added-only, permitted only in a **page region whose bbox contains no PDF text runs** before the stage — the whole page on an `image-only` page, each uncovered image region on a `mixed` page; every such region is marked `provenance = ocr` and excluded from the source-retention metric *(region scope per ratified note N-1)* |
| **I-7** | End-to-end | `C(EPUB) ⊎ chars(all Removed) == C_0 ⊎ chars(all Added)` — **a release gate** |

**I-3 is the load-bearing one.** The following are all `Conserving`, meaning plain multiset equality holds across them and any deviation is a bug, not a policy: reading order, block typing, heading detection and levels, chapter and structure roles, verse/quote classification, list nesting, footnote linking, image anchoring, chapter splitting, XHTML serialization, **and every LLM edit without exception**. Drop caps are `Conserving` too, which is precisely how the invariant catches the classic drop-cap bug — the character emitted twice shows up as an unexplained `Added`. Small caps are a CSS class and never a case fold, therefore `Conserving`.

### 5.5 Budgets (all `source = provisional`)

| Reason(s) | Budget as fraction of `|C_0|` |
|---|---|
| `RunningHeader` + `RunningFooter` + `PageNumber` | ≤ 0.04 *(provisional)* |
| `OverdrawDedup` | ≤ 0.02 *(provisional)* |
| `OcrLayerDuplicate` | ≤ 0.60 **per page** *(provisional)* |
| `Dehyphenate` | ≤ 0.005 *(provisional)* |
| `DecorativeGlyph` | ≤ 0.002 *(provisional)* |
| every other reason | ≤ 0.001 *(provisional)* |
| global non-OCR removal | ≤ 0.08 *(provisional)* |

A budget breach is not a crash. It stops the offending stage's remaining removals, emits a `Warning` with the reason and the measured fraction, records the breach in the report, and lets the conversion finish — a book with its running heads left in is far better than no book.

### 5.6 Cost

`C(D)` is a histogram over roughly a megabyte of text: a `[u32; 128]` ASCII fast path plus a hashmap tail, sub-millisecond. Twelve stages per book makes the whole conservation apparatus a rounding error against the 0.5 s/page budget *(provisional)*. The ledger is spans plus counts; it is not per-character provenance.

### 5.7 What the conservation law does **not** protect

Say this out loud in the report UI, because a green ledger is not "the text is right":

- **A wrong dehyphenation join conserves the multiset exactly.** At the dictionary-only keep-hyphen recall of 31.7 % measured on 776,700 hyphenated words (R2 §B.7), a 300-page novel yields hundreds of silently corrupted words while I-1 through I-7 all report green. This is why the tiny classifier (85.8 % keep-hyphen recall at no accuracy cost) and the fail-closed keep-hyphen rule exist as *separate* safeguards.
- **A wrong heading level, a wrong cluster label, a wrong reading order, a wrong caption association.** All conserve characters perfectly. Their checks are the stage-specific ones in `docs/PIPELINE.md` and the structural validators in §7.
- **A wrong table grid** with the right cell text.

---

## 6. Confidence and escalation

### 6.1 v1 uses structural predicates, not calibrated scores

No calibration exists, no gold set exists, and no published work calibrates these signals for PDF→EPUB — R10 §4.4 calls that "the largest single risk to this architecture". The v1 answer (RT C4, D17) is to *not need* calibration: every escalation trigger is expressed as a **structural predicate** that requires no threshold fitting, is unit-testable on day one, and generates the calibration data as a side effect. Every escalation logs a labelled hard case with its signals; the first 200 books converted are the calibration corpus.

`Confidence { method, score: Option<f32>, signals, escalated, fallback_used }` therefore ships with `score = None` in v1. The `signals` vector is still populated — that is the data the calibration will later consume.

| Decision | v1 escalation predicate (no threshold) | Deterministic fallback |
|---|---|---|
| Metadata | XMP/DocInfo `dc:title` absent **or** matches the boilerplate list (`^Microsoft Word - `, `\.(docx?\|indd\|pages)$`, `^untitled`, empty) | filename parse + largest-font block on pp. 1–3 |
| Book structure | PDF outline absent **and** printed-TOC parse yielded < 3 entries | heading size-rank + keyword rules (EN/DE/TR) |
| Heading roles | > 1 candidate style cluster **and** numbering-regex coverage < 100 % | size-rank ordering |
| Verse / quote | indent present **and** short-line ratio in `[0.35, 0.75]` **and** block budget remains | `blockquote` if indented, else `paragraph` |
| Dehyphenation | joined form absent from the in-document dictionary **and** absent from the lexicon **and** not both halves independently attested | **keep the hyphen** (fail-closed; no LLM in v2) |
| Run-in headings | bold/italic run at paragraph start, ≤ 8 words, terminated by `.` / `—` / `:` | not a heading (candidates ride along in the heading-roles call) |

### 6.2 The four gates

An LLM edit is applied only when the full conjunction holds:

```
escalation_predicate(subject)  ∧  cfg.ai.enabled  ∧  budget_remains
        ↓ call
    Gate S  ∧  Gate L  ∧  Gate V     (Gate D was the predicate, evaluated before the call)
        ↓ all pass                    ↓ any fails
    apply the edit                  revert to the deterministic answer,
                                    record fallback_used = true, emit a warning
```

**Gate D — deterministic uncertainty.** The predicate above. If deterministic evidence is sufficient, the model is never consulted. This alone removes most false-repair opportunities, because false repairs overwhelmingly land on cases that were already right (R10 §4.3).

**Gate S — schema and self-consistency.** The output parses against the grammar; ids are bijective with the input set (none invented, none dropped); every enum value is legal; arity matches. Any failure rejects the *whole* response, never a subset.

**Gate L — locality and conservation.** The edit must be `Conserving` (I-3): labels, levels, roles and CSS classes only. This is a type-level property — the `apply` function for each task takes a label map, not text — plus the I-1/I-3 check that runs after the stage regardless.

**Gate V — post-edit statistics do not worsen.** Defined as a **fixed ordered tuple of region-valid statistics** with an explicit epsilon, because most Gopher statistics are document-level and undefined on a block (`min_doc_words 50` fails on any block), and comparing a 20-dimensional vector without a combination rule means Gate V either never fires or always fires (RT B13).

```
V(region) = ( dup_line_ratio,            ε = 0.01
              top_2gram_ratio,           ε = 0.01
              top_3gram_ratio,           ε = 0.01
              non_alpha_word_ratio,      ε = 0.01
              heading_tree_violations )  ε = 0      (integer: level skips + non-monotonicity)

Rule: for every component i, V_after[i] ≤ V_before[i] + ε_i.  Otherwise revert.
Statistics undefined on the region (too few words) are skipped, not defaulted.
```

The region is the set of blocks the edit touches — for `verse_quote` that is ≤ 30 blocks; for `heading_roles` and `book_structure` the region is the whole content, where `heading_tree_violations` is the load-bearing component. The Gopher thresholds themselves come from a production implementation (`datatrove`, R10 §6.18) and are used here as *relative* comparisons, not absolute filters.

**Label authority ≠ deletion authority (D13.5, RT A8).** The heading-roles enum includes `running_head`. The LLM may *propose* it; only the deterministic furniture remover deletes, and only when its own cross-page repetition evidence independently agrees. Cross-page repetition is the highest-confidence verdict in the whole matrix (R10 §6.6), and it is a signal no per-page model can access. This is what keeps every LLM edit `Conserving` without amputating the label vocabulary.

### 6.3 Budgets (all `source = provisional`, all enforced)

```toml
ai.enabled                     = false      # v1 default; opt-in
llm.max_calls_per_book         = 8
llm.max_blocks_per_book        = 30
llm.max_wallclock_share        = 0.25       # hard stop, not a target
llm.max_output_tokens_per_call = 1500
inventory.max_clusters         = 24         # above this: no LLM call at all
inventory.min_body_char_share  = 0.60       # below this: clustering is invalid
inventory.holdout_disagree_max = 0.20       # above this: reject the mapping
```

Task 4 is batched at **10 blocks per call**, so `llm.max_blocks_per_book = 30` costs at most 3 calls and leaves 5 of the 8 for tasks 1–3 including `book_structure` chunking *(ratified note N-4)*. When the cap would still be exceeded, tasks are dropped in a fixed priority order — `verse_quote` first, then `book_structure` chunks beyond the first, then `heading_roles`, never `metadata` — so that budget exhaustion degrades predictably rather than by call ordering.

`llm.max_wallclock_share` is computed against **total** wall-clock including the LLM, so a 300-page book at the 0.5 s/page deterministic budget *(provisional)* permits ≤ 50 s of LLM. D9's gate G4 is tightened to that figure for the reference 300-page book on machine L *(ratified note N-5)*. When the share is breached mid-book, the AI path stops for the remainder of that book and the report says so.

### 6.4 How calibration replaces predicates later

`thresholds.toml` is the single source of every numeric constant, and every entry carries provenance:

```toml
[furniture.band_fraction]
value      = 0.07
source     = "provisional"          # binary | published | provisional | calibrated
evidence   = "R2 §B.4 — a percentage of page height (7–8 %) generalizes across page sizes better than PyMuPDF's fixed 50 pt"
owner      = "maintainer"
review_by  = "2026-12-31"

[epub.split_bytes]
value      = 260000
source     = "published"
evidence   = "Calibre --flow-size default; the size required by Adobe Digital Editions (R5 §A13)"
owner      = "maintainer"

[paragraphs.unwrap_factor]
value      = 0.45
source     = "published"
evidence   = "Calibre pdf_input unwrap_factor default (R1 §C.1); the generic HTML path uses 0.4 (R10 §6.4)"
owner      = "maintainer"

# after calibration, an entry gains its evidence of calibration:
[headings.cluster_zgap]
value        = 1.8
source       = "calibrated"
evidence     = "eval/calibrate.py run 2027-03-11; risk-coverage at false-repair ≤ 1 %"
n            = 412
risk_target  = 0.01
coverage     = 0.63
diagram      = "eval/reports/headings_cluster_zgap_reliability.svg"
owner        = "maintainer"
```

CI fails on a `provisional` entry with no owner or an expired `review_by`. This makes D17 a mechanism rather than a claim.

`openconvert eval calibrate` (driving `eval/calibrate.py`) fits per-signal thresholds by the risk-coverage method (Geifman & El-Yaniv; R9 §C.10): fix a **false-repair-rate target of ≤ 1 % per category** (R9 §C.8), take whatever coverage falls out, and attach a reliability diagram with **adaptive binning** — naive fixed-width ECE has documented flaws (Nixon et al., R9 §C.9). Fitting happens on **real-producer strata only**; our own renderers' output is for regression and unit correctness, never for threshold fitting (D18, RT A9). Promotion from `provisional` to `calibrated` requires a reviewed commit carrying the diagram, `n`, and the before/after false-repair rate. Until then the predicate stands, and a predicate that is merely *coarse* is strictly safer than a threshold that is *invented*.

---

## 7. The validate → repair loop

### 7.1 Shape

```
document → epub (emit)  →  Tier-1 validate  →  structural validate
                                  ↓ issues
                       message-id → repair mapping table (static)
                                  ↓ at most one repair per (file, node) per iteration,
                                    applied in order of (severity, message_id, location)
                              regenerate → re-validate
                                  ↓ M strictly decreased and no new message id?
                          yes → loop (cap 3)          no → stop, revert last repair
```

### 7.2 Termination

**Measure.** `M = (fatal_count, error_count, warning_count)`, compared lexicographically. A repair is applied only if it **strictly decreases `M`** *and* introduces **no message id absent before**. With a strictly decreasing well-founded measure, termination is guaranteed in at most `|messages|` steps; the cap of 3 *(provisional)* is a safety bound, not the termination argument (RT A10).

**Cycle detection.** The EPUB content is hashed each iteration with timestamps excluded. A repeated hash halts with `status = repair_oscillation` and a report entry. Strict decrease alone does not catch every cycle when two repairs interact.

**Confluence by construction.** The mapping table is partitioned so that at most one repair touches a given `(file, node)` per iteration, and repairs are applied in a fixed total order. Two repairs can therefore never race for the same node inside one iteration, which is what makes the loop reproducible.

**Ordering.** Tier-1 (internal, always on) runs first because it is milliseconds and catches the structural classes; the structural validator (conservation I-7, `noteref`↔`footnote` bijection, `page-list` target resolution, image-count parity, duplicate detection, heading-tree sanity) runs second; EPUBCheck (Tier 2, CI or the optional validation pack) runs last and only when available. Repairs are keyed by message id across all three sources in one table.

**After the cap.** The EPUB is still written — a slightly invalid EPUB is more useful than none. The report is marked `invalid`, the offending message ids are listed verbatim, and the UI says so plainly rather than reporting success (D13.7).

**Unmapped message ids are never guessed.** They are logged verbatim, surfaced to the user, and open a maintainer issue. The table's coverage is a measurable quality metric, which is the point (R10 §6.19).

### 7.3 Repair-fire rate as a release metric

We own the generator, so **every repair that fires is a bug in our emitter**. A corrective patch to the zip is untyped, order-dependent, and leaves the emitter broken. Therefore the corpus-wide repair-fire rate is a release-gate metric with **target zero**, tracked per message id. Any repair firing more than a handful of times across the corpus opens an issue against the emitter, not against the repair table. Reframed this way, "0 EPUBCheck errors on the corpus" stops being a validation goal and becomes a generator-correctness goal — the only version of it that scales for one maintainer (RT A10).

The typed XHTML builder (D5) is the preventive half of the same argument: `Flow` / `Phrasing` / `Sectioning` types make `<figure>` inside `<p>`, `<a>` inside `<a>`, and `<aside>` in phrasing context fail to compile, so the entire content-model error class — the class a hand-rolled emitter is most likely to produce, and the class plain well-formedness cannot see (RT A6) — never reaches the repair loop.

---

## 8. Process model

### 8.1 Boundary and rationale

`oc-core` is a library from day one, linked by both `openconvert` and the desktop app. v1 executes conversions **out of process** for two concrete benefits: deterministic RAM reclamation (`kill()` returns 100 % of the memory) and cancellation-by-kill. Crash isolation is only genuinely needed at the C/C++ boundaries — PDFium and llama.cpp — and stating the rationale this way keeps the boundary *movable* later without relitigating the decision (D13.1, RT A5). One code path serves GUI, CLI, CI and benchmarks.

### 8.2 IPC protocol (RT C2)

**Channels.** stdin = control (NDJSON). **stderr = events (NDJSON).** stdout = data only (`--dump-stage -`), empty in GUI mode. Reserving stdout keeps `--dump-stage` and shell piping usable and removes the flag-collision bug that a shared stream guarantees.

**Framing.** One JSON object per line, `\n`-terminated, UTF-8, hard cap **8 KiB** — the engine truncates and sets `"truncated": true`. Common envelope `{"v":1,"t":<type>,"seq":<u64>,"ts_ms":<u64>, …}`; `v` is the protocol major and the GUI hard-errors on mismatch.

| Event (stderr) | Payload |
|---|---|
| `hello` | `engine_version, ir_version, protocol, pdfium_version, capabilities[]` |
| `job` | `job_id, input_sha256, pages, phase:"started"` |
| `stage` | `name, phase:"begin"\|"end", elapsed_ms` |
| `progress` | `stage, done, total, unit` — coalesced to ≤ 10/s |
| `warning` | `code, severity, args{}, block_ids?[]` — code + args only; the GUI localizes |
| `llm` | `call_id, purpose, cached, tokens_in, tokens_out, ms` |
| `heartbeat` | every 2 s — lets the GUI distinguish "slow stage" from "hung process" |
| `done` | `status:"ok"\|"failed"\|"cancelled", report_path, output_path?` |
| `fatal` | `code, message, backtrace_id` |

**Control (stdin).** `{"t":"cancel"}`, `{"t":"ping"}`, optionally `{"t":"pause"}` / `{"t":"resume"}`.

**Cancellation contract.** `cancel` sets an atomic flag polled at stage boundaries **and inside per-page loops**. The engine reaches `done{status:"cancelled"}` within **2 s**, deletes temp files, and exits **3**. If it has not exited within 5 s the supervisor terminates the job object.

**Exit codes.** `0` ok · `1` conversion failed (report written) · `2` usage error · `3` cancelled · `101` panic. **Never parse stderr text to determine the outcome.**

**Bulk data never crosses the pipe.** `report.json`, `--dump-stage` output and contact sheets are files whose paths appear in `done`.

**Atomic output.** The EPUB is written to `<output>.oc-tmp-<rand>` in the *destination directory* — same filesystem, so the rename is atomic — and renamed only on success. Cancellation or failure deletes it.

**Process supervision.**
- *Windows:* the app creates a Job Object (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` + `JOB_OBJECT_LIMIT_PROCESS_MEMORY` + `JOB_OBJECT_LIMIT_JOB_TIME`) and assigns the engine to it; the engine creates a **nested** job for `llama-server` / `tesseract`. Every spawn sets `CREATE_NO_WINDOW`, or a console flashes on every conversion.
- *Unix:* the engine is `setsid`'d into its own process group; the supervisor kills with `kill(-pgid)`. Linux additionally sets `PR_SET_PDEATHSIG=SIGTERM`. macOS has no `PDEATHSIG`, so the app must also clean up on `applicationWillTerminate`.
- The engine installs `SIGTERM`/`SIGINT` handlers and a Windows console-ctrl handler that tear down children. A `Drop` guard alone is insufficient: panic-abort and `TerminateProcess` both skip it.

**Resource limits, always on.** `--max-pages` (3000), `--max-memory` (4 GiB via job object / `setrlimit(RLIMIT_AS)`), per-stage wall-clock deadline, max image pixels (100 MP declared), max decompressed stream bytes (256 MB), max output size. A degenerate PDF — 3 pages, 40 M glyphs — must fail cleanly, not OOM the machine. Otherwise the stated benefit of out-of-process execution is not actually taken.

**Job spec.** The GUI passes **one** argument: the path to a validated job-spec JSON in an app-controlled directory (§11.3). Everything else lives in the file, which is what makes Tauri's argument allow-list meaningful rather than a path regex (RT B15).

**Stale-sidecar footgun.** `externalBin` requires a `-$TARGET_TRIPLE` suffix on our own binary, staged before `tauri build`, and `tauri dev` will happily run a *stale* staged engine. The `hello` handshake is the cheap fix: the GUI hard-errors on any `engine_version` / `ir_version` / `protocol` mismatch rather than debugging the GUI against last week's engine.

### 8.3 Threading

No `async` in the pipeline core; it is CPU-bound (D1). `rayon` provides per-page parallelism inside `ingest` and image decoding; stages themselves run sequentially so that the conservation check after each stage has a single well-defined state to compare. Progress and cancellation cross thread boundaries as a `Arc<dyn ProgressSink>` and an `Arc<AtomicBool>`, both synchronous — a progress callback that allocates or blocks is a bug, and the `≤ 10/s` coalescing happens in the sink implementation, not in the stages.

The engine's rayon pool size and `llama-server`'s `-t` are set from one place so they do not oversubscribe the machine; when the engine owns the server it passes `-t` explicitly rather than letting both default to all cores.

### 8.4 Determinism contract (D13.8, RT B6)

| Condition | Guarantee |
|---|---|
| `--no-ai`, same OS, same version | byte-identical EPUB — tested |
| `--no-ai`, cross-OS | byte-identical — tested in CI, which is why image codecs are pure Rust |
| AI enabled, cache hit | byte-identical |
| AI enabled, cold | **not guaranteed.** llama.cpp greedy decoding is not bit-reproducible across thread counts and backends, because reduction order changes FP rounding |

The content-addressed cache *is* the determinism mechanism for the AI path. Cassette recording pins the thread count so that recorded outputs are reproducible on that fixed configuration.

---

## 9. The AI adapter

### 9.1 Provider abstraction

`LlmProvider` has two v1 implementations:

- **`LocalSidecar`** — a `llama-server` we own or one the app owns, reached over loopback with a per-run API key.
- **`OpenAiCompatible`** — base URL plus optional key. Ollama is this implementation with auto-detection on `localhost:11434`, its `format` field used for the JSON schema, and an explicit `num_ctx` override because Ollama's default of 2048 would silently truncate our prompts. LM Studio and other local servers are a custom endpoint. **No cloud preset in v1.**

Both speak the same wire format — `POST /v1/chat/completions` with `messages`, `temperature: 0` and `response_format`. Sharing one wire format is what makes every LLM call mockable in `cargo test`, and it is why the OpenAI-compatible endpoint — not `/completion` — is the interface even for the sidecar we own.

**Thinking control** is the one place the providers genuinely differ, so it is a trait method: `LlmProvider::thinking_control()`. `LocalSidecar` sends `chat_template_kwargs: {"enable_thinking": false}` in the request body; Ollama sends `think: false`; a generic OpenAI-compatible endpoint gets `/no_think` appended to the **shared system prefix** (§9.3) for Qwen-family models, which keeps that prefix byte-identical across all four tasks and so keeps prefix reuse intact. The lever is best-effort; the check is not — for **every** provider, any `<think>` content in the response, or any output that does not match the grammar, fails gate S (§6.2) and the pipeline falls back deterministically with `fallback_used = true` recorded on the `Decision`. A provider that ignores its knob degrades safely and visibly rather than leaking reasoning into the IR.

> **Implementation note, 2026-09-23 (Phase 11) — PROVISIONAL, needs maintainer ratification.**
> Ollama's OpenAI-compatibility layer drops `format`, `options.num_ctx`, `keep_alive` and `think`
> without a word (its `ChatCompletionRequest` has none of them), so the Ollama adapter
> (`oc_ai::provider::ollama`) speaks Ollama's own `POST /api/chat` — the same messages, greedy
> decoding and answer, in the one request shape where the context size is honoured. Every Ollama
> request sets `num_ctx` ≥ the prompt's byte length + template overhead + `max_tokens`, and
> `truncate: false`, `shift: false`. The consent check lives in `oc-net`'s transport constructors
> (`oc_net::consent::authorize`) rather than in a `Transport::endpoint()` method: a transport to a
> host off this machine cannot be built without a `ConsentRecord` naming it. A capability probe
> (`oc_net::detect::probe`: `/props`, `/api/tags`, `/v1/models`) picks the adapter for an endpoint;
> a generic OpenAI-compatible server is treated as constraining nothing (schema in the prompt,
> `W_LLM_UNCONSTRAINED`). See `docs/DECISIONS_LOG.md`, 2026-09-23, Phase 11.

### 9.2 Grammars and prompt versioning

Each task owns a **GBNF grammar** as the canonical artifact, plus a JSON Schema rendered from the same source of truth and sent as `response_format: {"type":"json_schema", …}` where the server prefers it. Every prompt artifact for a task version lives in one directory: **`crates/oc-ai/prompts/<task>/v<N>/{system.md,user.tmpl,grammar.gbnf,schema.json}`** — `system.md` is the shared byte-identical prefix (§9.3), `user.tmpl` the per-task payload template. The Rust modules under `crates/oc-ai/src/prompt/v<N>/` are thin `include_str!` wrappers over these files, so nothing in the prompt path is a Rust string literal and a prompt edit is reviewable as a text diff. The `grammar_hash` in the cache key is the SHA-256 of **`grammar.gbnf`** itself, so a grammar edit invalidates cached decisions.

Schemas are deliberately **flat and short** — enums and spans, never nested objects — because stricter format constraints measurably degrade reasoning quality ("Let Me Speak Freely?", R10 §4.3). The JSON schema you add for safety costs accuracy; keeping it flat is how that cost is minimized.

Prompts are versioned by directory (`prompts/<task>/v<N>/`). A version bump invalidates every cache entry and every cassette keyed to the old version, which forces deliberate re-recording instead of silent staleness (R9 §C.4).

### 9.3 Prefix discipline

**One byte-identical system prefix** — the taxonomy plus the shared few-shot exemplars — is used by all four tasks. Only the user message varies. Combined with `-np 1` and a single slot, this means one warm prefix serves an entire book. This is a design element, not an optimization: `--cache-reuse` works by KV shifting and is therefore architecturally unavailable to hybrid recurrent models, whose state cannot be partially erased; those rely on exact-prefix slot reuse and `--context-checkpoints` instead (RT A3). Making the prefix byte-identical is the one mechanism that works for both families.

### 9.4 Cache and traces

```
cache key = sha256( model_id ‖ prompt_version ‖ grammar_hash ‖ rendered_user_message )
path      = cache/llm/<key[0..2]>/<key>.json
```

Every applied LLM decision writes a `Decision` with `LlmTrace { model_id, prompt_version, input_sha256, output_sha256, cached, ms }`. The report can therefore answer "which model, which prompt, what did it see, what did it say" for every AI-influenced choice in the book, offline, months later.

### 9.5 Cassettes for tests

Cassette key is the cache key. Fast-unit and integration tiers replay only and never touch a model; the nightly tier is the sole place cassettes are re-recorded, as an explicit reviewed action, with a pinned thread count (R9 §B.9, §C.4). A cassette diff is a meaningful review artifact: it says the model's behaviour for this exact prompt changed. Canary tests are a small fixed cassette-backed set whose schema validity and semantic assertions must survive every prompt edit — they catch structural regressions such as "the model started wrapping JSON in code fences" that quality metrics miss.

### 9.6 The four v1 tasks

Never per page. Once per book, over compressed inventories, with geometry pre-digested into categorical words — never raw coordinates (R10 §4.1). Dehyphenation via LLM is **dropped from v1**: a kilobyte-scale logistic/CRF classifier over character features lifts keep-hyphen recall from 31.7 % to 85.8 % at no accuracy cost and is cheaper, more accurate and more testable (R2 §B.7, D13.6).

---

#### Task 1 — `metadata` (R10 §6.16)

*Pre-gate:* XMP/DocInfo `dc:title` absent or boilerplate. *Input:* verbatim text of pages 1–3 with inline `[LARGE]` / `[MEDIUM]` / `[SMALL]` / `[CENTERED]` annotations, ~200–400 tokens. No coordinates. *Prompt:* copy strings exactly; return null for absent fields; do not guess.

```json
{ "title": "Die Verwandlung", "subtitle": null,
  "authors": ["Franz Kafka"], "translator": null,
  "publisher": "Kurt Wolff Verlag", "date": "1915" }
```

*Validation:* **verbatim-substring check on every non-null field** against the input pages, case- and whitespace-normalized. This is the strongest and cheapest anti-hallucination check anywhere in the pipeline: a fabricated author cannot be a substring of a title page it was not on. Any field failing rejects the whole response. Title length 1–200 chars; language must match `whatlang`'s verdict. *Fallback:* largest-font block, then filename parse. *Note:* Gate V is vacuous here — metadata text is outside `C` — and the verbatim check is its substitute.

#### Task 2 — `heading_roles` (R10 §6.7)

*Pre-gate (all must hold, else no call at all):* cluster count ≤ 24; the modal (body) cluster holds ≥ 60 % of non-whitespace characters; silhouette above the configured floor. When the scaffolding is invalid the LLM cannot rescue it — warn and stay deterministic. When the body-font mode changes across a page range, cluster **per segment** rather than per book.

*Input:* per cluster, `{cluster_id, size_z, weight, italic, alignment, is_centered, count, starts_page_ratio, examples: [≤5 verbatim strings]}` — typically 6–12 clusters, 300–800 tokens. Plus 8–10 *individual runs* sampled from those clusters and **not** shown as exemplars, as a held-out probe.

```json
{ "clusters": [ {"cluster_id": 0, "role": "body"},
                {"cluster_id": 1, "role": "chapter_heading"},
                {"cluster_id": 2, "role": "running_head"},
                {"cluster_id": 3, "role": "epigraph"} ],
  "holdout":  [ {"run_id": 8821, "role": "chapter_heading"},
                {"run_id": 9014, "role": "body"} ] }
```

*Validation:* Gate S (bijection over cluster ids, legal enum). **Held-out self-consistency:** if the per-instance labels disagree with the cluster labels on more than 20 % of the holdout, reject the entire mapping and fall back to deterministic size-rank. This is a free oracle for "the clustering was wrong", available on day one with no gold set. Additionally `chapter_heading` clusters must have count ≥ 2 and ≤ 200, and `epigraph` must not be the most frequent large-font cluster. *Deletion authority:* a `running_head` label is a proposal only (§6.2). *Fallback:* size-rank ordering with a "heading levels may be imprecise" warning.

#### Task 3 — `book_structure` (R10 §6.8, RT A8)

*Pre-gate:* PDF outline absent **and** printed-TOC parse < 3 entries. When an outline exists it is ground truth and the call is skipped entirely.

*Input:* the flat heading list `[{idx, text, page, style_cluster}]`, typically 20–80 entries. Output is **boundary / run-length**, not one object per heading — a reference work with 1,200 headings would otherwise produce ~10 K output tokens and make an id-bijection failure near-certain (RT A8):

```json
{ "frontmatter_end_idx": 12,
  "part_boundaries": [13, 47, 88],
  "backmatter_start_idx": 104 }
```

Above 200 headings the list is chunked with overlap and the boundaries are stitched; each chunk's indices are validated in its own range before stitching. *Validation:* all indices strictly increasing and within range; front matter contiguous at the start; back matter contiguous at the end; chapters monotone with page order; every chapter has ≥ 1 page. Any violation rejects the whole response. *Fallback:* deterministic level ranking plus EN/DE/TR keyword rules. *Failure handling:* revert, warn, and expose the TOC in the review UI — a TOC is the one thing users reliably fix by hand.

#### Task 4 — `verse_quote` (R10 §6.13)

*Pre-gate:* indent present, short-line ratio in `[0.35, 0.75]`, block budget remains, `llm.max_blocks_per_book = 30` not exhausted. Batched at exactly **10 blocks per call** (ratified note N-4), so the 30-block cap costs at most 3 of the 8 calls.

*Input:* per block `{id, text (verbatim, ≤ ~60 words), indent: "shallow"|"deep", lines, avg_line_words, centered, monospace}` — categorical geometry, no numbers.

```json
[ {"id": "k3f9qm2p1a", "type": "verse"},
  {"id": "b7x0nn4rd2", "type": "blockquote"},
  {"id": "q1m8vv6te4", "type": "paragraph"} ]
```

*Validation:* ids bijective; **text byte-identical before and after** (only the wrapper changes); `preformatted` requires the monospace flag; `verse` requires ≥ 3 lines and short-line ratio > 0.6 — the model cannot override strong deterministic counter-evidence. *Fallback:* `paragraph`; a missed blockquote is cosmetic, a wrongly declared `<pre>` breaks reflow on a phone. *Risk profile:* the edit is a CSS class, so a wrong answer degrades presentation and cannot corrupt text.

---

## 10. Repository structure

```
openconvert/
├── Cargo.toml                    # workspace: members = ["crates/*", "apps/desktop/src-tauri"]
├── deny.toml                     # §3.2 — licence allow-list + socket/async bans
├── rust-toolchain.toml           # pinned toolchain
├── thresholds.toml               # SINGLE source of every numeric constant, with provenance
├── models.toml                   # model registry: repo ‖ commit-sha ‖ file ‖ sha256 ‖ tier ‖ licence
├── crates/
│   ├── oc-model/                 # IR, ids, ledger, canonical JSON, overrides schema
│   │   └── fixtures/             # committed IR fixtures per ir_version, for migration tests
│   ├── oc-pdf/                   # PdfBackend trait + pdfium impl + lopdf object access
│   ├── oc-text/
│   │   └── data/                 # EN/DE/TR word-frequency lists (generated by eval/, CC0-sourced)
│   ├── oc-layout/
│   ├── oc-structure/
│   ├── oc-epub/
│   ├── oc-validate/
│   │   └── repairs.toml          # message_id → repair mapping table (static, reviewed)
│   ├── oc-ai/
│   │   ├── prompts/<task>/v1/    # system.md, user.tmpl, grammar.gbnf, schema.json
│   │   │                         # versioned by directory; a bump invalidates cache + cassettes
│   │   │                         # src/prompt/v1/*.rs are include_str! wrappers over these
│   │   └── tests/cassettes/<task>/<key-hex>.json + index.json   # content-addressed; nightly-only re-record
│   ├── oc-net/                   # the ONLY crate that opens a socket
│   ├── oc-core/                  # orchestrator: stages, ledger checks, escalation, repair, report
│   ├── oc-testkit/               # test-only: fixture builders, assertion runner, structural digest
│   └── openconvert/              # the CLI binary = the engine
├── xtask/                        # dev tasks: vendor-pdfium, fetch-llama-server, fixtures,
│                                 # thresholds-lint, ci-lint, stage-sidecars
├── apps/desktop/
│   ├── src/                      # TypeScript UI (Svelte), conservative CSS baseline
│   ├── src-tauri/                # thin Rust: job-spec writer, supervisor, model manager
│   └── tests/                    # Vitest
├── eval/                         # Python. Never shipped, never a build dependency.
│   ├── src/oc_eval/              # the `oc-eval` CLI: corpus, generate, mutate, ground_truth,
│   │                             # metrics, bench, compare, calibrate
│   ├── model_gate.py             # the nine promotion gates G1–G9 (D9)
│   ├── results/model_gate/       # <model>__<build>__<machine>.json — the gate's source of truth
│   │                             # (docs/MODEL_GATE.md is generated from these)
│   └── reports/                  # committed diagrams and calibration evidence
├── corpus/
│   ├── manifest.json             # url ‖ sha256 ‖ licence ‖ producer stratum ‖ holdout flag
│   ├── download.py               # fetches into corpus/files/ (git-ignored), verifies SHA-256
│   ├── fixtures/                 # typst/, handmade/, mutations/, scanned/, crash/
│   └── files/                    # .gitignore'd; populated by download.py. The frozen ≥100-document
│                                 # real-world holdout is the `holdout: true` subset of the manifest,
│                                 # not a separate directory.
├── docs/
│   ├── DECISIONS.md              # the ADR — authoritative
│   ├── IR_SKETCH.md              # the IR — authoritative
│   ├── ARCHITECTURE.md           # this file
│   ├── PIPELINE.md               # stage-by-stage
│   └── IMPLEMENTATION_PLAN.md    # phase order and acceptance criteria
└── .github/workflows/
    ├── ci.yml                    # fmt, clippy -D warnings, nextest, cargo-deny, unshare -n suite
    ├── release.yml               # 3-OS bundle, signing, notarization, Ed25519 updater manifest
    ├── epubcheck.yml             # EPUBCheck + Ace as hard gates on the corpus
    ├── tier1-parity.yml          # Tier-1 validator vs EPUBCheck's public corpus, per-message-ID
    ├── nightly.yml               # full corpus, benchmarks, cassette refresh, calibration checks
    └── dom-checks.yml            # Playwright DOM assertions (Chromium per PR, WebKit nightly)
```

No large binaries in git. The corpus is a manifest plus a download script; `ours(*)` strata may not exceed 40 % of it, and a release may not pass on `ours(*)` alone (D18).

---

## 11. Configuration and presets

### 11.1 Precedence

```
CLI flags  >  job-spec  >  user config  >  preset  >  defaults(thresholds.toml)
```

TOML throughout. The user config lives in the platform config directory; the job spec is written by the GUI into an app-controlled directory. A preset is a **named overlay onto the threshold table**, applied below the job-spec layer: `preset` sets values, `job-spec.config_overlay` and CLI flags override them, and `thresholds.toml` supplies everything nobody mentioned. Provenance survives the merge, so the report can say which layer set a given value.

### 11.2 Presets

`auto` inspects the document classification (`book-prose`, `academic-multicolumn`, `scanned`, `slides/other`) and the `/Producer` fingerprint and selects one of the others; the choice is recorded in the report and is user-overridable.

| Preset | What it changes |
|---|---|
| `auto` | selects from the below by `DocClass` + producer family; the default |
| `novel` | single column assumed unless gutters are unambiguous; drop-cap and small-caps detection on; verse escalation on; tables rare, image fallback cheap; chapter split at every `h1` |
| `academic` | multi-column expected; footnote and caption detection weighted up; `page-list` always emitted; bibliography hanging-indent guard on; split at `h1`+`h2` |
| `textbook` | list and table detection weighted up; figure/caption association strict; heading numbering regexes given more weight than font size |
| `poetry` | verse predicates loosened; line breaks preserved by default; unwrap factor lowered; the verse/quote LLM task is the first to keep its budget rather than the first to lose it |
| `scanned` | OCR routing forced where a text layer is absent; `OcrLayerDuplicate` budget raised to the per-page cap; suspicious-text statistics reported rather than acted on; dehyphenation held to the fail-closed rule |

### 11.3 Job-spec schema sketch

```json
{
  "spec_version": 1,
  "job_id": "01JB2K7QF3T4Z0",
  "input":  { "path": "/abs/in.pdf", "sha256": "…", "password_file": null },
  "output": { "path": "/abs/out.epub", "report_path": "/abs/out.report.json", "overwrite": false },
  "preset": "auto",
  "config_overlay": { "images": { "max_longest_px": 1600 },
                      "epub":   { "split_bytes": 260000 } },
  "ai": { "enabled": false,
          "endpoint": "http://127.0.0.1:51733/v1",
          "api_key_file": "/abs/run-key",
          "model_id": "qwen3-1.7b-q4_k_m",
          "consent_nonloopback": false,
          "tasks": ["metadata", "heading_roles", "book_structure", "verse_quote"] },
  "ocr": { "mode": "auto", "tesseract_path": null, "languages": ["deu", "tur", "eng"] },
  "limits": { "max_pages": 3000, "max_memory_bytes": 4294967296, "stage_deadline_ms": 300000,
              "max_image_pixels": 100000000, "max_stream_bytes": 268435456,
              "max_output_bytes": 2147483648 },
  "overrides_path": null,
  "dump_stages": [],
  "locale": "en"
}
```

The spec is validated against a committed JSON Schema before anything else happens; a spec that fails validation exits `2` with a `fatal` event naming the offending pointer. The GUI and the CLI produce the *same* spec — that is what makes "reproduce this conversion from the report" a real feature.

---

## 12. Extension points and post-v1 hooks

Each of these is a seam that already exists in v1, so that adding the capability is an addition rather than a refactor.

| Post-v1 capability | Existing seam | What has to be built |
|---|---|---|
| **ML layout escalation** | `layout` already computes a per-page confidence and the escalation predicate already routes; the pre-mask input to XY-cut is already a list of typed regions | ONNX export of `docling-layout-egret-medium` (DFINE-m, 19.5 M params, Apache-2.0, ~0.334 s/page CPU, R2 §D.4), `ort` as an optional feature, an optional-download layout pack. Escalating 5 % of a 300-page book costs ~5 s |
| **VLM page re-analysis** | `PageClass::BrokenText` and low-confidence table regions are already first-class verdicts with an override | a second provider kind behind `LlmProvider` (or a sibling trait), capped at ≤ 2 % of pages, and only on pages whose text layer is broken or whose table grid failed |
| **Overrides UI (block level)** | `Overrides.blocks` is in the schema from day one; every block has a stable id; the report already deep-links to block ids | UI only: a block inspector that writes `BlockOverride { role, level, merge_with_next }` |
| **Second `PdfBackend`** | the trait exists and PDFium sits behind it; `backend_id()` is already recorded in the report | a `hayro` implementation once it exposes char-level `font_weight` / `is_generated` / `is_hyphen` / `render_mode`; until then `pdftotext` is the differential string oracle in CI (RT B3) |
| **OCR pack** | `ocr.mode` and `ocr.tesseract_path` already exist; v1 uses the system Tesseract | a signed post-install pack (tesseract + `tessdata_fast`) once the vcpkg static-triplet build recipe and three-platform signing are proven (D4) |
| **Validation pack** | `oc-validate` already shells to EPUBCheck when present | a jlink'd minimal JRE + `epubcheck.jar` (~40–50 MB) through the same download mechanism as the model (D6) |
| **SVG for vector regions** | `VectorRegion` is already an IR type with `is_rule` and a bbox; v1 rasterizes at 2× | a path→SVG emitter and the manifest `svg` property. No OSS PDF→EPUB converter does this (R1 §C.4) |
| **Font embedding** | the emitter already knows which glyph ranges are rare | WOFF2 subsetting from the PDF's embedded program, licence checks, EPUB font obfuscation where required (R5 §A9) |
| **Parse worker per page range** | `PdfBackend` is already the only door to PDFium | `openconvert __parse-worker` so a PDFium segfault costs one page range rather than the book (D16, hardening phase) |

---

## 13. Explicit non-goals for v1

Stated so that a future contributor does not read their absence as an oversight (D16).

1. **No ML layout model.** Heuristics only. Docling's egret/heron are Apache-2.0 but safetensors-only, so shipping one means our own ONNX export plus ~20–30 MB of `ort`.
2. **No VLM page re-analysis.**
3. **No MathML.** PDF equations become cropped images with alt text. Reading-system MathML support remains fragmented (R5 §A6).
4. **No font embedding.** Rely on the reader's font stack; warn on rare-script glyph ranges.
5. **No fixed-layout EPUB.** Reflowable only, including for scans.
6. **No RTL or CJK support.** The line/word assembly and reading-order code assumes LTR horizontal script.
7. **No SVG for vector regions.** Rasterized at 2×.
8. **No LLM OCR post-correction**, and never on by default — LLMs measurably degrade OCR text, German included (R10 §6.14).
9. **No LLM dehyphenation.** Dropped in favour of the tiny classifier (D13.6).
10. **No block-level correction UI.** The IR supports `overrides.json` from day one; the v1 UI edits metadata and the TOC only.
11. **No cloud LLM preset.** BYO endpoint exists; no provider is pre-configured.
12. **No telemetry, no crash reporting, no remote model registry.** `models.toml` ships with the app.
13. **No per-OS golden screenshots on pull requests.** DOM assertions on Chromium per PR, WebKit nightly, screenshots at release time on one OS, human-reviewed (D7, RT B8).
14. **No PDFium parse-worker isolation.** Deferred to the hardening phase.

---

## Notes for the Chief Architect

Six places where `DECISIONS.md` or `IR_SKETCH.md` appeared to me to be internally inconsistent or under-determined. Each note states the conflict, what this document does about it, and the smallest repair I could see.

**All six have since been ratified by the Chief Architect and folded back into the ADR.** The resolutions are recorded below as `Resolution (ratified)` and are applied throughout `PIPELINE.md`, in the amended passages of this document (§5.3 `Reason` enum, §5.4 invariant I-6, §6.3 budgets, §11.1 precedence), **and now in `DECISIONS.md` (D13.4, D13.6, D13.11, D9) and `IR_SKETCH.md` (the `Reason` enum, the `ingest` stage-kind line, the I-6 note)**. The notes are kept below as the record of what changed and why, not as open items.

**N-1. I-6 forbids the OCR that D13.10 requires on `mixed` pages.** RT C1's I-6 permits `reason = Ocr` "only on pages `p` where `C(text of p) = ∅` before the stage". D13.10 routes `mixed` pages to "OCR only image regions not covered by text, insert as blocks positioned by bbox" — on a page that by definition already has text. As written the two cannot both hold, and `mixed` pages are common in real books (plates, facsimile appendices). *This document* describes `mixed`-page OCR as specified in D13.10 and marks the invariant check as the open item. *Smallest repair:* scope I-6 to **regions rather than pages** — "permitted only in regions with no pre-existing text coverage" — and give the region case its own budget, or add a distinct `OcrRegion` reason so the page-level and region-level cases have separate budgets and separate retention treatment.

**Resolution (ratified).** I-6 is amended to region scope: `reason = Ocr` is permitted on any page region whose bbox contains no PDF text runs — the whole page on an `image-only` page, each uncovered image region on a `mixed` page. Each such region is marked `provenance = ocr` and excluded from source retention. No new `Reason` variant is needed. Applied in §5.4 above and throughout `PIPELINE.md` §3.

**N-2. `Ocr` has no owning stage.** IR_SKETCH's stage-kind list gives `ingest` the reason set `{GeneratedSpace, ClippedOffPage, OverdrawDedup, OcrLayerDuplicate}`, and no other stage declares `Ocr`. But `Ocr` is in the closed `Reason` enum and OCR creates runs, which only `ingest` does. Under I-2 (declared reasons) any OCR entry is currently an invariant violation regardless of which stage emits it. *This document and PIPELINE.md* assign OCR to `ingest`. *Smallest repair:* extend `ingest`'s declared reason set to include `Ocr`.

**Resolution (ratified).** OCR is owned by `ingest`, which runs it after page classification whenever an engine is available. `ingest`'s declared reason set becomes `{GeneratedSpace, ClippedOffPage, HiddenText, OverdrawDedup, OcrLayerDuplicate, Ocr}`. Applied in `PIPELINE.md` §0.2 and §3.

**N-3. No `Reason` covers invisible or hidden text that is not an OCR sandwich.** R1 §A.7.5 documents white-on-white text and off-page text as real, and A.7.4 documents clipped text. `ClippedOffPage` covers the last of those. Render-mode-3 text on a page with *no* image layer (a redaction artifact, SEO keyword stuffing) and fill-colour-equals-background text have no reason to be removed under. *This document* folds them into `ClippedOffPage`, which is a poor name for the case. *Smallest repair:* rename `ClippedOffPage` → `NotVisiblyRendered` and let it cover clipped, off-page, invisible-render-mode and colour-on-colour, with one budget; or add `InvisibleText` as a fifteenth variant. Either is a `Reason`-enum change and therefore an `ir_version` question.

**Resolution (ratified).** `HiddenText` is added to the closed `Reason` enum as a fifteenth variant rather than renaming `ClippedOffPage`. The split is: `ClippedOffPage` = geometrically absent (outside the CropBox, or removed by a clipping path); `HiddenText` = rendered but not visible (render mode 3 on a page that is not an OCR sandwich, or a fill colour within the delta-E tolerance of the local background). Both are owned by `ingest`. Applied in §5.3 above.

**N-4. The LLM call budget and the four tasks are arithmetically inconsistent.** D13.6 sets `≤ 8 calls/book`, and specifies task 4 as "≤ 30 blocks/book, batched", which at R10 §6.13's 5–10 blocks per call is 3–6 calls. Adding metadata (1), heading roles (1) and structure (1, or more with the >200-heading chunking rule) reaches 6–9+, so a reference work with a long heading list plus 30 ambiguous blocks exceeds the cap before anything goes wrong. *This document* defines a fixed degradation order (`verse_quote` first, then extra structure chunks, then heading roles, never metadata) so the overflow is at least deterministic. *Smallest repair:* fix the verse/quote batch size at 10 (making task 4 exactly 3 calls) and give chunked structure its own sub-budget outside the 8.

**Resolution (ratified).** `≤ 8 calls per book` stands as a single total budget, and task 4 is batched at **10 blocks per call**, so `llm.max_blocks_per_book = 30` costs at most 3 calls and leaves 5 for tasks 1–3 including `book_structure` chunking. No separate sub-budget. The degradation order stands as the tie-breaker. Applied in §6.3 above.

**N-5. G4 and the wall-clock share cap disagree.** D9's G4 allows the full per-book call set to take ≤ 90 s on reference machine L for a 300-page book. D13.11's performance budget is ≤ 0.5 s/page, i.e. ~150 s deterministic for the same book. A 90 s LLM set on a 150 s deterministic base is a 37.5 % share, which breaches D13.6's `≤ 25 %` **hard stop** — so a model could pass G4 and then be halted mid-book by the budget on the very same input. *This document* treats the 25 % share as authoritative because D13.6 calls it a hard stop. *Smallest repair:* restate G4 as "≤ 25 % of measured wall-clock on the same book, and ≤ 90 s absolute", i.e. make the absolute number a ceiling on top of the share rather than an alternative to it.

**Resolution (ratified).** G4's 90 s is reinterpreted as the LLM-only budget on machine L, and the `≤ 25 %` share is computed against **total** wall-clock including the LLM. A 300-page book at 0.5 s/page is ~150 s deterministic, permitting ≤ 50 s of LLM — so **G4 is tightened to ≤ 50 s for the reference 300-page book**. Applied in §6.3 above.

**N-6. Presets and `thresholds.toml` overlap without a stated resolution.** D13.11 gives the precedence chain `CLI > job-spec > user config > preset > defaults` and separately says thresholds live in `thresholds.toml`. But a preset that changes the paragraph unwrap factor or the furniture band fraction *is* changing a threshold, so `thresholds.toml` is simultaneously "defaults" (bottom of the chain) and "the single source of every numeric constant". D17's CI rule — fail on a `provisional` entry with no owner — also has no defined meaning for a value a preset introduced. *This document* defines a preset as a named overlay onto the threshold table applied at the `preset` layer, with provenance surviving the merge. *Smallest repair:* say so in D13.11, and require preset overlays to be declared inside `thresholds.toml` (e.g. `[preset.poetry.paragraphs.unwrap_factor]`) so the provenance and `review_by` rules apply uniformly to every number in the system.

**Resolution (ratified).** A preset is a **partial override map over `thresholds.toml` keys by name**, so the precedence chain applies key-by-key: a preset that sets `paragraphs.unwrap_factor` overrides that key and nothing else, and every other key still resolves from `thresholds.toml` with its own provenance intact. Preset overlays are declared inside `thresholds.toml` so the `source` / `owner` / `review_by` rules apply uniformly. Applied in §11.1 above and `PIPELINE.md` §0.5.
