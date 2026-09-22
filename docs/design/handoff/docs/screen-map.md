# Screen map

| Route | Screen / state | Mockup |
|---|---|---|
| `queue` | Empty + first-run card; empty after "Not now" | queue.html §1 · i18n.html (DE, TR) |
| `queue` | Drag-over: all PDFs; includes non-PDFs | queue.html §2 · design-system.html (DropZone) |
| `queue` | 1 running + 2 queued; 40 files (scrolling, positions, removing a waiting job) | queue.html §3 |
| `queue` | Running: determinate, indeterminate + heartbeat, not responding, cancelling → cancelled | queue.html §4 |
| `queue` | Every error-table row: unparseable, too damaged, encrypted → password, memory/page limit, cancelled, OCR unavailable (+ copy command), font missing, image failed, AI unavailable, model fallback | queue.html §5 |
| `result` | Expanded row: passed · passed with warnings · invalid · AI on · AI enabled but unavailable | result.html §1 |
| `result` | Edit metadata → Fix and rebuild → partial rebuild → updated result · stale corrections refused | result.html §2 |
| `result` | Review TOC (rename, level; other changes saved as proposals) → Fix and rebuild | result.html §3 |
| `result` | Export diagnostic bundle review (after the native save dialog) | result.html §4 |
| `result` | Proposals: output file exists; no EPUB reader | result.html §5 |
| `report` | AI off + EPUBCheck not run · AI on + EPUBCheck ran + every conditional note | report.html §1–2 |
| `preview` | Chapter navigation, fixed approximate label, jump from a warning link | report.html §3 |
| `settings` | AI assistance, Provider, Presets, Advanced, Packs; proposals: Language, Network log, About & updates | settings.html |
| `settings` | Dialogs: non-loopback consent, license acceptance, clear cache | settings.html (Dialogs) |
| `models` | Model manager (one row downloading); every ModelRow state | settings.html · design-system.html |
| `firstrun` | License → download → Cancel → installed | firstrun.html §1 |
| — (pre-route) | Blocking startup errors: protocol mismatch; engine version mismatch | firstrun.html §2 |
| all | DE and TR flows; 200 % text; 720 × 520 minimum; focus order; SR; reduced motion | i18n.html |

Dialogs are overlays on their route, not routes. `firstrun` is `models` opened at the default tier from the first-run card, with the queue still running underneath.
