> **Record.** This is the prompt given to Claude Design on 2026-09-22 that produced `handoff/`. It is kept verbatim except for Appendix A, which in the prompt as sent was the full text of `docs/UI_UX.md` at commit `e6d02df`, byte for byte. It is not duplicated here; read `docs/UI_UX.md`.

# OpenConvert — Desktop UI: design system & mockup brief

You are designing the desktop UI of **OpenConvert**, an open-source, local-first desktop app (Windows, macOS, Linux) that converts PDF into high-quality, reflowable **EPUB 3.3**. Its principle: **Deterministic first. AI where necessary. Validate everything. Repair only what is broken.**

The UI has not been built yet. Its UX specification is finished and ratified. It is reproduced **verbatim in Appendix A and is binding**. Your job is the visual design, a design system, and high-fidelity mockups of that specification that can be implemented exactly as drawn. **You are not redesigning its information architecture, flows or rules.**

Conversation language: you may talk to me in Turkish. Every artifact (UI copy, component names, specs, the handoff document) is in English, plus German and Turkish where this brief asks for them.

---

## 0. How we work: three stages, and you stop after each one

**Stage 1: Explore (breadth).** Show **five** clearly different visual directions (four at minimum), with the same content on the same screens so they can be compared side by side. → **Stop and wait.** I pick 2–3.

**Stage 2: Deepen (2–3 directions).** Develop only the directions I pick, far enough that their real differences show: component states, the hard screens, dark mode, German/Turkish strings, accessibility. → **Stop and wait.** I pick one.

**Stage 3: Complete (1 direction).** Deliver the full design system and every screen, state and component in the §8 inventory, plus a handoff package (§9). It goes into the project repository's docs and is later implemented in Svelte 5, so it must be buildable exactly as drawn.

Never skip ahead to a later stage, and never merge directions unless I ask. If something in the spec is ambiguous or seems to conflict, ask me instead of resolving it silently.

---

## 1. The product in brief

- The user drops one or many PDFs. Each becomes a job in a queue. The engine converts **one job at a time**. When a job finishes, its row expands into a result panel. From there the user can open the EPUB, preview it, read a detailed conversion report, or fix metadata or the TOC and rebuild in a few seconds.
- Conversion runs **entirely on the user's machine**. There is no account, no cloud, no telemetry and no crash reporting. The only network activity is a download the user explicitly starts (a model or a pack), or an LLM endpoint the user configures.
- AI (a small local LLM) is **off by default** and optional. Deterministic conversion is the complete product. AI is an opt-in improvement to four narrow, once-per-book decisions.
- Audience: readers, students, researchers, archivists and digital-library volunteers. They care about getting a good book file, and many of them are not technical. A technical minority wants to audit what happened. Both are served: simple by default, with full detail one click away in the report.
- Tone: calm, precise, trustworthy and quietly competent. The app's personality is **honesty**. It never fakes progress, never hides a failure and never celebrates AI.

---

## 2. The binding spec in short (Appendix A wins on any conflict)

Every sentence of Appendix A is a requirement. These are the rules that are easiest to break visually:

1. **Honest progress.** Every progress bar, percentage and spinner is driven by a real engine event. A stage that reports a count shows a determinate bar, and it may also show the count with its unit, e.g. "132 of 214 pages". A stage with no count shows an indeterminate indicator. **A bar never creeps forward on a timer.** During an indeterminate stage, a subtle "still working" pulse driven by the heartbeat shows the job is alive. Animating between two real values is fine, and the change is instant under reduced motion. Inventing values in between is not.
2. **Only user-facing stage labels:** Analyzing → Extracting → Reconstructing → Building → Checking → (Repairing, shown only when it actually runs, which is rare) → Complete. Engine stage names (`inspect`, `ingest`, …) never appear in the main UI.
3. **No AI gimmicks.** AI involvement appears as a plain count ("AI-assisted decisions: 4"). It sits *next to* "Deterministic processing", never above it, and only on jobs where AI was actually enabled. No sparkles, magic wands, glowing gradients, robot or assistant avatars, "AI is thinking…" copy, or anthropomorphizing of any kind.
4. **Validation is never softened.** There are three states: passed / passed with warnings / invalid. Each is shown equally plainly. An invalid EPUB is still saved and can still be opened, and the UI says "invalid" in plain words.
5. **Errors are never generic.** "Conversion failed" on its own is never acceptable. Every condition in Appendix A §4 gets its own specific response.
6. **The first run is drop-and-done.** There is no modal on launch. The drop zone works immediately, and the AI opt-in is a small dismissible card, never a gate.
7. **Settings are optional.** Nothing on the default path requires opening them. Raw threshold values never appear on the main settings surface.
8. **Cancel always visibly does something:** "Cancelling…" and then "Cancelled", within 2 s (with a 5 s hard fallback).
9. **The row-expansion model:** a completed job's queue row expands into its result panel. Keep this information architecture: drop zone → queue → result, with settings to the side.
10. **Accessibility & i18n:**
    - Full keyboard operation, with a logical focus order (drop zone → queue → settings).
    - Explicit ARIA labels.
    - Progress is announced as stage-label changes, not as a stream of numbers.
    - Light and dark themes follow `prefers-color-scheme`, and `prefers-reduced-motion` is honored.
    - OS font scaling is respected (`rem` units).
    - Languages: English, German, Turkish. No RTL.

**About the spec's cross-references:** a few section numbers inside Appendix A are wrong.
- "First-run design (§5)" means §3.
- "the full conversion report viewer (§7)" means §5.
- "§8's warning-template requirement" means §6.

Tags such as D13.2, R6 §3.3 or RT B15 point to internal decision records. You don't need them.

---

## 3. Extra binding requirements from project documents that outrank the UI spec

These come from the implementation plan and the architecture decision record, both of which rank above Appendix A. Where they add something, design both. Where a number differs, use the one in this section.

- **Not responding:** if no heartbeat arrives for **more than 6 s**, the row switches to a "not responding" state, which is distinct from a stage that is merely slow.
- **Result panel headline quality facts:** in addition to the spec's lines, the result panel shows:
  - character retention
  - image parity ("12 of 12 images")
  - footnote linkage ("38 of 38 notes linked")
  - EPUBCheck status: shown when the optional validation pack is installed, otherwise "not run"
- **The first run is one screen:** the first-run state (drop zone + card) also says plainly: no telemetry, no network on the conversion path, what the optional download costs in megabytes, and what it buys. It stays non-blocking, with no modal.
- **The queue at scale:** dropping 40 PDFs creates 40 jobs. Exactly one runs and the rest show their position in the queue. Every job, queued or running, can be cancelled or removed.
- **Blocking startup errors** are the only acceptable blocking screens. They cover two cases:
  - (a) the engine speaks a different protocol version;
  - (b) the version of the bundled engine differs from the app's.

  Show a clear message, not a half-working, degraded UI.
- **Stale corrections are refused.** If saved metadata/TOC corrections were made for a different engine IR version, the app refuses to apply them and says so clearly. It never ignores them silently.
- **Rebuild after edits:** "Edit metadata" / "Review TOC" → "Fix and rebuild" re-runs only the late stages, from cache. The rebuild progress shows only the stages that actually run (Reconstructing → Building → Checking → Complete) and takes seconds. Frame it as "fix and rebuild", never as "start over".
- **Custom LLM endpoint:**
  - The fields are the base URL, the model name, and an **API key file**. The key file is chosen with a file picker: the key is read from a file and is never typed into a text field or stored in the config.
  - A non-loopback host requires the consent dialog. The dialog names the host and says plainly that document text will leave this computer.
  - When Ollama is detected on this machine, the UI lists its installed models to choose from.
- **Additions to the report viewer.** Leave room for:
  - the consent record (host + time) when a non-loopback endpoint was used
  - the document classification and the preset that was chosen automatically
  - a note when the PDF had owner-password restrictions
  - a note when AI was enabled but unavailable
  - the list of pages kept as images because OCR was not available
  - a Linux sandbox status line (e.g. "Sandbox: Landlock applied" / "not supported on this kernel")
- **Model manager rows render exactly the engine's `ModelReadiness` fields:** installed status, size, RAM estimate, CPU expectation, license, and the path to the license file. The UI layer invents nothing.
  - There are five rows: Qwen3-0.6B (low RAM / fast), **Qwen3-1.7B (default)**, Qwen3-4B (quality), Qwen3.5-2B and Qwen3.5-4B (experimental).
  - The license is always shown, and an explicit click to accept it is always required before the first download.
- **The OCR pack may not ship in v1.0.** It is deferred until its build and signing recipe is proven, and v1 uses a system-installed Tesseract when one is present. Its row needs a "not available in this version" state as well as the normal states.
- **Warnings are template-driven.** The engine never sends prose, only a code and its arguments, which are rendered through localized templates. Severity is info / warn / error. Design the warning line as a template-driven component, not as free text.

---

## 4. Implementation constraints: the mockups must be buildable exactly as drawn

- **Stack:** a Tauri 2 desktop shell with Svelte 5 + TypeScript. The webview differs by OS: WebView2 on Windows, WKWebView on macOS, **WebKitGTK ≥ 2.44** on Linux (the lower bound).
- **CSS baseline (from the spec):**
  - Use flexbox and CSS custom properties for layout and theming.
  - **No `:has()`, no container queries, no subgrid.**
  - Also avoid effects that look different on WebKitGTK, such as looks that depend on `backdrop-filter`/blur, or heavy shadows used as structure.
  - Size everything in `rem`.
- **Everything is bundled; nothing is remote.**
  - The app's Content-Security-Policy blocks network access from the UI: no CDN fonts, no remote icons or images, no embeds.
  - Prefer the platform's system font stacks, which also respect OS text-size settings.
  - If a direction depends on a bundled typeface, name it and its license and flag it. The project's dependency allow-list is MIT, Apache-2.0, BSD-2/3, ISC, MPL-2.0, Unicode, Zlib and CC0. Anything else (including OFL-1.1) needs a maintainer decision.
  - Icons come from an open set under an allowed license (e.g. Lucide/ISC, Tabler/MIT, Phosphor/MIT), used as inline SVG.
- **Neutral across platforms.** It should feel at home on Windows, macOS and Linux, and must not be a macOS imitation. Assume native window decorations. The app's header row ("OpenConvert" + settings) sits inside the window.
- **Accessibility targets (these are CI checks):**
  - Zero serious/critical axe violations on the queue, result, report and settings screens.
  - WCAG 2.2 AA text contrast in both themes.
  - A visible focus indicator on every interactive element, at ≥ 3:1 against its surroundings.
  - Pointer targets of at least 24×24 px.
  - Information is never conveyed by color alone: validation and warning states always carry text and/or an icon as well.
- **Localization stress:**
  - German runs about 30 % longer and has long compound words. Turkish has a dotted and a dotless i (i/İ, ı/I).
  - Don't rely on `text-transform: uppercase` or letter-spaced all-caps labels, because Turkish casing breaks unless it is handled.
  - Layouts must carry the longest string in each language without losing meaning to truncation.
- **Dimensions:**
  - Mock at 1024×720 logical px.
  - Also propose and show the smallest supported window size (around 720×520, or justify another).
  - Check the main screens at 200 % OS text size.
- **Output format:** plain HTML + CSS with custom properties. No Tailwind or other utility-framework-only output, and no CDN. Class and component names should map cleanly to Svelte components.

---

## 5. Sample content (the same content in every direction, so they can be compared)

**Queue:**
1. `the-wheels-of-chance.pdf` — Reconstructing, 62 % (132 of 214 pages) — [Cancel]
2. `moby-dick.pdf` — queued (#2)
3. `malleus-maleficarum.pdf` — queued (#3)

**Result (the spec's example):** `the-wheels-of-chance.pdf` → `the-wheels-of-chance.epub`
- 214 pages · 1,842 blocks · 12 images · 0 tables · 11 chapters
- Deterministic processing (AI off, which is the default, so there is no AI line)
- Validation: passed with warnings (2)
  - p. 34: Low-confidence hyphenation join kept as hyphenated
  - p. 201: 1 image below minimum resolution
- Headline facts: character retention 99.6 % · 12 of 12 images · 38 of 38 notes linked · EPUBCheck: not run (validation pack not installed) *(placeholder numbers)*
- AI-on variant: "Deterministic processing · AI-assisted decisions: 4"

**Other jobs, to demonstrate states:**
- `die-verwandlung.pdf` (German) — Complete, passed
- `araba-sevdasi.pdf` (Turkish) — Complete, **invalid** (shown plainly; the file is still saved)
- `parish-register-1871.pdf` — completed; 48 scanned pages kept as images because Tesseract is not installed, with a copy-pasteable install command, e.g. `brew install tesseract tesseract-lang` / `sudo apt install tesseract-ocr tesseract-ocr-deu tesseract-ocr-tur` / on Windows, the UB-Mannheim installer named as text (the app never opens a URL itself)
- `annual-report-locked.pdf` — encrypted, with an inline password prompt on the row
- `download (3).pdf` — "This file doesn't look like a valid PDF" + Export diagnostic bundle (no retry button)
- `world-atlas-hires.pdf` — "This document exceeds the memory limit for a single conversion", suggesting that the limit be raised in Advanced

**Model manager** *(placeholder values; the real ones come from `ModelReadiness`)*:

| Model | Status | Size | RAM | CPU | License |
|---|---|---|---|---|---|
| Qwen3-0.6B (low RAM) | not installed | ~0.5 GB | ~1 GB | fast | Apache-2.0 |
| Qwen3-1.7B (default) | installed | 1.1 GB | ~1.5–2 GB | moderate | Apache-2.0 |
| Qwen3-4B (quality) | not installed | 2.4 GB | ~3 GB | slower, higher quality | Apache-2.0 |
| Qwen3.5-2B (experimental) | not installed | 1.3 GB | — | — | Apache-2.0 |
| Qwen3.5-4B (experimental) | not installed | ~2.6 GB | — | — | Apache-2.0 |

Example CPU expectation string: "first call ~5–15 s on a 4-core laptop".

**Sample warning lines (EN, draft):**
- info — "3 tables were kept as images because their structure could not be recovered reliably."
- warn — "Pages 1–48 are scanned images. OCR isn't installed, so they were kept as images."
- warn — "Heading levels jump from 1 to 3 on p. 88."
- warn — "1 footnote on p. 57 has no matching reference in the text."
- warn — "The EPUB is larger than 50 MB; some readers may open it slowly."
- error — "The EPUB did not pass validation after 3 repair attempts. It was saved and marked invalid."

**Draft translations** *(for layout stress-testing only; the final strings come from the locale tables and a native review)*:

| EN | DE | TR |
|---|---|---|
| Drop PDF here | PDF hier ablegen | PDF'yi buraya bırakın |
| or Select PDF… | oder PDF auswählen… | veya PDF seçin… |
| Analyzing | Wird analysiert | Analiz ediliyor |
| Extracting | Wird extrahiert | Ayıklanıyor |
| Reconstructing | Wird rekonstruiert | Yeniden yapılandırılıyor |
| Building | Wird erstellt | Oluşturuluyor |
| Checking | Wird geprüft | Denetleniyor |
| Repairing | Wird repariert | Onarılıyor |
| Complete | Fertig | Tamamlandı |
| Queued | In Warteschlange | Sırada |
| Not responding | Reagiert nicht | Yanıt vermiyor |
| Cancel / Cancelling… | Abbrechen / Wird abgebrochen… | İptal / İptal ediliyor… |
| Open in reader | Im Reader öffnen | Okuyucuda aç |
| Show in folder | Im Ordner anzeigen | Klasörde göster |
| Deterministic processing | Deterministische Verarbeitung | Deterministik işleme |
| AI-assisted decisions: 4 | KI-gestützte Entscheidungen: 4 | Yapay zekâ destekli kararlar: 4 |
| Validation: passed with warnings (2) | Validierung: bestanden, mit Warnungen (2) | Doğrulama: uyarılarla geçti (2) |
| Validation: invalid | Validierung: ungültig | Doğrulama: geçersiz |
| Approximate preview — your reading device may differ. | Ungefähre Vorschau – Ihr Lesegerät kann abweichen. | Yaklaşık önizleme — okuma cihazınız farklı görünebilir. |
| Set up AI assistance / Not now | KI-Unterstützung einrichten / Nicht jetzt | Yapay zekâ desteğini kur / Şimdi değil |
| Export diagnostic bundle | Diagnosepaket exportieren | Tanılama paketini dışa aktar |

---

## 6. Stage 1 deliverables: five directions

For **each** direction:
- **Name + concept (3–4 sentences):** who it's for, how it expresses honesty and calm, and what sets it apart.
- **Mini token sheet:** color palette (light + dark), type scale, spacing, radius, how elevation and separation are handled, and the icon set.
- **Three screens in light mode, with identical content in every direction:**
  1. The empty state with the first-run AI card
  2. Converting: a queue with 1 active job + 2 queued
  3. The result panel expanded (passed with warnings)
- **The same result panel in dark mode.**
- **A one-line risk note** (contrast, WebKitGTK, German string length, …).

Stage 1 can be mid-fidelity, but typography, color and density must be real.

The five must be genuinely different. Vary at least the color strategy, typography, density, shape language and how progress is visualized, while every direction obeys the spec. None should look like a generic SaaS dashboard template. You may use, remix or replace these starting territories:

- **Native utility:** feels like a first-party tool of the operating system. Neutral, restrained, system font, disappears into the work.
- **Typeset:** editorial and book-derived. Careful typographic rhythm, a paper-toned light theme and an ink-toned dark theme. The respect for books is visible.
- **Instrument:** precise and measurement-minded. Tabular figures, thin rules, a ledger-like report, and progress as a real measurement.
- **Quiet workshop:** warm, tactile, calm and softly rounded. Welcoming to readers who aren't technical.
- **Archive / catalog:** high contrast, accessibility first, strong type, cues taken from library catalog cards.

**End Stage 1** with a compact comparison table covering, for each direction: how it expresses honest progress, AI restraint, accessibility risk, i18n risk, WebKitGTK risk, and personality. Add your recommendation. **Then stop.**

---

## 7. Stage 2 deliverables: the 2–3 chosen directions (light + dark)

- **Tokens** expanded into semantic tokens (surface / text / border / accent / focus / status / progress).
- **Core components with every state:**
  - drop zone (idle, drag-over with a valid PDF, drag-over including a non-PDF, keyboard focus)
  - queue row (queued, running determinate, running indeterminate + heartbeat pulse, not responding, cancelling, cancelled, failed, needs password, complete)
  - stage/progress indicator
  - validation line (3 states)
  - warning line (info / warn / error, with page link)
  - buttons (primary / secondary / quiet / destructive / icon)
  - toggle, radio, text field, banner, card
- **The hard screens:**
  - the result panel in all 3 validation states, plus the AI-on variant
  - the conversion report viewer
  - settings, with the model manager showing one row downloading
  - the custom-endpoint consent dialog
  - two failure rows (unparseable PDF; encrypted PDF + inline password)
- **One screen (queue + result) in German and one in Turkish.**
- **Accessibility notes:** measured contrast of the key color pairs, the focus ring spec, and what the screen reader announces when the stage changes.

**End Stage 2** with a comparison and your recommendation. **Then stop.**

---

## 8. Stage 3: full inventory (the single chosen direction)

### 8.1 Foundations
- Semantic color tokens, light + dark: background, surface, raised surface, border, text (primary/secondary/tertiary), accent, focus ring, status (success/warning/danger/info), progress track/fill, disabled
- Type scale (rem), with tabular-figure usage defined for numbers
- Spacing scale, radii, separation/elevation rules
- Motion tokens and a reduced-motion equivalent for each one
- Icon set (with license) and icon sizes
- Focus indicator spec

### 8.2 Components (every state: default, hover, active, focus-visible, disabled; loading where relevant; light + dark)
- App header (title + settings button)
- Drop zone: idle / drag-over, valid / drag-over, includes a non-PDF / keyboard focused
- Buttons: primary, secondary, quiet, destructive, icon
- **Queue row:** queued (with position) · running determinate · running indeterminate + heartbeat pulse · not responding · cancelling · cancelled · each failure type · needs password (inline prompt) · complete, collapsed · complete, expanded (= result panel) · rebuilding (partial re-run)
- Stage sequence + progress: determinate, indeterminate, count + unit, Repairing appearing only when it runs
- Validation line: passed / passed with warnings / invalid
- Processing line: "Deterministic processing" and "Deterministic processing · AI-assisted decisions: N"
- Headline quality facts (retention, image parity, note linkage, EPUBCheck)
- Warning line: info / warn / error, page link, disabled link when no preview is available
- Non-modal banner (AI unavailable this run; model fallback notice)
- Copyable command notice (OCR install), with a copy button and a "copied" state
- First-run card: collapsed / expanded (license, download progress, Cancel)
- Dialogs: non-loopback consent (naming the host) · license acceptance · clear-cache confirmation (warning that the cache holds document text) · blocking startup error
- Form controls: toggle, radio group, segmented control / select (presets), text field, password field, file-picker field (API key file), number fields with units (threads, max pages, max memory), inline validation message
- **Model/pack row:** not installed · license shown, awaiting acceptance · downloading (real progress + Cancel) · download failed (Retry, resumes) · installed (Delete) · incompatible → fell back to another tier · "default" and "experimental" tags · not available in this version (OCR pack)
- Settings section navigation, table (ledger, timing), tooltip, empty states

### 8.3 Screens and flows
1. Main window: empty state + first-run card; empty state after the card is dismissed
2. Drag-over states
3. Queue: 1 running + 2 queued; **queue of 40 files** (scrolling, positions, cancelling a queued job)
4. Every running state (determinate, indeterminate, not responding, cancelling → cancelled)
5. Result panel: passed · passed with warnings · invalid · AI on · AI enabled but unavailable (banner)
6. Every row in the error table (Appendix A §4): unparseable · encrypted → password · OCR unavailable (completed, with images) · font missing · image extraction failed · LLM unavailable · model download failed · model incompatible · validation failed / repair failed (invalid) · memory/resource limit · cancelled
7. **Conversion report viewer** (all of Appendix A §5 plus the §3 additions of this brief): the AI-off version and the AI-on version; EPUBCheck run / not run
8. **Preview window:** the "Approximate preview — your reading device may differ." label always visible; chapter/TOC navigation; jumping to a page from a warning link
9. **Edit metadata** (title, author, language; the IR stores authors as a list, so the author field must be able to hold more than one name) → Fix and rebuild → partial rebuild progress → updated result; the stale-corrections refusal state
10. **Review TOC** (rename a heading, change its level; flag any other operation as a proposal) → Fix and rebuild
11. **Export diagnostic bundle:** the user picks a location in the native save dialog → a review screen showing what is in the bundle before sharing (placeholder contents: conversion report, engine event log, app/engine versions, OS info). Nothing is ever sent automatically.
12. **Settings:**
    - AI assistance (the explanatory card on first opening + a single toggle)
    - Model manager
    - Provider (Built-in / Ollama (detected) + model selection / Custom endpoint → consent dialog)
    - Presets (document type auto/novel/academic/textbook/poetry/scanned + quality fast/balanced/thorough; raw values never shown)
    - Advanced (thread count, max pages, max memory, **Clear cache**, Dump stages, document-language override, a PDF password field that is kept only for the duration of the job and never saved)
    - Packs (OCR, validation)
13. **First-run AI setup:** license → one-click download with real progress → Cancel → installed
14. **Blocking startup errors:** protocol mismatch; stale engine version
15. **Cross-cutting:**
    - light + dark for everything
    - the main flows in DE and TR
    - main + result screens at 200 % text size and at the minimum window size
    - a keyboard focus-order map
    - screen-reader annotations (ARIA labels, live-region announcements)
    - reduced-motion variants

---

## 9. Handoff package (this goes into the repository)

1. **`tokens.css`:** every token as a CSS custom property, in rem. Light values go on `:root`, dark values under `@media (prefers-color-scheme: dark)`.
2. **A spec for each component:** anatomy, states, tokens used, keyboard behavior, ARIA role / label / live region.
3. **A screen map:** every screen mapped to the planned Svelte routes: `queue`, `result`, `report`, `preview`, `settings`, `models`, `firstrun`.
4. **A motion spec:** each animation with its reduced-motion equivalent.
5. **A string inventory:** every visible string with a proposed key (EN), plus the DE/TR drafts used in the mockups.
6. **A decisions list:** every decision you made that the spec didn't dictate, and every open question.
7. **The HTML/CSS prototype files.**

**Open items the spec doesn't settle.** Propose something for each, clearly labeled as a proposal:
- Where the UI language selector lives (EN/DE/TR; default follows the OS). *(This is separate from the document-language override in Advanced.)*
- The output file already exists (by default the output goes next to the input, and overwrite is off by default).
- "Open in reader" when the OS has no EPUB reader.
- An "Update available" notice (the app has a signed auto-updater, except in the Flatpak build, which Flathub updates).
- A way to view the local network audit log (every outbound connection the app makes is logged locally, so a user can verify "no network unless I asked").
- A global "Report a problem" entry point, in addition to the per-job "Export diagnostic bundle".

---

## 10. Do not

- Show fake or timer-driven progress, or a spinner that has no event behind it.
- Use AI glitter: sparkles, magic wands, gradient glows, assistant avatars, "thinking" narration, or AI placed above deterministic processing.
- Add a cloud provider quick-pick (no "OpenAI" or "Anthropic" button), any account or sign-in, cloud sync, a telemetry/analytics opt-in, or an upsell.
- Show a modal on first launch, or require a setup wizard before the first conversion.
- Write "Conversion failed" with no specific reason. Offer a retry button for a file that can't be parsed.
- Add a block-level editing UI (out of v1), an RTL layout, or a page-level classification editor.
- Use a remote font, icon, image or script.
- Use `:has()`, container queries or subgrid, or ship a look that only works with `backdrop-filter`.
- Show engine-internal stage names in the main UI, or raw threshold numbers on the main settings surface.
- Soften or hide an `invalid` validation result.

---

## Appendix A: UI/UX specification (`docs/UI_UX.md`, verbatim, authoritative)

*[The full text of `docs/UI_UX.md` at commit `e6d02df`, verbatim. Not duplicated here.]*

---

## Appendix B: Known engine warning codes (the enum grows; each one is rendered from a localized template, code + arguments)

W_LOW_RETENTION · W_TABLE_AS_IMAGE · W_UNMAPPED_VALIDATION_ID · W_STYLE_INVENTORY_INVALID · W_CAPTION_AMBIGUOUS · W_LANG_UNSTABLE · W_IMAGE_ONLY_PAGES · W_REPAIR_FIRED · W_ORNAMENT_DROPPED · W_EPUB_INVALID · W_ZONES_OUT_OF_ORDER · W_XHTML_OVERSIZE · W_VALIDATION_UNREPAIRABLE · W_SECTION_PAGES_NOT_MONOTONE · W_REPAIR_OSCILLATION · W_NO_TEXT_EXTRACTED · W_DUPLICATE_BLOCKS · W_PAGE_BREAK_UNPLACED · W_NOTE_UNMATCHED · W_LIST_NUMBERING_GAP · W_HEADING_LEVEL_SKIP · W_HEADING_COUNT_IMPLAUSIBLE · W_HEADINGS_OUT_OF_PAGE_ORDER · W_EPUB_LARGE · W_BROKEN_TEXT_PAGES · W_LANG_FALLBACK · W_OCR_LOW_CONFIDENCE · W_OCR_ENGINE_MISSING · W_OCR_LANG_MISSING · W_LLM_UNCONSTRAINED · W_LLM_PREFIX_COLD · W_LLM_BUDGET_EXHAUSTED · W_COMPLEX_LAYOUT · W_CMYK_NAIVE

Severity: `info` | `warn` | `error`. Each warning can carry a page reference; the UI turns it into a page link where the preview is available.
