# R5 — EPUB Output: Generation, Validation, and Rendering for Visual QA

**Project:** OpenConvert (open-source, local-first PDF → reflowable EPUB desktop converter)
**Priorities:** low RAM, fast startup, small binary, minimal deps, open source
**Research date:** 2026-09-09 · **Status:** research only, no implementation
**Scope:** Track R5 — EPUB generation, validation, and rendering for visual/structural QA

> Citation convention: every non-obvious claim carries an inline `[Title](URL)` link to its source. Claims I could not verify with a live source this session (search budget ran out mid-task) are explicitly marked **[UNVERIFIED]** and should be re-checked before any decision is finalized on them.

---

## 0. Executive summary and tentative recommendations

- **EPUB profile to emit:** EPUB 3.3 as the primary/only rendition (no EPUB 2 dual-build). Include an NCX (`toc.ncx`, `application/x-dtbncx+xml`) alongside `nav.xhtml` purely for legacy-reader compatibility — it's cheap to generate and the EPUB 3.3 spec still lists it as a supported legacy feature — but do not maintain an EPUB 2 package. [EPUB 3.3](https://www.w3.org/TR/epub-33/)
- **Validation strategy:** build a small internal Rust validator (zip/OCF structure, OPF required elements, manifest↔spine consistency, media-type/extension checks, XHTML well-formedness, nav.xhtml presence) that runs on every conversion at near-zero cost, and treat full EPUBCheck (Java) as an **optional, opt-in "strict validate" button and a CI gate** — not a runtime dependency of the app. This avoids bundling a JVM in the default install while still giving users/CI access to the authoritative validator. A promising pure-Rust alternative, **epubveri**, is emerging but is pre-1.0 (v0.4.4) and AGPL-3.0-licensed, so treat it as a future option, not a v1 dependency. [veripublica/epubveri](https://github.com/veripublica/epubveri)
- **Rendering/QA strategy:** don't bundle headless Chromium (Playwright's Chromium alone is ~281 MB on disk [Playwright Browsers docs](https://playwright.dev/docs/browsers)) — instead reuse the desktop app's own OS-provided WebView (WebView2/WKWebView/WebKitGTK, whichever the shell already embeds) to render each XHTML chapter offscreen for a screenshot-based smoke test, **and** run a cheap DOM/heuristic structural QA pass (element counts, `scrollWidth>clientWidth` overflow detection, image-presence, text-density) on every conversion since it needs no rendering engine at all. Reserve pixel-diff tooling (odiff/pixelmatch/dssim) for optional visual-regression testing between OpenConvert versions on a fixed corpus — not for PDF-vs-EPUB comparison, where reflow makes pixel diffing meaningless; use structural/heuristic comparison for that instead.

---

## PART A — EPUB Format Decisions

### A1. EPUB 3.3 as the W3C Recommendation, and EPUB 2 compatibility

EPUB 3.3 became a **W3C Recommendation on 25 May 2023**, alongside EPUB Reading Systems 3.3 and EPUB Accessibility 1.1 — it is the current normative spec as of today (2026-09-09), with no EPUB 3.4/4.0 Recommendation superseding it yet. [EPUB 3.3 becomes a W3C Recommendation](https://www.w3.org/news/2023/epub-3-3-becomes-a-w3c-recommendation/) · [EPUB 3.3 spec text](https://www.w3.org/TR/epub-33/)

EPUB 3.3 is explicitly a **consolidation/clarification** release over EPUB 3.2, not a content model overhaul — it folds in the errata and clarifications accumulated since 3.2 and adds a formal test suite. [W3C Invites Implementations of EPUB 3.3](https://www.w3.org/blog/news/archives/9553)

Calibre's own converter guidance is blunt about the EPUB2-vs-3 tradeoff for a general-purpose converter: *"EPUB 2 is the most widely compatible, only use EPUB 3 if you know you actually need it."* [calibre ebook-convert manual](https://manual.calibre-ebook.com/generated/en/ebook-convert.html) — this reflects the long tail of older reading systems (older ADE, some library-lending platforms) that never got EPUB 3 support. For a **new, 2026-launched converter**, however, targeting current reading systems (Apple Books, Kobo, KOReader, Thorium, modern calibre, and Kindle's EPUB ingestion path) means EPUB 3.3 is the right default output; the NCX-for-legacy-readers pattern (below) is the low-cost hedge rather than a parallel EPUB 2 build.

### A2. Required package-document metadata

Per EPUB 3.3 §5.5.3 (Package metadata), every package document **MUST** include:
- `dc:identifier`, with the `package` element's `unique-identifier` attribute pointing to it
- `dc:title`
- `dc:language`
- `dcterms:modified` (an ISO 8601 UTC timestamp of the last modification) [EPUB 3.3](https://www.w3.org/TR/epub-33/)

A converter should regenerate `dcterms:modified` on every rebuild (many validators/reading systems treat a stale or missing value as a metadata error) and should mint a stable UUID (`urn:uuid:...`) for `dc:identifier` at first conversion, keeping it stable across re-conversions of the same source so caching/de-dup logic in reading systems doesn't treat updates as new books.

### A3. Manifest and spine properties

EPUB 3.3 Appendix D (manifest/spine vocabularies) defines the `properties` attribute values a converter needs to know about:

| Property | Where | Meaning |
|---|---|---|
| `nav` | manifest `item` | marks the EPUB navigation document |
| `cover-image` | manifest `item` | marks the cover raster/SVG image |
| `svg` | manifest `item` | content document embeds inline SVG |
| `mathml` | manifest `item` | content document embeds MathML |
| `scripted` | manifest `item` | content document uses JavaScript |
| `remote-resources` | manifest `item` | content document references resources outside the EPUB container |
| `page-spread-left` / `page-spread-right` | spine `itemref` | hints for pre-paginated/FXL rendering | [EPUB 3.3](https://www.w3.org/TR/epub-33/)

A converter that emits any inline SVG or MathML **must** set the corresponding property or EPUBCheck will flag it (`OPF-014`, see §B7) — this is one of the most common auto-generated-EPUB errors.

The spine `itemref` element's `linear` attribute (EPUB 3.3 §5.7.2) lets a converter mark auxiliary content (e.g., a generated glossary or an appendix pulled from PDF back-matter) as `linear="no"` so linear "next page" reading skips it while it stays reachable from the TOC/links. [EPUB 3.3](https://www.w3.org/TR/epub-33/)

### A4. NCX — still worth emitting

NCX (`toc.ncx`, media type `application/x-dtbncx+xml`) is formally a **legacy/backward-compatibility feature** in EPUB 3.3 §5.9.5, not required, but still recognized by the spec. [EPUB 3.3](https://www.w3.org/TR/epub-33/) Given it's cheap to derive from the same heading structure used to build `nav.xhtml`, and it meaningfully improves the reading experience on older or budget e-ink firmware that predates full EPUB 3 nav support, emitting both `nav.xhtml` (required) and `toc.ncx` (legacy aid) is the pragmatic default — this mirrors what calibre's converter does. [calibre conversion docs](https://manual.calibre-ebook.com/conversion.html)

### A5. epub:type semantics and pop-up footnotes

`epub:type` (EPUB 3.3 Appendix C, the EPUB Structural Semantics vocabulary) is used to mark structural roles — `chapter`, `footnote`, `noteref`, `toc`, `bodymatter`, etc. — on ordinary HTML elements via the `epub:type` attribute. [EPUB 3.3](https://www.w3.org/TR/epub-33/) · [DAISY KB: The epub:type Attribute](https://kb.daisy.org/publishing/docs/html/epub-type.html)

The well-established pattern for **pop-up footnotes** — supported by Apple Books/iBooks and (with the same markup) degrading gracefully to an in-line jump link on readers without pop-up support — is:
```html
<a epub:type="noteref" href="#fn1">1</a>
...
<aside epub:type="footnote" id="fn1">Note text…</aside>
```
Apple's own asset guide documents this pattern explicitly. [Apple Books Asset Guide: Pop-up Footnotes](https://help.apple.com/itc/booksassetguide/en.lproj/itccf8ecf5c8.html) · [gist: popup footnotes that degrade gracefully](https://gist.github.com/NickBarreto/9811005) This `aside`/`epub:type="footnote"` + `noteref` pairing is the correct target for a PDF→EPUB converter's footnote-reconstruction logic, since PDF footnotes (superscript markers + bottom-of-page text) map naturally onto it, and it fails safe (plain link) on readers that don't implement pop-ups.

### A6. MathML support — thin and fragmented in 2026

MathML support across EPUB reading systems remains a persistent weak spot. Older surveys (EPUBSecrets, dating to 2012 with a 2024 revision note) found only a handful of systems (Apple/iBooks, Readium-based apps, Infogrid Pacific's Azardi, VitalSource) rendered MathML at all, mostly via the MathJax polyfill rather than native support. [MathML Support in EPUB Reading Systems – EPUBSecrets](https://epubsecrets.com/mathml-support-in-epub-reading-systems.php) — **this source is dated and I could not find a fresher 2025/2026 comprehensive survey this session; treat the current state as [UNVERIFIED]** beyond the general, still-true pattern: native MathML rendering is inconsistent enough that MathJax-polyfilled SVG/PNG fallback images embedded via `<img>` (with the manifest `mathml` property still set on the source markup where MathML is retained) is the safer default for any content with real equations. Kindle-side reports as recently as its KDP community forum show ongoing inconsistency — a MathML fraction rendering correctly on Kindle mobile/Previewer but failing on Kindle Desktop — reinforcing that MathML is not implementation-complete across the KFX pipeline either. [KDP Community: MathML fraction rendering](https://kdpcommunity.com/s/question/0D58V00008KT0I7SAL/simple-fraction-using-mathml-does-not-render-on-kindle-desktop-but-works-on-phone-and-kindle-previewer) For OpenConvert (PDF source, where equations are typically images or embedded fonts/glyphs anyway rather than semantic math), the pragmatic v1 approach is: don't attempt PDF→MathML reconstruction; keep equation regions as cropped images with alt text, and leave true MathML support as a post-v1 stretch goal.

### A7. Core media types (images, fonts, audio)

Per EPUB 3.3 §3.2 (Core Media Types), the following can be used **without a fallback**:
- **Images:** GIF, JPEG, PNG, **WebP**, SVG (`image/svg+xml`)
- **Audio:** MP3, AAC-in-MP4, **Opus**-in-Ogg
- **Fonts:** TrueType, OpenType, WOFF, WOFF2
- **Other:** XHTML, CSS, JavaScript, SMIL [EPUB 3.3](https://www.w3.org/TR/epub-33/)

Two additions are specific to 3.3 and matter for a converter targeting current EPUB, not older 3.0/3.2 content: **WebP** (`image/webp`) and **Opus audio** were newly promoted to core media types in 3.3 — under EPUB 3.2 they required a fallback. [w3c/epubcheck issue #1189: Changes to core media types in 3.3](https://github.com/w3c/epubcheck/issues/1189) This means a PDF→EPUB converter emitting WebP-encoded raster images (smaller than PNG/JPEG at equivalent quality) is safe to do so for EPUB-3.3-declared output without needing an `<img>`/`<picture>` fallback — but only on reading systems that have actually implemented the 3.3 media-type list; conservative converters (or ones that want maximum reader compatibility beyond spec-compliance) should still default to JPEG/PNG and treat WebP as an opt-in size optimization. Any format outside the core list (BMP, TIFF, etc.) requires a `picture`/`source` fallback structure per DAISY's guidance. [DAISY KB: Core Media Types](https://kb.daisy.org/publishing/docs/epub/cmt.html)

EPUBCheck 5.3.0 (the current release, see §B1) also newly accepts `application/x-font-ttf` as a core font media type in addition to the canonical `font/ttf`, reflecting real-world variance in how tools label TrueType fonts. [EPUBCheck v5.3.0 release notes](https://github.com/w3c/epubcheck/releases/tag/v5.3.0)

### A8. CSS / reflowability best practices

The DAISY Knowledge Base's reflow guidance and multiple production-workflow write-ups converge on the same rules for reflowable EPUB CSS, all directly relevant to a PDF→EPUB converter that must **not** carry over PDF's absolute-positioning model:
- Avoid `position: absolute`/`fixed` and any pixel-perfect page layout — reflowable EPUB content must flow with reader-controlled font size/viewport. [DAISY KB: Reflow](https://kb.daisy.org/publishing/docs/css/reflow.html)
- Avoid fixed `width`/`height` in px on block content; use relative units (`em`, `%`, `rem`).
- `page-break-before/after/inside` (EPUB 2/CSS2.1-era) and the modern `break-before/after/inside` (CSS Fragmentation) properties are both still meaningfully supported by reading systems and should be used deliberately (e.g., `break-before: page` on chapter starts) rather than relying on manual empty paragraphs. [BlitzTricks — CSS tricks to improve your eBooks](https://friendsofepub.github.io/eBookTricks/)
- `widows`/`orphans` CSS properties have inconsistent EPUB reading-system support; industry practice (e.g. InDesign-EPUB workflows) is to not rely on them for pagination-critical control since reflow makes "page" a moving target anyway — they matter more for fixed-layout/print-like EPUBs. [Adobe Community: widow/orphan EPUB](https://community.adobe.com/t5/indesign-discussions/prevent-widow-and-orphan-from-indesign-to-epub/m-p/9590521)

For a converter architecture, this means: PDF's absolute-positioned text runs (columns, sidebars, captions) need to be re-flowed into normal block/inline flow during conversion — carrying over PDF coordinates as CSS `position:absolute` would technically "work" in some webview-based readers but breaks font-resizing/reflow (the core value proposition of EPUB over PDF) and will read badly on e-ink readers.

### A9. Fonts — embedding vs relying on reader fonts

Core font media types are TrueType, OpenType, WOFF, and WOFF2 (§A7 above); WOFF2 is the smallest and is a core type with no fallback requirement, so it's the preferred embedding format when a converter chooses to subset/embed fonts at all. [EPUB 3.3](https://www.w3.org/TR/epub-33/)

Font embedding for a PDF→EPUB converter is a genuine design decision, not an obvious default:
- **Embed when:** the PDF uses a distinctive, non-system font load-bearing to the content's identity (e.g., a technical document with a custom monospace/math font, or where system-font substitution would be jarring) — and only after checking font-license permission (most commercial fonts license embedding under obfuscation only; EPUB's font obfuscation mechanism exists exactly for this, and EPUBCheck restricts obfuscation to font-core-media-types specifically). [w3c/epubcheck: Restrict obfuscation to fonts](https://github.com/w3c/epubcheck/issues/1291)
- **Don't embed when:** the content is plain-prose text (the overwhelming majority of PDF→EPUB conversions) — rely on the reading system's own font stack and let the user's chosen reading font apply, which is exactly the flexibility reflowable EPUB is for. Embedding a font here only inflates file size and can *fight* the user's accessibility font preferences.

A pragmatic converter default: **do not embed fonts by default**; add an opt-in "preserve original typeface" mode later that embeds a WOFF2-subset of only the glyphs actually used (subsetting keeps size small), sourced from the PDF's embedded font program when legally embeddable, using EPUB's font-obfuscation only when the license requires it. [EPUB Fonts — toolkit.bot](https://toolkit.bot/blog/epub-fonts)

### A10. Fixed-layout EPUB — when to use it, and pitfalls

Fixed-layout (FXL) EPUB pins exact page geometry (like PDF/print) rather than reflowing. Industry guidance is consistent: use FXL for **comics/manga, children's picture books, cookbooks, and heavily-designed coffee-table/illustrated books** where preserving exact visual layout matters more than reflow; use reflowable for everything text-dominant. [eBOUND Canada: Fixed Layout vs. Reflowable EPUBs](https://www.eboundcanada.org/resources/fixed-layout-versus-reflowable-epubs/)

Pitfalls specifically relevant to OpenConvert's "scanned/comic PDF" edge case:
- **Accessibility regression:** FXL removes the reader's ability to resize text/reflow, which is a real accessibility cost — screen-reader/low-vision users lose the main benefit of EPUB over PDF. [eBOUND Canada](https://www.eboundcanada.org/resources/fixed-layout-versus-reflowable-epubs/)
- **Higher QA burden and cost:** FXL requires per-page/per-spread visual QA (closer to print production) rather than "does it flow correctly," which is a heavier and more manual process than reflowable QA. [eBOUND Canada](https://www.eboundcanada.org/resources/fixed-layout-versus-reflowable-epubs/)
- **Uneven retailer/reader support**, especially for older EPUB2-era FXL extensions; EPUB3 FXL is broadly but not universally supported. [eBOUND Canada](https://www.eboundcanada.org/resources/fixed-layout-versus-reflowable-epubs/)

**Recommendation for OpenConvert:** detect scanned/image-only PDFs (no extractable text layer, or OCR confidence very low) and route them to a **fixed-layout EPUB with an underlying OCR text layer** (image displayed, invisible OCR'd text overlaid for search/select/accessibility) rather than force-reflowing garbage OCR text — this is the standard "image + hidden text" pattern used by scanning pipelines, adapted to EPUB FXL's page-image model. For everything else (the vast majority of real-world PDFs — text documents, reports, books), reflowable is correct and is the converter's primary target.

### A11. Reader compatibility matrix (informed synthesis)

| Reader | Engine/basis | EPUB 3 nav | Pop-up footnotes | MathML | Notes |
|---|---|---|---|---|---|
| **Apple Books** | WebKit-based | Yes | Yes (native, documented pattern) | Partial (historically among the better-supported) | [Apple Books Asset Guide](https://help.apple.com/itc/booksassetguide/en.lproj/itccf8ecf5c8.html) |
| **Kindle (native/Send-to-Kindle)** | Converts to KFX internally | Via KFX conversion of nav | Inconsistent (KDP forum reports show MathML fraction rendering differences across Kindle Desktop vs mobile/Previewer) | Inconsistent | Since **late 2022**, Send to Kindle accepts EPUB directly (Amazon converts it internally to KFX); MOBI/AZW direct-send was discontinued in Aug 2022. [Wikipedia: Amazon Kindle](https://en.wikipedia.org/wiki/Amazon_Kindle) |
| **Kobo** | Proprietary (kepub extensions on top of EPUB3) | Yes | Supported via kepub-specific markup extensions (historically requested/discussed in kobo's own epub-spec repo) | [UNVERIFIED — could not confirm current state this session] | [kobolabs/epub-spec issue on footnote support](https://github.com/kobolabs/epub-spec/issues/59) |
| **Adobe Digital Editions (ADE)** | Own rendering engine | Yes (EPUB3) | Limited/inconsistent historically | Weak | ADE's ~260KB-per-XHTML-file practical limit still drives calibre's default flow-size splitting (see §A13) |
| **Google Play Books** | Web-based | Yes | [UNVERIFIED] | [UNVERIFIED] | Accepts EPUB uploads per official partner guidelines [Google Play Books Partner Center: EPUB files](https://support.google.com/books/partner/answer/3316879?hl=en) |
| **Thorium Reader** | Electron + Readium Desktop toolkit (open source, EDRLab) | Yes, strong EPUB3/accessibility support (it's a reference Readium implementation) | Yes | Better than most (Readium-based) | [EDRLab: Thorium Reader](https://www.edrlab.org/software/thorium-reader/) |
| **Calibre viewer** | Own Qt/WebEngine-based viewer | Yes | Reasonable | Reasonable | — |
| **KOReader** | crengine (own layout engine, not a full browser engine) | Partial — crengine is a purpose-built e-book rendering engine, not a full CSS3/HTML5 engine, so complex CSS/JS-dependent EPUB3 features can behave differently than in a browser-based reader | Limited | Limited | [koreader/crengine](https://github.com/koreader/crengine) |
| **Moon+ Reader / Lithium** | Proprietary (Android) | Generally good practical EPUB3 support per community reports | [UNVERIFIED] | [UNVERIFIED] | — |

**Practical takeaway:** the common denominator across all of the above is solid support for reflowable EPUB3 with `nav.xhtml`, standard CSS, and JPEG/PNG images — that's the safe target. Pop-up footnotes via `aside`/`epub:type="footnote"` degrade gracefully everywhere. MathML and Kobo/Kindle-specific extension features are the areas with real fragmentation, so OpenConvert should not depend on them for v1 correctness.

### A12. Accessibility metadata

EPUB Accessibility 1.1 (W3C Recommendation, same date as EPUB 3.3) defines, per its §2.2, metadata that publications **MUST** include:
- `schema:accessMode` (e.g., `textual`, `visual`)
- `schema:accessibilityFeature` (e.g., `structuralNavigation`, `tableOfContents`, `alternativeText`)
- `schema:accessibilityHazard` (e.g., `noFlashingHazard`, `none`)

...and **SHOULD** include:
- `schema:accessModeSufficient`
- `schema:accessibilitySummary` (note: this changed from required in 1.0 to recommended in 1.1)

A `schema:conformsTo` value naming the conformance level (e.g., "EPUB Accessibility 1.1 - WCAG 2.1 Level AA") is required for publications claiming formal accessibility conformance. [EPUB Accessibility 1.1](https://www.w3.org/TR/epub-a11y-11/)

For a PDF→EPUB converter, `accessibilityHazard: none` and `accessMode: textual` (plus `visual` if images are present) are safe, honest defaults to auto-populate; claiming `accessibilityFeature: structuralNavigation`/`tableOfContents` is only honest if the converter actually reconstructs a real heading-based nav document (which it should, per §A2–A4) rather than a flat page list. Full WCAG-conformance claims should not be auto-generated — that requires actual verification the tool cannot guarantee from PDF source alone.

### A13. What calibre's ebook-convert emits (TOC, splitting, file-size limits)

Calibre's `ebook-convert` (documented for calibre 9.14.0, current as fetched) offers:
- **Inline TOC:** `--epub-inline-toc` inserts a TOC as part of the visible book content (in addition to `nav.xhtml`), with placement controlled by `--epub-toc-at-end`. [calibre ebook-convert manual](https://manual.calibre-ebook.com/generated/en/ebook-convert.html)
- **Automatic file splitting:** calibre splits any generated XHTML file larger than a configurable size, **defaulting to 260KB**, explicitly because *"most EPUB readers cannot handle large file sizes. The default of 260KB is the size required for Adobe Digital Editions."* This can be disabled (`--flow-size 0`) but the default reflects real ADE limitations that are still relevant for any converter aiming at broad reader compatibility, not just modern webview-based readers. [calibre ebook-convert manual](https://manual.calibre-ebook.com/generated/en/ebook-convert.html)
- **EPUB version selection:** `--epub-version` toggles 2 vs 3, with calibre's own docs recommending EPUB 2 "unless you know you actually need" EPUB 3 (§A1). [calibre ebook-convert manual](https://manual.calibre-ebook.com/generated/en/ebook-convert.html)

**Implication for OpenConvert:** replicate the ~260KB-per-XHTML-chapter split threshold as a safety default (configurable), since it's a well-tested empirical number from calibre's decades of real-world reader compatibility testing, not an arbitrary choice.

---

## PART B — Validation

### B1. EPUBCheck — the authoritative validator

EPUBCheck is the official W3C/DAISY conformance checker, implemented in Java, **BSD-3 licensed** (per its GitHub repo/DAISY project page), currently at **v5.3.0** (released 2025-09-01, confirmed via both the GitHub release page and the W3C public-publishingcg mailing list announcement), described as "the latest production-ready release," supporting EPUB 3.3 conformance checking. [EPUBCheck v5.3.0 release](https://github.com/w3c/epubcheck/releases/tag/v5.3.0) · [W3C mailing-list announcement](https://lists.w3.org/Archives/Public/public-publishingcg/2025Sep/0000.html) · [DAISY: EPUBCheck project page](https://daisy.org/activities/projects/epubcheck/)

**Packaging/distribution facts confirmed:**
- Distributed as a JAR, invoked `java -jar epubcheck.jar publication.epub`. [DAISY KB: EPUBCheck](https://kb.daisy.org/publishing/docs/epub/validation/epubcheck.html)
- Also published to Maven Central as `org.w3c:epubcheck`. [EPUBCheck releases page](https://w3c.github.io/epubcheck/releases/)
- Wrapped for other ecosystems: a **PyPI `epubcheck` package** exists (a Python wrapper that still requires/bundles a JRE invocation) and a **RubyGems `epubcheck-ruby`** gem. [epubcheck · PyPI](https://pypi.org/project/epubcheck/) [UNVERIFIED whether the PyPI wrapper bundles its own JRE vs requiring a system Java — worth checking directly before depending on it.]

**What I could NOT confirm this session (mark [UNVERIFIED], recommend direct follow-up):**
- **No official GraalVM native-image build or jlink minimal-runtime distribution of EPUBCheck was found** in the release notes or docs I could fetch — EPUBCheck ships as a plain JAR requiring an installed JRE (baseline historically Java 8, current minimum for 5.x [UNVERIFIED — not explicitly re-confirmed for 5.3.0 in the sources fetched]). This is a real constraint: **bundling EPUBCheck in a "low RAM, fast startup, small binary" desktop app means bundling or requiring a JVM**, which directly conflicts with OpenConvert's stated priorities. A community-maintained Docker image (`gnue/epubcheck` on Docker Hub) exists, confirming people do containerize it, but that's not a path to a small native desktop binary. [Docker Hub: gnue/epubcheck](https://hub.docker.com/r/gnue/epubcheck/tags)
- **Typical runtime per book:** not confirmed with a concrete benchmark number this session — general JVM-startup overhead (order of several hundred ms to ~1s cold-start) plus schema validation of a multi-chapter book suggests low-single-digit seconds per book is a reasonable expectation, but this is **[UNVERIFIED]** and should be benchmarked directly against representative OpenConvert output before being used in any UX-latency decision (e.g., "should validation block the UI").
- **JAR/dependency footprint:** not confirmed with a specific MB figure this session — **[UNVERIFIED]**.

This strongly supports the recommendation in §0/§B8: EPUBCheck should be **optional** (CI, or a "Run full validation" button that shells out to a user- or app-installed JRE, or is bundled only in a separate/optional download), never a hard runtime dependency of the core conversion path.

### B2. Ace by DAISY — accessibility checker

Ace ("Accessibility Checker for EPUB") is DAISY's Node.js-based accessibility auditor, distributed as `@daisy/ace` on npm, open source. [daisy/ace on GitHub](https://github.com/daisy/ace) · [@daisy/ace on npm](https://www.npmjs.com/package/@daisy/ace) It checks things EPUBCheck explicitly does not — the DAISY KB's own EPUBCheck page recommends running Ace (and DAISY's "SMART" tools) *in addition to* EPUBCheck for accessibility coverage, since EPUBCheck's own documentation states it "provides limited CSS validation, cannot verify scripts, and lacks comprehensive accessibility checks." [DAISY KB: EPUBCheck](https://kb.daisy.org/publishing/docs/epub/validation/epubcheck.html) Like EPUBCheck, Ace is a Node.js tool (not a lightweight native binary), so the same "optional/CI-only" bundling logic applies if OpenConvert ever adds accessibility auditing.

### B3. Alternative non-Java validators

The most significant finding of this research track is **epubveri** (`veripublica/epubveri`), a **pure-Rust EPUB validator** explicitly positioned as *"a fast, JVM-free, embeddable alternative to epubcheck."* [veripublica/epubveri](https://github.com/veripublica/epubveri)

Key facts:
- **Architecture:** Cargo workspace with a core validation library, a WASM binding (`epubveri-wasm`, so it can even run in-browser without a JVM), and a harness that measures accuracy against EPUBCheck's own official test corpus. [epubveri ARCHITECTURE.md](https://github.com/veripublica/epubveri/blob/main/docs/ARCHITECTURE.md)
- **Validation coverage:** five-layer pipeline (ZIP integrity → OCF container → OPF package document → content documents [XHTML/SVG/MathML] → optional extensions [Media Overlays, EDUPUB, Dictionaries, Indexes, Multiple Renditions]) using custom RELAX NG and Schematron engines. [epubveri ARCHITECTURE.md](https://github.com/veripublica/epubveri/blob/main/docs/ARCHITECTURE.md)
- **Measured accuracy vs EPUBCheck's test suite:** 98.8% of invalid files correctly flagged with matching error codes, 1.1% false-positive rate on valid files. The maintainers explicitly describe it as *"not yet a drop-in replacement"* but suitable for production use in many scenarios. [veripublica/epubveri README](https://github.com/veripublica/epubveri)
- **Maturity:** pre-1.0, currently **v0.4.4**, "under active development." Known gaps include incomplete CSS validation and some whole-container checks; some EPUBCheck message codes (e.g. `OPF-097`) have no corresponding test coverage in epubveri yet. [epubveri ARCHITECTURE.md](https://github.com/veripublica/epubveri/blob/main/docs/ARCHITECTURE.md)
- **License:** dual-licensed **AGPL-3.0** (free) / commercial (for closed-source embedding). [veripublica/epubveri README](https://github.com/veripublica/epubveri)

**Assessment for OpenConvert:** epubveri is exactly the kind of dependency that matches OpenConvert's low-RAM/fast-startup/small-binary/Rust-friendly priorities architecturally — but at pre-1.0 with a 98.8%/1.1% accuracy profile (not yet spec-complete) and an AGPL license, it is not yet a safe default-bundled replacement for the internal validator (§B8) in v1. It's worth **watching closely and re-evaluating in a future round** once it reaches 1.0 — and worth reaching out to the maintainers, since an embeddable Rust validator is precisely what a Rust-based converter needs long-term. The AGPL-3.0 license also needs explicit legal review against whatever license OpenConvert itself adopts before any integration (AGPL's network-copyleft clause is less relevant for a fully local-first desktop app with no server component, but the copyleft obligations on the codebase itself still apply).

No other credible Rust/Go/Python-native (non-JVM) EPUB validator with meaningful spec coverage surfaced in this research beyond epubveri; most "alternatives" found were either GUI wrappers around EPUBCheck itself (below) or unrelated tools (e.g. `kepubify` is a Kobo-format *converter*, not a validator, and `epubtest.org` is a reader-compatibility test-suite site rather than a standalone validator tool).

### B4. Readium's own checks

Readium's toolkits (readium-js, the Kotlin/Swift toolkits, the streamer/parser layer) perform **parsing-time validation** as a side effect of building their internal `Publication` model (rejecting malformed OPF, resolving manifest/spine references) but Readium does not ship a standalone "conformance checker" comparable to EPUBCheck — its parsers are permissive-by-necessity since they must open real-world (sometimes non-conformant) EPUBs users already own. [Readium architecture: streamer/parser](https://readium.org/architecture/streamer/parser/metadata.html) This makes Readium's parser useful as a **"does it actually open in a real reading-system codebase" smoke test** (see Part D — rendering via a Readium-based reader is a legitimate QA signal) but not a substitute for EPUBCheck-grade conformance checking.

### B5. pagina EPUB-Checker — GUI wrapper

`pagina EPUB-Checker` (paginagmbh/EPUB-Checker on GitHub) is a standalone desktop GUI application for Windows/macOS/Linux that wraps EPUBCheck (and, per the W3C's own "Apps and Tools" page, is one of the recommended GUI front-ends alongside FlightDeck and an Oxygen XML Editor plugin). [paginagmbh/EPUB-Checker](https://github.com/paginagmbh/EPUB-Checker) · [W3C EPUBCheck: Apps and Tools](https://w3.org/publishing/epubcheck/docs/apps-and-tools) It still bundles/requires a JRE under the hood (it's a GUI shell around the same Java tool), so it doesn't change the JVM-dependency calculus for OpenConvert — it's useful as a reference for what a "nice EPUBCheck UX" looks like, and as a manual verification tool for developers, not as an embeddable dependency.

### B6. Most common EPUBCheck errors emitted by converters — what the internal validator should target

Cross-referencing converter-error guides and EPUBCheck's own issue tracker, the errors that **converter output** (as opposed to hand-authored EPUB) most commonly trips are:

| Code | Meaning | Typical converter cause |
|---|---|---|
| **RSC-005** | Error while parsing file (malformed XML/HTML) | Unclosed tags, bad entity encoding, leftover cruft from Word/Google-Docs-style source HTML, or — in PDF→EPUB's case — malformed markup from PDF text/layout extraction | [formatmyebook.com error guide](https://formatmyebook.com/blog/kdp-common-errors-guide/) |
| **RSC-012** | Fragment identifier not defined | TOC/nav links or footnote `noteref`/`footnote` id pairs pointing at IDs that don't exist — a real risk if PDF-derived heading/footnote reconstruction ever mis-links | [formatmyebook.com error guide](https://formatmyebook.com/blog/kdp-common-errors-guide/) |
| **OPF-014** | Manifest `properties` should be declared | SVG/MathML/scripted content present in a document but not flagged in the manifest `item` — directly maps to §A3 above | [formatmyebook.com error guide](https://formatmyebook.com/blog/kdp-common-errors-guide/) |
| **PKG-007** | mimetype file not first / not stored uncompressed | Using a generic zip library/CLI that doesn't special-case the `mimetype` entry — a classic hand-rolled-zip mistake (see §C5) | [formatmyebook.com error guide](https://formatmyebook.com/blog/kdp-common-errors-guide/) |
| **OPF-027** | Undefined property | Mostly an InDesign-export artifact (page-nav anchors), lower relevance to a PDF-source converter | [formatmyebook.com error guide](https://formatmyebook.com/blog/kdp-common-errors-guide/) |

This is a useful, concrete checklist: OpenConvert's internal lightweight validator (§B8) should, at minimum, guarantee **RSC-005-class well-formedness**, **RSC-012-class link/id resolution**, **OPF-014-class manifest-property completeness**, and **PKG-007-class zip-structure correctness** — since these four categories are both the most common failures in generated (not hand-authored) EPUB and the cheapest to check without a full RELAX NG/Schematron engine (they're structural/consistency checks, not deep content-model validation).

### B7. Strategy recommendation: internal lightweight validator + optional full EPUBCheck

Given:
1. EPUBCheck requires a JVM with no confirmed lightweight/native distribution (§B1) — directly at odds with "small binary, fast startup, low RAM."
2. The most common converter-generated errors (§B6) are structural/consistency issues fully checkable without a JVM or a full schema engine.
3. A promising pure-Rust alternative (epubveri) exists but isn't yet mature/license-compatible enough to bundle by default (§B3).

**Recommended architecture:**
- **Tier 1 (always-on, built into OpenConvert, Rust, no external process):** a hand-written validator covering OCF/zip structure (mimetype first+stored, `META-INF/container.xml` present and valid), OPF required-metadata presence (§A2), manifest↔spine referential integrity, manifest `properties` completeness for SVG/MathML/scripted content (§A3/§B6), media-type/extension consistency, and XHTML well-formedness (a plain XML parse, which any Rust XML crate gives for free). This runs in milliseconds, ships in the core binary, and directly targets the error classes in §B6.
- **Tier 2 (optional, explicit user action or CI):** shell out to EPUBCheck if a JRE is present/user opts in ("Run full EPUBCheck validation"), or integrate epubveri once it matures past 1.0 and its license is confirmed compatible. Never make this a blocking step of the default conversion flow.
- **Tier 3 (CI only, not shipped to end users):** run EPUBCheck (and optionally Ace) against a regression corpus of real-world PDFs on every OpenConvert release, so spec conformance is continuously verified without imposing a JVM dependency on end-user machines.

---

## PART C — EPUB Generation Libraries

### C1. Rust

- **`epub-builder`** (crates.io, currently maintained under `lise-henry/epub-builder`, MPL-2.0 licensed): the most established Rust EPUB-writing crate, latest published version **0.8.0 (Feb 3, 2025)**, modest but non-trivial usage (~1,195 downloads/month, #71 in the "Text processing" crates.io category). [lib.rs: epub-builder](https://lib.rs/crates/epub-builder) It supports both EPUB 2.0.1 (default) and EPUB 3.0.1 output, auto-generates the boilerplate files (`mimetype`, `content.opf`, `toc.ncx`, `nav.xhtml`), and offers optional inline-TOC generation — but the maintainers themselves note it does *not* handle "various EPUB features" and provides no default CSS/XHTML templates, leaving book styling entirely to the caller. [github.com/lise-henry/epub-builder](https://github.com/lise-henry/epub-builder) Its EPUB 3 target is **3.0.1**, not the current 3.3 — a gap OpenConvert would need to verify/patch (in practice EPUB 3.0.1-shaped output is largely forward-compatible with 3.3 since 3.3 is a clarifying revision, but exact manifest-property vocab and newer media types like WebP/Opus would need manual handling on top of the crate).
- **`epub` crate** (crates.io): a **reading**, not writing, library — relevant only if OpenConvert ever needs to re-parse its own or third-party EPUB output (e.g., for the internal validator or a "re-open to verify" QA step), not for generation.
- **Underlying primitives** a hand-rolled or `epub-builder`-based generator would use regardless: a **zip crate** (`zip` is the standard choice) for the OCF container — critical detail: the `mimetype` entry must be the *first* entry in the archive and **stored, not deflated** (per §B6's PKG-007), which the `zip` crate supports via explicit per-entry compression method control — and **`quick-xml`** for writing the OPF/nav/NCX/XHTML XML, which is a fast, low-dependency streaming XML writer well-suited to a low-RAM, fast-startup tool.

### C2. Python

**`ebooklib`** (PyPI `EbookLib`) is the dominant Python EPUB read/write library — current version **0.20**, last released **26 October 2025** (actively maintained, confirmed this session), licensed **AGPLv3+**, and explicitly supports both EPUB2 and EPUB3. [PyPI: EbookLib](https://pypi.org/project/EbookLib/) Its AGPL license is a material consideration for any commercial or permissively-licensed downstream use, though for a fully open-source, local-first tool like OpenConvert it's less of an obstacle than for a SaaS product. Since OpenConvert's stated priorities (low RAM, fast startup, small binary) push toward Rust rather than a Python runtime anyway, `ebooklib` is more relevant as a **reference implementation to study** (its OPF/nav generation code is a good example of correct EPUB3 structure) than as a direct dependency.

`epubmaker` — no strong, actively maintained, standalone project surfaced in this research distinct from calibre's own internal `calibre.ebooks.epub` conversion pipeline; not recommended as a dependency.

### C3. JS/TS

The Node.js EPUB-generation ecosystem has a clear lineage: the original **`epub-gen`** package (by `cyrilis`) is the ancestor of several forks; the actively maintained continuation is **`@lesjoursfr/html-to-epub`**, which explicitly credits itself as "Inspired by cyrilis/epub-gen" / forked from it. [github.com/lesjoursfr/html-to-epub](https://github.com/lesjoursfr/html-to-epub) It supports generating **either EPUB 3 (default) or EPUB 2** via a version option. [github.com/lesjoursfr/html-to-epub](https://github.com/lesjoursfr/html-to-epub) I could not confirm epub-gen's own npm page's explicit deprecation notice directly this session (404 on fetch) — treat "epub-gen itself is stale, use the lesjoursfr fork" as the reasonable and community-repeated conclusion but **[UNVERIFIED against epub-gen's own current README]**. These are all **HTML→EPUB wrappers** (they take pre-rendered HTML chapters and handle packaging), which is architecturally the closest analog to what OpenConvert needs — but they're Node.js-based, so relevant only if any part of OpenConvert's pipeline runs JS (e.g. a bundled webview-based conversion step), not for the core Rust binary.

### C4. C#

- **`VersOne.Epub`** — an actively maintained EPUB **reader** (parsing) library for .NET, current NuGet version **3.3.6**. [NuGet: VersOne.Epub](https://www.nuget.org/packages/VersOne.Epub/) It is explicitly a reader, not a writer — confirmed by the task brief and by its NuGet description focus; not usable for generation.
- **`EpubSharp`** — an older C# EPUB reader library (`asido/EpubSharp`); like VersOne.Epub, reader-focused, and appears less actively maintained (no recent release signal surfaced in this research).

**Assessment:** the C# ecosystem has no strong, actively maintained *EPUB-writing* library equivalent to Rust's `epub-builder` or JS's `html-to-epub` — this is a moot point for OpenConvert regardless, since C#/.NET is not indicated as the implementation language given the stated Rust-friendly, low-RAM priorities, but worth noting for completeness.

### C5. Hand-rolling the package — is it preferable?

The task brief's framing is correct: an EPUB is fundamentally "just" a zip file (with one special first entry) containing an XML metadata file (OPF), one or more XML navigation files, XHTML content documents, and assets. Given:
- `epub-builder`'s EPUB3 target is 3.0.1, not 3.3, and it explicitly doesn't cover many features (§C1);
- The core correctness risks are almost entirely about *getting the zip/OCF structure and OPF/manifest exactly right* (§B6's PKG-007, OPF-014, RSC-012) — a small, well-tested surface area — rather than needing a large abstraction library;
- OpenConvert's priorities (low RAM, fast startup, small binary, minimal deps) favor **fewer, thinner dependencies** over a full-featured builder crate carrying assumptions OpenConvert may not want (e.g., `epub-builder`'s built-in NCX/nav generation logic, templating choices);

**hand-rolling the OPF/nav/NCX writer directly (via `quick-xml`) and the OCF zip packaging (via the `zip` crate, with explicit control over the mimetype entry's compression method) is a reasonable and arguably preferable approach for OpenConvert**, rather than depending on `epub-builder`. The risk surface hand-rolling avoids (getting zip/OPF exactly right) is well-documented (§B6) and directly testable via the Tier-1 internal validator (§B7) — so hand-rolling doesn't trade away safety, it trades a coarse third-party abstraction for direct control that matches the project's dependency-minimalism goal. `epub-builder` remains worth vendoring code/ideas from (it's MPL-2.0, permissive enough to read/adapt) even if not taken as a direct dependency.

---

## PART D — Rendering EPUB for Visual/Structural QA

### D1. Headless Chromium (Playwright/CDP) — size verdict

Playwright's own documentation confirms bundled browser disk sizes: **Chromium ~281 MB**, Firefox ~187 MB, WebKit ~180 MB, each downloaded and cached separately from the Playwright package itself. [Playwright: Browsers](https://playwright.dev/docs/browsers) This is **flatly incompatible with OpenConvert's "small binary" priority** as a bundled runtime dependency — 281 MB dwarfs any reasonable target size for a desktop converter's install footprint. It could conceivably be used **only** as a CI/dev-machine tool (for generating a "golden" regression corpus, run once by maintainers, never shipped to end users) — never as an in-app dependency.

### D2. The desktop app's own embedded WebView — recommended approach

Since OpenConvert (per the project brief) is a desktop app, it almost certainly already embeds a WebView for its UI (Tauri's WebView2/WKWebView/WebKitGTK being the natural low-overhead choice for a Rust app matching the stated priorities, versus Electron's bundled Chromium which carries the same size problem as Playwright). **Reusing that already-present WebView to render EPUB XHTML chapters offscreen and capture a screenshot is the only rendering approach that adds effectively zero marginal binary size/RAM**, since the engine is already resident for the UI.

Feasibility notes:
- Tauri exposes webview screenshot capability via its `tao`/webview layers, and community tooling (e.g. MCP servers wrapping `tauri_webview_screenshot`) confirms this is a workable, if not deeply documented, pattern. [glama.ai: tauri webview screenshot tool](https://glama.ai/mcp/servers/hypothesi/mcp-server-tauri/tools/tauri_webview_screenshot) Off-screen/headless rendering of a webview (rendering without a visible window) has open discussion/requests in Tauri's own `tao` windowing-library issue tracker, suggesting it's achievable but not a first-class, trivially-documented Tauri feature as of the sources found. [tauri-apps/tao issue #289: Off screen rendering](https://github.com/tauri-apps/tao/issues/289)
- **Determinism across OS webviews is a real and significant caveat, not a minor detail** — WebView2 (Chromium/Blink on Windows), WKWebView (WebKit on macOS), and WebKitGTK (WebKit on Linux) are **three different rendering engines** with their own font-rendering, subpixel/anti-aliasing, and CSS-edge-case behavior. A pixel-level screenshot taken via WebView2 on a Windows CI runner will **not** pixel-match a WKWebView screenshot from a macOS dev machine even for identical input — this rules out cross-platform pixel-diff regression testing using the native webview unless the comparison is always same-OS-to-same-OS (e.g., "does this Windows build's output differ from last Windows build's output," never "does Windows match macOS"). This reinforces §D9's recommendation to prefer structural/DOM-based comparison over pixel comparison wherever the comparison must be platform-independent.
- WebView2 itself does not need to be bundled on Windows — it's normally an OS/Edge-provided "Evergreen" runtime that Microsoft documents distribution strategies for (evergreen vs. fixed-version bundling), so if Tauri is the shell, WebView2 availability is a Windows-install-time concern, not something OpenConvert's binary needs to carry. [Microsoft Learn: Distribute your app and the WebView2 Runtime](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution)

### D3. epub.js / foliate-js / Readium-based reader libraries in a webview

**foliate-js** (`johnfactotum/foliate-js`) is directly relevant: it's a **standalone, MIT-licensed, no-build-step JS library** (native ES modules) that renders EPUB (and MOBI/KF8/FB2/CBZ/PDF) using the same **CSS multi-column pagination** approach as epub.js, and — critically — is explicitly usable in **any modern webview**, not just Electron/browsers, since it depends only on standard Web APIs (Blob, TextDecoder, DOMParser). [github.com/johnfactotum/foliate-js](https://github.com/johnfactotum/foliate-js) Its own docs are candid that *the library itself is "not stable... expect it to break,"* though its code underlies the shipped, stable Foliate Linux e-reader app, so it's real-world battle-tested despite the API-stability caveat. [github.com/johnfactotum/foliate-js](https://github.com/johnfactotum/foliate-js) This makes foliate-js a strong candidate for the actual **rendering harness** loaded inside OpenConvert's own webview for QA screenshots — it gives a realistic, paginated, spec-following rendering of the generated EPUB rather than a naive raw-XHTML dump, at effectively no size cost (it's a handful of JS files, not a bundled binary).

**Thorium Reader** (EDRLab, built on the Readium Desktop toolkit) is the most complete open-source *reference reading system* — strong EPUB3/accessibility support since it's essentially a reference Readium implementation. [EDRLab: Thorium Reader](https://www.edrlab.org/software/thorium-reader/) It's Electron-based, so **not embeddable inside OpenConvert** without inheriting Electron's size (defeats the purpose), but it is an excellent **external, manual/CI cross-check tool**: maintainers can periodically open OpenConvert's generated EPUBs in Thorium to sanity-check against a best-in-class independent implementation, separate from the in-app QA pipeline.

### D4. Servo / Ladybird as embeddable engines — 2026 status

**Servo**, as of Phoronix's January 2026 coverage, is at version **0.0.5** — explicitly experimental by its own version numbering, with active work in early 2026 on Ogg audio, CSS property support, cryptographic/TLS features, and — notably — **"continuing to enhance its ability to embed Servo inside other applications,"** confirming embeddability is an active project goal, not yet a finished capability. [Phoronix: Servo Browser Engine Starts 2026](https://www.phoronix.com/news/Servo-January-2026) Given the 0.0.x versioning and ongoing core-feature work (JS module loading, IndexedDB still gaps per the same source), Servo is **not yet production-ready as an embeddable rendering engine for a shipping desktop app in 2026** — worth tracking, not adopting now.

**Ladybird** was found referenced in searches (independent, non-Chromium engine, SerenityOS-derived) with active 2026 development coverage, but I did not fetch a detailed status page this session — **[UNVERIFIED]** beyond confirming it exists and has ongoing 2026 development activity per secondary press coverage (piunikaweb, thestackstories). Given Ladybird's own public roadmap has historically targeted a 2026 "usable browser" milestone with embeddability as a longer-term goal, treat it the same as Servo: **watch, don't adopt for v1**.

### D5. WeasyPrint

WeasyPrint (BSD-licensed, Python) renders HTML+CSS to PDF/PNG and is a mature, well-established layout engine — but it is fundamentally a **print/paged-media layout engine**, not a reflowable-EPUB-pagination engine, and pulling in a Python runtime + WeasyPrint's own dependency chain (Pango, cairo, etc.) directly conflicts with a Rust-based, low-RAM, fast-startup, small-binary desktop app. Its relevance to OpenConvert is narrow: it could be useful **only** as an external, maintainer-run tool for rendering a static snapshot of an EPUB chapter's HTML+CSS to compare general layout sanity — but the webview-based approach (§D2) already covers this without adding a Python dependency, so WeasyPrint is not recommended for OpenConvert's pipeline.

### D6. Blitz (Rust: Stylo + Taffy + Vello/Skia)

Blitz (`DioxusLabs/blitz`) is the most architecturally aligned option with OpenConvert's Rust-native, modular philosophy — it combines **Stylo** (Servo's CSS engine) for CSS, **Taffy** for box-level layout, **Parley** for text layout, and an **AnyRender** abstraction supporting Vello or Skia for painting, exposed as composable crates (`blitz-dom`, `blitz-html`, `blitz-shell`). [github.com/DioxusLabs/blitz README](https://github.com/DioxusLabs/blitz/blob/main/README.md) However, **as of the README fetched this session, Blitz explicitly describes itself as "beta"** — it can already render many popular no-JS websites (Wikipedia, old Reddit) but the maintainers state directly there are *"still many bugs and missing features"* and that they are *"actively working on bringing it up to production quality."* [github.com/DioxusLabs/blitz README](https://github.com/DioxusLabs/blitz/blob/main/README.md) No explicit headless-screenshot API was confirmed in the README content fetched — plausible given the modular paint-backend architecture, but **[UNVERIFIED]**.

**Assessment:** Blitz is the single most promising long-term rendering dependency for OpenConvert given the shared Rust/low-overhead philosophy, and is worth prototyping against — but its own maintainers' "beta, not production quality yet" framing means it should not be the *only* QA rendering path in a v1; pair it with (or fall back to) the webview approach (§D2), which uses production-grade, already-present engines.

### D7. Render-free structural checks

The lowest-cost, most deterministic QA layer requires no rendering engine at all — pure XML/DOM-tree analysis of the generated XHTML:
- Heading hierarchy sanity (no skipped levels, at least one `h1`-equivalent per chapter)
- Image `<img>` tags all have non-empty `alt` and resolve to a manifest entry
- No empty content documents / no near-empty chapters (a common PDF-extraction failure signature)
- Rough text-density heuristics (character count vs. element count) to catch garbled/duplicated OCR or extraction artifacts
- Internal link (`href="#..."`) resolution against actual element IDs — directly pre-empting EPUBCheck's RSC-012 (§B6)

This tier should run on **every** conversion, be essentially free (milliseconds, no engine), and catch a meaningful fraction of real conversion defects before any rendering-based check is even attempted.

### D8. Visual regression comparison tooling

For **same-engine, same-OS, version-to-version** regression testing (not PDF-vs-EPUB comparison, see §D9):
- **pixelmatch** (`mapbox/pixelmatch`) — the standard, small, fast, dependency-light JS pixel-diff library; well-established baseline choice.
- **odiff** (`dmtrKovalenko/odiff`) — a faster, SIMD-first image comparison library with a Node.js API, positioned as a faster alternative to pixelmatch. [github.com/dmtrKovalenko/odiff](https://github.com/dmtrKovalenko/odiff)
- **dssim** (`kornelski/dssim`) — a **Rust** library/CLI implementing multiscale SSIM ("image similarity comparison simulating human perception"), which is more tolerant of minor sub-pixel/anti-aliasing differences than raw pixelmatch-style diffing — a better fit both technically (perceptual similarity, not exact-pixel) and architecturally (Rust, no JS runtime needed) for OpenConvert if any pixel-level comparison is done at all. [github.com/kornelski/dssim](https://github.com/kornelski/dssim)

Given §D2's finding that cross-webview-engine pixel comparison is fundamentally unreliable, the realistic use case for any of these tools in OpenConvert is narrow: **same-OS CI regression testing** ("did this release change this chapter's rendered appearance versus last release, on the same CI runner/OS") — for which `dssim` (Rust, perceptual, no Node dependency) is the best-fitting choice.

### D9. Comparing a PDF page render vs. an EPUB render — pixels are the wrong tool

This is the crux of R5's QA design problem, and the task brief's own framing is correct: pixel-level comparison between a PDF page (fixed geometry) and an EPUB chapter (reflowed, viewport-dependent) is close to meaningless — the same content can legitimately span 1 "page" in PDF and wrap across many different amounts of vertical space in EPUB depending on font size, viewport width, and reading-system CSS defaults. There is no stable "expected pixel layout" to diff against.

**The correct comparison is structural/content-based, not pixel-based:**
- **Text presence/density parity:** extract plain text from both the source PDF (via the existing PDF-parsing pipeline OpenConvert already needs) and the rendered/parsed EPUB chapter, and diff at the text-content level (e.g., normalized-whitespace string diff or token-set comparison) — catching dropped paragraphs, duplicated text, or garbled extraction, which is the dominant real-world PDF→EPUB failure mode, far more often than "the text is present but looks different."
- **Image-presence parity:** verify every image extracted from the PDF page has a corresponding `<img>` in the EPUB manifest/chapter (count and rough size-class matching), not pixel-matching the images.
- **Overflow/layout-sanity via DOM measurement, not pixels:** load the rendered chapter in the webview (§D2) and query `element.scrollWidth > element.clientWidth` (horizontal overflow — near-certain sign of a CSS mistake, e.g. a leftover fixed-width table or pre-formatted block from PDF extraction) and analogous vertical-overflow-within-a-fixed-container checks if any fixed-height elements exist. This is a well-established, standard DOM technique for overflow detection (`Element.scrollWidth` — MDN). [MDN: Element.scrollWidth](https://developer.mozilla.org/en-US/docs/Web/API/Element/scrollWidth) It requires the webview to actually render/layout the page (so it's a step up from §D7's pure-XML checks) but is still cheap, deterministic, and — importantly — **engine-agnostic in its pass/fail semantics** even though the exact overflow pixel amount will vary by engine, "any overflow at all" vs. "no overflow" is a stable, cross-platform-comparable signal.
- **Whitespace-ratio / text-density comparison** (proportion of the rendered area that is "empty" vs. text/image) as a coarse structural signal that both formats produced roughly proportionate content, without needing pixel alignment.

Screenshots (from §D2) remain valuable, but as a **human-in-the-loop spot-check artifact** (does chapter 3 look obviously broken to a person glancing at it), not as an automated pass/fail signal — that role should go to the structural/DOM checks above, which are deterministic and engine-independent in a way pixel diffing is not.

### D10. Concrete, low-cost recommendation

1. **Always-on (every conversion, milliseconds, no rendering engine):** Tier-1 structural validator (§B7/D7) — well-formedness, manifest/spine consistency, link resolution, heading sanity, image/manifest parity, PDF-vs-EPUB text-content diff.
2. **Always-on, cheap (uses the app's already-resident webview, no new dependency):** load each generated chapter in the app's WebView2/WKWebView/WebKitGTK offscreen, run DOM-based overflow detection (`scrollWidth>clientWidth`) and basic layout-sanity queries; optionally render through **foliate-js** (§D3) loaded into that same webview for a more realistic paginated-rendering check rather than raw unstyled XHTML.
3. **Manual/on-demand, cheap:** take a webview screenshot of a few representative chapters for human visual spot-checking in the app's own QA UI — explicitly not used for automated pass/fail.
4. **CI-only, maintainer-run, not shipped:** full EPUBCheck (§B1/B7) run against a regression corpus; Playwright/headless-Chromium or WeasyPrint (§D1/D5) only if a maintainer wants an additional independent-engine cross-check, never bundled; **dssim**-based same-OS pixel-regression diffing (§D8) across OpenConvert releases as an extra CI signal.
5. **Watch, don't adopt yet:** Blitz (§D6) and epubveri (§B3) are the two dependencies most worth re-evaluating in 6–12 months — both are Rust-native, philosophically aligned, and immature; either could become the "obvious default" once past beta/1.0.

---

## Source list

**W3C / official specs**
- [EPUB 3.3 (W3C Recommendation)](https://www.w3.org/TR/epub-33/)
- [EPUB 3.3 becomes a W3C Recommendation](https://www.w3.org/news/2023/epub-3-3-becomes-a-w3c-recommendation/)
- [W3C Invites Implementations of EPUB 3.3, EPUB Reading Systems 3.3, EPUB Accessibility 1.1](https://www.w3.org/blog/news/archives/9553)
- [EPUB Accessibility 1.1 (W3C Recommendation)](https://www.w3.org/TR/epub-a11y-11/)
- [EPUBCheck project page (DAISY)](https://daisy.org/activities/projects/epubcheck/)
- [EPUBCheck GitHub repository](https://github.com/w3c/epubcheck)
- [EPUBCheck v5.3.0 release](https://github.com/w3c/epubcheck/releases/tag/v5.3.0)
- [EPUBCheck releases index](https://w3c.github.io/epubcheck/releases/)
- [w3c/epubcheck issue #1189 — core media type changes in 3.3](https://github.com/w3c/epubcheck/issues/1189)
- [w3c/epubcheck issue #1291 — restrict obfuscation to fonts](https://github.com/w3c/epubcheck/issues/1291)
- [W3C mailing list: EPUBCheck v5.3.0 announcement](https://lists.w3.org/Archives/Public/public-publishingcg/2025Sep/0000.html)
- [W3C EPUBCheck: Apps and Tools](https://w3.org/publishing/epubcheck/docs/apps-and-tools)
- [W3C EPUBCheck reporting docs](https://www.w3.org/publishing/epubcheck/docs/report/)

**DAISY Knowledge Base**
- [DAISY KB: Core Media Types](https://kb.daisy.org/publishing/docs/epub/cmt.html)
- [DAISY KB: Reflow](https://kb.daisy.org/publishing/docs/css/reflow.html)
- [DAISY KB: The epub:type Attribute](https://kb.daisy.org/publishing/docs/html/epub-type.html)
- [DAISY KB: EPUBCheck](https://kb.daisy.org/publishing/docs/epub/validation/epubcheck.html)
- [DAISY: Ace by DAISY](https://daisy.org/activities/software/ace/)
- [daisy/ace GitHub](https://github.com/daisy/ace)
- [@daisy/ace on npm](https://www.npmjs.com/package/@daisy/ace)

**Reading systems / footnotes / MathML**
- [Apple Books Asset Guide: Pop-up Footnotes](https://help.apple.com/itc/booksassetguide/en.lproj/itccf8ecf5c8.html)
- [gist: popup footnotes degrading gracefully](https://gist.github.com/NickBarreto/9811005)
- [kobolabs/epub-spec issue #59 — footnote support](https://github.com/kobolabs/epub-spec/issues/59)
- [MathML Support in EPUB Reading Systems — EPUBSecrets](https://epubsecrets.com/mathml-support-in-epub-reading-systems.php)
- [KDP Community: MathML fraction rendering inconsistency](https://kdpcommunity.com/s/question/0D58V00008KT0I7SAL/simple-fraction-using-mathml-does-not-render-on-kindle-desktop-but-works-on-phone-and-kindle-previewer?language=en_US)
- [Wikipedia: Amazon Kindle (Send to Kindle EPUB support, MOBI deprecation)](https://en.wikipedia.org/wiki/Amazon_Kindle)
- [EDRLab: Thorium Reader](https://www.edrlab.org/software/thorium-reader/)
- [github.com/edrlab/thorium-reader](https://github.com/edrlab/thorium-reader)
- [koreader/crengine](https://github.com/koreader/crengine)
- [Google Play Books Partner Center: EPUB files](https://support.google.com/books/partner/answer/3316879?hl=en)

**CSS / fonts / fixed layout**
- [BlitzTricks — CSS tricks to improve your eBooks](https://friendsofepub.github.io/eBookTricks/)
- [Adobe Community: widow/orphan control InDesign→EPUB](https://community.adobe.com/t5/indesign-discussions/prevent-widow-and-orphan-from-indesign-to-epub/m-p/9590521)
- [EPUB Fonts — Embedding, Subsetting, and Licensing — toolkit.bot](https://toolkit.bot/blog/epub-fonts)
- [eBOUND Canada: Fixed Layout vs. Reflowable EPUBs](https://www.eboundcanada.org/resources/fixed-layout-versus-reflowable-epubs/)

**Calibre**
- [calibre ebook-convert manual](https://manual.calibre-ebook.com/generated/en/ebook-convert.html)
- [calibre e-book conversion overview](https://manual.calibre-ebook.com/conversion.html)

**Validation alternatives**
- [veripublica/epubveri (README)](https://github.com/veripublica/epubveri)
- [epubveri ARCHITECTURE.md](https://github.com/veripublica/epubveri/blob/main/docs/ARCHITECTURE.md)
- [epubveri CHANGELOG.md](https://github.com/veripublica/epubveri/blob/main/CHANGELOG.md)
- [paginagmbh/EPUB-Checker](https://github.com/paginagmbh/EPUB-Checker)
- [Docker Hub: gnue/epubcheck](https://hub.docker.com/r/gnue/epubcheck/tags)
- [epubcheck · PyPI](https://pypi.org/project/epubcheck/)
- [formatmyebook.com: Common EPUB & KDP Validation Errors Decoded](https://formatmyebook.com/blog/kdp-common-errors-guide/)
- [Readium architecture: streamer/parser/metadata](https://readium.org/architecture/streamer/parser/metadata.html)

**Generation libraries**
- [lib.rs: epub-builder](https://lib.rs/crates/epub-builder)
- [github.com/lise-henry/epub-builder](https://github.com/lise-henry/epub-builder)
- [PyPI: EbookLib](https://pypi.org/project/EbookLib/)
- [github.com/lesjoursfr/html-to-epub](https://github.com/lesjoursfr/html-to-epub)
- [NuGet: VersOne.Epub](https://www.nuget.org/packages/VersOne.Epub/)
- [github.com/asido/EpubSharp](https://github.com/asido/EpubSharp)

**Rendering / QA**
- [Playwright: Browsers (bundled sizes)](https://playwright.dev/docs/browsers)
- [tauri-apps/tao issue #289 — offscreen rendering](https://github.com/tauri-apps/tao/issues/289)
- [glama.ai: tauri webview screenshot tool](https://glama.ai/mcp/servers/hypothesi/mcp-server-tauri/tools/tauri_webview_screenshot)
- [Microsoft Learn: Distribute your app and the WebView2 Runtime](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution)
- [github.com/johnfactotum/foliate-js](https://github.com/johnfactotum/foliate-js)
- [Phoronix: Servo Browser Engine Starts 2026 With Many Notable Improvements](https://www.phoronix.com/news/Servo-January-2026)
- [github.com/DioxusLabs/blitz (README)](https://github.com/DioxusLabs/blitz/blob/main/README.md)
- [github.com/mapbox/pixelmatch](https://github.com/mapbox/pixelmatch)
- [github.com/dmtrKovalenko/odiff](https://github.com/dmtrKovalenko/odiff)
- [github.com/kornelski/dssim](https://github.com/kornelski/dssim)
- [MDN: Element.scrollWidth](https://developer.mozilla.org/en-US/docs/Web/API/Element/scrollWidth)

---

## Explicit [UNVERIFIED] items flagged for follow-up

1. Whether the PyPI `epubcheck` Python wrapper bundles its own JRE or requires a system Java install.
2. EPUBCheck 5.3.0's exact minimum Java version requirement (only an older 5.0.0-alpha "Java 8" data point was confirmed).
3. EPUBCheck's typical wall-clock runtime per book (no benchmark source found this session).
4. EPUBCheck JAR/dependency install-footprint size in MB.
5. Whether `epub-gen`'s own npm README currently carries an explicit deprecation/redirect notice to `@lesjoursfr/html-to-epub` (fetch attempts returned 403 this session).
6. Current (2026) Kobo and Google Play Books MathML support state, and Moon+/Lithium pop-up-footnote and MathML support — no fresh, citable 2025/2026 source found.
7. Ladybird's precise current (Sept 2026) embeddability/maturity status beyond secondary press coverage confirming active development.
8. Whether Blitz exposes a documented headless/offscreen screenshot API (README fetched didn't explicitly address this).

These should be re-verified with targeted follow-up research (or direct hands-on testing) before being treated as settled in an architecture decision document.
