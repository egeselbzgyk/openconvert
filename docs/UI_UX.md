# OpenConvert — Desktop UI Architecture

**Status:** Draft for v1 planning, written against `DECISIONS.md` (final for v1 planning) and `IR_SKETCH.md`. Never contradicts `DECISIONS.md`; disagreements are collected at the end.
**Date:** 2026-09-09

---

## 1. Principles

**Simple.** The UI's job is to get a PDF to an EPUB with as few decisions demanded of the user as possible, and to make the decisions it does ask honest and legible rather than clever. A drop zone, a queue, a result. Settings exist, but nothing in the default path requires opening them.

**Honest progress.** Progress is never fabricated. Every stage indicator, every percentage, every spinner is driven by an actual engine event (D13.2's `progress`/`heartbeat` NDJSON stream) — if a stage has no meaningful count to report, the UI shows an indeterminate spinner rather than inventing a number that creeps upward on a timer. This matters specifically because OpenConvert's pipeline stages vary wildly in how well their progress can be estimated (page-by-page extraction has a natural denominator; a single once-per-book LLM call does not), and a UI that fakes smoothness where the engine cannot provide it teaches the user not to trust the parts where the engine *can*.

**No AI gimmicks.** AI assistance is off by default in v1 (D17: `ai.enabled = false`). When it is on, its involvement is disclosed as a count, not celebrated as a feature — "AI-assisted decisions: 6" sits next to "Deterministic processing," not above it, and the UI never anthropomorphizes the model or narrates its reasoning. The product's claim is that conversion works well without AI and is honestly better with it, not that AI is doing something impressive.

**First-run is not overwhelming.** A user's first conversion should be droppable-and-done. The model manager, the provider settings, the quality presets, the advanced thresholds — none of it is required reading before the first PDF becomes an EPUB. First-run design (§5) exists specifically to keep the AI-opt-in conversation out of the way of that first conversion while still making it easy to find afterward.

## 2. Information architecture

### 2.1 Main window: drop zone and queue

The default, empty-state view is a single large drop target:

```
Drop PDF here
or [ Select PDF… ]
```

Native drag-and-drop delivers file *paths* directly (Tauri's `onDragDropEvent`, not `File` objects — D2), which matters because the pipeline is entirely file-path-driven; there is no intermediate "read the file into the webview" step to route around. Dropping (or selecting) more than one file populates a **queue** — each file becomes its own job row, processed according to the engine's own concurrency policy (one `llama-server` instance, `-np 1`, shared across the batch — D13.2's ownership resolution — so queued jobs needing AI assistance share the already-warm model rather than each spawning their own).

### 2.2 Per-job progress: real stage names, mapped from real events

The stage names shown to the user are **not** the engine's internal stage names (`inspect`, `ingest`, `text`, `furniture`, `layout`, `paragraphs`, `structure`, `document`, `epub`, `validate`, `repair`, `report` — `IR_SKETCH.md`'s `--dump-stage` vocabulary). Those names are correct for a developer reading `--dump-stage` output and wrong for a reader of a progress bar. The mapping:

| Engine stage(s) | UI label |
|---|---|
| `inspect`, `ingest` | **Analyzing** |
| `text`, `furniture` | **Extracting** |
| `layout`, `paragraphs`, `structure`, `document` | **Reconstructing** |
| `epub` | **Building** |
| `validate` | **Checking** |
| `repair` | **Repairing** (shown only when it actually runs — most conversions never see this label at all, since a repair firing at all is treated as an emitter bug, not a normal step — D13.7) |
| `done{status: "ok"}` | **Complete** |

Each label is driven by the `stage{name, phase: "begin"|"end"}` event stream (D13.2); the progress fraction inside a label comes from `progress{stage, done, total}` events, coalesced to at most 10/s by the engine so the UI never has to throttle a flood on its own. **When a stage has no natural count** (the once-per-book LLM calls, for instance, or a single-pass validation), the UI shows an indeterminate spinner for that label rather than a percentage — this is the concrete mechanism behind the "honest progress" principle in §1, not just a stated value. A `heartbeat` event arriving every 2 seconds (D13.2) is what lets the UI distinguish "this stage is just slow" from "this process has hung" — the queue row shows a subtle "still working" pulse driven by heartbeats even during an indeterminate stage, so silence past ~4–6 seconds without a heartbeat is itself the signal something is wrong, distinct from a stage that is legitimately taking a while.

**Cancel honors the 2-second contract.** The cancel button sends a `cancel` control message on stdin; the UI shows a brief "Cancelling…" state and expects `done{status: "cancelled"}` within 2 seconds (D13.2). If that deadline passes, the UI does not hang waiting — the supervisor terminates the job at 5 seconds regardless (§"Tauri boundary," below), and the row transitions to a cancelled state either way. The user is never shown a cancel button that appears to do nothing.

### 2.3 Result panel

On `done{status: "ok"}`, a job's row expands into a result panel:

- **Output path**, with **"Open in reader"** (hands the file to the OS default EPUB handler) and **"Show in folder"** actions.
- **Summary line**: pages, text blocks, images, tables, chapters — plain counts, pulled from the IR's own structural digest (`document.sections`, `document.figures`, `document.tables` — `IR_SKETCH.md`).
- **"Deterministic processing"** shown as a plain status line when AI was off (the default). When AI was on: **"AI-assisted decisions: N"**, where N is the count of `Decision` entries whose `method = Llm` (`IR_SKETCH.md`) — shown *only* when AI is actually enabled for that job, never as a permanent zero-value line that implies the feature is always present.
- **"Validation: passed / passed with warnings / invalid"** — the three states D6/D13.7 actually produce (EPUBCheck-clean with no warnings; clean of errors but with warnings; or `invalid`, D13.7's explicit post-repair-cap outcome where the EPUB is still written and the report says so plainly). This line is never omitted or softened — an `invalid` result is shown as plainly as a passing one, consistent with §1's honesty principle.
- **Warnings list, with page links.** Each `Warning{code, severity, args, blocks, page}` (`IR_SKETCH.md`) renders as one localized line (§8's warning-template requirement) with a link that, where the preview is available, jumps to the relevant page.
- **"Details"** opens the full conversion report viewer (§7).
- **"Preview"** opens the generated XHTML rendered in the Tauri webview itself, explicitly labeled **"Approximate preview — your reading device may differ."** This is not automated QA and is not treated as one; it is the same webview engine a user's own OS provides, shown so they can page through the result before deciding whether it's good, never used as the app's own correctness check.
- **"Edit metadata"** and **"Review TOC"** — the v1 correction surface. Per D16, block-level correction is deliberately out of v1 (the IR supports `overrides.json` keyed by block ID from day one, but the UI does not expose block-level editing yet); what v1 *does* expose is metadata fields (title, author, language) and the top-level TOC/heading structure. A correction writes to `overrides.json` (`IR_SKETCH.md`'s `Overrides` type: `metadata: Option<MetadataPatch>`, `toc: Option<Vec<TocPatch>>`) and triggers a **partial re-run** of the affected stages, not a full re-conversion from page one. This is a ratified decision, not an interpretation: the re-run starts from the **cached `structure` output** and re-executes exactly `document` → `epub` → `validate` → `repair` → `report`. Everything upstream of `structure` — `inspect`, `ingest`, `text`, `furniture`, `layout`, `paragraphs` — is untouched, because a metadata or TOC edit cannot change a glyph. The UI's framing to the user is "fix and rebuild," a few seconds, not "start over."
- **"Export diagnostic bundle"** — the manual, user-reviewed export `SECURITY.md` §10 describes. This button does not silently attach and send anything; it writes a bundle to a location the user picks and shows them what is in it before they decide whether to share it further.

### 2.4 Settings

**AI assistance.** Off by default in v1, with an explanatory card visible the first time the settings panel is opened (not buried under a toggle) stating plainly what turning it on costs — a model download, additional RAM while converting, additional CPU time — and what it buys — better handling of ambiguous headings, book structure, and verse/quote classification, per D13.6's four tasks. The toggle itself is a single switch; the explanation is what makes the switch an informed choice rather than a blind one.

**Model manager.** List of available model tiers (Qwen3-0.6B, Qwen3-1.7B default, Qwen3-4B, and the `experimental`-tier Qwen3.5 variants — D9), each row showing: download/installed status, on-disk size, a RAM estimate, a CPU-cost expectation ("fast" / "moderate" / "slower, higher quality"), and **the model's license, shown and requiring explicit acceptance before the first download of any model whose license carries a compliance obligation beyond a standard NOTICE** (`LICENSE_AND_DEPENDENCIES.md` §5 — this applies to the Apache-2.0 default tier trivially, since Apache-2.0 has nothing to actively accept beyond attribution, but the manager's design does not special-case that; it always shows the license and always requires the click, so a future conditionally-permissive model addition does not need a UI change to become compliant). Actions per row: **Download** (with a progress bar and a **Cancel** that actually cancels the in-flight download, not just hides it), **Delete** (frees the disk space, does not affect other installed tiers).

**Provider.** Three options, matching D10's `LlmProvider` implementations: **Built-in** (the app-owned `llama-server` sidecar — the default whenever AI is on and no other provider is configured), **Ollama** (auto-detected on `localhost:11434`; shown as available with no consent dialog, since loopback traffic never leaves the machine — D10, `SECURITY.md` §8), and **Custom endpoint** (any OpenAI-compatible base URL, with the **non-loopback consent dialog** naming the host and stating that document text leaves the machine before it can be selected — `SECURITY.md` §8). No cloud preset exists in v1 (D10) — there is no "OpenAI" or "Anthropic" quick-pick button; a user who wants a cloud endpoint types its URL in as a custom endpoint and goes through the same consent dialog as any other non-loopback host.

**Quality vs. speed preset.** The preset selector (`auto`/`novel`/`academic`/`textbook`/`poetry`/`scanned` — D13.11) picks the document-type preset; a separate `fast`/`balanced`/`thorough` **quality preset** is the same mechanism on a second axis. Both are **key-by-key partial override maps over `thresholds.toml` keys by name** (D13.11), declared inside `thresholds.toml` itself and resolved at the `preset` layer of the `CLI > job-spec > user config > preset > defaults` chain — so a quality preset that sets thread count, per-stage deadlines and escalation aggressiveness overrides exactly those keys and every other threshold still resolves with its own provenance intact. Neither preset surfaces raw threshold values in the main settings surface. `auto` (the default) uses D13.10's document-classification result to pick a preset automatically; the user only overrides it when the automatic choice is visibly wrong.

**Advanced.** Thread count, `--max-pages`/`--max-memory` overrides (surfaced here specifically because they are the two caps a power user hitting `SECURITY.md` §4's limits on a genuinely huge document needs to raise deliberately), **Clear cache** (the action `SECURITY.md` §10 requires be easy to find, given the cache stores document text), **Dump stages** (writes `--dump-stage` output for debugging — a developer/support-request feature, not a mainstream one), language override (bypasses `whatlang` auto-detection — D13.11), and a password field for encrypted PDFs stored only for the duration of a job, never persisted (D13.11's encryption handling).

**Packs.** OCR pack and validation pack (`LICENSE_AND_DEPENDENCIES.md` §6), each shown with the same status/download/delete pattern as a model row, and the same license-shown-before-download discipline.

## 3. First-run flow

On first launch, with no model installed and `ai.enabled = false` (the shipped default), the app does **not** interrupt the user with a modal asking them to choose. The drop zone is immediately usable — **conversion works fully without a model, from the first launch** (D17's `ai.enabled = false` default is not a crippled trial state; deterministic conversion is the complete v1 product for a user who never touches AI settings). A small, dismissible card near the drop zone offers the AI opt-in in plain terms:

```
┌────────────────────────────────────────────────────┐
│  AI-assisted structure detection is available and    │
│  off by default. It can improve heading and chapter  │
│  detection on ambiguous documents.                    │
│                                                        │
│  Costs: ~1 GB download, ~1–2 GB RAM while converting, │
│  a few extra seconds per book.                        │
│                                                        │
│  [ Set up AI assistance ]        [ Not now ]          │
└────────────────────────────────────────────────────┘
```

Choosing "Set up AI assistance" opens the model manager directly to the default tier's download row, with the license shown and a **one-click download** that shows real progress (from `oc-net`'s download, not a synthetic bar) and a working **Cancel**. Choosing "Not now" dismisses the card for that session; it reappears, low-key, in Settings rather than nagging on every launch.

## 4. Error-handling UX

Every row in this table maps a failure the engine can report (via `warning`/`fatal` events, D13.2, or a `done{status: "failed"}` outcome) to a specific, non-generic UI response — the design principle being that "conversion failed" is never an acceptable message on its own.

| Condition | UI response |
|---|---|
| PDF cannot be parsed | Job row shows a clear failure state naming the problem in plain language ("This file doesn't look like a valid PDF" / "This PDF is too damaged to read"), with **Export diagnostic bundle** offered directly on the failed row. No retry-as-is button, since retrying an unparseable file changes nothing. |
| Encrypted PDF | A **password prompt** appears inline on the job row (D13.11: empty-user-password is tried automatically first; a prompt appears only if that fails). Owner-password-only restriction flags do not block conversion — the job proceeds and the report notes the restriction was present, per D13.11. |
| OCR unavailable (image-only/scanned page detected, no system Tesseract found) | A non-blocking notice with a **copy-pasteable install command** for the user's detected OS (D4's "system-Tesseract-first" v1 posture) and a link to the OCR pack in Settings once that pack exists. The conversion still completes — the affected pages ship as images with a warning, not as a hard failure. |
| Font missing (rare-script glyph range with no embedded/available font) | A warning on the affected pages noting the script; the text still ships (fonts are never embedded by default — D13.11) with a note that reader-side font substitution may render it imperfectly. |
| Image extraction failed for a specific image | A per-image warning with a page link; the surrounding content still converts. One bad image never fails the whole job. |
| LLM unavailable, corrupted, or still downloading, with AI enabled | A **non-modal banner**, not a dialog the user must dismiss to keep working: "AI assistance unavailable this run — converted deterministically." The job completes on the deterministic path (D13.11's explicit fail-open behavior for this case) and the report notes it. |
| Model download failed | The model-manager row shows a failed state with **Retry**, and the download resumes from where it left off where the transport supports it, rather than restarting from zero. |
| Model incompatible (fails D9's promotion/health gates at load time) | The app falls back to the next-safest installed tier automatically where one exists, with a notice explaining the fallback; if none is installed, this collapses to the "LLM unavailable" row above. |
| EPUB validation failed | The result panel's Validation line reads **"invalid"** plainly (§2.3) — the file is still saved and still openable, and the warnings list explains what failed. Nothing about this state is hidden or softened. |
| Repair failed (D13.7's post-cap outcome) | Same as "EPUB validation failed" above — a repair that could not bring the document clean within its iteration cap results in an `invalid`-marked, still-saved file, not a withheld one. |
| Out of memory / resource limit hit (`SECURITY.md` §4's caps) | A specific message naming which limit was hit ("This document exceeds the memory limit for a single conversion") with a concrete suggested next step — raise `--max-memory` in Advanced settings — rather than a generic crash message. |
| Cancelled | The row shows a clean "Cancelled" state (exit code 3, D13.2) with no partial output at the destination path — the temp-file-then-atomic-rename mechanism (D13.2) means there is never a half-written file for the UI to explain away. |

## 5. Explainability: the conversion report

**Report structure**, drawn directly from the IR's own `Document`, `Ledger`, and `Decision` types (`IR_SKETCH.md`) rather than being a separately-invented schema — the report viewer is a rendering of data the engine already produces for its own invariant checks, not a parallel summarization layer that could drift from what actually happened:

- **Counts**: pages, blocks by kind, images, tables, footnotes, chapters (from `Document`'s own structural fields).
- **AI-assisted corrections by kind**: a breakdown of `Decision` entries with `method = Llm`, grouped by `DecisionKind`, each entry showing the chosen value, the deterministic fallback that was available, and — via `LlmTrace` — the model ID and prompt version used (`IR_SKETCH.md`'s `Decision.llm: Option<LlmTrace>`).
- **Warnings by page**: every `Warning` entry, grouped by `page`, with its `severity` and localized message.
- **EPUBCheck status**: the Tier-2 result (D6) when it ran — error/warning counts, or "not run" when only the always-on Tier-1 validator executed.
- **Ledger totals by reason**: per-`Reason` character counts (`Ledger.entries`, grouped — D13.4), shown as a compact table so a user or contributor can see at a glance how much text was attributed to furniture removal, dehyphenation, OCR, and so on, and — for anyone auditing a specific conversion — verify the conservation-law accounting is legible, not just internally checked.
- **Timing per stage**: derived from the `stage{name, phase, elapsed_ms}` event log (D13.2), shown per the UI-facing stage names in §2.2.
- **Model/prompt versions**: recorded once per job (not per decision) as a summary line, in addition to the per-decision detail above.

A sample rendering (abbreviated):

```
Conversion report — "The Wheels of Chance.pdf"

Pages: 214    Blocks: 1,842    Images: 12    Tables: 0    Chapters: 11

Deterministic processing: complete
AI-assisted decisions: 4
  · Heading role assignment (3 clusters)   model: qwen3-1.7b  prompt: v3
  · Book structure boundaries              model: qwen3-1.7b  prompt: v3

Validation: passed with warnings (2)
  p.34   low-confidence dehyphenation join kept as hyphenated (fail-closed)
  p.201  1 image below minimum resolution threshold

Ledger (characters, by reason):
  RunningHeader+Footer+PageNumber   1,204   (0.021 of C_0)
  Dehyphenate                          89   (0.0016 of C_0)
  LigatureExpand                       41   (paired add/remove)

Timing:  Analyzing 0.4s · Extracting 1.1s · Reconstructing 2.8s ·
         Building 0.6s · Checking 0.2s   (total 5.1s)
```

## 6. Accessibility and internationalization (v1)

**Keyboard and focus.** Every action reachable by mouse — drop-zone activation, queue-row actions, settings controls, model-manager download/delete/cancel — is reachable and operable by keyboard, with a defined, logical focus order (drop zone → queue → settings, not a DOM-order accident). **ARIA labels** are set explicitly on the drop zone (its two interaction modes — drag target and the "Select PDF…" button — are both labeled, not left to visual-only affordance), on progress indicators (announcing stage-label changes, not raw percentages, so a screen-reader user hears "Reconstructing" rather than a stream of numbers), and on the queue (each row identifiable by filename and status, not by position alone).

**Visual.** `prefers-color-scheme` is honored for dark/light, following the same light/dark-token discipline any theme-aware surface needs. OS font scaling is respected throughout — the UI is built in `rem` units, not fixed pixel sizes, so a user's system-level text-size preference actually changes the app's UI, not just the previewed EPUB content. `prefers-reduced-motion` is honored — progress transitions and the first-run card's appearance are instant rather than animated for users who have asked the OS for that.

**Localization: EN, DE, TR.** Three string tables ship in v1, matching the languages named throughout the deterministic pipeline's own design (Turkish-aware folding, DE/TR word-frequency lists — D13.11, `LICENSE_AND_DEPENDENCIES.md` §9). The load-bearing rule: **every engine `WarningCode` has a template in every locale, and this is CI-checked** — a warning code shipped by the engine (`IR_SKETCH.md`'s `Warning.code: WarningCode`, "stable enum, localized via templates") with no corresponding template in one of the three shipped locales is a build failure, not a runtime fallback-to-English surprise a user discovers mid-conversion. This closes the specific failure mode where a new engine warning code is added, the English string is written, and the DE/TR templates are simply forgotten — the CI check makes forgetting impossible rather than merely discouraged.

**RTL is explicitly not in v1's UI.** The pipeline's own per-block `xml:lang` handling (D13.11) and general Unicode-correctness work do not imply a mirrored, RTL-aware *application UI* — that is a distinct, larger effort (bidi-aware layout throughout the control panel, not just correct text direction inside converted content) and is out of scope for v1, consistent with D16's broader "RTL/CJK" exclusion for the conversion pipeline itself.

## 7. UI tech

**TypeScript + Svelte 5**, chosen over vanilla TS/HTML for a small, specific reason rather than general framework preference: the settings surface, model manager, and queue all involve enough reactive state (download progress, per-job status, locale-switched strings, theme tokens) that hand-rolled DOM updates would either accrete their own ad hoc reactivity layer or become error-prone, while Svelte's compile-to-vanilla-JS output keeps the bundle small and dependency-light — consistent with the same low-footprint priority that shaped the Tauri-over-Electron decision (D2) in the first place. Svelte 5's runes-based reactivity is a good match for the event-stream-driven UI model in §2.2 (an NDJSON event becomes a store update becomes a re-render, with no virtual-DOM diffing overhead layered on top of an already-small update).

**Conservative CSS baseline**, because the webview itself varies by platform (WebView2 on Windows, WKWebView on macOS, WebKitGTK on Linux — D2): flexbox and CSS custom properties (variables) for layout and theming, deliberately **no `:has()`, no container queries, no subgrid** — features that are broadly available on the newest engine versions but not reliably across the WebKitGTK version spread a Linux user's distro might actually ship. **WebKitGTK ≥ 2.44 is the documented floor** (Ubuntu 24.04/Debian 13-era), chosen because the app's own webview renders only an internal control panel — a drop zone, a queue, a settings form, a report viewer — none of which needs cutting-edge CSS, and because visual-fidelity validation of anything more demanding (the generated EPUB content itself) happens in CI against real engines (Chromium and WebKit via Playwright — D7), never inside the shipped app's own webview.

## 8. The Tauri boundary

The app is a thin Rust shell around the same `oc-core` library the CLI links (D13.1) — it does not reimplement pipeline logic, and it never does more than spawn, supervise, and relay events for the actual work:

- **The app spawns the CLI as a sidecar** (`externalBin`, D2), passing it **exactly one argument**: the path to a validated job-spec JSON file, written into an app-controlled directory (D13.2, closing the concern that a Tauri capability allow-list scoping arbitrary `--input`/`--output`/`--model-path` flags is only as meaningful as the argument validator behind it — RED_TEAM §B15). Everything the job needs — input path, output path, preset, thresholds overrides, model path, endpoint configuration — lives inside that one file, not on the command line.
- **The app owns `llama-server`.** Per D13.2's resolved ownership question: the desktop app supplies a long-lived, app-owned server instance so the model loads once per batch rather than once per conversion; the CLI, when run standalone (not via the GUI), spawns and owns its own short-lived instance instead. The GUI's job-spec points the engine at the app-owned endpoint via `--llm-endpoint`/`--llm-api-key-file`, so from the engine's point of view both modes are the same code path — it always speaks HTTP to *an* endpoint, it just does not always own the process behind it.
- **The model manager lives in Rust, in `oc-net`** — the one crate in the workspace permitted to open sockets (`SECURITY.md` §8) — not in the TypeScript/Svelte frontend. The UI layer only ever displays state `oc-net` reports and issues commands (`download`, `cancel`, `delete`) through Tauri's IPC; it never constructs a download URL or touches the filesystem's model directory directly.
- **Events flow one way into a UI store.** stderr NDJSON events from the engine subprocess (D13.2) are relayed by the app's shell-plugin integration into a Svelte store; every visible piece of job state in §2's information architecture — stage label, progress fraction, warnings, the final result — is a projection of that store, not independently tracked state the UI maintains in parallel. This is what makes "progress is never fake" (§1) an architectural property rather than a discipline someone has to remember to uphold in every component that shows a progress indicator.
- **Process supervision matches `SECURITY.md` §3 exactly** — the app is the one creating the Windows Job Object / Unix process group the engine and its own children run inside, and the one enforcing the 2-second cancellation deadline and 5-second hard-kill fallback described there. This document does not restate those mechanics; it only notes that the UI-visible cancel button and the security-model process-teardown guarantee are the same mechanism observed from two sides.

## 9. Wireframes

**Main window — empty state (drop zone):**

```
┌───────────────────────────────────────────────────────────┐
│  OpenConvert                                     [ ⚙ ]     │
├───────────────────────────────────────────────────────────┤
│                                                               │
│                 ┌─────────────────────────┐                 │
│                 │                         │                 │
│                 │     Drop PDF here        │                 │
│                 │                         │                 │
│                 │   or  [ Select PDF… ]    │                 │
│                 │                         │                 │
│                 └─────────────────────────┘                 │
│                                                               │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ AI-assisted structure detection is available, off    │   │
│  │ by default.        [ Set up AI assistance ] [Not now]│   │
│  └─────────────────────────────────────────────────────┘   │
└───────────────────────────────────────────────────────────┘
```

**Converting (queue with one active job):**

```
┌───────────────────────────────────────────────────────────┐
│  OpenConvert                                     [ ⚙ ]     │
├───────────────────────────────────────────────────────────┤
│  the-wheels-of-chance.pdf                                    │
│  Reconstructing  [██████████░░░░░░░░░░]  62%       [Cancel] │
│                                                               │
│  moby-dick.pdf                                    queued    │
│  malleus-maleficarum.pdf                          queued    │
└───────────────────────────────────────────────────────────┘
```

**Result panel (expanded):**

```
┌───────────────────────────────────────────────────────────┐
│  the-wheels-of-chance.pdf                          Complete │
│  → the-wheels-of-chance.epub                                │
│    [ Open in reader ]  [ Show in folder ]                   │
│                                                               │
│  214 pages · 1,842 blocks · 12 images · 0 tables · 11 chap. │
│  Deterministic processing · AI-assisted decisions: 4         │
│  Validation: passed with warnings (2)                        │
│    p.34  low-confidence dehyphenation kept as hyphenated     │
│    p.201 1 image below minimum resolution                    │
│                                                               │
│  [ Details ]  [ Preview ]  [ Edit metadata ]  [ Review TOC ] │
│  [ Export diagnostic bundle ]                                │
└───────────────────────────────────────────────────────────┘
```

**Settings — model manager:**

```
┌───────────────────────────────────────────────────────────┐
│  Settings                                          [ ✕ ]    │
├───────────────────────────────────────────────────────────┤
│  AI assistance          [●───]  On                          │
│                                                               │
│  Model manager                                               │
│  ┌───────────────────────────────────────────────────────┐ │
│  │ Qwen3-1.7B (default)  installed   1.1 GB   Apache-2.0  │ │
│  │   RAM: ~1.5–2 GB   CPU: moderate         [ Delete ]    │ │
│  ├───────────────────────────────────────────────────────┤ │
│  │ Qwen3-4B (quality)    not installed  2.4 GB Apache-2.0 │ │
│  │   RAM: ~3 GB   CPU: slower               [ Download ]  │ │
│  ├───────────────────────────────────────────────────────┤ │
│  │ Qwen3.5-2B (experimental)  not installed  1.3 GB       │ │
│  │   License: Apache-2.0            [ Download ]          │ │
│  └───────────────────────────────────────────────────────┘ │
│                                                               │
│  Provider:  (●) Built-in   ( ) Ollama (detected)             │
│             ( ) Custom endpoint…                              │
│                                                               │
│  Packs:  OCR pack [ not installed — Download ]                │
│          Validation pack [ not installed — Download ]         │
└───────────────────────────────────────────────────────────┘
```

**First-run (AI opt-in card, expanded):**

```
┌───────────────────────────────────────────────────────────┐
│  Set up AI assistance                              [ ✕ ]    │
├───────────────────────────────────────────────────────────┤
│  Downloads Qwen3-1.7B (Apache-2.0 license, shown below)      │
│  ~1.1 GB · ~1.5–2 GB RAM while converting · a few extra       │
│  seconds per book.  Conversion works fully without this.      │
│                                                               │
│  [ Apache License 2.0 — full text ▾ ]                        │
│                                                               │
│  [██████████████░░░░░░░░░░]  58%  412 MB / 1.1 GB  [Cancel] │
└───────────────────────────────────────────────────────────┘
```

---

## Notes for the Chief Architect

All three items previously open here have been ratified and are now stated as decisions in the body of this document; they are listed for the record.

1. **Partial re-run after a metadata/TOC edit is the intended v1 behaviour** (§2.3), not a full reconversion: an `overrides.json` write re-runs `document` → `epub` → `validate` → `repair` → `report` only, from the cached `structure` output.
2. **Quality presets `fast|balanced|thorough` are key-by-key threshold override maps** (§2.4), exactly the same mechanism as the document-type presets, resolving at the `preset` layer of D13.11's precedence chain.
3. **Svelte 5 is confirmed** (§7) as the UI framework.
