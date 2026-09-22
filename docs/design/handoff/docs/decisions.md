# Decisions and open questions

## Decisions made in the design (the spec doesn't dictate them; each can be overturned)

1. **Output format.** The handoff is plain HTML + CSS (`tokens.css`, `oc.css`) with BEM-style class names that map to Svelte components. There are no utility classes and nothing loads from a CDN. The earlier exploration canvases were built in the design tool's own component format and are not part of the handoff.
2. **Typeface.** System stack only: `ui-rounded` (macOS) → Segoe UI (Windows) → system-ui → Cantarell / Noto Sans (Linux). Nothing is bundled, so there is no license question. The mono stack is also system-only.
3. **Icons.** Lucide (ISC), inline SVG symbols in `icons.svg`, stroke 1.75.
4. **Drop zone once jobs exist.** It becomes a strip above the queue, so the focus order stays drop zone → queue → Settings.
5. **Settings focus order.** The header is last in the DOM and drawn first with `order:-1`. No positive tabindex anywhere.
6. **Queue position.** "Queued (#2)" counts the running job as #1, as in the brief's sample.
7. **Not responding.** Offers only Cancel. The row recovers on its own when heartbeats return.
8. **Cancelled rows.** Offer "Convert again" and Remove. Unparseable files never offer a retry.
9. **Mixed drops.** PDFs are added; each skipped file is named. The whole drop is never rejected.
10. **"Not now".** Hides the first-run card for the session. It does not return to the main window on later launches; the explanation lives at the top of Settings › AI assistance. *(Please confirm; the spec wording can be read both ways.)*
11. **Download cost wording.** The card uses MB ("~1,100 MB download") as brief §3 requires, instead of Appendix A's "~1 GB".
12. **Consent dialog.** Opens with focus on Cancel. The allow button names the host.
13. **Password.** The inline row prompt and the Advanced field both say "Used for this job only, never saved".
14. **Bulk removal.** "Remove all waiting…" asks for confirmation once. Removing a single waiting job does not.
15. **Minimum window.** 720 × 520.
16. **Stale corrections.** Refused with a warning banner. The old overrides file is kept and linked so nothing is lost.
17. **Preview page.** Renders in the book's own light colors in both app themes.
18. **Numbers.** Formatted with `Intl.NumberFormat` per locale (TR "%62", DE "1.842").

## Proposals for the open items in brief §9

| Item | Proposal | Where |
|---|---|---|
| UI language selector | Settings › Language: "System (English)" + English / Deutsch / Türkçe written in their own language; applies without restart | settings.html |
| Output file exists | Never overwrite: save as "name (2).epub" and say so on the output row | result.html §5 |
| No EPUB reader on the OS | Info banner naming the folder and example readers; Show in folder; the app never opens a URL | result.html §5 |
| Update available | Banner in Settings › About & updates ("Install and restart" / "Later"); a daily check that can be turned off; the whole row is hidden in the Flatpak build | settings.html |
| Network audit log | Settings › Network log: a table of every connection (time, host, reason, bytes) + "Show log file" | settings.html |
| Report a problem | Settings › About & updates › "Report a problem…" → the same reviewed diagnostic bundle flow; nothing is sent | settings.html |

## Resolved questions (decided by design; each can still be overturned)

1. **"Not now" on the first-run card.** It hides the card for the session and does not bring it back to the main window on later launches. The explanation stays at the top of Settings › AI assistance. The first run has to be drop-and-done, and a card that returns every launch is nagging.
2. **Unparseable vs too damaged.** The UI keeps two messages: "This file doesn't look like a valid PDF" (bad header or not a PDF) and "This PDF is too damaged to read" (a PDF whose structure can't be recovered). If the engine can only report one failure code, the first message is used for both. Neither offers a retry.
3. **Cancel on a not-responding row.** Same contract as any cancel: "Cancelling…" at once, a cancel message on stdin, a hard kill at 5 s. There is no special case: a stalled process may still read stdin, and one behavior is easier to trust.
4. **OCR install command.** If the engine reports missing Tesseract languages (W_OCR_LANG_MISSING with language args), the command lists only those packs. Otherwise it shows the full command for the detected OS (Tesseract + deu + tur).
5. **Ledger shares.** The percentage of source text is primary ("2.1 %"). The raw "0.021 of C₀" sits beside it in mono for auditors. report.html follows this.
6. **Network log.** It is its own Settings section, visible by default, because "no network unless you asked" should be checkable without digging.
7. **Missing-font warning.** The template asks for a `script` argument ("Georgian text on pp. 73–75 …"). If the engine sends no script name, the fallback template is "Text on pp. 73–75 uses characters that have no font in the PDF. …".
8. **DE/TR strings.** The drafts ship in the mockups only. Turkish is reviewed by the maintainers (native speakers). German needs one native reviewer before 1.0. CI already blocks any WarningCode without a template in every locale.
