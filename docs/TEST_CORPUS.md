# Test Corpus — Dataset Design for OpenConvert

**Status:** Reference document supporting `DECISIONS.md` D18 (corpus honesty) and D11 (testing). Does not itself decide anything; where this document and `DECISIONS.md` differ, `DECISIONS.md` governs.
**Date:** 2026-09-09.
**Sources:** `research/round1/R7_test_corpus_and_synthetic.md` (primary), `research/round2/RED_TEAM_REVIEW.md` §A9 (corpus over-fit findings, adopted into D18), `research/round1/R9_benchmarks_metrics_testing.md` (scoring metrics, test tiers). Cited as `R7 §x`, `RT §A9`, `R9 §x`. Every source in the tables below was researched under a hard rule R7 states explicitly and this document repeats: **nothing here is "cleared."** Every candidate file needs an individual, dated license check at the moment it is actually added to the corpus, not just at planning time. Items not independently re-verified are marked `[UNVERIFIED]`.

---

## 1. Principles

Six commitments, all traceable to D18 and R7 §D, that govern every decision below:

1. **License-clean by construction, not by exception.** Every file in the redistributed corpus carries a fully-populated `license` block (§7) validated against an explicit allowlist; a CI check refuses any commit that adds a corpus file without one (R7 §D.1).
2. **Stratified by producer, reported per stratum, never in aggregate.** Files are bucketed on `/Producer`+`/Creator`: `pdfTeX`, `InDesign`, `Word`, `Ghostscript`, `ABBYY/scanner`, `ours(Typst)`, `ours(WeasyPrint)`, `unknown`. This is not a bookkeeping nicety — RT §A9 found that a corpus built primarily from OpenConvert's own renderers is *structurally* easier than reality, not just smaller: WeasyPrint/Typst/Chromium emit clean `ToUnicode` CMaps, no Type 3 fonts, no double-drawn fake bold, no invisible OCR sandwich, and (worse) Typst tags PDFs by default while real-world PDFs are tagged only **12.6%** of the time (R7 §A.0, item 20's context; RT §A9). A score computed only in aggregate hides exactly this gap.
3. **`ours(*)` ≤ 40% of the corpus, and a release cannot pass on `ours(*)` strata alone.** Synthetic and self-rendered fixtures are the ground-truth factory (§5), not the accuracy oracle. Real-producer strata carry the release gate.
4. **A frozen, ≥100-file real-world holdout, never used to fit a threshold.** Thresholds (`thresholds.toml`, D17) are calibrated on real-producer strata excluding the holdout; the holdout exists only to report, and its score gap against the calibration strata is itself tracked as a first-class metric — a widening gap is the early warning for renderer over-fit (RT §A9). **The holdout unit is the document**, for books and papers alike: one monograph is one unit however many pages it has. **DocLayNet pages count as units only inside the page-level layout stratum**, which is scored and reported separately and **cannot substitute for the ≥ 100 real-document holdout** — 100 pages drawn from a handful of PDFs measure a handful of producers, which is the exact over-fit this principle exists to detect.
5. **Synthetic struct trees are stripped by default.** Typst 0.14+ emits PDF/UA-1-tagged output by default (R7 §B.3); reality does not. Tagged synthetic fixtures are kept as a small, explicitly labelled, separate bucket sized to reality (~12%), so the "we have a struct tree" fast path is exercised in the same proportion it will be exercised in production, not on 100% of synthetic input.
6. **Never vendor large binaries into the main git history.** The repository commits a small, text-diffable manifest (§7) plus a download script that fetches real-world files from their canonical source (or a project-controlled GitHub Releases mirror) and verifies SHA-256 before use. Synthetic fixtures are, where generation is deterministic, not stored as binaries at all — the generator, its seed, and the (tiny) expected ground truth are committed, and the PDF is regenerated at test time (R7 §D.2).

---

## 2. Source-by-source license findings

Condensed from R7 §A.0, with the red-team additions (RT §A9) folded in as the sources actually used to satisfy Principle 2. **[VERIFIED via fetch]** means R7 confirmed the claim by directly fetching the cited page this round; **[UNVERIFIED]** means it rests on general knowledge or a search-result snippet and must be re-confirmed before ingestion.

| Source | License status | Use / caution |
|---|---|---|
| **Standard Ebooks** | CC0 on all work product (markup, cover art). **[VERIFIED]** No native PDF — this is the gap Part 5 exists to fill. | Ground-truth factory (§5.1), not a real-PDF source: every PDF derived from it is `ours(*)`. |
| **Project Gutenberg** | PG's own packaged files: verbatim-redistribution-only license + trademark terms; underlying text is free once PG boilerplate is stripped. **[VERIFIED]** No native PDF pipeline. | Source text for `ours(*)` synthetic fixtures; do not redistribute PG's own packaged files with header intact. |
| **Internet Archive** (PD scans) | Per-item; well-digitized pre-1929 books commonly `NOT_IN_COPYRIGHT`. **[VERIFIED]** on a sample item — PDF, EPUB, plain text, hOCR, ABBYY OCR XML, JP2, DAISY all available side by side. | **Real scanned-and-OCR'd category**, with hOCR as free ground truth for the OCR text. Check the rights field per item — IA hosts both PD and non-PD material. |
| **arXiv** | Per-paper license badge (CC BY 4.0 confirmed on a sample paper, **[VERIFIED]**); no bulk-queryable license field via the HTML API docs. | **Real pdfTeX category** with LaTeX source as free structural ground truth (§5.1). Verify each paper's badge individually; do not bulk-scrape. |
| **PLOS / eLife** | Blanket CC BY 4.0 / CC BY (journal-wide). **[VERIFIED]** | Two-column, figure/table-heavy real PDFs. |
| **DocLayNet** (IBM/DS4SD) | CDLA-Permissive-1.0. **[VERIFIED]** 80,863 pages, six document domains (financial, scientific, laws/regulations, tenders, manuals, patents). | Real per-page layout ground truth (§5.2) for domains no literary source covers. Pull a small subset via the HF `datasets` loader — never vendor the full 35.5 GB (§7). |
| **OAPEN / DOAB** open-access monographs | CC BY 4.0 / CC BY-SA per book, filtered in the OAPEN library UI. **Added by RT §A9** as "the single best untapped source for genuinely InDesign-typeset, openly-licensed *books*." | **Real InDesign trade-book typography** — the corpus's best source for the InDesign producer stratum, which no other source in this table supplies. See §3 for the selection method (titles not pre-specified). |
| **Internet Archive PD scans with hOCR** | As above — repeated here because RT §A9 names it explicitly as an "untapped source" for real ABBYY OCR layers and real scanner artifacts, distinct from the clean-book IA items already listed. | Real scanned/OCR-sandwich category (§4). |
| **US federal government works** | Public domain, 17 U.S.C. §105. **[VERIFIED]** (NASA, GAO, IRS, CRS, NIST SPs, federal court opinions). | Real, modern, clean-typeset PD reports — a producer stratum (typically Word/InDesign/Ghostscript) distinct from literary sources. |
| **EU Publications Office** | CC BY 4.0 / CC0 for Commission-published reports/studies since April 2019. **[VERIFIED policy adoption]**; the Official Journal's *authentic* legislative text has a separate, lower-confidence status not confirmed to the same precision. | Real EU-house-style reports; treat legislative-text PDFs as a separate, lower-confidence case. |
| **Deutsches Textarchiv (DTA)** | Plain-text export: unrestricted ("im Sinne der Gemeinfreiheit"). Annotated XML/HTML: CC BY-SA 4.0. **[VERIFIED]** Whether DTA ships a direct per-text PDF is `[UNVERIFIED]`. | German historical-text source; also the source R7 and D15 both point to for CC0-clean German word-frequency data (not a corpus item, a dictionary input). |
| **Wikisource (incl. Turkish)** | Transcription/markup layer typically CC BY-SA/GFDL; underlying source text separately PD. WSexport converts to PDF/EPUB on demand. **[UNVERIFIED — fetch blocked by robots.txt this round]**. | Turkish-language PD source, low sample count expected; verify live before ingesting. |
| **DergiPark** (Turkish academic journals) | Aggregates independently-licensed journals; several open access under CC BY. **[UNVERIFIED per-journal]**. | Turkish academic two-column register. **Verify the specific journal's license, not the platform's terms**, per article. |
| **Isartor test suite** | "Freely downloadable and usable without restriction." **[VERIFIED]** PDF/A-1b conformance files, one deliberate spec violation per file, cataloged. Exact file count `[UNVERIFIED, commonly cited ~34]`. | The **broken** category's cleanest source — defects are intentional and documented, not accidental. |
| **Excluded / caution-flagged, do not add** | — | **DocBank** (README says "DO NOT re-distribute" despite an Apache-2.0 code badge — a direct contradiction). **RVL-CDIP / FUNSD** (murky IIT-CDIP tobacco-litigation provenance, research-use reputation). **Projekt Gutenberg-DE** (claims copyright, browse-only). **CommonCrawl / SAFEDOCS PDF corpus** (unclear per-file copyright — local evaluation only, never redistributed; §7's `LOCAL_EVAL_ONLY` area). **UN documents** (not automatically public domain). **pdf.js / pdfium bundled test corpora** (provenance not tracked per file). |

---

## 3. Candidate file table

Columns per the brief. **Cat.** = simple/difficult/broken/edge. **Producer stratum** per Principle 2. **GT** (ground-truth type) per §5. Page counts are approximate and unverified at exact-count precision (R7's own caveat) unless the row says otherwise. This is a representative ~45-item sample, not an exhaustive manifest — the actual `corpus/manifest.json` (§7) is the operative artifact.

| # | Title | Source | License | Pages | Cat. | Producer stratum | Expected problems | Expected output | GT type | Diff. |
|---|---|---|---|---|---|---|---|---|---|---|
| 1 | Alice's Adventures in Wonderland (PG ed.) | gutenberg.org/ebooks/11 | PD (strip header) | ~100 | Simple | `pdfTeX` (via our render of PG text) `[UNVERIFIED — producer depends on chosen renderer]` | none — clean single-column prose | Single-column novel, no images | latex-source-adjacent | 1 |
| 2 | Alice's Adventures in Wonderland (1901 NYPL scan) | archive.org/details/alicesadventures00carr2 | PD, `NOT_IN_COPYRIGHT` **[VERIFIED]** | ~200 | Edge / scanned | `ABBYY/scanner` | OCR noise, illustrations interleaved, period typography | Same text as #1, image-only pages with hOCR-derived text | tagged-pdf-structtree-adjacent / hOCR | 4 |
| 3 | Pride and Prejudice | gutenberg.org/ebooks/1342 | PD | ~300 | Simple | `unknown`/`ours` depending on source PDF | none | Long single-column novel | manual/latex-adjacent | 1 |
| 4 | Moby-Dick | gutenberg.org/ebooks/2701 | PD | ~650 | Difficult | `unknown` | very long chapters, footnote-like asides, verse epigraphs | Deep chapter hierarchy | manual | 3 |
| 5 | The Odyssey (trans.) | gutenberg.org/ebooks/1727 | PD | ~350 | Difficult | `unknown` | translator's notes, verse/prose mix | Verse structure test | manual | 3 |
| 6 | Complete Works of Shakespeare | gutenberg.org/ebooks/100 | PD | ~1,500–2,000 `[UNVERIFIED exact]` | Edge (very large) | `unknown` | verse layout, stage directions, huge file size | Very-large-PDF stress fixture | manual | 4 |
| 7 | Romeo and Juliet | gutenberg.org/ebooks/1513 | PD | ~120 | Difficult | `unknown` | verse layout, dramatis personae, act/scene structure | Compact drama-formatting edge case | manual | 3 |
| 9 | Crime and Punishment (trans.) | gutenberg.org/ebooks/2554 | PD | ~550 | Simple + footnotes | `unknown` | translator footnotes (edition-dependent) | Baseline + footnote linkage test | manual | 2 |
| 10 | Adventures of Sherlock Holmes | gutenberg.org/ebooks/1661 | PD | ~300 | Difficult | `unknown` | short-story collection — internal chapter boundaries | Multi-work-in-one-file segmentation | manual | 3 |
| 11 | Elements of Euclid (Byrne, colour ed.) | gutenberg.org/ebooks/21076 `[UNVERIFIED ebook ID]` | PD | ~300 | Edge (math/graphics) | `unknown` | colour geometric diagrams substituting for algebraic text, diagrams mid-sentence | Best PD math/typography torture test | manual | 5 |
| 12 | *Within and Without* (George MacDonald) | standardebooks.org (slug verified R7) | CC0 | ~150 (est.) | Simple / GT-pair | `ours(Typst)` / `ours(WeasyPrint)` | verse drama structure | GT-pair baseline | **generated-from-xhtml** | 1 |
| 13 | *Malleus Maleficarum* (trans. Summers) | standardebooks.org (slug verified R7) | CC0 | ~500 (est.) | Difficult / GT-pair | `ours(*)` | dense scholarly footnotes, Latin phrases, block quotes | Best CC0 footnote-heavy GT pair | generated-from-xhtml | 3 |
| 14 | *The Wheels of Chance* (H. G. Wells) | standardebooks.org | CC0 | ~200 (est.) | Simple / GT-pair | `ours(*)` | none | Clean GT-pair baseline | generated-from-xhtml | 1 |
| 15 | *Brigadier Gerard Stories* | standardebooks.org | CC0 | ~250 (est.) | Simple / GT-pair | `ours(*)` | short-story collection, internal headings | Heading-segmentation GT pair | generated-from-xhtml | 2 |
| 16 | *The Fall of Robespierre* | standardebooks.org | CC0 | ~60 (est.) | Simple / GT-pair, small | `ours(*)` | verse drama, dual-author metadata | Small GT-pair fixture | generated-from-xhtml | 2 |
| 17 | *Hand and Ring* (A. K. Green) | standardebooks.org | CC0 | ~350 (est.) | Simple / GT-pair | `ours(*)` | none | Larger GT-pair novel | generated-from-xhtml | 1 |
| 18 | *A Book-Lover's Holidays* (T. Roosevelt) | standardebooks.org | CC0 | ~300 (est.) | Difficult / GT-pair | `ours(*)` | nonfiction with illustrations/photo plates | Image/caption-focus GT pair | generated-from-xhtml | 3 |
| 19 | "Mamba" (arXiv 2312.00752) | arxiv.org/abs/2312.00752 | CC BY 4.0 **[VERIFIED]** | ~30 | Difficult | `pdfTeX` (real) | equations, algorithm blocks, tables, dense citations, possible multi-column | Real pdfTeX LaTeX-structured paper | latex-source | 4 |
| 20 | Additional CC-BY arXiv papers (2-column) | arxiv.org/abs/* (verify badge per paper) | CC BY 4.0, per-paper | 8–40 | Difficult | `pdfTeX` (real) | two-column layout, inline math, footnotes, figures | Layout diversity within one real producer | latex-source | 4 |
| 21 | PLOS ONE article (any) | journals.plos.org/plosone | CC BY 4.0 **[VERIFIED, journal-wide]** | 8–20 | Difficult | `unknown` (publisher toolchain) | figures, tables, supplementary refs, two-column-ish | Real biomedical-paper register | doclaynet-annotation (partial) | 3 |
| 22 | eLife research article (any) | elifesciences.org | CC BY **[VERIFIED, journal-wide]** | 15–30 | Difficult | `unknown` | figures, data tables, structured digest sections | Second real-biomedical register | doclaynet-annotation (partial) | 3 |
| 23–25 | DocLayNet subset — financial / patent / gov-tender samples | github.com/DS4SD/DocLayNet | CDLA-Permissive-1.0 **[VERIFIED]** | 1–2/doc | Edge | `unknown` (mixed real producers) | dense tables, non-literary layout, claims numbering, forms | Real per-page layout ground truth (COCO boxes) | doclaynet-annotation | 4 |
| 26 | Isartor test suite (full set) | pdfa.org/resource/isartor-test-suite/ | Unrestricted **[VERIFIED]** | 1 each, ~30+ | Broken | `unknown` (deliberately malformed) | one cataloged PDF/A-1b spec violation per file | Robustness/error-handling regression set | manual (defect is documented) | 5 |
| 27–29 | NASA history/technical PD volumes | nasa.gov/history | PD (§105) | 200–400 (est.) | Difficult/Edge | `unknown` (US-gov toolchain) | dense tables, diagrams, photo captions, sidebars, possible rotated figure pages | Real modern US-gov typeset register | manual | 3–4 |
| 30 | GAO report (any recent) | gao.gov | PD | 30–100 | Difficult | `unknown` | charts, tables, appendices, recommendation boxes | Real modern clean-typeset tabular report | manual | 2 |
| 31 | IRS Form 1040 instructions | irs.gov/forms-pubs | PD | ~40 | Edge (forms) | `unknown` | dense tables, fillable fields, multi-column instructions | Forms/tables edge case | manual | 3 |
| 32 | IRS Form W-9 | irs.gov/forms-pubs | PD | 1 | Edge (very small) | `unknown` | single page, form fields | Very-small-PDF fixture | manual | 1 |
| 33 | CRS report (any current) | crsreports.congress.gov | PD `[UNVERIFIED exact statutory citation]` | 20–60 | Difficult | `unknown` | dense footnotes, legal citations | Footnote/citation-dense nonfiction register | manual | 3 |
| 34 | NIST SP (numbered series) | nist.gov/publications | PD | 50–300 | Difficult/Edge | `unknown` | deep section numbering, cross-references, appendices | Technical-standard structure | manual | 3 |
| 35 | US federal court opinion | courtlistener.com | PD (edicts-of-government doctrine) `[UNVERIFIED case citation]` | 10–80 | Difficult | `unknown` | dense footnotes, block quotes, citation strings | Legal-citation-heavy register, footnote linking stress | manual | 3 |
| 37 | EU Commission report/study | publications.europa.eu | CC BY 4.0 **[VERIFIED policy]** | 20–100 | Difficult | `unknown` (EU house style) | possible multilingual parallel versions, EU-house tables/footnotes | Non-US clean-typeset register | manual | 3 |
| 39 | DTA text (plain-text export) | deutsches-textarchiv.de | Plain text unrestricted **[VERIFIED]** | 100–400 | Edge (historical) | `ours(*)` (we render it) | old orthography, possible Fraktur-transcription artifacts | German historical-text edge case | generated-from-xhtml-adjacent | 4 |
| 41 | PD Turkish text via Turkish Wikisource | tr.wikisource.org via wsexport | Text PD; transcription CC BY-SA `[UNVERIFIED live]` | varies | Edge (Turkish diacritics) | `ours(*)` | ç, ş, ğ, ı, ö, ü coverage, hyphenation rules | Turkish character/hyphenation edge case | generated-from-xhtml-adjacent | 3 |
| 43 | DergiPark article, CC BY-verified | dergipark.org.tr (per-journal check) | CC BY, per-journal `[UNVERIFIED per-article]` | 8–20 | Difficult | `unknown` | Turkish academic two-column layout, references, tables | Turkish academic-paper register — **the only real (non-synthetic) Turkish item in this table** | manual | 4 |
| 45 | Very large (1,000+ page) PD bound volume | archive.org (bound periodical search) `[UNVERIFIED specific item]` | PD, per-item check | 1,000+ | Edge (very large) | `ABBYY/scanner` or `unknown` | huge page count, mixed layouts across issues, running heads | Genuine very-large-PDF stress test | manual | 4 |
| **OAPEN-1..N** | OAPEN/DOAB monographs — see selection method below | library.oapen.org | CC BY 4.0 / CC BY-SA per book | varies | Difficult | **`InDesign`** | real InDesign spacing/kerning, real trade-book typography, dense footnotes in some titles | The corpus's **only real InDesign stratum source** | manual (hand-annotated golden subset) | 3–4 |

**OAPEN selection method (deliberately not pre-specified titles, per instruction — R7 did not identify this source, RT §A9 added it as a category):** query the OAPEN library, filter `license = CC BY 4.0` (or `CC BY-SA`), `subject ∈ {literature, history}`, `format = PDF available`; select 8–12 titles spanning a range of page counts (short monograph to 400+ pages) and at least one title with heavy footnote apparatus. Record the exact query parameters and retrieval date in the manifest entry's `source` block (§7) so the selection is reproducible even though the specific titles are not fixed here. Every OAPEN candidate needs the same per-file, per-ingestion license snapshot as every other source (Principle 1) — CC BY vs. CC BY-SA changes the redistribution terms and must be recorded per book, not assumed uniform across the OAPEN catalog.

Rows omitted from this condensed table (Bundestag Drucksachen, Resmî Gazete, EUR-Lex authentic legislative text) remain `[UNVERIFIED]` in R7 §A.1 at file-specific granularity and are not promoted into the working manifest until that verification happens.

---

## 4. Categories required by the brief

| Category | Corpus items (§3) | Synthetic fixtures (§6) |
|---|---|---|
| **Simple** | #1, #3, #12, #14, #17 | Single-column Typst/WeasyPrint render of a short SE title, no defects injected |
| **Difficult** | #4, #5, #9, #10, #13, #18–22, #27–30, #37, #43, OAPEN | Multi-column Typst fixture with footnotes + a table; a two-column-journal-style WeasyPrint fixture |
| **Broken** | #26 (Isartor, whole set) | Mutation-catalogue outputs (§6.2): stripped ToUnicode, Type 3 substitution, damaged xref, torn CropBox |
| **Edge — German umlauts** | #39 (DTA) | Typst fixture with a full ä/ö/ü/ß inventory in headings, body, and hyphenated compounds |
| **Edge — Turkish characters** | #41, #43 | Typst fixture with ç/ş/ğ/ı/ö/ü, including dotted/dotless-i case-folding traps in headings and body |
| **Edge — ligatures** | #12–18 (SE originals use standard ligature-bearing typefaces) | Typst/WeasyPrint fixture forcing fi/fl/ff ligature substitution, both expanded and un-expanded in the source font |
| **Edge — math** | #11 (Euclid), #19–20 (arXiv) | Typst fixture with inline and display math (Typst's native math mode) |
| **Edge — Unicode (non-Latin quotations)** | #13 (Latin), #18 (potential), general literary sources with epigraphs | Typst fixture embedding a Greek or Cyrillic block quotation inside an otherwise Latin-script book |
| **Edge — very large** | #6, #45 | A generated, deterministic 1,000+ page Typst fixture (many short chapters, looped content) |
| **Edge — very small** | #32 (1-page form) | A 1-page Typst fixture with a single heading and paragraph |
| **Edge — image-only** | #2 (scanned), #26 subset | Rasterize-and-wrap pipeline output (§6.3) with no text layer at all |
| **Edge — multilingual (mixed-language body)** | #5 (translator's notes), #13 (Latin phrases), #37 (EU parallel versions) | Typst fixture with an EN-primary body containing a French or Latin block quotation, to exercise per-block language detection (D13.11) |

---

## 5. Ground-truth strategy

### 5.1 Standard Ebooks XHTML → ground truth JSON + three independent PDF renderers

The highest-leverage, lowest-cost ground-truth source (R7 §B.1). Because Standard Ebooks' markup is CC0 and semantically rich (proper `<h1>`–`<h6>`, `epub:type` distinguishing chapters/poems/drama/front-matter, `<a epub:type="noteref">`/endnote structures, `<figure>`/`<figcaption>`), the pipeline is:

1. Parse the source XHTML DOM directly into ground-truth JSON: `{headings: [{level, text, id}], paragraphs: [...], footnotes: [{marker_id, body_text, referring_paragraph_id}], images: [{src, alt, caption, position}]}`.
2. Render the **same** XHTML to PDF via **three independent renderers**: WeasyPrint (BSD, direct HTML/CSS→PDF, primary — shares the HTML/CSS family with the source), Typst (Apache-2.0, via a restructured Typst-markup translation of the same content), and headless Chromium via Playwright (a second, independent cross-check). Three renderers from one ground-truth source is deliberate: it separates "OpenConvert's PDF→EPUB converter is wrong" from "this specific PDF renderer's quirk is wrong," which a single-renderer ground truth cannot do.
3. Score OpenConvert's output against the **original XHTML-derived** ground truth, not against the intermediate PDF — so the test measures "did we recover the true document structure," independent of which renderer produced the input.

This is a free, scalable (thousands of Standard Ebooks titles), exact-match ground-truth factory. It is also entirely `ours(*)` — Principle 3's 40% cap applies to it directly.

### 5.2 arXiv LaTeX → structure

`\section`/`\subsection` give exact heading hierarchy; `\footnote` gives footnotes; `\includegraphics`+`\caption` give figure/caption pairs; math environments give exact LaTeX-source ground truth for formulas — no OCR pipeline needs to guess the equation, the ground-truth string already exists. Budget for a curated subset of "LaTeX-parses-cleanly" papers rather than assuming every CC-BY arXiv paper is usable unmodified — custom macros and conditionals will break a naive parser on some fraction of real papers.

### 5.3 DocLayNet subset — real per-page layout ground truth

CDLA-Permissive-1.0, per-page bounding-box + category ground truth on real, non-literary documents (financial reports, patents, manuals, tenders) that no literature-derived source touches. This is *page-level layout* ground truth, not full-document reading-order-across-pages ground truth — use it to validate the layout-classification stage specifically (is a block correctly typed as heading/table/footnote/figure), and use §5.1/§5.2 to validate end-to-end document reconstruction.

### 5.4 Tagged-PDF / PDF/UA structure trees — a separate ~12% bucket

A tagged, PDF/UA-conformant PDF embeds a `/StructTree` with `/S` (structure type), `/K` (kids, giving reading order), and marked-content linking structure nodes to content-stream runs — machine-extractable ground truth with no external source document needed. Typst 0.14+ ships PDF/UA-1-tagged output *by default* (R7 §B.3), so every synthetic Typst fixture doubles as a tagged-ground-truth fixture at zero extra authoring cost. But per Principle 5, this doubling is deliberately **not** allowed to become the corpus's default state: tagged synthetic fixtures are kept to a separate, explicitly labelled ~12% bucket — matching the real-world 12.6% tagging rate the red team found (R7 §A.0 context, RT §A9) — precisely so the "we have a struct tree" fast path is exercised in the proportion it will actually see in production. Real-world tagged PDFs (some accessibility-mandated EU/government documents, especially post-European-Accessibility-Act) supply a second, genuinely-real source of this ground-truth type; tagging quality in the wild is inconsistent and each candidate needs a quick struct-tree sanity pass before being trusted.

### 5.5 Hand-annotated golden set on REAL PDFs

The "broken" category and two specific structural judgments — **furniture removal** (running headers/footers) and **verse-vs-quote classification** — have no external structural ground truth by definition, and per RT §A9's decisive correction, **both must be labelled on real PDFs, not on `ours(*)` renders**: running heads in synthetic fixtures are perfectly stable, so a repetition-ratio threshold calibrated on them is far too permissive against real books with chapter-varying heads and first-page suppression, and verse/quote geometry in a hand-tuned synthetic fixture does not reproduce the ambiguity that makes the task hard in the first place. Budget a **~50-file hand-annotated golden set of real PDFs** (drawn from the OAPEN, Internet Archive, and CRS/court-opinion strata in §3, which supply the InDesign and government-typeset registers that most exercise these two judgments) — expensive per file, but small, because most of the corpus's ground-truth *volume* still comes from §5.1's synthetic factory.

### 5.6 Scoring metrics

| Aspect | Metric | Notes |
|---|---|---|
| Body text fidelity | CER/WER after normalization (Unicode NFC, hyphenation-rejoining, ligature expansion applied before diffing — R9 §A.10 flags this normalization gap in Nougat's own metric code as a mistake to avoid repeating) | Standard OCR/ASR-derived metric |
| Heading structure | Precision/recall/F1 on (heading text, level) pairs, **plus tree-edit distance** between predicted and ground-truth heading trees | Tree-edit distance penalizes "right text, wrong level" more informatively than flat F1 alone |
| Reading order | Edit-distance between predicted and ground-truth block-ID sequence, normalized by length — **OmniDocBench-compatible** (R9 §A.11) | **Plus Kendall's tau** as a secondary signal: edit distance is harsher on insertion/deletion of whole blocks, Kendall's tau is a purer "did we get relative order right" signal once the block set itself is correct — the two together separate content-loss bugs from pure-ordering bugs |
| Table structure | **TEDS** (structure + content) and **TEDS-S** (structure-only, cell-substitution cost fixed) — R9 §A.14 | Report both; a wrong grid is a worse failure than wrong text in a correct grid |
| Footnote linking | Precision/recall/F1 on (marker, footnote-id) pairs | Derivable exactly from §5.1's SE ground-truth pairs, where the noteref↔note relationship is explicit in the source markup |
| Images/figures | Count precision/recall + caption-to-image association accuracy | Cheap, catches the most common real bug (images silently dropped) |
| Aggregate | A weighted combination reported **per category (simple/difficult/broken/edge) and per producer stratum**, never as one global number | Prevents a large, easy `ours(*)` stratum from masking failure on the small but decisive real/broken/edge strata |

---

## 6. Synthetic generator

### 6.1 The decision: `cargo xtask fixtures`, a Rust dev tool, plus Python for the remaining paths

D18 and R7 Part C both point toward a Typst-first toolchain; this document states the concrete shape. **Decision (ratified by the Chief Architect): the workspace `xtask` binary (`cargo xtask fixtures`), using the `typst` and `typst-pdf` crates (both embeddable, per R7 §C.1's confirmed fetch of Typst's own documentation: "You can use the crate `typst` as a Rust library... embed the Typst compiler in your own applications"), generates the Typst-sourced fixtures deterministically and in-process — no shelling out to a `typst` CLI, no Python dependency for this path.** Python handles the paths where its ecosystem is currently ahead of Rust's: WeasyPrint (the CSS-family renderer for the Standard-Ebooks-XHTML pairing, §5.1), ReportLab (canvas-level content-stream ordering control for defect injection, §6.2), and img2pdf (the final step of scanned-simulation, §6.3). This mirrors R7 §C.2's own two-track recommendation and keeps the corpus-generation code in the same language as the pipeline it tests wherever that is the leverage point (Typst generation, mutation), while not forcing WeasyPrint's mature CSS engine or Tesseract's Python bindings to be reimplemented for no benefit.

The generator lives in `xtask/` (a workspace member, dev/test tooling, never linked into the shipped app; `eval/` stays Python-only per D14). The `typst` crate version is pinned in the workspace `Cargo.lock`, which is what makes regeneration deterministic across machines; no `typst` CLI is installed or pinned anywhere. `eval/` does not contain a Typst path at all.

### 6.2 Defect catalogue, keyed to the failure taxonomy

Each defect is authored as a **mutation of a valid fixture from §6.1**, not as a from-scratch malformed file, so every defect fixture's pre-corruption ground truth is already known exactly (R7 §C.2). Mutation tooling:

- **lopdf** (Rust, MIT) — parses and mutates existing PDFs: strip/alter `ToUnicode` CMaps, remove or rename font subsets, reorder or corrupt content-stream operators, strip XMP/structure metadata, damage xref tables.
- **qpdf**, **QDF mode** (Apache-2.0, C++/CLI) — converts a PDF into a human-readable, line-diffable text form; the defect is then a small, reviewable git diff against the QDF text, reassembled with `qpdf`. This is the standout recommendation for keeping "what exactly is broken about fixture #N" auditable in code review, rather than a binary patch.
- **pdf-writer** (Rust, dual MIT/Apache-2.0, from the Typst ecosystem) — for defects authored from scratch with full manual content-stream operator ordering, the Rust-native equivalent of ReportLab's canvas trick.
- **ReportLab** (Python, BSD) — same purpose in Python: `drawString(x, y, text)` emits content-stream operators in exactly the call order given, independent of visual position, so wrong-reading-order and overlapping-text defects are directly authorable.

| Defect | Taxonomy source | Tool |
|---|---|---|
| Wrong text order (glyphs drawn out of visual order) | R1 §D.6 real-world quirk, R9 §B.4 metamorphic relation | ReportLab / pdf-writer |
| Overlapping text | Same | ReportLab / pdf-writer |
| Missing/stripped `ToUnicode` | R2 §B.8, corroborated D3's residual-risk note on `has_unicode_map_error()` | lopdf |
| Type 3 fonts (substituted for a normal font) | R1 §D.6, D3's stated alternative candidates | lopdf / pdf-writer |
| Fake bold via double-draw (overdraw) | R1 §D.6 #2, D13.4's `OverdrawDedup` reason | ReportLab (duplicate `drawString` calls) |
| Render-mode-3 OCR sandwich | D13.10's `ocr-sandwich` page class | pdf-writer (invisible text render mode) |
| CropBox offset vs. MediaBox | R1 §D.6 #1 — "silently deleted body text in a shipping 2026 tool" | qpdf QDF edit |
| Weird/unusual point sizes | General robustness | Typst fixture parameter |
| Multi-column layout | Reading-order stress | Typst fixture (native multi-column support) |
| Broken images (bad SMask, corrupt stream) | D13.11 image policy | lopdf |
| Bad/irregular word spacing | Justification-heuristic stress | ReportLab (explicit glyph-advance control) |
| Headers/footers mixed into body flow | Furniture-detection stress | Typst fixture (deliberately inconsistent band position) |
| Rotated pages | `/Rotate` handling (D13.3's normalized-space invariant) | qpdf QDF edit (`/Rotate` value) |
| Tiled/repeated images (ornaments) | D13.11's ornament-drop rule | Typst fixture (small image repeated per page) |
| Soft hyphens (U+00AD) at line breaks | D13.4's `SoftHyphen`/`Dehyphenate` reasons | Typst fixture (explicit soft-hyphen insertion) |

### 6.3 Scanned-document simulation

A separate small pipeline, deliberately not part of the Typst/`xtask fixtures` path: render a clean fixture → rasterize pages (pdfium or a Rust `image`-crate renderer; mutool is explicitly excluded from this path on AGPL grounds, per D15) → inject noise/skew/blur/JPEG artifacts (Pillow or a Rust `image`-crate equivalent) → wrap as an image-only PDF via **img2pdf** (LGPL-3.0, lossless embedding) — optionally run Tesseract afterward to add a deliberately imperfect OCR text layer, simulating an Internet-Archive-style scanned book. This produces the image-only and OCR-sandwich edge-case fixtures in §4 without depending on any AGPL tool in the CI hot path.

### 6.4 Determinism policy

Per Principle 6 and R7 §D.2: store the generator script + fixed seed/config + the (tiny) expected ground truth, and **regenerate the PDF at test time** rather than storing it, whenever the toolchain is deterministic. Typst's own determinism across versions/platforms is not assumed — it is tested explicitly: a CI job regenerates every fixture and diffs the output against the last-committed byte-identical copy, failing loudly on drift rather than silently accepting a changed fixture. Where a tool's cross-platform/cross-version determinism cannot be guaranteed (a real risk flagged but not resolved for ReportLab and ligature-shaping paths, R7 §C.1), the project falls back to storing one golden binary per fixture, with the same regenerate-and-diff check as the failure detector rather than the passing path.

---

## 7. Corpus organization

### 7.1 `corpus/manifest.json` schema

Extends R7 §D.1's schema with `producer_stratum` (Principle 2) and `holdout: bool` (Principle 4) — both required additions for D18 to be enforceable, not merely stated:

```json
{
  "id": "stable-slug-or-uuid",
  "title": "string",
  "source": {
    "name": "Project Gutenberg | Standard Ebooks | Internet Archive | arXiv | OAPEN | ... | synthetic-generator",
    "url": "canonical source URL",
    "retrieved_date": "YYYY-MM-DD",
    "archive_snapshot_url": "Wayback Machine URL, recorded in case the source disappears",
    "selection_query": "for query-based sources (e.g. OAPEN filter parameters), the exact query used"
  },
  "license": {
    "name": "CC0-1.0 | CC-BY-4.0 | CC-BY-SA-4.0 | PD-US-Gov | PD-old-work | CDLA-Permissive-1.0 | Apache-2.0 | MIT | ...",
    "url": "license text URL",
    "verified_by": "person/agent",
    "verified_date": "YYYY-MM-DD"
  },
  "sha256": "hex digest of the exact file",
  "file_size_bytes": 0,
  "pages": 0,
  "category": "simple | difficult | broken | edge-case",
  "producer_stratum": "pdfTeX | InDesign | Word | Ghostscript | ABBYY-scanner | ours-typst | ours-weasyprint | unknown",
  "holdout": false,
  "difficulty": 1,
  "expected_problems": ["two-column", "footnotes", "rotated-pages", "image-only", "..."],
  "expected_output_characteristics": {
    "headings": 0, "images": 0, "tables": 0, "footnotes": 0, "languages": ["en"]
  },
  "ground_truth_type": "generated-from-xhtml | latex-source | doclaynet-annotation | tagged-pdf-structtree | manual-annotation | none",
  "ground_truth_ref": "path or URL to the ground-truth file",
  "generator": "xtask-fixtures-<typst-crate-version> | weasyprint-<version> | reportlab-<version> | lopdf-mutation-<script> | n/a-real-world",
  "defect_injection": ["shuffled-content-stream", "stripped-tounicode", "..."]
}
```

`holdout: true` marks membership in the frozen ≥100-file real-world set (Principle 4). A CI check enforces: (a) every `license.name` validates against an explicit allowlist (CC0-1.0, CC-BY-4.0, CC-BY-SA-3.0/4.0, PD-US-Gov, PD-old-work, CDLA-Permissive-1.0, Apache-2.0, MIT, BSD-3-Clause) and rejects an explicit blocklist (research-only, "do not redistribute," CommonCrawl-derived, RVL-CDIP/FUNSD-derived, license unknown); (b) `ours(*)` producer strata never exceed 40% of the non-holdout corpus; (c) any file with `holdout: true` is excluded from any script path that writes to `thresholds.toml`.

### 7.2 Download script and mirroring

The download script fetches each real-world file from its canonical source URL (or a project-controlled GitHub Releases mirror, tried first for speed and stability, falling back to the original source), verifying the manifest's recorded `sha256` before the file is used by any test. **GitHub Releases** serves as a lightweight, free, versioned CDN for a `corpus.tar.gz` of cleared real-world files. **Git LFS** is a fallback, not the default — GitHub's free-tier LFS bandwidth is metered and can be exhausted by CI traffic across many contributor forks (R7 §D.2).

### 7.3 Size budgets

| Tier | Scope | Budget |
|---|---|---|
| **Fast CI path** (every PR) | Curated ~30–50 file subset, at least one item per category, regenerated synthetic fixtures at near-zero storage cost | ≤100 MB total |
| **Nightly/full path** | Larger few-hundred-file corpus (more literary works, more DocLayNet pages, more arXiv/PLOS/eLife papers, the large NASA/Shakespeare-class volumes), downloaded on demand, not vendored | ~2–5 GB |
| **Never-vendored, reference-only** | Full DocLayNet (35.5 GB), CommonCrawl/SAFEDOCS (8 TB) | External download instructions only, for local non-redistributed evaluation |

### 7.4 License allow/block lists and annual re-verification

Enforced in CI per §7.1's checks. Beyond the automated gate: treat the §2 source table as a living document, re-verified **at least annually** and immediately on any signal that a source's hosting changed (this round's research already surfaced one live example — the CIA World Factbook's reported February 2026 discontinuation, itself `[UNVERIFIED — single AI-summarized source, R7 §A.0 item 18]` and therefore not currently in the working corpus at all). For any two-layer-license dataset (DocLayNet/PubLayNet-style: annotation license ≠ underlying-document license), record and satisfy both layers explicitly, not just the more permissive one.

### 7.5 `LOCAL_EVAL_ONLY` area

A clearly separate, non-redistributed area (a `LOCAL_EVAL_ONLY.md` plus a download script requiring an explicit opt-in flag) for anything like the CommonCrawl/SAFEDOCS PDF corpus — useful for local robustness/fuzzing but never part of the redistributed open-source test suite. The redistribution boundary is a deliberate, hard-to-bypass-by-accident mechanism (a separate opt-in flag and a separate script), not a policy note a contributor could miss.

### 7.6 Holdout unit and sourcing target

**Unit.** For books and papers the holdout unit is the **document** — one monograph, one paper, one scan is one unit regardless of page count. **DocLayNet pages are units only within the page-level layout stratum**, which is scored and reported separately and can never be counted toward, or substituted for, the ≥ 100 real-document holdout of Principle 4. This closes the per-page shortcut explicitly: a hundred pages from six PDFs exercise six producers, and producer diversity is the property the holdout exists to measure.

**Sourcing target (execution target, Phase 7).** The ≥ 100 documents are assembled to this shape, with `holdout: true` set in `corpus/manifest.json` (§7.1) at the moment of admission and frozen thereafter:

| Source | Target count | License basis |
|---|---|---|
| OAPEN / DOAB monographs | ~40 | CC BY / CC BY-SA |
| Internet Archive scans | ~20 | public domain |
| arXiv papers | ~15 | CC-BY |
| US-Gov / EU publications | ~15 | PD-US-Gov / CC-BY |
| DergiPark (CC-BY) + DTA-derived | ~10 | CC-BY / PD |
| **Total** | **~100** | |

The Turkish and German slices (DergiPark, DTA-derived, OAPEN's German-language monographs) are not optional filler: they are the only real-document evidence the pipeline gets for the two non-English languages it claims to support, and a holdout that reaches 100 documents by dropping them has not met this target.

---

## 8. How the corpus feeds tests

The corpus is consumed by the three test tiers `TEST_STRATEGY.md` defines in detail; this section states only the corpus-side contract for each:

- **Fast tier** (every PR, D11's `cargo nextest`/`insta`/Vitest/pytest suites): the ~30–50 file fast-CI subset (§7.3) plus regenerated `cargo xtask fixtures` output. No live LLM calls (cassette-replayed only, per `oc-ai`'s test design), no external oracle binaries. Fixtures here are the tiny, hand-made, single-concern kind (R9 §B.2) — one rotated page, one ligature, one hyphenated word broken across a line — each with explicit fact assertions rather than only a snapshot diff, so intentional refactors do not spuriously fail the suite.
- **Integration tier** (a representative ~20-file slice mixing page types per Principle 2's stratification, plus differential testing against `pdftotext`/Calibre oracles per R9 §B.5): still cassette-only for LLM calls, but real external-binary oracle calls are allowed, and this is where the §5.1 three-renderer ground-truth pairs and the §5.5 hand-annotated real-PDF golden set are exercised.
- **Nightly tier**: the full corpus (§7.3's 2–5 GB path), the frozen ≥100-file holdout scored and reported but never fitted against, the McNemar/false-repair-rate paired comparison for any `ai.enabled` LLM path (per `LLM_EVALUATION.md` §7), and the CI regenerate-and-diff determinism check for every synthetic fixture (§6.4). This is also the one tier where cassette re-recording and any bounded live-model calibration runs happen, per D11 and D17's `eval calibrate` process.

Per-stratum, per-category reporting (§1 Principle 2, §5.6) is the contract every tier honors: no test-tier report may present a single aggregate pass rate without the producer-stratum and category breakdown alongside it.

---

## Notes for the Chief Architect

1. **Real-world holdout scale — resolved as an execution target, not a definitional change.** The §3 candidate table produces on the order of 40–60 named real-world candidates, short of Principle 4's ≥ 100. The ratified resolution rejects the per-page shortcut: the holdout unit stays the **document** (Principle 4, §7.6), DocLayNet pages count only in the separately-reported page-level layout stratum, and the gap is closed by sourcing rather than by redefinition — the Phase-7 sourcing target in §7.6. Flagging it still, because the candidate table alone should not be read as "the holdout is already assembled."
2. **Several §2/§3 sources remain individually unverified and are marked as such throughout** (DergiPark per-journal licenses, the Isartor exact file count, Wikisource-TR's live status, WeasyPrint's CSS GCPM/footnote-via-float completeness against a real Standard Ebooks file — R7's own Appendix item 4 flags this last one as worth a spike before committing to WeasyPrint as the primary §5.1 renderer). None of these change the corpus *design*; all of them gate specific files' or tools' actual admission and should be tracked as a checklist against R7 §D.1's per-file verification requirement, not treated as resolved by this document.
