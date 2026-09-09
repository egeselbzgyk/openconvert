# R2 — PDF Parsing/Extraction Libraries & Deterministic Layout Analysis

**Project:** OpenConvert (local-first PDF → reflowable EPUB, deterministic-first, small local LLM only where necessary)
**Track:** R2
**Date:** 2026-09-09
**Status:** Research only. No implementation.
**Constraints driving every judgment below:** low RAM, fast startup, small binary, minimal dependencies, open-source-compatible licensing (the app is open source, but the Chief Architect should assume users may want to fork it into proprietary builds — so copyleft matters).

---

## 0. Executive summary (read this first)

1. **The single biggest architectural decision in R2 is the license of the parsing backend, not its speed.** MuPDF (and therefore PyMuPDF, mupdf-rs, mupdf.js) is AGPL-3.0-or-commercial. PDFium is BSD-3. Poppler is GPL-2/GPL-3. unipdf is proprietary. For an open-source desktop app that wants to remain re-usable, PDFium is the only mature C/C++ engine with a permissive license.

2. **No single library gives you everything.** PDFium gives excellent per-character geometry + font info + rendering under BSD, but deliberately hides the PDF object model (no structure tree, no marked content, no XMP, weak vector-path introspection). MuPDF gives the richest extraction surface in the industry (including built-in `dehyphenate`, `segment`, `structured`, `accurate-bboxes` options) but at AGPL. You will likely need **PDFium + a small object-level parser** (e.g. `lopdf` in Rust, `pypdf` in Python) to recover outlines/tagged structure/XMP.

3. **Deterministic layout analysis is genuinely good enough for the 80% case, and there are hard numbers to prove it.** On UW-III, Voronoi (5.5% text-line error) and Docstrum (6.0%) beat XY-cut (17.1%) decisively (Shafait et al., DAS 2006). For reading order, plain XY-cut hits **100% on Manhattan layouts** but collapses to **75% on OmniDocBench multi-column** and **49.7% on wrap-around layouts** (arXiv 2607.01018). Books — OpenConvert's main target — are overwhelmingly Manhattan. This is the strongest argument for the deterministic-first thesis.

4. **Small CPU layout models are affordable but not free.** Docling's `heron` (RT-DETRv2-r50vd, 42.9M params, **Apache-2.0 weights**) costs **0.643 s/image on a 4-thread AMD EPYC 7763**; `egret-m` (DFINE-m, 19.5M) costs **0.334 s/image** (arXiv 2509.11720). Docling's full pipeline uses **~6.2 GB RAM** for 225 pages. That is a hard conflict with "low RAM, fast startup". Most DocLayNet YOLO checkpoints are **AGPL-3.0** (Ultralytics-derived) — a license trap that is easy to miss.

5. **Tentative recommendation:** Rust host + **PDFium via `pdfium-render`** as primary backend, **`hayro` tracked as the pure-Rust escape hatch** (not yet production-ready), plus **`lopdf`** for object-level access PDFium hides. Deterministic layout stack: Docstrum/whitespace hybrid for segmentation, column-aware XY-cut for reading order, font-size/weight clustering for headings, cross-page repetition for headers/footers, dictionary+frequency dehyphenation. Optional, off-by-default `egret-m` ONNX layout model as an escalation path for pages the heuristics flag as low-confidence.

---

# PART A — PDF parsing / text-extraction libraries (2026 state)

## A.0 What "low-level data" means here — the checklist

For PDF→EPUB you need, in rough priority order:

| Capability | Why EPUB needs it |
|---|---|
| Per-character/glyph bbox + origin | Line/word/paragraph reconstruction, superscript detection, drop caps |
| Font name + size + weight + style flags | Heading detection, bold/italic → `<strong>`/`<em>`, footnote markers |
| Text render mode | Skip invisible OCR layers (mode 3) that would duplicate text |
| Clipping state | Skip text clipped out of view |
| Image XObjects + SMask | Extract figures with correct transparency |
| Vector paths | Table ruling-line detection; drop-cap/ornament detection |
| Annotations / links | Preserve internal cross-references as EPUB links |
| Outlines / bookmarks | Free, high-quality EPUB TOC when present |
| Tagged-PDF structure tree / marked content | The single best signal that exists — when present |
| XMP metadata | Title/author/language for OPF |
| Page raster rendering | OCR fallback + visual QA / diffing |

Only MuPDF and PDFBox come close to covering all of it in one library.

---

## A.1 MuPDF (C) — and PyMuPDF / mupdf-rs / mupdf.js

**License: AGPL-3.0-or-later, dual-licensed commercially by Artifex.** Artifex states: "Most of our products are dual-licensed under open source with the GNU AGPLv3 license or with commercial license agreements", and that AGPL use means "you cannot use these products in server-based applications without disclosing your own application's full source code under AGPL" ([artifex.com/licensing](https://artifex.com/licensing)). The PyMuPDF maintainer's own words: *"To comply with the open source AGPL you must remain open source and freeware"* ([PyMuPDF Discussion #971](https://github.com/pymupdf/PyMuPDF/discussions/971)). Artifex acquired PyMuPDF outright ([PDF Association](https://pdfa.org/artifex-software-acquires-pymupdf/)), so there is no "the binding is more permissive than the engine" escape.

**Low-level data exposed — best in class.** MuPDF's structured-text (`stext`) extraction is the richest of any library surveyed. Options include ([MuPDF stext options](https://mupdf.readthedocs.io/en/latest/reference/common/stext-options.html)):

- `preserve-ligatures` — do not expand ligatures into constituents (i.e. by default MuPDF *does* de-ligature for you)
- `preserve-whitespace`, `preserve-spans`, `inhibit-spaces`
- **`dehyphenate`** — "Attempt to join up hyphenated words" (built in!)
- `accurate-bboxes` — "Calculate char bboxes from the outlines"; `accurate-ascenders`; `accurate-side-bearings`
- `clip` / `clip-rect=x0:y0:x1:y1` — "Do not include text that is completely clipped"
- `collect-styles` — detect fake bold, strikeout, underline
- **`structured`** — collect structure markup (tagged PDF)
- **`segment`** — "Attempt page segmentation"; **`table-hunt`** — hunt for tables in segmented pages
- `vectors` / `lazy-vectors` / `fuzzy-vectors` — vector bboxes in output, merging abutting rules
- `use-cid-for-unknown-unicode`, `use-gid-for-unknown-unicode`, `ignore-actualtext`
- `paragraph-break`

That list is remarkable: MuPDF ships deterministic dehyphenation, page segmentation, and table hunting *inside the extraction engine*. If licensing were not an issue this would shorten OpenConvert's Part B work substantially.

**PyMuPDF extraction variants and their relative cost** ([PyMuPDF Appendix 1](https://pymupdf.readthedocs.io/en/latest/app1.html)), baseline `TEXT` = 1.00:

| Variant | Relative time | What it gives |
|---|---|---|
| TEXT | 1.00 | plain text, `sort=True` for TL→BR reorder |
| BLOCKS | 1.00 | block bboxes + concatenated lines |
| WORDS | 1.02 | word bboxes + (block, line, word) indices |
| XML | 2.72 | char-level: font, size, color, coordinates |
| XHTML | 3.32 | semantic text + images |
| HTML | 3.54 | full layout, font refs, positions |
| DICT | 3.93 | blocks/lines/spans + bboxes, binary images |
| RAWDICT | 4.50 | **per-character** dicts: origin, bbox, char |

Critical operational note from the same page: **excluding images drops RAWDICT from 4.50× to 1.68×** — "images have a very significant impact". For OpenConvert, extract text and images in separate passes.

PyMuPDF's own benchmark ([Appendix 4](https://pymupdf.readthedocs.io/en/latest/app4.html)) claims text extraction is 3.42× faster than XPDF, 12.69× faster than PyPDF2, 28.37× faster than PDFMiner; rendering at 150 DPI 1.76× faster than XPDF. Independent third-party numbers agree on the ballpark (see A.11).

**Rendering:** yes, full raster + SVG + display lists.

**Packaging:** PyMuPDF bundles MuPDF as a C engine with **no mandatory external dependencies**; wheels for Python 3.10–3.14 on Windows (x86/x64), macOS (x86_64/arm64), Linux manylinux x86_64/aarch64 and musllinux x86_64 ([PyMuPDF README](https://github.com/pymupdf/PyMuPDF)).

**mupdf-rs (Rust):** **AGPL-3.0**, v0.8.0 released 2026-06-22. Builds MuPDF from source via `mupdf-sys`; "requires a C/C++ toolchain and libclang for bindgen", plus Fontconfig dev package on Linux. No wasm/Android system-font support. 201 stars ([messense/mupdf-rs](https://github.com/messense/mupdf-rs)). **Build complexity is the highest of any option in this report** — bindgen + a full C++ build in your CI for three desktop platforms.

**mupdf.js (WASM, Node/browser/Bun/Deno):** maintained by Artifex, **AGPL v3 + commercial**. `StructuredText.asJSON(scale)` returns blocks → lines → `{wmode, bbox, font{name,family,weight,style,size}, x, y, text}`. Note the documented gotcha: *"You must extract the structured text with 'preserve-spans'!"* for font changes to appear in JSON ([mupdf.js](https://github.com/ArtifexSoftware/mupdf.js), [StructuredText ref](https://mupdf.readthedocs.io/en/latest/reference/javascript/types/StructuredText.html)).

**Security / CVE volume** ([stack.watch/product/artifex/mupdf](https://stack.watch/product/artifex/mupdf/)):

| Year | 2018 | 2019 | 2020 | 2021 | 2022 | 2023 | 2024 | 2025 | 2026 |
|---|---|---|---|---|---|---|---|---|---|
| CVEs | 19 | 5 | 3 | 3 | 1 | 8 | 3 | 2 | 5 |

Notable 2026: **CVE-2026-3308** — integer overflow in `pdf_image.c` → heap OOB write → potential arbitrary code execution, CVSS 7.8.

**Verdict:** technically the best engine in this report, and the AGPL makes it unusable for OpenConvert unless the whole project accepts AGPL *and* accepts that downstream forks inherit it. **Flagged as a license trap.**

---

## A.2 PDFium (BSD-3) — and pdfium-render / pypdfium2

**License: BSD-3-Clause** (Chromium's PDF engine). The popular prebuilt distribution `bblanchon/pdfium-binaries` is itself MIT for the build scripts ([repo](https://github.com/bblanchon/pdfium-binaries)).

**Low-level data exposed** ([public/fpdf_text.h](https://pdfium.googlesource.com/pdfium/+/refs/heads/main/public/fpdf_text.h)) — per-character, which is exactly the granularity layout analysis wants:

- `FPDFText_GetCharBox` — tight per-char bbox; `FPDFText_GetLooseCharBox` — full glyph bounds
- `FPDFText_GetCharOrigin` — x,y origin
- `FPDFText_GetMatrix` — effective transform per character
- `FPDFText_GetCharAngle` — rotation in radians
- `FPDFText_GetFontSize` (points), `FPDFText_GetFontInfo` (**name + PDF font flags**), `FPDFText_GetFontWeight`
- `FPDFText_GetFillColor`, `FPDFText_GetStrokeColor` (RGBA)
- `FPDFText_IsGenerated` (chars PDFium synthesized, e.g. inferred spaces — **important**: you must filter these or they pollute geometry), `FPDFText_IsHyphen`, `FPDFText_HasUnicodeMapError`
- `FPDFText_CountRects`/`GetRect`, `FPDFText_GetBoundedText`, `FPDFText_GetTextObject`
- Search: `FindStart/Next/Prev`, `GetSchResultIndex`, `GetSchCount`

Font *flags* from `GetFontInfo` give serif/italic/fixed-pitch/symbolic per the PDF spec — enough for bold/italic runs without heuristics.

**What PDFium deliberately does NOT expose** — this is the key limitation and pypdfium2 states it plainly: *"PDFium's public interface does not provide access to the raw PDF data structure"* and lacks *"APIs to read/write PDF dictionaries, streams, name/number trees"* ([pypdfium2 README](https://github.com/pypdfium2-team/pypdfium2)). Consequences for OpenConvert:

- **No tagged-PDF structure tree / marked-content access** — you lose the single best structural signal when a PDF is properly tagged.
- **No XMP metadata** access.
- Outlines/bookmarks: available via `fpdf_doc.h` bookmark APIs (present in the public API), links likewise. [UNVERIFIED at the exact-signature level — I did not fetch `fpdf_doc.h`.]
- Vector paths: available through the page-object API (`FPDFPageObj_*`, path segments), but coarser than MuPDF's device model.
- Image XObjects with SMask: `FPDFImageObj_*` exists including bitmap extraction; SMask handling is not directly surfaced as a first-class concept. [UNVERIFIED]

**Rendering:** yes — this is PDFium's primary job. Full raster with configurable scale/rotation/flags.

**Performance & memory.** The PDF Association's cross-engine survey ([Survey of Open-Source Solutions](https://pdfa.org/wp-content/uploads/2021/06/Survey-of-OpenSource-Solutions.pdf), i7-960 @ 1.6 GHz):

| Corpus | MuPDF | PDFium | Poppler | Ghostscript |
|---|---|---|---|---|
| Ghent 600dpi (4 pp) | 7.5 s | 7.9 s | 17.6 s | 9.0 s |
| Altona 600dpi (17 pp) | 17.9 s | 22.8 s | 73.5 s | 34.9 s |
| PDF 1.7 spec 150dpi (1310 pp) | **60.4 s (PDFium)** | — | 148.3 s | 151.2 s |

(On the 1310-page text-heavy corpus PDFium is *fastest* at 60.4 s vs MuPDF 99.1 s — the corpus most like a book.) Peak memory on the same corpus: XPDF 31 MB, MuPDF 87 MB, Poppler 91 MB, Ghostscript 131 MB. On the graphics-heavy 600dpi corpora MuPDF ballooned to 325–542 MB. **Caveat: this survey is from 2021; treat as directional, not current.**

**Binary size.** `pdfium-binaries` publishes shared libraries only (.so/.dll/.dylib). Published archive sizes for the mobile targets in release 151.0.7891.0 (2026-06-15): Android arm 2.65 MB, arm64 3.17 MB, x64 3.28 MB; iOS arm64 3.11–3.34 MB. Desktop archive sizes were not listed on the release page I fetched; expect the same 3–5 MB compressed / ~8–12 MB uncompressed order of magnitude for a no-V8, no-XFA build. [UNVERIFIED — desktop figures]

**Platforms:** Android (arm/arm64/x64/x86), iOS (arm64/x64, catalyst/device/simulator), Linux (arm/arm64/ppc64/x64/x86, **glibc and musl**), macOS (arm64/x64/**universal**), Windows (arm64/x64/x86), experimental WASM. Two variants: with and without V8. **Builds triggered automatically every Monday since 2017** — 410 releases; latest 151.0.7891.0 on 2026-06-15.

**Build complexity:** building PDFium *yourself* is genuinely painful (Chromium's `depot_tools` + `gn` + `ninja`; the getting-started doc shows manual static-lib link lines including libpng/libjpeg/freetype, `-framework AppKit` on macOS, `-lstdc++`) ([getting-started.md](https://pdfium.googlesource.com/pdfium/+/HEAD/docs/getting-started.md)). **Do not build it. Consume `bblanchon/pdfium-binaries`.** That single decision converts PDFium from "hardest to build" to "easiest to ship".

**Bindings:**

- **`pdfium-render` (Rust)** — v0.9.4, released 2026-09-06 (three days ago), **MIT OR Apache-2.0**, MSRV 1.61 (1.80.1 with the `image` feature). Exposes `PdfPageText::chars()` → `PdfPageTextChar` with `bounds()`, `loose_bounds()`, `font_size()`, `unscaled_font_size()`, `font_name()`, `font_weight()`, `fill_color()`, `text_render_mode()`, `is_generated()`, `is_hyphen()`. Page objects: paths, images, text, XObject forms. **Links PDFium dynamically at runtime via `libloading`** (with optional static-link and WASM features) — meaning your Rust binary stays small and PDFium ships as a sidecar .so/.dll/.dylib you can update independently ([docs.rs/pdfium-render](https://docs.rs/pdfium-render/latest/pdfium_render/)). This maps *very* well onto "small binary, fast startup".
- **`pypdfium2` (Python)** — v5.10.1 (June 2026), **Apache-2.0 / BSD-3-Clause** dual, ctypes bindings, wheels for Linux/Windows/macOS + experimental Android/Termux, 113 releases, active CI. Documented caveats: **not thread-safe**, and raw-API use can violate object lifetimes ([pypdfium2](https://github.com/pypdfium2-team/pypdfium2)).

**Security / CVE volume:** PDFium is the most-attacked PDF engine on earth (it is Chrome's). 2026 alone shows a steady stream of use-after-free and buffer-overflow CVEs (e.g. CVE-2026-6306 buffer overflow, CVE-2026-11303 UAF RCE, CVE-2026-5889 crypto flaw exposing encrypted PDFs) ([SentinelOne CVE-2026-6306](https://www.sentinelone.com/vulnerability-database/cve-2026-6306/), [TheHackerWire CVE-2026-11303](https://www.thehackerwire.com/chrome-pdfium-use-after-free-rce-cve-2026-11303/)). **Read this the right way:** high CVE *volume* here is a proxy for enormous fuzzing investment and a weekly patch cadence, not for poor quality. But it does mean OpenConvert must (a) track `pdfium-binaries` weekly builds, and (b) treat PDF parsing as untrusted-input processing — ideally in a subprocess or with OS sandboxing.

**Verdict: the default choice for a permissively licensed, low-RAM, cross-platform desktop app.**

---

## A.3 Poppler (GPL) — pdftotext / pdftohtml / pdfalto

**License: GPL-2.0/GPL-3.0.** For an open-source EPUB tool this is *usable* but viral, and it kills any proprietary fork. Also, on Windows/macOS shipping Poppler means shipping a GPL binary blob users must be able to relink.

**`pdftotext` options** ([pdftotext(1)](https://www.mankier.com/1/pdftotext)): `-layout` (preserve physical layout), `-raw` (content-stream order), `-bbox` (XHTML with per-word bboxes), **`-bbox-layout`** ("bounding box information for each **block, line, and word**"), `-tsv` (TSV with bbox), `-htmlmeta`, `-fixed <n>`, `-nodiag` (drop diagonal text), `-enc`. `-bbox-layout` is the classic "cheap structured extraction from a subprocess" trick and gives you a three-level hierarchy for free.

**Maintenance:** excellent. Monthly cadence, latest **26.09.0 on 2026-09-03**; 26.08.0 (Aug 2), 26.07.0 (Jul 2), 26.06.0 (Jun 2), 26.05.0, 26.04.0 ([Poppler releases](https://poppler.freedesktop.org/releases.html)).

**Security / CVE volume** ([stack.watch/product/freedesktop/poppler](https://stack.watch/product/freedesktop/poppler/)):

| Year | 2018 | 2019 | 2020 | 2021 | 2022 | 2023 | 2024 | 2025 |
|---|---|---|---|---|---|---|---|---|
| CVEs | 11 | 17 | 2 | 1 | 3 | 9 | 2 | 0 |

Notable: CVE-2024-56378 (JBIG2Bitmap::combine OOB read), CVE-2022-38784 (JBIG2 integer overflow, 7.8 High).

**pdfalto** ([kermitt2/pdfalto](https://github.com/kermitt2/pdfalto)) deserves a separate mention: **GPL-2.0**, a fork of `pdf2xml` built on **Xpdf 4.05** (not Poppler proper), latest v0.6.0 **Feb 2026**, actively maintained. It emits ALTO XML with block/line/token-level text plus font name, bold/italic style, and coordinates, plus sidecar files `_metadata.xml`, `_annot.xml` (annotations + links), `_outline.xml` (embedded TOC), and a `.xml_data/` dir of extracted vector and bitmap images. **This is the exact feature set GROBID needs, and it is a good reference design for what OpenConvert's intermediate representation should contain** — even if you can't use the GPL code itself.

**Verdict:** great reference implementation and a fine *optional* backend on Linux where Poppler is already installed. Not the primary.

---

## A.4 pdf.js (Apache-2.0)

**License: Apache-2.0.** Community-driven, supported by Mozilla ([mozilla/pdf.js](https://github.com/mozilla/pdf.js)).

**Data exposed:** `page.getTextContent()` returns items with `str`, `dir`, `width`, `height`, `transform` (the full 6-element text matrix — so you get position *and* scale/rotation), `fontName` (an internal id you resolve via `commonObjs`), and `hasEOL`. **It does not give per-glyph boxes**; this is a long-standing, explicitly acknowledged limitation ([issue #7996 "Calculating the position of individual glyphs in getTextContent"](https://github.com/mozilla/pdf.js/issues/7996), [issue #12884 "Get all glyphs and their bounding boxes post rendering?"](https://github.com/mozilla/pdf.js/issues/12884)). You get item-level (roughly span-level) geometry and must reconstruct char positions from advance widths. For a layout engine that wants char boxes for superscript/drop-cap detection, this is a real handicap.

**Rendering:** yes in browser (canvas). In Node it requires the `canvas` package (native build) or `@napi-rs/canvas`, which reintroduces a native dependency — undercutting the "pure JS" advantage.

**Security:** **CVE-2024-4367** — arbitrary JavaScript execution on opening a malicious PDF, via the font path when `isEvalSupported` is enabled ([Mozilla advisory GHSA-wgrm-67xf-hhpq](https://github.com/mozilla/pdf.js/security/advisories/GHSA-wgrm-67xf-hhpq), [Codean Labs writeup](https://codeanlabs.com/2024/05/cve-2024-4367-arbitrary-js-execution-in-pdf-js/)). Mitigation is `isEvalSupported: false`. This is a reminder that "memory-safe language" ≠ "safe PDF parser".

**Verdict:** the right answer *if and only if* the host language is TypeScript/Node and you accept item-level geometry. Otherwise it is strictly weaker than PDFium for this task.

---

## A.5 pdfminer.six / pdfplumber (MIT)

**pdfminer.six** — MIT, pure Python. Its value is not speed but that **`LAParams` is a documented, tunable, deterministic layout engine** ([composable analysis reference](https://pdfminersix.readthedocs.io/en/latest/reference/composable.html)):

| Param | Default | Meaning |
|---|---|---|
| `line_overlap` | 0.5 | overlap fraction (of shorter char height) to call chars same-line |
| `char_margin` | 2.0 | max gap (relative to char width) to keep chars in one line |
| `word_margin` | 0.1 | gap above which a space is inserted |
| `line_margin` | 0.5 | max gap (relative to line height) to group lines into a paragraph |
| **`boxes_flow`** | 0.5 | −1.0 → order purely by horizontal position; +1.0 → purely vertical; `None` disables the advanced pass and orders by bottom-left corner |
| `detect_vertical` | False | vertical (CJK) text |
| `all_texts` | False | run layout analysis inside figures |

`boxes_flow` is the closest thing to a "standard" tunable reading-order knob in the Python ecosystem and is worth reimplementing (it is ~50 lines of logic, not a dependency).

**pdfplumber** — MIT, built on pdfminer.six, actively maintained by Jeremy Singer-Vine and Samkit Jain, tested on Python 3.10–3.14 ([jsvine/pdfplumber](https://github.com/jsvine/pdfplumber)). Char objects expose `text, fontname, size, adv, upright, height, width, x0, x1, y0, y1, top, bottom, doctop, matrix, mcid, tag, stroking_color, non_stroking_color, object_type`. **Note `mcid` and `tag`** — pdfplumber surfaces marked-content ids, which PDFium cannot. Table strategies: `lines`, `lines_strict`, `text`, `explicit`, with snap/join/intersection tolerances. `extract_text(layout=True)` mimics physical layout via `x_density`/`y_density`. Built-in `.to_image()` visual debugging without external tools.

**Cost:** slow. In the py-pdf benchmark pdfplumber averaged **9.5 s** vs PyMuPDF/pypdfium2 at **0.1 s**, and — surprisingly — scored **lowest on quality at 75%** ([py-pdf/benchmarks](https://github.com/py-pdf/benchmarks)).

**Verdict:** superb prototyping/ground-truth tool and a reference for layout heuristics. Too slow and too RAM-hungry to be a shipping backend.

---

## A.6 pypdf (BSD-3)

Pure Python, BSD-3-Clause, actively maintained under the py-pdf org (10.2k stars, 2,516 commits) ([py-pdf/pypdf](https://github.com/py-pdf/pypdf)). Handles split/merge/crop/transform, text extraction, encryption, metadata, annotations. Benchmarked at **3.5 s** average with **96% quality** — notably *better quality than pdfplumber* at ~1/3 the time ([py-pdf/benchmarks](https://github.com/py-pdf/benchmarks)).

**Its real value for OpenConvert is object-model access**, not extraction speed: outlines, `/StructTreeRoot`, `/Metadata` XMP, named destinations — exactly the things PDFium hides. In a Python host, **pypdfium2 + pypdf** is a natural pairing (fast geometry + object model), both permissive.

No rendering.

---

## A.7 Rust-native crates

| Crate | Version / date | License | Text extraction? | Rendering? | Verdict |
|---|---|---|---|---|---|
| **`lopdf`** | 0.35.0, 2025-01-19 | MIT | No (object-level only; `replace_text()` exists) | No | Mature, 94,974 dl/mo, 138 dependents. **Use it for the object model PDFium hides.** ~263k SLoC. ([lib.rs/lopdf](https://lib.rs/crates/lopdf)) |
| **`pdf` (pdf-rs)** | 0.9.0, 2023-11-30 | MIT | Partial | Separate pathfinder-based renderer | ~19k dl/mo. **No release in ~3 years — treat as semi-stale.** ([lib.rs/pdf](https://lib.rs/crates/pdf)) |
| **`pdf-extract`** | 0.7.7, 2024-05-10 | [UNVERIFIED — MIT per repo, not confirmed on lib.rs page] | Plain text only (`extract_text_from_mem()`); no documented position/font API | No | 5,547 dl/mo, 350 KB crate, ~17 MB of deps. **Too coarse for layout work.** ([lib.rs/pdf-extract](https://lib.rs/crates/pdf-extract)) |
| **`hayro`** | `hayro` 0.4.0 (2025-10-02); `hayro-interpret` **0.7.0 (2026-07-13)** | Apache-2.0 (repo says dual Apache-2.0/MIT) | **No dedicated text-extraction API** — you implement the `Device` trait and intercept glyph draw ops | **Yes** — CPU rasterizer to PNG, plus SVG via `hayro-svg` | See below |
| **`pdf_oxide` / "PDF Oxide"** | 0.3.69, 2026-06-27 | MIT OR Apache-2.0 | **Yes** — `extract_spans()` (font name + size), `extract_chars()` (char positions + font) | Yes (optional `rendering` feature) | See below |
| **`pdfsink-rs`** | 0.2.14 | MIT | Yes (text/word/line/table/layout) | PNG/JPEG | Built **on top of** `lopdf` + `pdf-extract`. Only 36 commits, 20 stars. **Too immature to depend on.** ([clark-labs-inc/pdfsink-rs](https://github.com/clark-labs-inc/pdfsink-rs)) |

### `hayro` — the pure-Rust escape hatch to watch

Pure Rust, **forbids `unsafe`**, no GPU dependency. Crates: `hayro` (raster), `hayro-interpret` (PDF → abstract `Device` commands), `hayro-syntax` (low-level parsing), `hayro-write`, `hayro-svg`, plus `hayro-jpeg2000` / `hayro-jbig2` / `hayro-ccitt` decoders and `hayro-postscript` / `hayro-cmap`. Self-described as *"the most comprehensive and feature-complete implementation of a PDF rasterizer in pure Rust"*, validated against 1,400+ PDFs. 679 stars, 1,722 commits, 97.9% Rust, MSRV **Rust 1.92**. ~1 MB crate, ~15 MB of dependencies (~332k SLoC). 24,551 downloads/month. ([LaurenzV/hayro](https://github.com/LaurenzV/hayro), [lib.rs/hayro](https://lib.rs/crates/hayro), [docs.rs/hayro-interpret](https://docs.rs/hayro-interpret))

The `Device` trait surface is genuinely useful for layout work: `ClipPath`, `StrokeProps`, `RasterImage`, `StencilImage`, `SoftMask` (i.e. SMask!), `Paint`, `BlendMode`, `FillRule`, `GlyphDrawMode`, `PathDrawMode`, `Function`/`TransferFunction`. Because you implement the device yourself, you can capture **exactly** what OpenConvert needs (glyph + transform + clip + render-mode) rather than accepting a library's opinion.

**The catch, in the maintainer's own framing:** *"experimental, work-in-progress"*, *"in very development stage"*, the API *"currently lacks pretty much any documentation"*, performance optimization is explicitly **not** a current priority, and there is **no encrypted-PDF support**, no blending/isolation/knockout groups, no color-key masking.

**On the "Typst folks" question:** the repo is `LaurenzV/hayro` (also mirrored at `pixelrootdev/hayro`). It is **not** under the `typst` GitHub org, and the README makes no claim of a Typst relationship. LaurenzV is a well-known Typst/krilla contributor, so the association is real at the person level but **not** at the project level. [Corrected from the brief's premise.]

### `pdf_oxide` — promising but treat vendor numbers with suspicion

Exists and is real: [github.com/yfedoseev/pdf_oxide](https://github.com/yfedoseev/pdf_oxide), 870 stars, MIT OR Apache-2.0, full Rust core with bindings for 19 languages, v0.3.69 (2026-06-27). Exposes `extract_spans()` and `extract_chars()` with positions and font info, optional rendering, PDF/A validation, form fields, annotations, OCR via PaddleOCR/ONNX.

Its published benchmark table ([pdf.oxide.fyi](https://pdf.oxide.fyi/rust/docs/comparison/vs-lopdf)):

| Library | Mean | Pass rate |
|---|---|---|
| PDF Oxide | **0.8 ms** | 100% |
| PyMuPDF | 4.6 ms | 99.3% |
| pypdfium2 | 4.1 ms | 99.2% |
| pypdf | 12.1 ms | 98.4% |
| pdfplumber | 23.2 ms | 98.8% |

**These are vendor-published, self-selected numbers with no described corpus. [UNVERIFIED]** A 5× speed advantage over MuPDF in pure Rust is an extraordinary claim; it most likely reflects a different workload definition (e.g. lazy parsing) rather than a like-for-like comparison. Worth a hands-on bake-off in Round 2; do not architect around it yet.

---

## A.8 Go

| Library | License | Notes |
|---|---|---|
| **`pdfcpu`** | Apache-2.0 | Pure Go, "minimal external dependencies", 1,122 commits, maintained by Horst Rutter (PDF Association member). Validate/optimize/split/trim/merge/encrypt/watermark/stamp/booklet/portfolio, extract images/fonts/metadata/content. **No page→bitmap rendering.** Text extraction is not a headline feature. ([pdfcpu/pdfcpu](https://github.com/pdfcpu/pdfcpu), [pdfcpu.io](https://pdfcpu.io/)) |
| **`ledongthuc/pdf`** | BSD-3 [UNVERIFIED] | Minimal, low-activity text extraction. Suitable for toy use only. ([pkg.go.dev](https://pkg.go.dev/github.com/ledongthuc/pdf)) |
| **`unipdf` (UniDoc)** | **PROPRIETARY** | LICENSE.md: *"This software package is a commercial product and requires a license code to operate."* EULA at unidoc.io/eula. **Not open source. Hard disqualification.** ([unipdf LICENSE.md](https://github.com/unidoc/unipdf/blob/master/LICENSE.md)) |

**Verdict: Go is the weakest host language for this project.** No permissive, mature, render-capable, glyph-level Go PDF library exists. You would end up cgo-binding PDFium anyway, at which point Rust is the better host.

---

## A.9 .NET — PdfPig (Apache-2.0)

**This is the sleeper pick, because it is the only library in the survey that ships deterministic layout analysis as a first-class, documented feature set.**

- **License: Apache-2.0.** v0.1.14 "Wessex Saddleback", **2026-03-22**; 1,849 commits, 2.5k stars, 319 forks. Alpha packages continue through 0.1.15-alpha (2026-05) ([UglyToad/PdfPig](https://github.com/UglyToad/PdfPig), [NuGet](https://www.nuget.org/packages/PdfPig/)).
- **Explicit API-stability warning:** *"While the version is below 1.0.0 minor versions will change the public API without warning."*
- **Data exposed:** `Letter` objects with text value, location, width, **font size in unscaled relative text units**, font name, and *"a rectangle which is the smallest rectangle that completely contains the visible region of the letter/glyph"* — i.e. true **glyph bounding boxes**, better than pdf.js and comparable to PDFium's `GetLooseCharBox`. Plus `page.GetImages()`, annotations (read-only), bookmarks/outlines, AcroForms (read-only), hyperlinks, embedded files, document metadata, encrypted PDFs. Lineage: a port of Apache PDFBox.
- **Not documented as exposed:** marked content / structure tree, SMask specifics, XMP, vector paths. [UNVERIFIED — some may exist undocumented.]
- **Rendering:** **not built in.** Use the separate **`PdfPig.Rendering.Skia`** (BobLd), v0.1.16.2 on NuGet ([repo](https://github.com/BobLd/PdfPig.Rendering.Skia)) — cross-platform PDF→image via SkiaSharp. That adds a SkiaSharp native dependency (~10–20 MB across RIDs). [UNVERIFIED — exact size]
- **Layout analysis** — see Part B; this is the crown jewel.

---

## A.10 Java — PDFBox (Apache-2.0) / Tika

PDFBox 3.0.8 is the current feature release (2.0.37 for the 2.x line); **3.0.x requires Java 8** per the download page, while the repo's build instructions say Java 11+ to build ([pdfbox.apache.org/download.cgi](https://pdfbox.apache.org/download.cgi), [apache/pdfbox](https://github.com/apache/pdfbox)). Apache-2.0. 3.1k stars, 13,321 commits — healthy.

`PDFTextStripper` + `TextPosition` gives per-glyph x/y, width, height, font, and font size; `PDFRenderer` rasterizes pages; `xmpbox` handles XMP; marked-content and the structure tree are reachable via `PDStructureTreeRoot`. **It is the most complete Apache-licensed PDF library in existence.**

**Disqualifier for OpenConvert:** a JVM. Fast startup and low RAM are explicit priorities; a JVM costs ~50–100 MB RSS and hundreds of ms of startup before any PDF is opened. Tika inherits the same problem plus a much larger dependency graph.

---

## A.11 Cross-library benchmark (independent)

[py-pdf/benchmarks](https://github.com/py-pdf/benchmarks), Intel i7-6700HQ @ 2.60 GHz, 14 PDFs from 284.8 KiB to 14.7 MiB, versions current through June 2025:

| Library | Avg speed | Avg quality |
|---|---|---|
| PyMuPDF | **0.1 s** | 96% |
| pypdfium2 | **0.1 s** | **97%** |
| Tika | 0.2 s | 95% |
| pdftotext (Poppler) | 0.3 s | 91% |
| pypdf | 3.5 s | 96% |
| pdfminer.six | 5.8 s | 89% |
| pdfplumber | 9.5 s | 75% |

**The headline finding for OpenConvert: pypdfium2 matches PyMuPDF on speed and beats it on quality — under BSD instead of AGPL.** That single row is the strongest empirical support for the PDFium recommendation.

---

## A.12 Part A summary matrix

| Library | License | Glyph bbox | Font name/size/flags | Render mode | Clip | Images+SMask | Vector paths | Annots/links | Outlines | Tagged/marked content | XMP | Renders pages | 2025–26 activity |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| MuPDF | **AGPL/comm** | ✅ (`accurate-bboxes`) | ✅ + `collect-styles` | ✅ | ✅ (`clip`, `clip-rect`) | ✅ (`SoftMask`) | ✅ (`vectors`) | ✅ | ✅ | ✅ (`structured`) | ✅ | ✅ | High |
| PDFium | **BSD-3** | ✅ tight + loose | ✅ name/size/weight/flags | ✅ | ⚠️ indirect | ✅ / SMask [UNVERIFIED] | ⚠️ page-object level | ✅ | ✅ | ❌ | ❌ | ✅ | Weekly builds |
| Poppler | **GPL** | ✅ (`-bbox-layout`: block/line/word) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ⚠️ partial | ✅ | ✅ | Monthly |
| pdf.js | Apache-2.0 | ❌ item-level only | ⚠️ id → resolve | ⚠️ | ⚠️ | ✅ | ✅ | ✅ | ✅ | ⚠️ | ✅ | ✅ (canvas) | High |
| pdfminer.six | MIT | ✅ | ✅ | ✅ | ⚠️ | ✅ | ✅ | ✅ | ✅ | ✅ (mcid) | ⚠️ | ❌ | Moderate |
| pdfplumber | MIT | ✅ | ✅ | ⚠️ | ⚠️ | ✅ | ✅ | ✅ | ✅ | ✅ (`mcid`, `tag`) | ⚠️ | ✅ (debug) | High |
| pypdf | BSD-3 | ⚠️ visitor-based | ⚠️ | ⚠️ | ❌ | ✅ | ⚠️ | ✅ | ✅ | ✅ | ✅ | ❌ | High |
| lopdf | MIT | ❌ | ❌ | ❌ | ❌ | ⚠️ raw | ⚠️ raw | ✅ raw | ✅ raw | ✅ raw | ✅ raw | ❌ | Moderate |
| hayro | Apache-2.0/MIT | ✅ via `Device` | ✅ via `Device` | ✅ (`GlyphDrawMode`) | ✅ (`ClipPath`) | ✅ (`SoftMask`) | ✅ | ⚠️ | ⚠️ | ❌ | ❌ | ✅ | High (experimental) |
| pdf_oxide | MIT/Apache-2.0 | ✅ `extract_chars` | ✅ `extract_spans` | ⚠️ | ⚠️ | ✅ | ⚠️ | ✅ | ✅ | ⚠️ | ✅ (PDF/A) | ✅ optional | High |
| pdfcpu | Apache-2.0 | ❌ | ⚠️ | ❌ | ❌ | ✅ extract | ⚠️ | ✅ | ✅ | ⚠️ | ✅ | ❌ | Moderate |
| unipdf | **PROPRIETARY** | — | — | — | — | — | — | — | — | — | — | — | — |
| PdfPig | Apache-2.0 | ✅ | ✅ | ⚠️ | ⚠️ | ✅ | ⚠️ | ✅ | ✅ | ⚠️ | ⚠️ | via Skia add-on | High |
| PDFBox | Apache-2.0 | ✅ `TextPosition` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ `PDStructureTreeRoot` | ✅ `xmpbox` | ✅ | High |

✅ = supported / ⚠️ = partial, indirect, or unverified / ❌ = not available

---

# PART B — Deterministic layout analysis: what is proven to work

## B.1 The foundational benchmark everyone cites

**Shafait, Keysers & Breuel, "Performance Comparison of Six Algorithms for Page Segmentation", DAS 2006** ([PDF](https://link.springer.com/content/pdf/10.1007/11669487_33.pdf)). Dataset: **UW-III, 978 images** (100 train / 878 test). Metric: text-line detection error ρ = |C ∪ S ∪ M| / |G| (missed lines, split boxes, horizontal merges).

| Algorithm | Error rate | Std dev |
|---|---|---|
| **Voronoi (Kise)** | **5.5%** | 12.3% |
| **Docstrum (O'Gorman)** | **6.0%** | 15.2% |
| Constrained text-line finding (Breuel) | 8.5% | 14.4% |
| **Whitespace analysis (Breuel)** | 9.8% | 18.3% |
| Smearing (RLSA) | 14.2% | 23.0% |
| **X-Y cut** | **17.1%** | 24.4% |

Stated weaknesses: X-Y cut *"fails in the presence of noise and tends to take the whole page as one segment"*; smearing misclassifies noise-merged text as non-text; whitespace analysis is *"sensitive to stopping rule parameters"*. All six ran in **under 7 seconds** on a 2 GHz AMD box (2006 hardware) — i.e. all are effectively free on modern CPUs.

**Interpretation for OpenConvert:** this benchmark is on *scanned images*. Born-digital PDFs give you exact glyph boxes with zero noise, which is precisely the condition where X-Y cut's noise weakness disappears and where its error rate should be far below 17.1%. **Do not read 17.1% as "XY-cut is bad for our use case."** But do note the ordering: Docstrum and Voronoi are more robust across the board, and Docstrum is the one that has a well-tested open implementation you can read (PdfPig).

Also relevant: **Breuel, "Two Geometric Algorithms for Layout Analysis", DAS 2002** ([Springer](https://link.springer.com/content/pdf/10.1007/3-540-45869-7_23.pdf)) — the maximal-empty-rectangle whitespace algorithm and constrained text-line finding. This is the algorithm PdfPig implements as `WhitespaceCoverExtractor`.

## B.2 Reading order — the numbers that matter most

**"Reading Order Inference for Complex Document Layouts"** (arXiv [2607.01018](https://arxiv.org/html/2607.01018v1), 2026):

| Layout type | XY-Cut | LayoutReader | Proposed (LM+NSP ensemble) |
|---|---|---|---|
| **Manhattan layouts** | **100%** | — | 96.0% |
| OmniDocBench multi-column English (140 pages) | 75% | 25% | 88% |
| ALTO wrap-around (Glossa Ordinaria, 23 geometries) | 49.7% | — | 94.8% |

The paper's XY-Cut baseline is PaddleOCR PP-StructureV3's implementation. **Read the first row carefully: on Manhattan layouts — which is what a novel, a textbook, or a technical manual is — plain recursive XY-cut is exactly right, and the learned method is *worse* (96.0% vs 100%).** This is the strongest single result supporting OpenConvert's deterministic-first thesis. It also tells you precisely where deterministic methods fail: wrap-around and non-Manhattan layouts (magazines, newspapers, medieval glosses), which are exactly the documents that make bad EPUBs anyway.

**XY-Cut++** (arXiv [2504.10258](https://arxiv.org/html/2504.10258v3), 2025) is the state of the art in *geometric* reading order and worth reimplementing. Three ideas:
1. **Pre-mask**: temporarily mask high-dynamic elements (titles, figures, tables) so they don't fragment the core text sort, then remap them via IoU-weighted distance.
2. **Multi-granularity segmentation**: choose split direction (H vs V) from *regional content density* rather than fixed thresholds.
3. **Cross-modal matching**: restore masked elements by global label priority.

Reported: **98.8 BLEU overall** on their DocBench-100 benchmark (98.6 on the 30-page complex ≥3-column subset, 98.9 on the 70-page regular subset), *"outperforms existing baselines by up to 24%"*, at **1.06× the FPS of geometry-only approaches** — i.e. essentially free. The paper explicitly diagnoses classic XY-cut: *"rigid threshold mechanisms of XY-Cut can introduce hierarchical errors when handling intricate layouts"*, notably with L-shaped regions that violate connectivity assumptions.

**Existing reading-order implementations to study:**

- **PdfPig `UnsupervisedReadingOrderDetector`** — applies **Allen's interval algebra** with tolerance T (default 5), combining `BeforeInReading` and `BeforeInRendering` relations, then topologically sorts. Based on Klampfl et al. (2013) ([PdfPig DLA wiki](https://github.com/UglyToad/PdfPig/wiki/Document-Layout-Analysis), [source](https://github.com/UglyToad/PdfPig/blob/master/src/UglyToad.PdfPig.DocumentLayoutAnalysis/ReadingOrderDetector/UnsupervisedReadingOrderDetector.cs)). PdfPig also offers a `RenderingReadingOrderDetector` that just averages `Letter.TextSequence` — i.e. trusts content-stream order.
- **pdfminer.six `boxes_flow`** — the single-scalar H-vs-V weighting described in A.5.
- **PyMuPDF `multi_column.py`** ([source](https://github.com/pymupdf/PyMuPDF-Utilities/blob/master/text-extraction/multi_column.py)) — the pragmatic, widely copied heuristic. Algorithm: extract horizontal text blocks (drop vertical text and text over images when `no_image_text=True`); sort by background color presence, then y0, then x0; **extend boxes rightward to the page margin where no intervening text exists**; merge adjacent boxes sharing vertical space and background. Parameters: header/footer strips (default **50 pt each**, excluded to bypass page furniture), a **10 pt join tolerance** when reordering boxes with nearly identical bottoms, and separate handling for text with different vector-graphic backgrounds (callout boxes). Documented failure modes: non-horizontal or RTL text, overlapping blocks, and image captions treated as regular text. It returns *bounding boxes only*, not text.
- **Poppler `TextOutputDev`** physical-layout mode backs `pdftotext -layout`; `-bbox-layout` exposes its block/line/word hierarchy for you to re-sort yourself.

## B.3 Page segmentation implementations you can read

PdfPig's `UglyToad.PdfPig.DocumentLayoutAnalysis` namespace is the most complete open, permissively licensed (**Apache-2.0**) reference implementation of classical DLA available ([wiki](https://github.com/UglyToad/PdfPig/wiki/Document-Layout-Analysis)):

**Word extractors**
- *Default*: horizontal only.
- *NearestNeighbourWordExtractor*: connects glyphs by **Manhattan distance with a 20% threshold** for known text direction, **Euclidean with 40%** for unknown; groups connected glyphs by DFS. Supports horizontal, axis-aligned-rotated, arbitrarily rotated and curved text; LTR and RTL. Parameters `FilterPivot`, `Filter`.

**Page segmenters**
- *RecursiveXYCut*: vertical cuts on gaps L→R, horizontal cuts on gaps B→T. Handles single/multi-column and (partially) L-shaped text. Parameters: minimum block width, dominant font height/width functions. Ref: Ha, Haralick & Phillips.
- *DocstrumBoundingBoxes*: bottom-up nearest-neighbour clustering; estimates within-line and between-line spacing, builds text lines then blocks. Handles single/multi-column, L-shaped **and rotated** text. Parameters: within-line angle bounds **[−30°, +30°]**, between-line angle bounds **[45°, 135°]**, between-line multiplier **1.3**. Ref: O'Gorman.
- *WhitespaceCoverExtractor*: Breuel's top-down maximal-empty-rectangle algorithm. Parameters: `maxRectangleCount` **40**, `fuzziness` **0.15**, `minWidth`/`minHeight`, `maxBoundQueueSize`.

**Decoration classifier**
- *DecorationTextBlockClassifier*: identifies **headers, footers, page numbers and archival information**. Requires **>2 pages** and uses **edit-distance comparison** across pages. Ref: Klampfl et al. (2013). This is exactly the running-header/footer algorithm OpenConvert needs, in readable Apache-2.0 C#.

## B.4 Running header / footer and page-number detection

**Canonical algorithm:** Lin, "Header and footer extraction by page-association", SPIE DRR X (2003) ([SPIE](https://www.spiedigitallibrary.org/conference-proceedings-of-spie/5010/0000/Header-and-footer-extraction-by-page-association/10.1117/12.472833.short), [Semantic Scholar](https://www.semanticscholar.org/paper/Header-and-footer-extraction-by-page-association-Lin/900c6e22c814ec03fa75186e83c908ed98ce7688)). The method associates candidate lines across pages by position + content similarity; repeated lines in the top/bottom bands across N pages are classified as furniture. **[UNVERIFIED — I could not retrieve the paper's reported accuracy figures; ResearchGate returned 429.]**

**Practical recipe distilled from the working implementations (PdfPig `DecorationTextBlockClassifier`, PyMuPDF `multi_column.py`, GROBID's segmentation model):**

1. Define top/bottom bands (PyMuPDF's default is **50 pt**; a percentage of page height, e.g. 7–8%, generalizes better across page sizes).
2. Cluster candidate lines by **y-position within band** across all pages.
3. Compare text with **normalized edit distance** after masking digit runs (so "Page 12" and "Page 137" match).
4. Require repetition on ≥ K pages (K ≈ 3, or a fraction like 20% of pages) **and** consider odd/even page parity separately — books commonly alternate recto/verso headers.
5. Page numbers: a line in a band whose masked form is *entirely* a number, and whose numeric values form a monotone arithmetic progression across pages. The arithmetic-progression check is what distinguishes a page number from a chapter number and is cheap and highly reliable.
6. Never delete a band line that is the *only* content on a page, or that shares font/size with body text and continues a sentence.

**Do not skip the parity and digit-masking steps** — they are the two that make the difference between 80% and near-perfect on real books. [This recipe is synthesis, not a cited result — treat as an engineering recommendation.]

## B.5 Heading detection

**Font-size heuristics work surprisingly well and are extremely cheap.** SciPlore Xtract used a rule-based font-size heuristic to extract titles from scientific PDFs and reported **77.9% accuracy vs 69.4% for CiteSeer's SVM**, with runtime **8:19 min vs 57:26 min** — i.e. better *and* ~7× faster than the ML baseline of its day ([Springer](https://link.springer.com/chapter/10.1007/978-3-642-15464-5_45)).

**GROBID** is the strongest reference for a cascaded, feature-based approach. It works on **layout tokens** carrying Unicode text, font size/name/style, and bounding boxes from `pdfalto`, plus indentation, spacing, vertical/horizontal position and character density ([GROBID Principles](https://grobid.readthedocs.io/en/latest/Principles/)). Its published PMC benchmark (GROBID 0.8.1, **1,943 PDFs, zero parse failures**, soft matching) ([GROBID PMC benchmark](https://grobid.readthedocs.io/en/latest/Benchmarking-pmc/)):

| Field | Precision | Recall | **F1** |
|---|---|---|---|
| Title | 92.15% | 91.82% | **91.98%** |
| Authors | 94.68% | 94.49% | **94.58%** |
| Abstract | 63.88% | 62.74% | **63.31%** |
| **Section titles** | 81.42% | 72.01% | **76.43%** |
| Figure titles | 78.63% | 61.52% | **69.03%** |
| Table titles | 82.27% | 72.35% | **76.99%** |
| Reference citations | 61.68% | 63.07% | **62.37%** |

**Calibrate expectations against that 76.43% section-title F1.** That is a mature, purpose-built, ML-assisted academic system on the document genre it was designed for. A heuristic heading detector on general books that reaches ~75–85% is doing well; anything claiming 95%+ across genres should be disbelieved.

**HiPS: Hierarchical PDF Segmentation of Textbooks** (arXiv [2509.00909](https://arxiv.org/html/2509.00909), 2025) evaluates heading hierarchy specifically on textbooks. Key findings relevant here:
- XML/font-only features suffer **high false-positive rates**; median tolerant precision ≈ 0.4–0.5 for XML-only + LLM.
- Adding **OCR spatial context** (headings are lines preceded/followed by empty lines) lifted median precision to ≈ 0.75–0.80.
- **Docling** showed strong recall (~0.8) but **precision only ~0.5–0.6** on this task.
- The **TOC-based PageParser baseline scored P_ED ≥ 0.9 (median) — the best of all approaches.**

**That last point is the most actionable finding in Part B.** If the PDF has an outline/bookmark tree or a printed table of contents, *use it* — matching heading candidates against a parsed TOC beats every font-clustering and ML approach measured. OpenConvert should treat "PDF has bookmarks" and "PDF has a parseable TOC page" as first-class fast paths.

**Recommended font clustering approach** (synthesis): compute the (rounded font size, weight-bit, italic-bit, font-family) tuple histogram weighted by character count. The mode is body text. Candidate heading styles are tuples with size > body size or bold-with-equal-size, whose total character share is small (< ~15%). Rank distinct heading styles by size descending to assign h1..h6. Require additional evidence per candidate line: short line (< ~60% of column width), line not ending in a sentence-continuing character, and vertical whitespace above greater than the body leading.

## B.6 Paragraph reconstruction, footnotes, captions, lists, drop caps

These have little published quantitative evaluation in isolation; the following are the standard cues, with sources where they exist.

**Paragraph reconstruction**
- Line grouping: pdfminer's `line_margin` (0.5 × line height) is a well-tested default for "same paragraph".
- Paragraph start: first-line **indentation** ≥ ~1 em relative to the block's dominant left edge, OR extra leading above.
- Paragraph end: a line whose right edge falls short of the block's dominant right edge by more than a word width (works because justified text pins every non-final line to the right margin). This is the most reliable single cue in justified books.
- MuPDF exposes this notion directly as the `paragraph-break` stext option — evidence the cue is considered sound by a major engine.

**Footnotes** — cues: (a) position in the bottom band; (b) font size measurably smaller than body (typically 0.75–0.85×); (c) preceded by a **superscript marker** in the body, detectable as a glyph whose baseline origin is raised relative to the line baseline by > ~0.3 × font size *and* whose font size is < ~0.8 × body; (d) often preceded on the page by a short horizontal rule (a vector path spanning < 40% of the column width, immediately above the small-font block). Note **PDF-to-EPUB footnote/endnote handling is widely acknowledged as the hardest part of the conversion** ([practitioner writeup](https://medium.com/@pdf2epub/the-hardest-part-of-pdf-to-epub-nobody-talks-about-footnotes-and-endnotes-718d3ea02fd0)). For EPUB the target is `epub:type="footnote"` + `role="doc-noteref"` popup notes; getting the *linkage* right (marker → note body) matters more than getting the classification right.
- Related published work: "Footnote identification within a PDF document" and ECCO-scale footnote detection over 32 million pages exist but I could not retrieve their metrics. **[UNVERIFIED]**

**Captions** — cues: (a) text block whose bbox is within ~1–2 line heights of an image/table bbox, horizontally overlapping it; (b) font smaller than body and/or italic; (c) regex `^(Fig(ure)?|Tab(le)?|Chart|Plate|Listing|Scheme)\s*\.?\s*[\dIVXA-Z]+[.:\s]`. GROBID's figure-title F1 of **69.03%** and table-title F1 of **76.99%** on PMC (above) are a fair upper-bound expectation for this class of cue. Note DocLayNet's Caption class human agreement is only **84–89 mAP** — even humans disagree about caption boundaries.

**Lists** — cues: (a) leading bullet glyphs (U+2022, U+2023, U+25AA, U+25CF, U+00B7, U+2043, "-", "*") or numbering patterns (`^\d+[.)]`, `^[a-z][.)]`, `^[ivxlc]+[.)]`, `^[A-Z][.)]`); (b) **hanging indent**: the first line's left edge is left of subsequent lines' left edge, consistently; (c) a run of ≥ 2 sibling items sharing marker type and indentation. Nesting = distinct indentation levels. DocLayNet's List-item class is the *easiest* non-Text class for models (YOLOv5x6 86.2 mAP, human 87–88) which suggests it is visually well-defined and heuristics should do well.

**Drop caps** — a single glyph at the start of a paragraph whose glyph bbox height is ≥ ~2× the body line height, whose baseline sits ~2–3 lines below the first line's baseline, and around which subsequent lines are indented. Handling: emit the character as normal text at the start of the paragraph and apply a CSS `float: left; font-size: 3em; line-height: 0.8` class, or simply normalize it away. Failing to detect it produces a stray one-character paragraph — a very visible EPUB defect. **[No published accuracy numbers found — UNVERIFIED.]**

## B.7 Dehyphenation

The best-measured deterministic component in this report. Freiburg bachelor thesis, "Improved Dehyphenation of Line Breaks for PDF Text Extraction" (Hernaes, 2019) ([PDF](https://ad-publications.cs.uni-freiburg.de/theses/Bachelor_Mari_Hernaes_2019.pdf)).

Dataset: ClueWeb12 extract, **776,700 hyphenated words, 13,112 expected (kept) hyphens**.

| Method | Accuracy | Specificity | **Recall** | Balanced accuracy |
|---|---|---|---|---|
| Baseline (IMDB vocabulary) | 98.76% | 99.91% | **31.67%** | 65.79% |
| Baseline (OntoNotes vocabulary) | 98.79% | 99.91% | **33.83%** | 66.87% |
| **CRF / logistic regression (OWL-QN)** | 98.75% | 98.98% | **85.78%** | **92.38%** |
| Bi-LSTM char LM | **99.25%** | 99.56% | 80.71% | 90.14% |

On a Wikipedia extract (299,919 words, 5,734 expected hyphens), the vocabulary baseline reached 99.51% accuracy / 88.39% balanced accuracy.

**Three lessons for OpenConvert:**
1. **Raw accuracy is a misleading metric here** — a do-nothing-clever baseline gets 98.8% because ~98% of line-break hyphens should simply be removed. **Recall on "keep the hyphen" is the number that matters**, and the dictionary baseline gets it right only ~32% of the time.
2. A small feature-based classifier (character bigrams, case, word shape) lifts keep-hyphen recall from ~32% to ~86% at no accuracy cost. This is a **kilobyte-scale model**, not an LLM — a great fit for OpenConvert's "deterministic first, tiny model only where needed" thesis.
3. Perfect accuracy is theoretically impossible: "email"/"e-mail" are both correct and the corpus contains both. Do not over-engineer.

**Also note: MuPDF ships a `dehyphenate` stext option** — worth reading its behaviour as a reference even if you can't use the code.

**Practical rules that get you most of the way:** only consider a hyphen at end-of-line (last glyph of a line whose next line begins a lowercase letter); never join across a paragraph, column, page, or block boundary; if the joined form appears elsewhere in the *same document*, join; if the hyphenated form appears elsewhere in the same document, keep. Same-document evidence is stronger than any external dictionary and costs nothing.

## B.8 Ligatures and Unicode normalization

- **MuPDF expands ligatures by default** (`preserve-ligatures` is the opt-*out*) — evidence that expansion is the sane default for extraction.
- PDFium does not expand; you will see U+FB00–U+FB06 (ﬀ ﬁ ﬂ ﬃ ﬄ ﬅ ﬆ) in the char stream. Map them explicitly.
- Apply **NFKC** cautiously — it will also normalize superscript digits (¹ → 1), which destroys a footnote-marker signal you may want. Recommended order: capture superscript-ness *from geometry* first, then normalize.
- Additional mappings needed in practice: soft hyphen U+00AD (strip), non-breaking hyphen U+2011, en/em dashes, curly quotes (keep — they're correct typography for EPUB), U+FFFD replacement chars (a symptom of a bad ToUnicode CMap — flag the page for OCR fallback), and PDFium's `HasUnicodeMapError` flag which tells you exactly this.
- **Ligature "guessing" for broken CMaps** is a real problem area — see the companion Freiburg master's thesis "Dehyphenation of Words and Guessing Ligatures" ([PDF](https://ad-publications.informatik.uni-freiburg.de/theses/Master_Sumitra_Corraya_2018.pdf)). **[Numbers not retrieved — UNVERIFIED.]**

## B.9 Table detection heuristics and their limits

**Two families, matching the two PDF realities:**
1. **Ruling-line ("lattice") detection** — find vector paths that are long, thin, axis-aligned; snap them into a grid; cells are grid intersections. Requires vector-path access (PDFium page objects, MuPDF `vectors`, pdfplumber's `rects`/`lines`).
2. **Whitespace ("stream") detection** — infer column separators from consistent vertical whitespace gutters across rows; infer row separators from line spacing.

**Camelot's own published comparison on ICDAR-2013 (67 PDFs)** ([Camelot docs](https://camelot-py.readthedocs.io/en/latest/user/comparison.html)):

| Tool | F1 | TEDS | Row acc | Col acc | Time |
|---|---|---|---|---|---|
| Camelot lattice (combined) | **0.778** | **0.789** | 0.762 | 0.829 | 101 s |
| Camelot lattice (vector) | 0.766 | 0.784 | 0.748 | 0.806 | **13 s** |
| tablers | 0.750 | 0.724 | 0.657 | 0.741 | 1.5 s |

Camelot's docs also concede that **Tabula's stream detection "is generally stronger than Camelot's *stream* parser."**

**The blunt takeaway: ~0.78 F1 / ~0.79 TEDS on *ruled* tables is the state of the deterministic art.** Borderless tables are materially worse. For EPUB this matters less than it looks: a reflowable EPUB renders tables poorly anyway, and the common fallback — emit the table as an image with the extracted text as `alt`/`aria` content, or emit a simple `<table>` without merged cells — is often the better product decision. **Recommendation: detect tables well enough to avoid destroying them (i.e. don't let a table become garbled paragraphs), but do not invest in high-fidelity table structure recovery for v1.**

## B.10 Part B summary — what to build

| Task | Deterministic method | Published evidence | Expected quality |
|---|---|---|---|
| Word/line grouping | Nearest-neighbour (Manhattan 20% / Euclidean 40%) | PdfPig | Very high on born-digital |
| Block segmentation | **Docstrum** primary, whitespace-rectangle secondary | 6.0% / 9.8% line error, UW-III (scanned) | High |
| Reading order (Manhattan) | Recursive XY-cut + column detection | **100%**, arXiv 2607.01018 | Excellent |
| Reading order (complex) | XY-Cut++ (pre-mask + density-driven splits) | 98.8 BLEU, DocBench-100 | Good |
| Reading order (wrap-around) | — | XY-cut 49.7% | **Poor — escalate** |
| Headings | TOC/outline match > font-size+weight clustering | TOC P_ED ≥ 0.9; GROBID section-title F1 76.43% | Good with TOC, moderate without |
| Headers/footers/page numbers | Cross-page repetition + edit distance + parity + arithmetic progression | PdfPig `DecorationTextBlockClassifier`; Lin 2003 | High |
| Paragraphs | Indent + short-final-line + leading | MuPDF `paragraph-break` | High on justified text |
| Dehyphenation | Same-doc evidence → dictionary → tiny CRF | 92.38% balanced acc (CRF) vs 66.87% (dict only) | High |
| Ligatures/Unicode | Explicit mapping + careful NFKC | MuPDF default expands | Very high |
| Footnotes | Bottom band + small font + superscript marker + rule | — **[UNVERIFIED]** | Moderate |
| Captions | Proximity to image + "Figure N" regex | GROBID fig-title F1 69.03% | Moderate |
| Lists | Marker regex + hanging indent | DocLayNet List-item easiest class | High |
| Tables | Ruling lines primary, whitespace fallback | Camelot lattice F1 0.778 | Moderate (ruled) / Poor (borderless) |
| Drop caps | Glyph height ≥ 2× line height + multi-line indent | — **[UNVERIFIED]** | Moderate |

---

# PART C — Small ML layout models on CPU (not LLMs)

## C.1 Docling layout models — the best-documented option, and Apache-2.0

**"Advanced Layout Analysis Models for Docling"** (arXiv [2509.11720](https://arxiv.org/html/2509.11720v1), 2025) is the most useful paper in this section because it reports **CPU** latency, which almost nobody does.

| Model | Backbone | Params |
|---|---|---|
| egret-m | DFINE-m | **19.5M** |
| egret-l | DFINE-l | 31.2M |
| egret-x | DFINE-x | 62.7M |
| old-docling | RT-DETRv1-r50vd | 42.9M |
| **heron** | RT-DETRv2-r50vd | **42.9M** |
| heron-101 | RT-DETRv2-r101vd | 76.7M |

**Latency (seconds per image):**

| Model | A100 GPU (bs 200) | **AMD EPYC 7763 CPU, 4 threads (bs 32)** | Apple M3 Max MPS (bs 50) |
|---|---|---|---|
| egret-m | 0.024 | **0.334** | 0.033 |
| heron | 0.031 | **0.643** | 0.044 |
| heron-101 | 0.028 | **0.988** | 0.062 |

**Accuracy (mAP, COCO-Tools, no post-processing):** DocLayNet — heron 0.699, heron-101 0.696; DocLayNet-v2 — heron-101 0.758; canonical DocLayNet — heron-101 **0.780**. Overall the paper claims **20.6%–23.9% mAP improvement over Docling's previous baseline**. Training corpus: **150,000 documents / 2.3M layout elements** across 17 canonical categories, combining post-processed DocLayNet, proprietary DocLayNet-v2, and WordScape.

**License: Apache-2.0 for the weights** — confirmed on the model card ([docling-project/docling-layout-heron](https://huggingface.co/docling-project/docling-layout-heron)), which also lists **17 classes**: Caption, Footnote, Formula, List-item, Page-footer, Page-header, Picture, Section-header, Table, Text, Title, Document Index, Code, Checkbox-Selected, Checkbox-Unselected, Form, Key-Value Region. Default confidence threshold 0.6. **This is the cleanest license situation of any layout model surveyed.**

**System-level cost from the Docling Technical Report** (arXiv [2408.09869](https://arxiv.org/html/2408.09869v5)): layout model runs at **72 dpi** input with sub-second CPU latency; **TableFormer takes 2–6 seconds per table on a standard CPU**; end-to-end on 225 pages:

| Hardware | Threads | Pages/sec | **Memory** |
|---|---|---|---|
| Apple M3 Max | 4 | 1.27 | **6.20 GB** |
| Apple M3 Max | 16 | 1.34 | — |
| Intel Xeon E5-2690 | 4 | 0.60 | **6.16 GB** |
| Intel Xeon E5-2690 | 16 | 0.92 | — |

**That ~6.2 GB figure is the number to show the Chief Architect.** It is the full Docling pipeline, not the layout model alone, but it illustrates how quickly an ML-first document pipeline violates "low RAM". A 300-page book at 0.6–1.3 pages/sec is 4–8 minutes of pure inference.

Docling package code is **MIT**; the model catalog also lists TableFormer (accurate & fast modes), DocumentFigureClassifier-v2.5, CodeFormulaV2, and VLMs (Granite-Docling-258M, SmolDocling-256M) ([model catalog](https://docling-project.github.io/docling/usage/model_catalog/)).

## C.2 DocLayNet-trained YOLO — fast, accurate, and an AGPL minefield

**[FreeOCR-AI/yolo-doclaynet](https://github.com/FreeOCR-AI/yolo-doclaynet)** ships DocLayNet-trained checkpoints across YOLOv8–v26:

| Family | Nano | Small | Medium | Large | Extra |
|---|---|---|---|---|---|
| YOLOv26 | 2.4M | 9.5M | 20.4M | 24.8M | 55.7M |
| YOLOv12 | 2.6M | 9.3M | 20.2M | 26.4M | 59.1M |
| YOLOv11 | 2.6M | 9.4M | 20.1M | 25.3M | 56.9M |
| YOLOv10 | 2.3M | … | … | … | 29.5M |
| YOLOv9 | 2.0M (t) | 7.2M | 20.1M | 25.5M | — |
| YOLOv8 | 3.2M | … | … | … | 68.2M |

Overall mAP50-95 spans **0.718 (YOLOv8n) to 0.82 (YOLOv26l)**. Best per-class (YOLOv26x): Text 0.903, Table 0.895, List-item 0.871, Footnote 0.839, Page-footer 0.689.

**LICENSE TRAP: the repository is AGPL-3.0** (inherited from Ultralytics). Ultralytics' AGPL covers the training code, and the derived weights are conventionally treated as AGPL-encumbered by the Ultralytics project. Weight-license details are **not separately stated** in the repo. **[Weights license UNVERIFIED — but assume AGPL until Ultralytics says otherwise.]**

**[opendatalab/DocLayout-YOLO](https://github.com/opendatalab/DocLayout-YOLO)** (arXiv [2410.12628](https://arxiv.org/html/2410.12628v1)): YOLO-v10 based with a Global-to-Local Controllability module, pretrained on synthetic DocSynth300K (treating document synthesis as 2D bin packing). DocLayNet **AP50 93.4 / mAP 79.7** with pretraining (93.0 / 77.7 without); D4LA AP50 82.4 / mAP 70.3. **Also AGPL-3.0.** No published FPS/hardware in the repo.

**A YOLOv8n/v11n at 2.3–3.2M params is ~6–13 MB in ONNX FP32 (~3–7 MB INT8) and would run in roughly 50–200 ms/page on 4 CPU threads at 640–1024 px input** — an order of magnitude cheaper than heron. **[Extrapolated, UNVERIFIED — must be measured.]** But the AGPL makes them unusable for OpenConvert unless the project accepts AGPL. **This is the single easiest license mistake to make in this space.**

## C.3 PP-DocLayout (PaddleOCR)

arXiv [2503.17213](https://arxiv.org/html/2503.17213v1): **23 layout categories** across academic papers, research reports, exam papers, books, newspapers, magazines; trained on 500,000 documents.

| Variant | mAP@0.5 | T4 GPU | **CPU** |
|---|---|---|---|
| PP-DocLayout-L | **90.4%** | 13.4 ms/page | — |
| PP-DocLayout-M | 75.2% | 12.7 ms/page | — |
| PP-DocLayout-S | — | 8.1 ms/page (~123 pages/s) | **14.5 ms/page** |

**PP-DocLayout-S at 14.5 ms/page on CPU is by far the best CPU number in this section** — ~23× faster than egret-m and ~44× faster than heron. It is also the model behind PaddleOCR's PP-StructureV3, whose XY-cut implementation was the baseline in the reading-order paper. PaddleOCR is Apache-2.0; **the specific weights license and the exact CPU used for the 14.5 ms figure are [UNVERIFIED]** — verify before committing, and note that PaddlePaddle runtime is a heavy dependency (though the model can be exported to ONNX).

**23 classes vs DocLayNet's 11 is a real advantage for books** (it covers things like headers/footers/abstract/formula variants that a book pipeline cares about).

## C.4 Surya

Code **Apache-2.0**; **model weights use a modified AI Pubs Open RAIL-M license — free for research, personal use, and companies under $5M funding/revenue; commercial licensing otherwise requires contacting Datalab** ([datalab-to/surya](https://github.com/datalab-to/surya)). 650M params, 83.3% on olmOCR-bench, ~5 pages/s on an RTX 5090.

**LICENSE TRAP and size trap.** A 650M-param model is not a "small CPU model", and a revenue-gated license is incompatible with an open-source project that wants unrestricted redistribution. **Exclude.**

## C.5 HURIDOCS — the most relevant comparison point in this whole section

[huridocs/pdf-document-layout-analysis](https://github.com/huridocs/pdf-document-layout-analysis) offers **two modes for the same 11-class task**, which is exactly the trade-off OpenConvert faces:

| Mode | Approach | CPU (i7-8700) | GPU (GTX 1070) | Accuracy |
|---|---|---|---|---|
| **LightGBM "fast"** | Gradient-boosted trees over **Poppler XML token features** (token-type classifier + segmentation model); **no visual input** | **0.42 s/page** | N/A | "slightly lower" |
| **VGT** (Vision Grid Transformer, Alibaba) | Full visual model, DocLayNet-trained | **13.5 s/page** | 1.75 s/page | F1 **0.962** on PubLayNet |

**A 32× CPU speedup for "slightly lower" accuracy, from a tree ensemble over geometric/font features rather than pixels.** This is the strongest existence proof that OpenConvert's deterministic-plus-tiny-classifier architecture is the right shape. A LightGBM/XGBoost model over the exact features PDFium already gives you (font size, weight, position, density, gap statistics) is **kilobytes-to-megabytes**, loads instantly, has no ONNX runtime dependency in the Rust case (`lightgbm` has pure-Rust inference options), and needs no page rasterization at all — which also saves you the render cost that dominates ML pipelines.

## C.6 Other options briefly

- **LayoutParser** ([Layout-Parser/layout-parser](https://github.com/Layout-Parser/layout-parser)) — Apache-2.0, 5.8k stars, but wraps **Detectron2** / EfficientDet / PaddleDetection. Detectron2 alone is a multi-hundred-MB dependency with a heavy install story, and the project shows 98 open issues with no clear recent release. **Exclude on dependency weight.** [Exact last-release date UNVERIFIED.]
- **TableFormer** — Docling's vision-transformer table structure model; **2–6 s/table on CPU** (Docling technical report). Too slow for interactive desktop use on table-heavy books; code is MIT via `docling-ibm-models`.
- **ONNX Runtime CPU deployment** — genuinely practical for all of the above. `ort` (Rust) and `onnxruntime` (Python/Node/C#) all ship prebuilt CPU binaries. Cost: **~10–30 MB of native runtime per platform** on top of the model. **[Size figure UNVERIFIED — measure.]** That is a meaningful fraction of a "small binary" budget for a feature that should be optional.

## C.7 The DocLayNet ceiling — why not to over-invest in layout models

From the DocLayNet paper (arXiv [2206.01062](https://arxiv.org/pdf/2206.01062)), Table 2 — mAP by class, including **human inter-annotator agreement**:

| Class | **Human** | MRCNN R50 | MRCNN R101 | FRCNN R101 | YOLOv5x6 |
|---|---|---|---|---|---|
| Caption | 84–89 | 68.4 | 71.5 | 70.1 | 77.7 |
| Footnote | 83–91 | 70.9 | 71.8 | 73.7 | 77.2 |
| Formula | 83–85 | 60.1 | 63.4 | 63.5 | 66.2 |
| List-item | 87–88 | 81.2 | 80.8 | 81.0 | 86.2 |
| Page-footer | 93–94 | 61.6 | 59.3 | 58.9 | 61.1 |
| Page-header | 85–89 | 71.9 | 70.0 | 72.0 | 67.9 |
| Picture | 69–71 | 71.7 | 72.7 | 72.0 | 77.1 |
| Section-header | 83–84 | 67.6 | 69.3 | 68.4 | 74.6 |
| Table | 77–81 | 82.2 | 82.9 | 82.2 | 86.3 |
| Text | 84–86 | 84.6 | 85.8 | 85.4 | 88.1 |
| Title | 60–72 | 76.7 | 80.4 | 79.9 | 82.7 |
| **All** | **82–83** | 72.4 | 73.5 | 73.4 | **76.8** |

Dataset: **80,863 pages, 11 classes, 6 document categories**, license **CDLA-Permissive-1.0** ([HF dataset card](https://huggingface.co/datasets/ds4sd/DocLayNet/raw/main/README.md)) — permissive, so DocLayNet-derived weights are *not* encumbered by the data; the AGPL problem comes purely from Ultralytics' training code.

**Two observations that should shape the architecture:**
1. **Page-footer: human 93–94 mAP, best model 61.1.** The models are dramatically *worse than humans* on exactly the class that deterministic cross-page repetition detection nails almost perfectly. **Don't use an ML model for headers/footers. Use repetition.**
2. **Overall human agreement is only 82–83 mAP.** heron's 0.699 on DocLayNet and DocLayout-YOLO's 79.7 are approaching a ceiling that is itself noisy. There is not a lot of headroom to buy.

## C.8 Cost/benefit: heuristics vs small ML vs small LLM

| Task | Pure heuristics | Small CPU model | Small local LLM | **Recommendation** |
|---|---|---|---|---|
| **Block classification** (text/title/list/table/figure/caption/footnote/header/footer) | Good for text/list/header/footer; moderate for caption/footnote; weak for formula | egret-m 0.334 s/page, heron 0.643 s/page; PP-DocLayout-S 14.5 ms/page; HURIDOCS LightGBM 0.42 s/page | Massive overkill; ~GB RAM; seconds/page | **Heuristics + optional LightGBM-style tabular classifier over PDFium features.** Reserve a vision model as an opt-in escalation. |
| **Reading order** | **100% on Manhattan**; 75% multi-column; 49.7% wrap-around | LayoutReader scored **25%** on OmniDocBench multi-column — *worse than XY-cut* | Plausible for pathological pages only | **Deterministic XY-cut / XY-Cut++. Do not use an ML reading-order model.** The evidence is unambiguous. |
| **Table structure** | Camelot lattice F1 0.778 / TEDS 0.789 on *ruled* tables; poor on borderless | TableFormer 2–6 s/table CPU | Very slow | **Ruling-line heuristics; degrade gracefully to image+alt-text.** Don't chase TEDS. |
| **Heading hierarchy** | TOC match P_ED ≥ 0.9; font clustering moderate | Docling: recall ~0.8, **precision only ~0.5–0.6** | Good at *labelling* a candidate list cheaply | **TOC/outline first, font clustering second, LLM only to disambiguate a shortlist of candidates.** |
| **Dehyphenation** | Dict baseline: 32% keep-recall | **CRF: 92.38% balanced accuracy, kilobytes** | Overkill | **Tiny CRF/logreg. Best ROI in the whole system.** |

---

# PART D — Recommendations

## D.1 License traps — the executive summary

| Component | License | Effect on OpenConvert |
|---|---|---|
| **MuPDF / PyMuPDF / mupdf-rs / mupdf.js** | **AGPL-3.0 or commercial** | Best engine; forces AGPL on OpenConvert *and every downstream fork*, including network-served derivatives. Artifex actively sells the alternative. **Avoid unless the project deliberately chooses AGPL.** |
| **Poppler / pdftotext / pdfalto** | **GPL-2/GPL-3** | Viral; blocks proprietary forks; complicates static linking and app-store distribution. Acceptable as an *optional, subprocess-invoked, system-installed* backend on Linux. |
| **unipdf (UniDoc)** | **Proprietary, license code required** | Hard disqualification. |
| **Ultralytics-derived YOLO layout weights** (yolo-doclaynet, DocLayout-YOLO) | **AGPL-3.0** | Easy to miss; shipping the weights arguably AGPLs the app. **Avoid.** |
| **Surya weights** | **Modified AI Pubs OpenRAIL-M, <$5M revenue** | Field-of-use restriction; not OSI-open. **Avoid.** |
| **PDFium** | **BSD-3** | ✅ Clean. |
| **pdfium-binaries build scripts** | MIT | ✅ Clean. |
| **pdfium-render, pypdfium2** | MIT/Apache-2.0, Apache-2.0/BSD-3 | ✅ Clean. |
| **Docling code / layout weights (heron, egret)** | MIT / **Apache-2.0** | ✅ Clean — the only permissive vision layout models found. |
| **DocLayNet dataset** | **CDLA-Permissive-1.0** | ✅ Clean — the data is not the problem; the training code is. |
| **PdfPig, PDFBox, pdf.js** | Apache-2.0 | ✅ Clean. |
| **pdfminer.six, pdfplumber, lopdf, pdf-extract, pdf_oxide, hayro, pdfsink-rs** | MIT / MIT-Apache | ✅ Clean. |
| **pypdf, pdfcpu** | BSD-3 / Apache-2.0 | ✅ Clean. |

## D.2 Recommended backend, per candidate host language

### Rust — **recommended host**

**Primary: PDFium via `pdfium-render` (MIT/Apache-2.0), consuming `bblanchon/pdfium-binaries` (BSD-3 engine, MIT scripts).**
- Per-char geometry, font name/size/weight/flags, render mode, fill/stroke colour, `is_generated`, `is_hyphen` — everything the layout stack needs.
- Dynamic loading via `libloading` keeps the Rust binary tiny and lets you ship/update PDFium as a ~3–10 MB sidecar per platform.
- Page rendering built in, for OCR fallback and visual QA diffing.
- Prebuilt binaries for every desktop target including Linux musl and macOS universal; weekly builds since 2017.
- v0.9.4 released **three days ago** (2026-09-06) — actively maintained.

**Companion: `lopdf` (MIT)** for the object model PDFium refuses to expose — outlines/bookmarks (belt-and-braces), `/StructTreeRoot` for tagged PDFs, `/Metadata` XMP, named destinations. `lopdf` is 94k downloads/month with 138 dependents; it's the mature choice.

**Watch, don't ship (yet): `hayro`.** Pure Rust, forbids `unsafe`, Apache-2.0/MIT, and its `Device` trait gives you *precisely* the interception points a layout engine wants (glyph + transform + `GlyphDrawMode` + `ClipPath` + `SoftMask`). The reasons not to ship it in v1: self-described as experimental with essentially no API docs, **performance explicitly not a priority yet**, no encrypted-PDF support, MSRV 1.92. Revisit in 6–12 months; if it matures, it eliminates the native sidecar entirely and is a strategic win for binary size and supply-chain simplicity.

**Evaluate but don't commit: `pdf_oxide`.** Real, permissive, actively developed, and has the right API shape (`extract_chars`, `extract_spans`, optional rendering). Its published benchmarks are self-reported and extraordinary; require a hands-on bake-off against pypdfium2 on a corpus you control before believing them.

**Reject:** `mupdf-rs` (AGPL + hardest build), `pdf-extract` (no positions), `pdf` crate (stale since Nov 2023), `pdfsink-rs` (too immature).

### Python

**Primary: `pypdfium2` (Apache-2.0/BSD-3).** Matches PyMuPDF on speed (0.1 s) and *beats* it on quality (97% vs 96%) in the independent py-pdf benchmark, under a permissive licence. Wheels everywhere. v5.10.1, June 2026. **Caveat: not thread-safe** — use process-level parallelism.

**Companion: `pypdf` (BSD-3)** for outlines, structure tree, XMP. **Development-only: `pdfplumber` (MIT)** for ground-truth generation and visual debugging — its `mcid`/`tag` char attributes and `.to_image()` debugger are excellent for building a regression corpus, but 9.5 s/doc and 75% quality disqualify it from shipping.

**Reject: PyMuPDF** on licence grounds despite being technically excellent.

### TypeScript / Node

**Primary: `pdf.js` (Apache-2.0)** — the only permissively licensed, mature option. Accept that you get **item-level, not glyph-level, geometry** (issues [#7996](https://github.com/mozilla/pdf.js/issues/7996), [#12884](https://github.com/mozilla/pdf.js/issues/12884)) and must reconstruct char positions from advance widths. **Set `isEvalSupported: false`** (CVE-2024-4367). Rendering in Node needs `@napi-rs/canvas` or `canvas`, reintroducing a native dependency.

**Alternative: PDFium via a native Node addon or `pdf_oxide`'s Node/WASM bindings** — better data, at the cost of a native dependency you were trying to avoid.

**Explicitly avoid `mupdf.js`** — AGPL, and it's Artifex's own funnel to a commercial licence.

### C# / .NET

**Primary: `PdfPig` (Apache-2.0)** — glyph bounding boxes, font name/size, images, annotations, bookmarks, forms, **and the best open classical layout-analysis library in existence** (Docstrum, RecursiveXYCut, whitespace cover, Allen-interval reading order, decoration/header-footer classifier). Add **`PdfPig.Rendering.Skia`** for rasterization. Caveat: sub-1.0 with explicit "minor versions will break the API" warning.

**Alternative: PDFium via `PDFiumCore`/`bblanchon.PDFium` NuGet** if PdfPig's extraction fidelity proves insufficient on hard PDFs.

**Reject:** anything requiring a JVM (PDFBox/Tika) on startup and RAM grounds — despite PDFBox being the most feature-complete Apache-licensed library available.

## D.3 The deterministic layout stack to build

Pipeline, in order. Every stage is cheap; every stage emits a confidence score.

```
0. Fast paths first
   ├─ Tagged PDF? Use /StructTreeRoot. Emit EPUB structure directly.   ← best signal
   ├─ Has outline/bookmarks? Use as heading ground truth.               ← TOC P_ED ≥ 0.9
   └─ Has a parseable printed TOC page? Match against it.

1. Glyph ingestion (PDFium)
   char, bbox (tight + loose), origin, matrix, font name/size/weight/flags,
   render mode, fill colour, is_generated, is_hyphen, has_unicode_map_error
   → drop render-mode-3 (invisible) text; flag has_unicode_map_error pages for OCR

2. Word & line assembly
   Nearest-neighbour: Manhattan 20% threshold (known direction),
   Euclidean 40% (unknown). DFS on the connectivity graph.        [PdfPig]

3. Page furniture removal  ← do this BEFORE segmentation
   Top/bottom bands (~7% of page height); cluster by y across pages;
   digit-masked normalized edit distance; odd/even parity;
   page numbers via monotone arithmetic progression.
   [PdfPig DecorationTextBlockClassifier; Lin 2003]

4. Block segmentation
   Docstrum (within-line ±30°, between-line 45–135°, multiplier 1.3)
   cross-checked against whitespace-rectangle cover (Breuel).
   Disagreement → low confidence flag.                            [6.0% / 9.8% UW-III]

5. Column detection + reading order
   Detect gutters via vertical whitespace projection; then
   XY-Cut++ style: pre-mask high-dynamic elements → density-driven
   split direction → remap masked elements by IoU-weighted distance.
   [100% Manhattan; 98.8 BLEU DocBench-100]

6. Block classification (heuristics)
   heading    : font-tuple clustering vs body mode + short line + whitespace above
   list       : marker regex + hanging indent + ≥2 siblings
   caption    : proximity to image/table bbox + "Figure N" regex + smaller font
   footnote   : bottom band + font < 0.85× body + superscript marker in body + rule above
   table      : long thin axis-aligned vector paths forming a grid
   figure     : image XObject bbox (+ SMask)
   drop cap   : glyph height ≥ 2× line height + multi-line indent around it

7. Text normalization
   ligature expansion (U+FB00–06); soft-hyphen strip; careful NFKC
   (capture superscript-ness from geometry FIRST); dehyphenation:
   same-document evidence → dictionary → tiny CRF.
   [CRF 92.38% balanced acc vs dict 66.87%]

8. Paragraph reconstruction
   line_margin ≈ 0.5× line height; paragraph start = indent ≥ 1em or extra
   leading; paragraph end = short final line (justified text).

9. Confidence gate → escalation
   Low-confidence pages only: optional ONNX layout model
   (egret-m, Apache-2.0, 0.334 s/page CPU) or the small local LLM.
```

**Key design principle: stage 3 before stage 4.** Removing running headers/footers before segmentation stops them from corrupting column detection and block statistics — this is the single highest-leverage ordering decision in the pipeline, and it's the ordering PyMuPDF's `multi_column.py` also uses (its 50 pt header/footer margins).

## D.4 If and when to add an ML layout model

**Default: off.** Ship heuristics-only. Then, in priority order:

1. **A tabular classifier (LightGBM/XGBoost) over PDFium features** — no rasterization, no ONNX runtime, kilobytes-to-megabytes, and HURIDOCS demonstrates it runs **32× faster than a vision model for "slightly lower" accuracy** (0.42 s/page vs 13.5 s/page CPU). This is the highest-ROI ML addition and barely counts as ML from a footprint perspective.
2. **`docling-layout-egret-medium` (DFINE-m, 19.5M params, Apache-2.0) exported to ONNX**, as an opt-in download, invoked *only* on pages the heuristics flag as low-confidence. At 0.334 s/page CPU, escalating 5% of a 300-page book costs ~5 seconds.
3. **PP-DocLayout-S** if its 14.5 ms/page CPU figure survives verification and its weights license checks out — it would be 23× cheaper than egret-m with 23 classes. **Verify both before committing.**

**Never ship:** Ultralytics-derived YOLO layout weights (AGPL), Surya (OpenRAIL-M + 650M params), Detectron2-based LayoutParser (dependency weight), TableFormer as a default (2–6 s/table).

## D.5 Biggest uncertainties

1. **`pdf_oxide`'s benchmark claims (0.8 ms vs PyMuPDF's 4.6 ms) are vendor-published with no described corpus.** If they hold on a real corpus, the Rust recommendation changes materially — you'd drop the native sidecar entirely. Needs a controlled bake-off.
2. **PDFium's actual coverage of image SMask, vector-path detail, and bookmark APIs** was not verified at the signature level. If SMask handling is weak, figure extraction quality suffers and a `lopdf`-based fallback path is needed.
3. **`hayro`'s trajectory.** If it reaches production quality with acceptable performance within ~12 months, pure-Rust becomes viable and the architecture simplifies dramatically. Its maintainer explicitly deprioritizes performance today.
4. **No published quantitative evaluation exists for footnote, caption, drop-cap, or list detection in isolation** on born-digital PDFs. GROBID's figure-title F1 (69.03%) is the closest proxy. OpenConvert will need to build its own labelled regression corpus — this is unavoidable and should be scheduled early.
5. **The Shafait 2006 segmentation numbers are on scanned images**, not born-digital PDFs with exact glyph boxes. The relative ordering (Voronoi ≈ Docstrum > whitespace > XY-cut) is probably robust; the absolute error rates are almost certainly pessimistic for our case. Needs re-measurement on a born-digital corpus.
6. **PDFium's `is_generated` characters** (PDFium-synthesized spaces) will pollute geometry-based heuristics if not filtered. The exact behaviour and how much it varies by PDF needs hands-on characterization.
7. **PP-DocLayout-S weights license and the CPU used for its 14.5 ms/page figure** are both unverified.
8. **PDFium desktop binary sizes** were not published on the release page fetched; only Android/iOS (2.65–3.34 MB) were visible. Verify the Windows/macOS/Linux figures against the "small binary" budget.

---

## Source list

**PDF engines & bindings**
- Artifex licensing (MuPDF/PyMuPDF AGPL + commercial) — https://artifex.com/licensing
- Artifex acquires PyMuPDF (PDF Association) — https://pdfa.org/artifex-software-acquires-pymupdf/
- PyMuPDF licence discussion #971 — https://github.com/pymupdf/PyMuPDF/discussions/971
- PyMuPDF README — https://github.com/pymupdf/PyMuPDF
- PyMuPDF Appendix 1 (text extraction variants + relative timings) — https://pymupdf.readthedocs.io/en/latest/app1.html
- PyMuPDF Appendix 4 (performance methodology) — https://pymupdf.readthedocs.io/en/latest/app4.html
- MuPDF structured-text options — https://mupdf.readthedocs.io/en/latest/reference/common/stext-options.html
- MuPDF StructuredText (JS) reference — https://mupdf.readthedocs.io/en/latest/reference/javascript/types/StructuredText.html
- mupdf-rs — https://github.com/messense/mupdf-rs
- mupdf.js — https://github.com/ArtifexSoftware/mupdf.js
- PDFium `public/fpdf_text.h` — https://pdfium.googlesource.com/pdfium/+/refs/heads/main/public/fpdf_text.h
- PDFium getting started (build) — https://pdfium.googlesource.com/pdfium/+/HEAD/docs/getting-started.md
- pdfium-binaries — https://github.com/bblanchon/pdfium-binaries
- pdfium-binaries releases (151.0.7891.0, 2026-06-15) — https://github.com/bblanchon/pdfium-binaries/releases
- pdfium-render docs — https://docs.rs/pdfium-render/latest/pdfium_render/ and https://docs.rs/crate/pdfium-render/latest
- pypdfium2 — https://github.com/pypdfium2-team/pypdfium2
- Poppler releases (26.09.0, 2026-09-03) — https://poppler.freedesktop.org/releases.html
- pdftotext(1) man page — https://www.mankier.com/1/pdftotext
- pdfalto — https://github.com/kermitt2/pdfalto
- pdf.js — https://github.com/mozilla/pdf.js
- pdf.js glyph position issue #7996 — https://github.com/mozilla/pdf.js/issues/7996
- pdf.js glyph bbox issue #12884 — https://github.com/mozilla/pdf.js/issues/12884
- pdfminer.six LAParams reference — https://pdfminersix.readthedocs.io/en/latest/reference/composable.html
- pdfplumber — https://github.com/jsvine/pdfplumber
- pypdf — https://github.com/py-pdf/pypdf
- lopdf — https://lib.rs/crates/lopdf
- pdf (pdf-rs) — https://lib.rs/crates/pdf
- pdf-extract — https://lib.rs/crates/pdf-extract
- hayro (repo) — https://github.com/LaurenzV/hayro
- hayro (crate) — https://lib.rs/crates/hayro
- hayro-interpret docs — https://docs.rs/hayro-interpret
- pdf_oxide repo — https://github.com/yfedoseev/pdf_oxide
- PDF Oxide docs / benchmarks — https://pdf.oxide.fyi/ and https://pdf.oxide.fyi/rust/docs/comparison/vs-lopdf
- pdfsink-rs — https://github.com/clark-labs-inc/pdfsink-rs
- pdfcpu — https://github.com/pdfcpu/pdfcpu and https://pdfcpu.io/
- ledongthuc/pdf — https://pkg.go.dev/github.com/ledongthuc/pdf
- unipdf LICENSE.md (commercial) — https://github.com/unidoc/unipdf/blob/master/LICENSE.md
- PdfPig — https://github.com/UglyToad/PdfPig
- PdfPig NuGet — https://www.nuget.org/packages/PdfPig/
- PdfPig.Rendering.Skia — https://github.com/BobLd/PdfPig.Rendering.Skia
- Apache PDFBox repo — https://github.com/apache/pdfbox
- Apache PDFBox downloads (3.0.8) — https://pdfbox.apache.org/download.cgi

**Benchmarks & security**
- py-pdf/benchmarks (speed + quality) — https://github.com/py-pdf/benchmarks
- PDF Association, Survey of Open-Source Solutions (rendering perf + memory) — https://pdfa.org/wp-content/uploads/2021/06/Survey-of-OpenSource-Solutions.pdf
- MuPDF CVEs by year — https://stack.watch/product/artifex/mupdf/
- Poppler CVEs by year — https://stack.watch/product/freedesktop/poppler/
- CVE-2026-6306 (PDFium buffer overflow) — https://www.sentinelone.com/vulnerability-database/cve-2026-6306/
- CVE-2026-11303 (PDFium UAF RCE) — https://www.thehackerwire.com/chrome-pdfium-use-after-free-rce-cve-2026-11303/
- CVE-2024-4367 (pdf.js arbitrary JS) — https://github.com/mozilla/pdf.js/security/advisories/GHSA-wgrm-67xf-hhpq
- CVE-2024-4367 technical writeup — https://codeanlabs.com/2024/05/cve-2024-4367-arbitrary-js-execution-in-pdf-js/

**Layout analysis algorithms**
- Shafait, Keysers, Breuel — Performance Comparison of Six Algorithms for Page Segmentation (DAS 2006) — https://link.springer.com/content/pdf/10.1007/11669487_33.pdf
- Breuel — Two Geometric Algorithms for Layout Analysis (DAS 2002) — https://link.springer.com/content/pdf/10.1007/3-540-45869-7_23.pdf
- PdfPig Document Layout Analysis wiki — https://github.com/UglyToad/PdfPig/wiki/Document-Layout-Analysis
- PdfPig UnsupervisedReadingOrderDetector source — https://github.com/UglyToad/PdfPig/blob/master/src/UglyToad.PdfPig.DocumentLayoutAnalysis/ReadingOrderDetector/UnsupervisedReadingOrderDetector.cs
- PyMuPDF multi_column.py — https://github.com/pymupdf/PyMuPDF-Utilities/blob/master/text-extraction/multi_column.py
- XY-Cut++ (arXiv 2504.10258) — https://arxiv.org/html/2504.10258v3
- Reading Order Inference for Complex Document Layouts (arXiv 2607.01018) — https://arxiv.org/html/2607.01018v1
- OmniDocBench — https://github.com/opendatalab/OmniDocBench
- Lin, Header and Footer Extraction by Page-Association (SPIE DRR X, 2003) — https://www.spiedigitallibrary.org/conference-proceedings-of-spie/5010/0000/Header-and-footer-extraction-by-page-association/10.1117/12.472833.short
- SciPlore Xtract (font-size title heuristic, 77.9%) — https://link.springer.com/chapter/10.1007/978-3-642-15464-5_45
- GROBID Principles — https://grobid.readthedocs.io/en/latest/Principles/
- GROBID PMC benchmark — https://grobid.readthedocs.io/en/latest/Benchmarking-pmc/
- HiPS: Hierarchical PDF Segmentation of Textbooks (arXiv 2509.00909) — https://arxiv.org/html/2509.00909
- Hernaes, Improved Dehyphenation of Line Breaks for PDF Text Extraction (2019) — https://ad-publications.cs.uni-freiburg.de/theses/Bachelor_Mari_Hernaes_2019.pdf
- Corraya, Dehyphenation of Words and Guessing Ligatures (2018) — https://ad-publications.informatik.uni-freiburg.de/theses/Master_Sumitra_Corraya_2018.pdf
- Camelot comparison / ICDAR-2013 numbers — https://camelot-py.readthedocs.io/en/latest/user/comparison.html

**ML layout models**
- Advanced Layout Analysis Models for Docling (arXiv 2509.11720) — https://arxiv.org/html/2509.11720v1
- docling-layout-heron model card (Apache-2.0) — https://huggingface.co/docling-project/docling-layout-heron
- Docling model catalog — https://docling-project.github.io/docling/usage/model_catalog/
- Docling Technical Report (arXiv 2408.09869) — https://arxiv.org/html/2408.09869v5
- DocLayNet paper (arXiv 2206.01062) — https://arxiv.org/pdf/2206.01062
- DocLayNet dataset card (CDLA-Permissive-1.0) — https://huggingface.co/datasets/ds4sd/DocLayNet/raw/main/README.md
- yolo-doclaynet (AGPL-3.0) — https://github.com/FreeOCR-AI/yolo-doclaynet
- DocLayout-YOLO (AGPL-3.0) — https://github.com/opendatalab/DocLayout-YOLO and https://arxiv.org/html/2410.12628v1
- PP-DocLayout (arXiv 2503.17213) — https://arxiv.org/html/2503.17213v1
- Surya (Apache-2.0 code, OpenRAIL-M weights) — https://github.com/datalab-to/surya
- HURIDOCS pdf-document-layout-analysis (LightGBM vs VGT) — https://github.com/huridocs/pdf-document-layout-analysis
- LayoutParser — https://github.com/Layout-Parser/layout-parser
