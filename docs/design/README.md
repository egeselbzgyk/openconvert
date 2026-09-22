# OpenConvert desktop UI: design system and mockups

**Status:** adopted for Phase 12 on 2026-09-23. Direction: "2b Quiet workshop".
**Source:** Claude Design project "Openconvert UI mockups" (`9471727b-d683-4a03-bc2a-087e1f62eec8`), folder `handoff/`.
**Produced from:** [`brief.md`](brief.md), the prompt given to Claude Design. The handoff's references to "brief §3 / §8 / §9" point there.

## Authority

- **Behaviour** is governed by `docs/DECISIONS.md` → `docs/IMPLEMENTATION_PLAN.md` (Phase 12) → `docs/UI_UX.md`. They win on any conflict.
- **Appearance** is governed by this package: tokens, component styles, class names, layouts, states and EN copy. `IMPLEMENTATION_PLAN.md` § "PHASE 12 → Visual design (binding)" says how it is ported.
- The design's own decisions and its proposals for the open items (`handoff/docs/decisions.md`) are the default. Overturning one is recorded in `docs/DECISIONS_LOG.md`.

## What is in `handoff/`

| File | What it is | Ships in the app? |
|---|---|---|
| `tokens.css` | Every design token as a CSS custom property, in rem. Light on `:root`, dark under `prefers-color-scheme`, plus reduced-motion values. | Yes, as `apps/desktop/ui/src/styles/tokens.css`. The `[data-theme]` blocks are gallery-only. |
| `oc.css` | One block per Svelte component, BEM classes, flexbox only. No `:has()`, container queries, subgrid or `backdrop-filter`. | Yes, as `styles/oc.css`. The `.is-hover` / `.is-active` / `.is-focus` mirrors are gallery-only. |
| `icons.svg` | Lucide icon sprite (ISC) + the two logo symbols. | Yes, inlined at build time (see below). |
| `logo/*.svg` | Brand mark (light, dark, small) and app icon (normal, small). | Yes. `app-icon.svg` is the source for `src-tauri/icons/`. |
| `docs/components.md` | Component specs: anatomy, states, tokens, keyboard, ARIA. | Spec. |
| `docs/screen-map.md` | Route → mockup map (`queue`, `result`, `report`, `preview`, `settings`, `models`, `firstrun`). | Spec. |
| `docs/motion.md` | Every animation, the real event behind it, and its reduced-motion equivalent. | Spec. |
| `docs/strings.md`, `strings/*.json` | String inventory: keys, EN, DE/TR drafts. | EN seeds `locales/en.json`. DE/TR are drafts (see below). |
| `docs/decisions.md` | Decisions the spec didn't dictate, proposals for its open items, and resolved questions. | Spec. |
| `index.html`, `design-system.html`, `queue.html`, `result.html`, `report.html`, `settings.html`, `firstrun.html`, `i18n.html`, `scaled.html` | The mockups. | No, reference only. |
| `gallery.css`, `gallery.js` | Gallery chrome: frames, dark mirroring, locale swapping. | No. |

**Viewing the mockups.** `gallery.js` fetches `icons.svg` and `strings/*.json`, and browsers block `fetch()` from `file://`. Serve the folder instead, then open `http://localhost:8000/`:

```
cd docs/design/handoff
python -m http.server
```

## Known disagreements with the spec: fix them when porting

All of these are placeholder values in the mockups. The right-hand column is the source of truth.

| Mockup says | Source of truth |
|---|---|
| Max pages "2,000" (`queue.html` §5, `settings.html` Advanced, `design-system.html` forms) | Default 3,000: `limits.max_pages` in `thresholds.toml` (D13.2, SECURITY §4). Render it from thresholds and never hard-code it. |
| Protocol "3" / "2", IR "3" / "4" (startup errors, stale corrections, About, report) | Today protocol `v: 1` and `ir_version = 1`. Render from the `hello` event and the overrides file. |
| "EPUBCheck 5.1" (`report.html`) | Whatever version the installed validation pack reports. CI pins 5.3.0. |
| Validation pack "~24 MB", licence "BSD-3-Clause" (`settings.html` Packs) | About 40–50 MB, because it bundles a jlink'd JRE plus `epubcheck.jar` (D6, LICENSE_AND_DEPENDENCIES §6). The row must show the JRE's licence (GPLv2 + Classpath Exception, per vendor; still to be verified) next to EPUBCheck's BSD-3-Clause. The row renders the pack registry, not constants. |
| Flatpak id `org.openconvert.OpenConvert` (`firstrun.html`) | `io.openconvert.OpenConvert` (Phase 15 packaging files). |
| Converter path `/opt/openconvert/oc-cli` (`firstrun.html`) | The engine binary is `openconvert` (the sidecar). Show the running install's actual path. |
| Update host `releases.openconvert.org` (`settings.html` Network log) | The updater manifest is a static JSON on GitHub Releases (D12). Open item: the Tauri updater is not `oc-net`, so whether its requests reach the network audit log (SECURITY §8) is still to be decided in Phase 12/15. |
| AI assistance card: "four once-per-book decisions" followed by three (`settings.html`) | D13.6's four tasks: metadata, heading roles, book structure, verse/quote. The copy must name all four. |
| Warning keys `W_DEHYPHEN_LOWCONF`, `W_IMAGE_LOWRES` (`strings/*.json`, `docs/strings.md`) | Illustrative only; they are not engine `WarningCode`s today. Locale tables are keyed by the real enum, and every code needs a template in every locale (Phase 12 test 12.9). |
| `strings/de.json`, `strings/tr.json` hard-code sample values in some keys (`queue.count`, `queue.queued`, `queue.stepOf`, `result.aiDecisions`, `validation.warn`, `warn.W_IMAGE_LOWRES`) and carry `x.*` keys | These are gallery fixtures. The contract is the `{placeholder}` form in `docs/strings.md` / `strings/en.json`, and the `x.*` keys are dropped. |
| Model sizes, RAM, CPU strings, cache size, all counts | Placeholders. Model rows render `ModelReadiness` (Phase 9). Everything else comes from engine events or the report. |

## Porting notes

- **CSP.** The app runs under `default-src 'self'; connect-src 'none'`.
  - The gallery's `fetch('icons.svg')` would be blocked, so inline the sprite at build time, as a raw import or an `Icons.svelte` component.
  - Locale tables are bundled, never fetched.
  - The mockups' inline `style="…"` attributes are gallery shortcuts. They become classes in `oc.css`.
  - Dynamic values (progress width, timeline segment flex) go through Svelte's `style:` directive, which writes through the CSSOM rather than a `style` attribute. Verify under the shipped CSP.
- **Colours.** `oc.css` has two literals, the preview page's paper and ink (`.oc-preview__page`, deliberately the book's own light colours in both themes). Move them into tokens so every colour lives in `tokens.css`.
- **Licences.** Lucide is ISC. Add its notice to the app's third-party notices. Fonts are system stacks only, so there is nothing to license.
- **Translations.** Turkish is reviewed by the maintainers. German needs one native reviewer before 1.0.

## Provenance of this import

- Every file under `handoff/` was copied byte for byte from the design project, with one exception. The six SVGs (`icons.svg` and `logo/*.svg`) carried an embedded C2PA content-credentials manifest (`<metadata><c2pa:manifest>…</c2pa:manifest></metadata>` plus an `xmlns:c2pa` attribute), signed over the file's original bytes. The import path used here cannot reproduce that block byte for byte, and a transcribed copy would carry an invalid signature, so it was removed. The drawing content is unchanged. The design project remains the source of record for the signed originals.
- Not imported: the exploration canvases (`OC-1a…1e`, `OC-2-*`, `OpenConvert Stage 1`, `OC-Swatches`, `OpenConvert Logo` as `.dc.html`), `oc-strings.js` and `support.js`. They are in the design tool's own format and are not part of the handoff (`handoff/docs/decisions.md` #1).
