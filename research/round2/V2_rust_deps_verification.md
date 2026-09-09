# OpenConvert — Round 2 Dependency Verification Report

Verification date: 2026-09-09. Method: WebFetch against exact URLs only (WebSearch quota exhausted). Each fetch is processed by a small summarization model, not read raw by me — for anything load-bearing (license SPDX ids, version numbers), I cross-checked docs.rs against the crates.io JSON API (`/api/v1/crates/<name>`) where possible, since that endpoint returned clean, mutually-consistent data. One mirror site (lib.rs) returned data that contradicted both docs.rs and the crates.io API and is flagged as unreliable below — treat any single-source, un-cross-checked finding with appropriate caution.

Status legend: **VERIFIED** (confirmed via at least one successful fetch of an authoritative URL) · **PARTIALLY VERIFIED** (fetch succeeded but content was incomplete/truncated, or only a low-confidence signal like a repo badge was available) · **COULD NOT VERIFY** (fetch failed, page didn't say, or I ran out of budget before checking) · **CONTRADICTED** (sources disagree).

---

## 1. `pdfium-render` (Rust)

**Version / License — VERIFIED.** Current version **0.9.4** (released 2026-09-06), license **`MIT OR Apache-2.0`**.
Source: https://crates.io/api/v1/crates/pdfium-render (JSON API) and https://docs.rs/pdfium-render/latest/pdfium_render/ (docs.rs "latest" page showed the same 0.9.4).
⚠️ Note: https://lib.rs/crates/pdfium-render returned **version 0.4.2 / "MIT/Apache" from Jan 2022** — this contradicts the two sources above and appears to be a stale/unreliable mirror snapshot. Disregard the lib.rs figure; 0.9.4 is the number I'd trust.

**`PdfPageTextChar` char/text API** — VERIFIED via https://docs.rs/pdfium-render/latest/pdfium_render/prelude/struct.PdfPageTextChar.html

| Method asked about | Result |
|---|---|
| `bounds()` | **Does not exist** |
| `tight_bounds()` | **Exists** — "Returns a precise bounding box for this character" |
| `loose_bounds()` | **Exists** — "Returns a loose bounding box for this character" |
| `origin()` | **Exists** |
| `font_size()` | **Does not exist** as such |
| `unscaled_font_size()` | **Exists** (a `scaled_font_size()` counterpart was also mentioned) |
| `font_name()` | **Exists** |
| `font_weight()` | **Exists** |
| `font_is_italic` | **Exists** as `font_is_italic()` |
| `text_render_mode()` | **Does not exist** — the method is named **`render_mode()`** |
| `fill_color()` | **Exists** |
| `is_generated()` | **Exists** |
| `is_hyphen()` | **Exists** |
| `has_unicode_map_error()` | **Not found** in the fetched content — COULD NOT VERIFY (may not exist, or may be on a different struct) |
| `matrix()` | **Exists** |
| rotation | No `rotation()` method surfaced; `angle_degrees()` and `angle_radians()` exist instead |

**Image object API (`PdfPageImageObject`)** — VERIFIED via https://docs.rs/pdfium-render/latest/pdfium_render/prelude/struct.PdfPageImageObject.html
- `get_raw_image()` — exists, returns `DynamicImage`, gated behind the crate's `image` feature; ignores filters/masks/transforms. `get_raw_bitmap()` and `get_raw_image_data()` are lower-level siblings.
- `get_processed_image()` — exists (also feature-gated), "accounts for filters, masks, and object transforms," plus scaled variants `..._with_width/height/size()`.
- **SMask/alpha handling**: no dedicated SMask/alpha-specific method name surfaced in the fetched excerpt. The "processed" methods claim to account for masks internally, which implies SMask compositing happens under the hood, but I could not find an explicit API surface for it — **COULD NOT VERIFY** the mechanism in detail.

**Path object API (`PdfPagePathObject`)** — VERIFIED via https://docs.rs/pdfium-render/latest/pdfium_render/prelude/struct.PdfPagePathObject.html. Construction: `move_to()`, `line_to()`, `bezier_to()`, `rect_to()`, `circle_to()`, `ellipse_to()`, `close_path()`. Properties: `fill_mode()`, `is_stroked()`, `segments()`, `set_fill_and_stroke_mode()`. Transforms: `transform()`, `apply_matrix()`, `matrix()`, plus `translate/scale/rotate_*/skew_*` shortcuts.

**Bookmarks API (`PdfBookmarks`)** — VERIFIED via https://docs.rs/pdfium-render/latest/pdfium_render/prelude/struct.PdfBookmarks.html. `root()` (root bookmark if any), `find_first_by_title(title)`, `find_all_by_title(title)` (breadth-first), `iter()` (depth-first prefix-order iterator over the whole tree). Individual `PdfBookmark` nodes navigate via `first_child()` / `next_sibling()`.

**Library binding** — VERIFIED via https://docs.rs/pdfium-render/latest/pdfium_render/prelude/struct.Pdfium.html
- `Pdfium::bind_to_library(path: impl AsRef<Path>) -> Result<Box<dyn PdfiumLibraryBindings>, PdfiumError>` — loads from a given path.
- `Pdfium::bind_to_system_library() -> Result<...>` — loads from system library search paths.
- `Pdfium::bind_to_statically_linked_library() -> Result<...>` — for a Pdfium statically linked into the executable; **requires the crate's `static` feature**; docs explicitly warn the app will crash immediately if Pdfium wasn't actually statically linked at compile time.
- Helpers `pdfium_platform_library_name()` / `pdfium_platform_library_name_at_path()` give the right filename per OS (`libpdfium.so`, `pdfium.dll`, etc).

---

## 2. `lopdf`

**Version / License — VERIFIED.** Version **0.45.0**, license **`MIT`**.
Source: https://crates.io/api/v1/crates/lopdf/0.45.0 (JSON API, license field = "MIT") — cross-checked against https://docs.rs/lopdf/latest/lopdf/ which independently reported the same 0.45.0. (Direct crates.io HTML page 404'd for me both times I tried it — used the JSON API instead, which worked.)
Optional Cargo features found: `async`, `chrono`, `chrono-clock` (default), `embed_image`, `font_embedding` (via `skrifa`), `jiff`, `serde`, `time`, `wasm_js`.

**`/StructTreeRoot` / `/Outlines` / `/Metadata` (XMP) access — PARTIALLY VERIFIED.** Per the docs.rs summary (single fetch, medium confidence — this is a large crate page and the summarizer may have missed items):
- **Outlines/bookmarks**: a `Bookmark` struct plus `add_bookmark()` / `build_outline()` methods exist — lopdf has purpose-built support here.
- **Metadata (XMP) and `/StructTreeRoot`**: **no dedicated typed struct or accessor was found**. lopdf appears to expose these only as generic `Dictionary`/`Object` values that the caller must locate and parse by hand (e.g., walking `Document.trailer` → `/Root` → `/StructTreeRoot`, or the document's `/Metadata` stream), not as a first-class tagged-PDF/metadata API. **Recommend a spot-check against the actual lopdf source/examples before relying on this**, since it came from a single incomplete page summary rather than an exhaustive symbol search.

---

## 3. Typst as a library

**Crate version / license — VERIFIED.** `typst` crate: version **0.15.1**, license **Apache-2.0** (per docs.rs page + GitHub repo license badge, https://docs.rs/typst/latest/typst/ and https://github.com/typst/typst). `typst-pdf`: version **0.15.1**, license **Apache-2.0** (https://docs.rs/typst-pdf/latest/typst_pdf/).

**In-process compilation — VERIFIED.** The `typst` crate exposes a top-level `compile()` function ("Compiles sources into an output") plus a `trace()` function and a `World` trait ("The environment in which typesetting occurs") that the host implements to provide files/fonts/config for compilation — confirming a document can be compiled in-process without shelling out. Source: https://docs.rs/typst/latest/typst/

**`typst-pdf` exists — VERIFIED** (https://docs.rs/typst-pdf/latest/typst_pdf/), with `PdfOptions` for export settings and a `PdfStandard` enum for conformance targets.

**Tagged PDF / PDF/UA-1 — VERIFIED, with one dating discrepancy to flag.**
- `PdfStandard` enum (https://docs.rs/typst-pdf/latest/typst_pdf/enum.PdfStandard.html) includes variants for PDF 1.4–2.0, PDF/A-1/2/3/4 (various conformance levels), **and `Ua_1` — "PDF/UA-1"**. This is how PDF/UA-1 conformance is selected: pass the desired `PdfStandard` value(s) into `PdfStandards`/`PdfOptions` at export time. I could not find the exact `PdfOptions` field name/signature in the fetched excerpt — **COULD NOT VERIFY** that precise wiring, only that the enum variant exists.
- GitHub releases (https://github.com/typst/typst/releases) confirm: **Typst 0.14.0 introduced tagged PDF as the default** ("Typst now produces *accessible* PDFs out of the box... Typst PDFs are now *tagged* by default") **and added PDF/UA-1 conformance support** ("Typst can now emit documents conforming to the PDF/UA-1 standard. PDF/UA-2 is not yet supported, but planned."). Some advanced accessibility helpers for complex tables (`pdf.header-cell`, `pdf.data-cell`, `pdf.table-summary`) are gated behind an `a11y-extras` feature.
- ⚠️ **Discrepancy**: the releases-page fetch reported the newest release visible as **0.14.2 (2025-12-12)**, while docs.rs independently reports the current published crate as **0.15.1**. Both facts came from real (non-erroring) fetches, so either the releases-page summarizer missed a newer entry higher up the page, or there's a real gap between "latest GitHub release tag" and "latest crates.io publish." **Recommend the architect re-check https://github.com/typst/typst/releases directly** before pinning a version; treat "0.15.1 is current, tagged-PDF/PDF-UA-1 landed in 0.14.0" as the reliable takeaway.

---

## 4. Dictionaries / word lists / language tooling

**German (igerman98 / de_DE) — COULD NOT VERIFY license precisely.** Fetching https://www.j3e.de/ispell/igerman98/ did not surface any license text in the summarized content. Two attempts to pull the LibreOffice dictionaries repo's German README directly failed: `README_de.txt` → 404, `README_de_DE_frami.txt` → fetch returned only "[binary data]" (likely an encoding/redirect issue, not a real absence). GitHub's `/tree/...` directory-listing pages are blocked for this tool by `robots.txt` (confirmed 403/ROBOTS_DISALLOWED on `github.com/LibreOffice/dictionaries/tree/master/de`, `/en`, `/tr_TR`, and `api.github.com/repos/.../contents/de` also 403'd, likely rate-limited). **I could not confirm the SPDX id for the German hunspell dictionary this session.** (It is widely reported elsewhere as GPL-2.0-or-later / GPL-3.0-or-later / LGPL-3.0 depending on the specific igerman98 sub-package, but I have no URL I successfully fetched this session that states this, so I am not marking it verified.) **Action item: re-fetch, or open the repo in a browser, before treating German dict licensing as settled — GPL is a materially different obligation than the permissive en_US license found below.**

**English (en_US) — VERIFIED, and notably NOT GPL.** https://raw.githubusercontent.com/LibreOffice/dictionaries/master/en/README_en_US.txt fetched successfully. It is a SCOWL-derived permissive license: *"Permission to use, copy, modify, distribute and sell these word lists, the associated scripts, the output created from the scripts, and its documentation for any purpose is hereby granted without fee, provided that the above copyright notice appears in all copies..."* — a BSD/Ispell-style permissive license, copyright Kevin Atkinson (2000–2018) with 2016 contributions from Benjamin Titze under similar terms. Also incorporates public-domain sources (Moby, UK English Wordlist, 12Dicts, WordNet, ENABLE, UKACD). **This is Apache-2.0-compatible**, unlike a copyleft license.

**Turkish (tr_TR) — COULD NOT VERIFY / PARTIALLY VERIFIED.** `LibreOffice/dictionaries/tr_TR/README_tr_TR.txt` → 404 (this path/filename may simply not exist in that repo — I did not confirm the correct filename before running out of budget). As a fallback I checked https://github.com/hrzafer/hunspell-tr, whose page metadata showed an **"MIT license"** badge — but I could not pull the actual `LICENSE` file text to confirm the SPDX id verbatim, so this is **PARTIALLY VERIFIED** only (repo-badge confidence, not full-text confidence).

**`zspell`** — VERIFIED version, **license unresolved**. Version **0.5.5** (https://crates.io/api/v1/crates/zspell). crates.io's own license field reads **`"Non-standard"`** — i.e., crates.io itself couldn't map the crate's declared license to a recognized SPDX string; docs.rs also did not surface a license in the fetched excerpt. Description confirms it's a "Native Rust library for spellchecking" aiming for **Hunspell `.aff`/`.dic` compatibility**, with `check()`/`check_word()`/`check_indices()`, stemming, and (unstable, feature-gated) suggestion generation. **Recommend manually checking the crate's `Cargo.toml`/repo `LICENSE` file** before depending on it in an Apache-2.0 product.

**`hyphenation`** — VERIFIED core facts, language/pattern-license details **COULD NOT VERIFY**. Version **0.8.4**, license **`Apache-2.0/MIT`** (https://crates.io/api/v1/crates/hyphenation, cross-checked docs.rs). Description: "Knuth-Liang hyphenation for a variety of languages." Docs.rs excerpt confirmed patterns "come bundled with the crate" (via `.bincode` files) and referenced a `Language` enum, but the fetched content did not enumerate which languages (I could not confirm `de`/`tr`/`en` specifically), nor the size of the embedded data, nor the license of the underlying TeX/hyph-utf8 hyphenation patterns themselves. **Needs a follow-up fetch of the `Language` enum page and the crate's repo README.**

**`lingua` (lingua-rs)** — VERIFIED. License **Apache-2.0** (https://github.com/pemistahl/lingua-rs). Confirmed binary-bloat concern is real: *"this will download the language model dependencies for all 75 supported languages, a total of approximately 300 MB"* by default. Per-language Cargo features exist to cut this down, e.g. `features = ["french", "italian", "spanish"]` with `default-features = false`; a WASM build note mentions a 288 MB default wasm artifact that can similarly be trimmed. **For OpenConvert, enabling only `german`/`english`/`turkish` (whatever the actual feature names are) is clearly advisable rather than the default all-languages build.**

**`whatlang`** — VERIFIED version/license, language coverage **COULD NOT VERIFY**. Version **0.18.0**, license **MIT** (https://crates.io/api/v1/crates/whatlang). Described as a "fast and lightweight language identification library." I did not confirm Turkish/German inclusion specifically this session — budget ran out before I could fetch the crate's supported-languages list.

**German compound splitter for Rust — COULD NOT VERIFY / likely does not exist.** I did not find (and did not have budget to search for) a Rust-native German compound-word splitter equivalent to CharSplit (which, as the prompt notes, is Python-only). No contradicting evidence either — this is a negative result from absence rather than a confirmed non-existence. **The prompt's own suggested fallback is reasonable and I'd endorse it**: a deterministic split — try splitting the German token at each internal position, accept a split where both halves (or the joined form, accounting for German's `-s-`/`-n-`/`-es-` Fugenlaute) are found in the hunspell dictionary/wordlist — is a sufficient, dependency-free approach for a spellcheck/hyphenation use case and avoids pulling in an ML compound splitter.

---

## 5. Docling layout models (Hugging Face)

**`docling-project/docling-layout-egret-medium`** — VERIFIED (https://huggingface.co/docling-project/docling-layout-egret-medium and its `/tree/main`):
- License: **`apache-2.0`** (YAML frontmatter).
- Params: **~19.5M**.
- Architecture: RT-DETR-based object detector.
- Input resolution: **not stated** in the README content I could fetch — COULD NOT VERIFY.
- ONNX availability: **No `.onnx` file present.** Repo contents: `.gitattributes` (1.52 kB), `README.md` (3.13 kB), `config.json` (4.21 kB), `model.safetensors` (**78.3 MB**), `preprocessor_config.json` (444 B). Weights are Safetensors-only.
- Classes (17): Caption, Footnote, Formula, List-item, Page-footer, Page-header, Picture, Section-header, Table, Text, Title, Document Index, Code, Checkbox-Selected, Checkbox-Unselected, Form, Key-Value Region.

**`docling-project/docling-layout-heron`** — VERIFIED (https://huggingface.co/docling-project/docling-layout-heron and its `/tree/main`):
- License: **`apache-2.0`**.
- Params: **~42.9M**.
- Architecture: RT-DETR v2; described as "the default layout analysis model of the Docling project."
- Input resolution: **not stated** in the fetched README — COULD NOT VERIFY.
- ONNX availability: **No `.onnx` file present.** Repo contents: `.gitattributes` (1.52 kB), `README.md` (3.22 kB), `config.json` (3.27 kB), `docling_heron_400.png` (96.9 kB), `model.safetensors` (**172 MB**), `preprocessor_config.json` (444 B). Weights are Safetensors-only, same 17-class taxonomy as egret-medium.

Net: both models are Apache-2.0 and safetensors-only today — **if OpenConvert wants ONNX Runtime for the layout stage, plan on converting these checkpoints yourself (e.g. via `optimum` / a safetensors→ONNX export step) rather than expecting upstream ONNX files.**

---

## 6. Tauri 2

**(a) Hidden windows — VERIFIED (Rust side), nuanced on JS side.**
- Rust `WebviewWindowBuilder` exposes a **`visible`** option to control initial visibility at construction (https://docs.rs/tauri/latest/tauri/webview/struct.WebviewWindow.html).
- Rust `WebviewWindow::eval()` exists — **"Evaluates JavaScript on this window."** — exactly the mechanism needed to drive a hidden window's page and pull results back (e.g. via `invoke` from the injected JS, or by polling state). `hide()`, `show()`, `is_visible()` (Rust) also exist and return `Result<...>`.
- JS-side `WebviewWindow` (https://v2.tauri.app/reference/javascript/api/namespacewebviewwindow/) has `hide()`, `show()`, `isVisible()`, but **no `eval()`** was found in the fetched excerpt — this is expected/consistent, not a contradiction: JS code already executes *inside* the webview, so an "eval from JS into itself" method wouldn't make sense; `eval()` is specifically a **Rust-host-side** capability for injecting JS into a window (hidden or not) from the Tauri backend.
- **Documented caveat about hidden windows not performing layout: none found.** I checked both the JS API reference and the Rust docs.rs page and found no explicit warning that a `visible:false` window skips layout/rendering. This is consistent with the task's expectation that this is **likely undocumented** — treat it as an open risk to validate empirically (e.g. test that `getBoundingClientRect()` / actual rendered layout metrics come back correct from a hidden `WebviewWindow` on each target OS) rather than something Tauri's docs promise either way.

**(b) Sidecar — VERIFIED** (https://v2.tauri.app/develop/sidecar/):
- `externalBin` binaries must be suffixed with the target triple: *"a binary with the same name and a `-$TARGET_TRIPLE` suffix must exist on the specified path."*
- Shell plugin sidecar API confirmed on both sides: Rust — `tauri_plugin_shell::ShellExt` trait, `shell().sidecar()` on `AppHandle`; JS — `Command.sidecar()` static method from `@tauri-apps/plugin-shell`.
- Capability scoping is required: *"Tauri requires you to give the sidecar permission to run the `execute` or `spawn` method"* via the capabilities file (e.g. `shell:allow-execute` plus a sidecar allow-list entry naming the specific binary).

**(c) Latest Tauri release — VERIFIED (with a caveat).** https://github.com/tauri-apps/tauri/releases shows the top release as **`tauri v2.11.5`**, dated "01 Jul" (year not explicitly restated in the summarized excerpt, but contextually 2026 given today's date and the changelog mentioning a `time` crate fix). **Recommend confirming the year directly** if it matters for your dependency-freshness audit.

---

## 7. Tesseract packaging

**Official tesseract-ocr/tesseract GitHub releases — PARTIALLY VERIFIED.** https://github.com/tesseract-ocr/tesseract/releases shows latest release **5.5.3** (2026-07-24) with "Assets 3" attached, but I could not get the fetch to enumerate the actual asset filenames (page showed a loading placeholder in the summarized content). **Historically, and consistent with tessdoc guidance below, the official repo release assets are source tarballs, not prebuilt cross-platform binaries** — I could not fully confirm this for 5.5.3's specific 3 assets, so treat "no official prebuilt cross-OS binaries" as PARTIALLY VERIFIED, not certain.

**UB-Mannheim Windows installer — VERIFIED.** https://github.com/UB-Mannheim/tesseract/wiki: *"The latest installers can be downloaded here: tesseract-ocr-w64-setup-5.5.3.20260724.exe (64 bit)"*, bundling Tesseract **5.5.3**. This remains the de facto standard prebuilt Windows installer.

**tessdoc install matrix — VERIFIED** (https://tesseract-ocr.github.io/tessdoc/Installation.html):
- Linux: distro packages (`apt install tesseract-ocr` on Ubuntu/Debian), RPM-based packages, PPA, AppImage, snap.
- macOS: `brew install tesseract` (Homebrew) or MacPorts.
- Windows: UB-Mannheim installers (32/64-bit, include training tools) — same installer line as above — or compile from source / CI-produced Visual Studio binaries.
- **Conclusion: there is no single official multi-OS binary release; the practical path per OS is (Linux) distro package, (macOS) Homebrew, (Windows) UB-Mannheim installer** — none of which are directly "sidecar-shippable" as-is without repackaging/vendoring, since they're installers/package-manager artifacts, not standalone relocatable binaries with bundled dependencies.

**`tesseract-rs` crate — VERIFIED core facts, TSV/confidence support unresolved.**
- Version **0.4.0**, license **MIT** (https://crates.io/api/v1/crates/tesseract-rs, cross-checked https://docs.rs/tesseract-rs/latest/tesseract_rs/).
- **Builds Tesseract + Leptonica from source** by default ("Built-in compilation of Tesseract and Leptonica"), with an option to link against a system-installed Tesseract instead — VERIFIED, meaning it can plausibly build with "no system deps" for the default path (modulo needing a C/C++ toolchain, which is a build-time not runtime dependency).
- **hOCR output confirmed** via a CLI example: `tesseract-rs -l eng+tur --psm 6 -o hocr scanned.png`.
- **TSV output and per-word confidence scores: COULD NOT VERIFY** — not present in the fetched doc excerpt; would need a deeper read of the crate's API (likely it exposes whatever the underlying Tesseract C API exposes, which does include confidence data, but I did not confirm this crate surfaces it in its Rust API).

**Pragmatic recommendation (my synthesis, not a verified fact):** Given (1) no clean official cross-platform prebuilt binaries exist, (2) Tauri's own sidecar model is designed exactly around "one externalBin per target triple," and (3) OCR is a heavyweight, rarely-changing dependency — **building the Tesseract CLI once per target OS in CI (e.g., via vcpkg's static triplets on Windows/macOS/Linux, or the UB-Mannheim build recipe on Windows) and shipping the resulting binary as a Tauri sidecar** is the more predictable path for a desktop app: it decouples the OCR toolchain from every contributor's Rust build, produces a binary whose CLI surface (TSV/hOCR/confidence via `--tsv`/`-c` flags) is exactly Tesseract's well-documented one, and matches the sidecar packaging Tauri already expects. `tesseract-rs` is a legitimate alternative if the team wants a single `cargo build` to produce OCR support without maintaining per-OS CI recipes — the tradeoff is slower/heavier builds (compiling Leptonica+Tesseract from source on every build machine, or building once in CI and still shipping the artifact as a sidecar anyway, in which case `tesseract-rs`'s main advantage—avoiding a vcpkg manifest—still applies but its FFI surface for TSV/confidence needs to be verified before committing to it).

---

## 8. EPUBCheck 5.3.0 — Java requirement: **COULD NOT VERIFY precisely**

- https://github.com/w3c/epubcheck/releases/tag/v5.3.0 — the release notes content I could fetch **did not mention a Java version requirement at all** (only feature/bugfix/dependency-update entries).
- https://github.com/w3c/epubcheck/blob/main/README.md and https://github.com/w3c/epubcheck (main repo page) both surfaced only: *"To build epubcheck from the sources you need Java Development Kit (JDK) 1.7 or above and Apache Maven 3.0 or above installed."* This is explicitly a **build**-time requirement in a "Build from sources" section, not a stated **runtime** minimum, and "JDK 1.7" reads like legacy boilerplate that likely hasn't been updated to reflect 5.3.0's actual minimum.
- **I could not find, via the URLs fetched, an explicit statement of the minimum JRE needed to *run* EPUBCheck 5.3.0.** Do not take "JDK 1.7" as the runtime answer — it's unverified for that purpose. Recommend checking the release's `pom.xml` (`maven.compiler.target`/`release`) or the EPUBCheck wiki directly, which I did not have budget to fetch this session.

---

## 9. Misc crates (version / license via crates.io JSON API)

All VERIFIED via `https://crates.io/api/v1/crates/<name>` unless noted; each independently returned clean version+license JSON.

| Crate | Version | License (SPDX) | Source |
|---|---|---|---|
| `quick-xml` | 0.42.0 | MIT | crates.io API |
| `zip` | 8.6.0 (pre-release 9.0.0-pre3 also listed) | MIT | crates.io API |
| `insta` | 1.48.0 | Apache-2.0 | crates.io API |
| `proptest` | 1.11.0 | MIT OR Apache-2.0 | crates.io API |
| `cargo-nextest` | 0.9.143 | Apache-2.0 OR MIT | crates.io API |
| `cargo-deny` | 0.20.2 | MIT OR Apache-2.0 | crates.io API |
| `criterion` | 0.8.2 | Apache-2.0 OR MIT | crates.io API |

⚠️ Flag for the architect: the prompt's framing expected `zip` to be "2.x/4.x" — the API instead reports **8.6.0**. Given the `zip` crate is known for frequent major-version churn and the crates.io API matched docs.rs exactly for both `pdfium-render` and `lopdf` earlier in this report (i.e., it's proven reliable this session), I'd lean toward trusting 8.6.0, but **this is exactly the kind of single-source number worth a 10-second manual spot-check on crates.io/crates/zip** before pinning it in `Cargo.toml`, since it's a bigger jump than expected.

---

## Fetch budget note

This report used roughly 60 WebFetch calls (over the ~30–45 suggested budget) because of a high rate of dead ends: crates.io's HTML pages 404'd for me consistently (worked around via the `/api/v1/crates/<name>` JSON endpoint, which was reliable), GitHub's `/tree/...` and `api.github.com/repos/.../contents/...` paths were blocked by `robots.txt` / rate-limited (worked around via `raw.githubusercontent.com` where I could, with mixed success), and one mirror (lib.rs) returned stale/wrong data requiring cross-checking. Items flagged **COULD NOT VERIFY** above are the ones where I stopped rather than continuing to burn additional fetches on low-probability retries; a follow-up pass with a fresh budget could close most of them (German dictionary license, Turkish dictionary license/path, EPUBCheck runtime Java minimum, hyphenation language list, and ONNX/input-resolution details for the Docling models are the highest-value next fetches).
