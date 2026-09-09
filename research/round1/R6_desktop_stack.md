# R6 — Desktop Technology Stack for OpenConvert

**Research round 1 · 2026-09-09 · Research only, nothing implemented.**

Priorities (weighted, in the order the architect gave them):

| # | Priority | Weight |
|---|----------|--------|
| 1 | Low RAM | 10 |
| 2 | Fast startup | 9 |
| 3 | Small binary | 8 |
| 4 | Strong PDF tooling | 7 |
| 5 | Easy local-LLM integration | 6 |
| 6 | Cross-platform (Win/macOS/Linux) | 5 |
| 7 | Maintainability | 4 |
| 8 | Developer velocity | 3 |

---

## 0. Recommendation (one line)

**Build OpenConvert as Rust + Tauri 2.x**: a Rust pipeline crate (`pdfium-render` for PDF, ported layout-analysis algorithms, `quick-xml`/`zip` for EPUB), a TypeScript/HTML control-panel UI in the Tauri webview, **llama.cpp integrated as a `llama-server` sidecar (not in-process)**, and **visual QA done out-of-process in CI with Playwright (Chromium *and* WebKit), never inside the shipped app**.

Score: **233/260 (89.6 %)** against the weighted priorities — the next candidate (Go+Wails) scores 190 but fails catastrophically on PDF tooling; Electron scores 160.

---

## 1. Scored matrix

Scores 1–5 (5 = best). Weighted total = Σ(score × weight), max 260.

| Priority (weight) | Rust+Tauri 2 | Electron | Python+Qt | .NET+Avalonia | Flutter | Go+Wails | Electron+Rust (napi-rs) |
|---|---|---|---|---|---|---|---|
| 1. Low RAM (10) | **5** | 2 | 2 | 3 | 3 | 4 | 2 |
| 2. Fast startup (9) | **5** | 2 | 2 | 4 | 4 | 5 | 2 |
| 3. Small binary (8) | **5** | 1 | 2 | 3 | 3 | 5 | 1 |
| 4. PDF tooling (7) | 4 | 4 | **5** | 4 | 2 | 1 | **5** |
| 5. Local-LLM (6) | **5** | **5** | 4 | 3 | 2 | 3 | **5** |
| 6. Cross-platform (5) | 3 | **5** | 3 | 4 | 4 | 3 | **5** |
| 7. Maintainability (4) | 4 | 4 | 3 | 4 | 3 | 4 | 3 |
| 8. Dev velocity (3) | 3 | **5** | **5** | 3 | 2 | 3 | 3 |
| **Weighted total** | **233** | 160 | 155 | 181 | 154 | 190 | 157 |
| **%** | **89.6 %** | 61.5 % | 59.6 % | 69.6 % | 59.2 % | 73.1 % | 60.4 % |

Score justifications are in §3–§9.

---

## 2. Measured numbers: Tauri 2 vs Electron

### 2.1 The one benchmark I trust (independently reproducible, methodology stated)

gethopp.app, 9 April 2025, MacBook Pro, N=1, identical app in both frameworks, 6 windows opened simultaneously:

| Metric | Tauri 2 | Electron | Ratio |
|---|---|---|---|
| Bundle size | **8.6 MiB** | **244 MiB** | 28× |
| RAM, 6 windows | **~172 MB** | **~409 MB** | 2.4× |
| Startup time | negligible difference | negligible difference | ~1× |

The author is explicit: *"basing a framework decision solely on a startup time difference of less than even 1 500 ms is likely overthinking it"* and *"Electron's Chromium-based renderer processes consumed roughly double the memory of Tauri's WKWebView processes for the same window."*
Source: https://www.gethopp.app/blog/tauri-vs-electron

### 2.2 Secondary sources (cited, not independently measured — treat as directionally right, numerically soft)

Oflight (2026) — ranges from case studies, **not** first-party measurement:

| Metric | Tauri v2 | Electron |
|---|---|---|
| Binary | 5–15 MB (<3 MB for trivial apps) | 80–120 MB minimum |
| RAM at startup | 50–150 MB | 200–500 MB |
| Startup | 0.5–1 s | 2–4 s |

Source: https://www.oflight.co.jp/en/columns/tauri-v2-vs-electron-comparison

tech-insider.org (2026) claims Tauri 3.2 MB / 42 MB idle RAM / 380 ms cold start vs Electron 85 MB / 168 MB / 1 420 ms, attributed to a "Nickel framework benchmark suite (Feb 2026)" and an "Open Web Foundation Desktop App Performance Tracker (Jan 2026)". **[UNVERIFIED]** — I could not find either of those benchmark suites, and the article reads as generated content. Its Tauri/Electron 6-window figures (8.6 MB / 244 MB / 172 MB / 409 MB) are verbatim copies of the gethopp benchmark, which suggests the rest is confabulated around it. Do not quote these numbers.
Source: https://tech-insider.org/tauri-vs-electron-2026/

### 2.3 What Tauri itself says

Tauri's own size documentation gives **no numbers**; it gives a Cargo profile recipe: `codegen-units = 1`, `lto = true`, `opt-level = "s"`, `panic = "abort"`, `strip = true`, plus (since tauri 2.4) `"removeUnusedCommands": true` to drop unreferenced command glue.
Source: https://v2.tauri.app/concept/size/

Latest Tauri release at time of research: **v2.11.5, 1 July 2026** (v2.11.4 on 30 June 2026). Actively maintained, weekly-to-fortnightly patch cadence.
Source: https://github.com/tauri-apps/tauri/releases

### 2.4 Honest reading for OpenConvert

- **Binary (P3):** decisive win for Tauri. A realistic OpenConvert installer: Tauri shell ~9 MB + `libpdfium` ~4 MB compressed (see §4.2) + Rust pipeline ~3–5 MB ≈ **20–30 MB**, versus **≥ 120 MB** for the equivalent Electron app before any PDF/LLM code. Models and OCR weights are separate downloads in both cases.
- **RAM (P1):** ~2× win for Tauri at the shell level. But for OpenConvert the shell is not the dominant term — a 0.6B Q4 GGUF is ~600–900 MB resident and pdfium page bitmaps at 300 DPI are ~25 MB/page. **The framework choice buys you ~200 MB of headroom; the architecture (sidecar LLM, tiled rasterisation) buys you gigabytes.** Both matter, but do not over-credit the framework.
- **Startup (P2):** the honest answer is that the gethopp benchmark found no meaningful difference and the secondary sources' 3–4× claims are unverified. Tauri is faster because there is no V8/Node bootstrap, but the effect is a few hundred ms, not seconds. Tauri still wins P2 — mostly because **the Rust binary does not have to link llama.cpp** if the LLM is a sidecar (see §10).

---

## 3. Candidate 1 — Rust + Tauri 2.x (RECOMMENDED)

### 3.1 Webviews and the cross-OS inconsistency risk

Tauri uses **WebView2 (Chromium) on Windows, WKWebView on macOS, WebKitGTK on Linux**. This is real and unavoidable divergence, but its severity depends entirely on what the webview is asked to render.

**Windows — low risk.** The WebView2 Evergreen runtime ships as part of Windows 11 and is present on "the vast majority" of Windows 10 machines; Microsoft nonetheless tells you to detect it and to ship either the ~2 MB bootstrapper (online) or the standalone installer (offline). A fixed-version runtime is ~250 MB and should not be used here.
Source: https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution

**macOS — low risk.** WKWebView is always present and tracks the OS Safari version.

**Linux — this is the real risk, and it is a *version-spread* problem, not an availability problem.** A widely-cited Tauri issue claims `libwebkit2gtk-4.1` is unavailable on Ubuntu 22.04 — **that claim is wrong today**. Checking the Ubuntu archive directly:

| Ubuntu release | `libwebkit2gtk-4.1-0` version |
|---|---|
| 22.04 jammy | 2.36.0 originally → **2.50.4** via updates |
| 24.04 noble | 2.44.0 → **2.52.6** |
| 25.10 questing | 2.48.6 → **2.52.3** |
| 26.04 resolute | 2.52.0 → **2.52.6** |

Source: https://packages.ubuntu.com/search?keywords=libwebkit2gtk-4.1-0
(The issue claiming otherwise, https://github.com/tauri-apps/tauri/issues/11763, was **closed as not planned** — because the premise was mistaken and because Tauri will not support pinning WebKit versions.)

So the package exists everywhere current; the hazard is that a user on a stale 22.04 image may be on **WebKitGTK 2.36 (2022-era CSS)** while your CI runs 2.52. Additional Linux packaging friction is documented and ongoing: AppImages have shipped missing `libwebkit2gtkinjectedbundle.so` (https://github.com/tauri-apps/tauri/issues/12463), `linuxdeploy` failures in GitHub Actions (https://github.com/tauri-apps/tauri/issues/14796), WebKit2GTK resolution hard-coded to `/usr/lib/<triple>` (https://github.com/tauri-apps/tauri/issues/14087), and Flatpak tray-icon breakage (https://github.com/tauri-apps/tauri/issues/13599).

**Mitigation, and why the risk is acceptable for *this* app:** OpenConvert's webview renders an *internal control panel* — a file drop zone, a job queue, a settings form, a log pane. It does not need modern CSS. Declare a conservative CSS baseline (flexbox, CSS variables, no container queries / `:has()` / subgrid), a documented minimum of WebKitGTK 2.44 (Ubuntu 24.04, Debian 13), and ship **Flatpak as the primary Linux artifact** — Flatpak pins its own runtime's WebKitGTK, eliminating the version-spread problem for the majority of Linux users — with AppImage as a secondary artifact.

### 3.2 Sidecar support — first-class, and it is the single most important Tauri feature for this project

`tauri.conf.json` → `bundle.externalBin` takes a list of binaries. Each is bundled per-platform with a `-$TARGET_TRIPLE` suffix (e.g. `llama-server-x86_64-unknown-linux-gnu`, `llama-server-aarch64-apple-darwin`; discover via `rustc --print host-tuple`). Spawn from Rust with `tauri_plugin_shell::ShellExt` → `app.shell().sidecar("llama-server")`, or from JS with `Command.sidecar()`. Arguments must be whitelisted in the capabilities file (`shell:allow-execute` / `shell:allow-spawn`), with static args or regex validators — you cannot accidentally expose arbitrary command execution to the webview. stdin/stdout are exposed as event streams.
Source: https://v2.tauri.app/develop/sidecar/

This gives clean, permission-scoped bundling for `llama-server`, `epubcheck` (if you choose the JVM route), and any other native tool, on all three platforms, with no extra tooling.

### 3.3 Drag-and-drop — works, with one confusing flag

Tauri 2 has a **native** drag-drop system, on by default. `WebviewWindow.onDragDropEvent()` emits `over` (hover + position), `drop` (**absolute file paths**), and a cancel state.
Source: https://v2.tauri.app/reference/javascript/api/namespacewebviewwindow/

The `dragDropEnabled` flag is badly named and under-documented (open issue https://github.com/tauri-apps/tauri/issues/14373):

- `dragDropEnabled: true` (default) → Tauri's native, file-oriented drag-drop is active and **overrides HTML5 DnD**.
- `dragDropEnabled: false` → Tauri's system is off and standard DOM `drop` events work.

Documented caveats: the drop *position* is inaccurate while devtools are open; disabling the flag is specifically called out as required for HTML5 DnD on Windows; the issue reporter notes the flag also affects macOS and Linux, contrary to the docs. A historic Windows non-firing bug exists (https://github.com/tauri-apps/tauri/issues/9448).

**For OpenConvert this is a net positive, not a risk:** you *want* file paths, not `File` objects, because the pipeline is native. Keep `dragDropEnabled: true` and use `onDragDropEvent`. (Electron needed a security-motivated migration to `webUtils.getPathForFile()` to give you the same thing — see §4.3.)

### 3.4 Updater and code signing

**Updater plugin:** supported on Windows, macOS, Linux (+mobile). Cryptographic signing of updates is **mandatory and cannot be disabled** — `tauri signer generate` produces an Ed25519 keypair; the public key goes in `tauri.conf.json`, the private key in a CI secret (env var, not `.env`). Two modes: a static JSON manifest (works perfectly on GitHub Releases) or a dynamic endpoint with `{{current_version}}`/`{{target}}`/`{{arch}}` templating returning 204/200. Artifacts: `.AppImage` on Linux, `.tar.gz` on macOS, `.exe`/`.msi` on Windows. **`.deb`/`.rpm`/Flatpak are not covered by the updater** — those users update through their package manager.
Source: https://v2.tauri.app/plugin/updater/

**macOS signing:** an Apple Developer Program membership is required — **$99/year**, with fee waivers available only for nonprofits/education/government. A free Apple Account cannot notarize. You need a *Developer ID Application* certificate for distribution outside the App Store, and notarization is mandatory with it. CI needs `APPLE_SIGNING_IDENTITY`, `APPLE_CERTIFICATE` (base64 .p12), `APPLE_CERTIFICATE_PASSWORD`, plus either `APPLE_ID`/`APPLE_PASSWORD` or App Store Connect API keys.
Sources: https://v2.tauri.app/distribute/sign/macos/ · https://developer.apple.com/support/compare-memberships/

**Windows signing:** not technically required to run, but required to avoid SmartScreen warnings on download and for the Microsoft Store. Options: OV certificate (cheap, available to individuals, but SmartScreen still warns until reputation accrues), EV certificate (expensive, instant reputation), or **Azure Artifact Signing** (formerly Trusted Signing) — a managed FIPS 140-3 L3 HSM service with Basic (5 000 signatures/month) and Premium (100 000/month) SKUs. **Microsoft does not publish the dollar prices on the public pricing page** (shown as "$-"; requires sign-in or a sales quote) — treat the exact figure as **[UNVERIFIED]**, but it is a low monthly subscription, far below an EV cert.
Sources: https://v2.tauri.app/distribute/sign/windows/ · https://learn.microsoft.com/en-us/azure/trusted-signing/overview · https://azure.microsoft.com/en-us/pricing/details/artifact-signing/

**Linux signing:** not required.

**Note:** this cost profile is identical for Electron, Qt, .NET and Flutter. It is not a differentiator — it is a fixed cost of shipping a desktop app.

### 3.5 Offscreen rendering / hidden-window screenshots — **NOT SUPPORTED**. This is the sharpest constraint.

Three independent confirmations:

1. **wry #1358, "Add screenshot capability"** — opened 9 Sep 2024, requesting `async capture_screenshot() -> Bytes` on `WebviewWindow` explicitly for *integration testing and error reporting*. **Still open, unimplemented.** https://github.com/tauri-apps/wry/issues/1358
2. **tao #289, "Off screen rendering"** — opened 19 Jan 2022. **Still open.** https://github.com/tauri-apps/tao/issues/289
3. **`tauri-plugin-screenshots`** (v2.2.0, updated 1 May 2025, ~21 k downloads) is *not* a webview capture API — its README states it "uses **xcap** to get window and monitor screenshots", i.e. OS-level compositor capture. That requires a **visible, mapped, unoccluded** window. https://github.com/ayangweb/tauri-plugin-screenshots · https://crates.io/crates/tauri-plugin-screenshots

**Conclusion:** you cannot do headless, pixel-accurate, in-app rendering QA in Tauri 2. This is the one place where Electron is genuinely better (§4.4). §11 explains why it does not change the recommendation.

### 3.6 Rust PDF libraries

| Library | License | Version / date | Verdict |
|---|---|---|---|
| **`pdfium-render`** | MIT/Apache-2.0 | **0.9.4, 6 Sep 2026** | **Primary.** Idiomatic high-level wrapper over Google's PDFium (the Chromium PDF engine). Rendering to bitmaps, **text and image extraction**, document creation/editing, form fields, annotations, signatures, attachments; `thread_safe` feature; WASM support. Binds **at runtime via `libloading`** — you ship `libpdfium.{so,dylib,dll}` alongside, or point at a system copy. 0.9.4 notes fix two double-free bugs in font/page-object handling. https://docs.rs/crate/pdfium-render/latest |
| **`hayro`** | MIT/Apache-2.0 | active, MSRV 1.92, ~679★ | **Secondary / cross-check.** Pure-Rust PDF interpreter + renderer, split into `hayro-syntax`, `hayro-interpret`, `hayro`, `hayro-svg`, plus its own JPEG2000/JBIG2/CCITT/CMap/PostScript decoders. Self-describes as "**still in a very development stage**" but "currently the most feature-complete" pure-Rust option, passing 1400+ PDFs from established test suites. Unsupported: knockout groups, non-embedded CID fonts. **Performance explicitly not yet a priority.** No FFI, no `libloading`, no C++ — attractive as a fallback backend and as a differential-testing oracle. https://github.com/LaurenzV/hayro |
| **`mupdf-rs`** | **AGPL-3.0** | 0.8.0, 22 Jun 2026 | **Reject unless OpenConvert is AGPL.** Excellent capability (plain text, **words with bounding boxes**, structured text, images, vector drawings; render to pixmap/SVG/display list) but builds MuPDF from source (needs C/C++ toolchain, libclang, fontconfig on Linux) and carries AGPL. https://github.com/messense/mupdf-rs |
| **`lopdf`** | MIT | 2.2k★, ~620 commits, MSRV 1.85 | **Utility.** Object-level parse/modify/create, object & xref streams, encryption, merging, basic text extraction. Not a layout engine. Useful for PDF surgery and for reading structure tags. https://github.com/J-F-Liu/lopdf |

**PDFium binaries:** `bblanchon/pdfium-binaries` (MIT) has built **automatically every Monday since 2017** for Android, iOS, Linux glibc (arm/arm64/ppc64/x64/x86), Linux musl, macOS (arm64/x64/universal), Windows (arm64/x64/x86), and experimental WASM, with V8 and non-V8 variants. https://github.com/bblanchon/pdfium-binaries
Size proxy: `pypdfium2` 5.13.0 wheels, which contain nothing but the binding + `libpdfium`, are **3.7 MB (manylinux x86_64), 3.5 MB (macOS arm64), 3.9 MB (win_amd64)** compressed. https://pypi.org/project/pypdfium2/#files → **≈4 MB compressed, ≈10 MB on disk per platform.**

**The one genuine gap, and its fix.** Rust has excellent *access* to PDF text (pdfium gives char-level boxes, font names, sizes, flags) but **no library that ships reading-order and page-segmentation algorithms**. Python has `pdfplumber`; .NET has PdfPig's `NearestNeighbourWordExtractor`, `DocstrumBoundingBoxes`, `RecursiveXYCut`, `UnsupervisedReadingOrderDetector`, `ContentOrderTextExtractor`.

**PdfPig is Apache-2.0** (https://github.com/UglyToad/PdfPig). Those algorithms are pure geometry over `(char, x, y, w, h, fontsize)` tuples — roughly 500–1 500 lines each, no PDF-format knowledge, no dependencies. **Porting them to Rust is legally clean (Apache-2.0, attribution + NOTICE) and is a bounded, well-specified task that Claude Code is very good at.** This converts P4's weakness from "missing capability" into "two weeks of transcription with property tests". It also makes the layout engine *yours*, which matters because reading-order quality is the core differentiator of a PDF→EPUB tool — you would end up rewriting it anyway in any stack.

### 3.7 LLM, OCR, EPUB, testing in Rust

- **LLM:** `llama-cpp-2` — **v0.1.156, updated 2 Sep 2026, 1 212 300 total / 622 916 recent downloads**, MIT/Apache, maintained by Utility AI, tracks llama.cpp closely. Requires `clang` for bindgen. The authors warn plainly: *"There is absolutely ways to misuse the llama.cpp API provided to create UB… we don't recommend using this crate for tasks where UB is not acceptable."* https://crates.io/crates/llama-cpp-2 · https://lib.rs/crates/llama-cpp-2 — **See §10: use the sidecar instead.**
- **OCR:** `ocrs` (Apache-2.0/MIT, 1.8k★, ~510 commits) — ML-based, ONNX models via the `rten` engine, less preprocessing than Tesseract. But: *"currently in an early preview. Expect more errors than commercial OCR engines"* and **Latin alphabet only**. https://github.com/robertknight/ocrs
  `leptess` (Tesseract+Leptonica bindings) is the mature option but drags in two C libraries. **Recommendation: ship Tesseract as a *sidecar* (`externalBin`), same mechanism as llama-server — no FFI, no build-time C dependencies, optional download.** Keep `ocrs` as a candidate for a pure-Rust future.
- **EPUB writing:** `epub-builder` (MPL-2.0, 0.8, 168★, EPUB 2.0.1 + 3.0.1, generates mimetype/toc.ncx/nav.xhtml/content.opf) is a reasonable starting point but the README itself lists real gaps — weak multi-author/multi-language metadata, no CSS templates, no XHTML content model. https://github.com/lise-henry/epub-builder
  **For a tool whose entire product is EPUB quality, hand-roll the OPF/NAV/XHTML with `quick-xml` + `zip`.** It is a few hundred lines, it is where your quality lives, and you need byte-level control (spine order, `epub:type` semantics, `page-list`, media overlays, `properties="scripted"`, deterministic zip with `mimetype` stored-first).
- **EPUB validation:** the reference validator, **W3C `epubcheck`, is Java** (https://github.com/w3c/epubcheck). The pure-Rust alternative **`epubveri`** (0.4.4, 8 Jul 2026) reports **98.8 % of epubcheck error-detection test cases caught with identical error codes and 98.9 % of valid files producing no false alarms**, but is pre-1.0 with **4 GitHub stars**, states it is "not a drop-in replacement for epubcheck yet", and is **dual-licensed AGPL-3.0 / commercial**. https://github.com/veripublica/epubveri
  **Recommendation: run real `epubcheck` in CI** (a JDK in the GitHub Action costs nothing) as the authority, and **write your own targeted OPF/NAV/XHTML/zip conformance checks in Rust with `quick-xml`** for the in-app fast path. Reconsider `epubveri` when it reaches 1.0 — and note that adopting it forces OpenConvert to be AGPL-3.0.
- **Testing:** `cargo test` + `insta` (snapshot the intermediate document model and the generated OPF/XHTML — this is exactly the right tool for a deterministic pipeline) + `proptest` (invariants on the layout algorithms: reading order is a total order, no character is lost or duplicated, bounding boxes stay within the page). Add `cargo-nextest` (faster, better isolation), `criterion` (per-stage perf budgets), and — **important for a tool eating untrusted PDFs** — `cargo-fuzz`/`arbitrary` on the parsing layer. `cargo deny` for licence and advisory gating (it will catch an accidental AGPL dependency at CI time, which given mupdf-rs/PyMuPDF/epubveri in this space is a real hazard).

---

## 4. Candidate 2 — Electron

### 4.1 Cost
244 MiB bundle and ~409 MB RAM for 6 windows (measured, §2.1); 80–120 MB / 200–500 MB in secondary sources. **This fails P1, P2 and P3 — the three highest-weighted priorities — simultaneously.** For a local-first document converter that competitors ship as a 200 MB Electron blob, "20 MB and 60 MB idle" is a genuine product differentiator, not just an engineering preference.

### 4.2 PDF
`pdf.js` (Apache-2.0, pure JS) is excellent and gives you text items with full transform matrices, plus canvas rendering — genuinely good raw material. `pdfium` is reachable through node bindings but the bindings are less maintained than `pdfium-render`. Same missing-layout-algorithms gap as Rust; the JS ecosystem has no `pdfplumber` either. Score 4, tied with Rust — **Electron does not actually win P4.**

### 4.3 LLM
`node-llama-cpp` is the best-in-class binding in any language: **prebuilt binaries for macOS, Linux and Windows** with cmake fallback, Metal and CUDA support, automatic chat-wrapper detection, and — the key feature — *"Force a model to generate output in a parseable format, like JSON, or even force it to follow a specific JSON schema"* enforced at the token-sampling level. https://www.npmjs.com/package/node-llama-cpp · https://github.com/withcatai/node-llama-cpp
This is a real advantage — but it is **fully neutralised by using `llama-server` as a sidecar**, which offers the same schema-constrained decoding over HTTP to *any* host language (§10).

### 4.4 Rendering QA — Electron's one genuine, decisive advantage
`webContents.capturePage()` works on hidden windows: *"The page is considered visible when its browser window is hidden and the capturer count is non-zero"*, with a `stayHidden` option to *"keep the page hidden instead of visible"*. Plus `printToPDF()` with full layout control and experimental tagged-PDF output, and full offscreen rendering (`webPreferences.offscreen`, GPU shared-texture or software output device, up to 240 FPS, frames stop when idle).
Sources: https://www.electronjs.org/docs/latest/api/web-contents · https://www.electronjs.org/docs/latest/tutorial/offscreen-rendering

You get in-process, headless, pixel-accurate rendering of your generated EPUB XHTML — for free, in the shipped binary.

**Why it still loses:** §11 shows this capability is obtainable in CI, for free, without paying Electron's tax in the shipped product — and Playwright additionally gives you a **WebKit** engine, which Electron cannot, and which is the engine you actually need to test for (macOS/Linux Tauri users, and Apple Books / Kobo, which are WebKit-based). Electron's advantage is real for *dev-time* QA and irrelevant for the *shipped* app.

### 4.5 Drag-and-drop
`File.path` was removed for security; apps now use `webUtils.getPathForFile(file)` from a preload script under context isolation, then IPC the path to main. https://www.electronjs.org/docs/latest/api/web-utils — i.e. Electron needed a migration to reach the ergonomics Tauri has by default.

**Verdict: rejected.** Loses the top three weighted priorities, ties on PDF, and its LLM and QA advantages are both obtainable without it.

---

## 5. Candidate 3 — Python + Qt (PySide6/PyQt6)

**PDF tooling is the best in any language — this is the only priority Python wins outright (score 5).**
- `pypdfium2` — **5.10.1/5.13.0, June 2026**, Apache-2.0 + BSD-3-Clause, **text extraction with positional information and boundary rectangles**, rendering to PIL/NumPy, 113 releases, 780★, wheels for every platform bundling pdfium. https://github.com/pypdfium2-team/pypdfium2
- `pdfplumber` — layout/word/table extraction, the de-facto reference implementation many people port from.
- `PyMuPDF` — fastest by a wide margin (its own benchmarks: text extraction over 7 031 pages in **8.01 s** vs XPDF 27.42 s, PyPDF2 101.64 s, PDFMiner 227.27 s; rendering 367.04 s vs XPDF 646 s) — but **AGPL-3.0 or commercial**. https://pymupdf.readthedocs.io/en/latest/about.html **Do not ship it.**

**Everything else is bad against P1–P3.**
- Packaged size: PyQt/PySide via PyInstaller measures **30–80 MB (Windows .exe), 50–120 MB (macOS .app), 40–90 MB (Linux AppImage)** for a *minimal* window with no heavy deps — and *"adding libraries like matplotlib, pandas, or NumPy increases the final size considerably."* https://www.pythonguis.com/faq/packaged-installer-file-sizes/ Add `llama-cpp-python` wheels, pdfium, ONNX runtime and you are at Electron scale with none of Electron's consistency.
- Startup: interpreter + Qt import + (with PyInstaller onefile) a decompress-to-temp step on every launch. Seconds, not milliseconds.
- Rendering QA needs **QtWebEngine — a bundled Chromium**, adding ~150 MB and reintroducing the exact cost you rejected Electron for.
- macOS signing/notarization of PyInstaller bundles is a well-known source of pain (nested dylibs each needing signature, hardened runtime vs `ctypes` dlopen).
- LLM: `llama-cpp-python` is MIT, actively released (**0.3.35, 17 Aug 2026**, wheels for CPU/CUDA 11.8–13.2/Metal/ROCm/Vulkan), with JSON and JSON-Schema modes. https://github.com/abetlen/llama-cpp-python/releases — fine, but the wheel/backend matrix is a persistent support burden.
- Maintainability: dynamic typing on a pipeline with a complex intermediate document model, and a packaging story that breaks on every dependency bump.

**Verdict: rejected as the shipped stack — but see §9, Python earns a permanent place off to the side.**

---

## 6. Candidate 4 — .NET 9/10 + Avalonia

The strongest runner-up, and it deserves respect.

- **PdfPig (Apache-2.0, 0.1.14 "Wessex Saddleback", 22 Mar 2026, 2.5k★, 1 849 commits)** is the single best *layout-analysis* library in any ecosystem: letters with position/font/glyph rectangles, `NearestNeighbourWordExtractor`, `DocstrumBoundingBoxes`, `UnsupervisedReadingOrderDetector`, `ContentOrderTextExtractor`, image extraction, annotations, bookmarks, form fields. https://github.com/UglyToad/PdfPig
- NativeAOT gives fast startup and a trimmed binary (~20–60 MB depending on trimming aggressiveness).
- **LLamaSharp** (MIT, v0.29.0, 3 800★, 506 forks) with prebuilt CPU (Win/Linux/macOS incl. Metal), CUDA 11/12 and Vulkan backends. But it binds a **pinned llama.cpp commit** (`815a2a59…`) and the version-mapping table shows it lags upstream by commits between releases — a real problem when new GGUF quantisations or architectures land. https://github.com/SciSharp/LLamaSharp
- **Avalonia has no first-party production WebView.** `Avalonia.WebView` returned 404 at the URL I tried; the ecosystem options are community projects with weak Linux support. **[UNVERIFIED — I could not fetch a current authoritative status page.]** No webview means no HTML rendering path at all for EPUB preview, and the whole UI must be written in XAML.
- PdfPig does **not** render pages; you would still bolt on PDFium via `PDFiumSharp`/`Docnet` (both largely stale) for rasterisation, which you need for OCR input and for before/after contact sheets.
- Developer velocity: the human maintainer does not write C#. XAML is a second learning curve on top of that, with less Claude Code leverage than Rust.

**Verdict: rejected.** Better than Electron/Python on the top three priorities, but the WebView gap kills the UI story, LLamaSharp's upstream lag is a chronic tax, and the PdfPig advantage is capturable by porting its Apache-2.0 algorithms into Rust (§3.6).

---

## 7. Candidate 5 — Flutter

`pdfrx` (MIT, v2.6.1 / `pdfrx_engine` 0.6.0, 273★, 1 332 commits) wraps PDFium for Android/iOS/Windows/macOS/Linux/Web, requiring Dart 3.13+ / Flutter 3.47+. It documents *text selection* and *search* but **not text extraction with positional data** — it is a **viewer**, not an extraction toolkit; the experimental `pdfrx_coregraphics` package carries an explicit warning. https://github.com/espresso3389/pdfrx

The Dart FFI llama.cpp packages (`llama_cpp_dart` and similar) are small, single-maintainer projects — nowhere near `node-llama-cpp` or `llama-cpp-2` in maturity **[UNVERIFIED — no authoritative maturity data found]**. Skia gives a beautiful, consistent UI you do not need. Dart is a third language for the maintainer. **Verdict: rejected** — the PDF story is the weakest of any candidate that has one at all.

---

## 8. Others (one paragraph each)

**Go + Wails.** Best raw numbers after Tauri (Go compiles to a small static binary, near-instant startup, low RAM) and the same webview model. Wails v2.15.0 is stable; **v3 is still in beta** as of this research, so you would build on v2 and face a migration. https://wails.io/docs/introduction The killer is PDF: **pdfcpu (Apache-2.0, ~1 122 commits)** does validation, optimisation, split/merge/trim, encryption, signatures, watermarks and *"extract and manipulate images, fonts, and metadata"* — but its `extract` operates on content streams, and it documents **no text extraction with positions and no layout analysis**. https://github.com/pdfcpu/pdfcpu The only serious Go PDF text stack is **UniPDF, which is AGPL-or-commercial**. You would be writing a PDF text engine from scratch in Go with no pdfium binding worth the name. **Rejected on P4.**

**Kotlin Multiplatform / Compose Desktop.** Desktop runs on the JVM; packaging is `jpackage` with a bundled trimmed JRE. JVM baseline heap plus Compose/Skia puts idle RAM well above Tauri's and startup in JVM-warmup territory — losing P1 and P2 by construction. Apache PDFBox is a mature, Apache-2.0 layout-capable PDF library (PdfPig is literally a port of it), which is a real asset; but you would also be adding a JVM to a project that is otherwise trying very hard to avoid one. The JetBrains page I fetched did not state per-platform stability or runtime-size figures **[UNVERIFIED]**. **Rejected on P1/P2.**

**Qt/C++.** Best-in-class RAM/startup/binary and Poppler is available, but developer velocity for a two-entity team (one of them an LLM) is the worst of any option — manual memory management, a build system that fights you, and signing/packaging complexity equal to everything else. **Rejected on P8, and it does not beat Rust on P1–P3.**

**Pure-Rust UIs — egui / iced / Slint / GPUI.** All would beat Tauri on RAM, startup and binary (no webview process at all — plausibly 15–30 MB RSS and <100 ms startup **[UNVERIFIED — no benchmark found]**). Fatal for this project: **none of them renders HTML/CSS**, so there is no EPUB preview, no path to reusing the generated XHTML for anything visual, and no way to show the user what their book will look like — for a tool whose *output is HTML*, giving up an HTML renderer is giving up the product's core feedback loop. Drag-and-drop is supported in all of them (winit-level file-drop events). Blitz (DioxusLabs) is the only Rust HTML/CSS renderer that could close the gap — Stylo for CSS, Taffy for layout, Parley for text, flexbox/grid/table/absolute, complex selectors, media queries, CSS variables, Apache-2.0/MIT, 3.7k★ — but it is **explicitly pre-alpha with maintainers actively discouraging production use**. https://github.com/DioxusLabs/blitz Revisit in 2027. **Rejected.**

**Wails vs Tauri specifically.** Same architecture, same webview-inconsistency risk, same sidecar-style packaging. Tauri wins on: v2 is stable while Wails v3 is beta; a far larger plugin ecosystem with a first-party updater; a permission/capability system that scopes sidecar execution; and — decisively — Rust's PDF ecosystem versus Go's.

---

## 9. The hybrid options

### 9.1 Tauri + Rust pipeline, with Python for offline eval only — **ADOPT THIS**

This is not a compromise, it is the right shape. The shipped artifact is pure Rust + Tauri. Alongside it, a separate `eval/` toolchain in Python does what Python is genuinely best at:

- **Corpus generation and management** — scraping/curating a few hundred representative PDFs (academic two-column, scanned books, magazines with pull quotes, RTL, CJK, math, tables).
- **Ground-truth and metric computation** — reading-order edit distance, character retention rate vs the PDF text layer, heading-detection F1, paragraph-boundary accuracy, footnote-linking precision.
- **Cross-implementation differential testing** — run `pdfplumber` and `pypdfium2` over the same corpus and diff against the Rust pipeline's intermediate model to find extraction regressions. This is how you catch layout bugs cheaply.
- **Reporting** — pandas + matplotlib + a notebook, regenerated per release.

**Two constraints.** (1) The eval tool is *distributed* if it lives in the repo, so **AGPL dependencies still bind it** — use `pypdfium2` (Apache/BSD) and `pdfplumber` (MIT), **not PyMuPDF**, or keep the eval harness in a separate repository. (2) It must never become a build dependency of the app; enforce with CI (the release job must not have Python available).

**Cost: near zero. Value: high** — it is the only cheap way to know whether a layout-algorithm change made conversions better or worse.

### 9.2 Electron UI + Rust core via napi-rs — **REJECT**

napi-rs is genuinely excellent (Node 22.13+/24+/26, zero-config `napi build`, ABI-stable single binaries, prebuilt cross-compilation for Windows/macOS/Linux/FreeBSD/Android across x86/x64/ARM64/ARM, WASM fallback, used by Next.js, SWC, Prisma, Polars, Turborepo — **and by Tauri itself**). https://napi.rs/

But this hybrid **pays both costs and buys one benefit**: you still ship 200 MB of Chromium and 400 MB of RAM (failing P1–P3), you still have to learn Rust (no relief on P8), and you *add* an N-API ABI boundary, a `napi build` × 3 OS × 2 arch prebuild matrix, and a second failure mode where the native module fails to load. The only thing it buys over Tauri is in-app headless Chromium for QA — which §11 gets for free. **Rejected.**

---

## 10. What the stack implies for LLM integration: **sidecar `llama-server`, not in-process**

This is a direct consequence of priorities 1, 2 and 3, and I recommend it *regardless* of which host language wins.

**`llama-server`** (llama.cpp `tools/server`) is a "fast, lightweight, pure C/C++ HTTP server" with an OpenAI-compatible chat-completions and embeddings API, and — critically — **schema-constrained output via both JSON Schema and GBNF grammar through `response_format`**. It supports parallel slots (`-np`), continuous batching, prompt caching (`--cache-prompt`), context checkpointing (`-ctxcp`), KV-cache offload control (`-kvo`), and flexible loading modes (`-lm`: mmap / mlock / DirectIO). https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md

### Why sidecar beats `llama-cpp-2` in-process, priority by priority

| Priority | In-process (`llama-cpp-2`) | Sidecar (`llama-server`) |
|---|---|---|
| **P1 low RAM** | Model RSS is inside your process. Unloading a model does **not** reliably return memory to the OS — allocator fragmentation keeps RSS high for the app's lifetime. | `kill()` returns **100 %** of model memory to the OS, instantly and deterministically. Spawn on first LLM use, terminate after N minutes idle. |
| **P2 fast startup** | The app binary links llama.cpp + all GGML kernels; dynamic-linker work and static initialisers run on every launch even when no model is used. | The app never touches LLM code until the user converts something. Startup is unaffected. Model load happens in the background, in another process. |
| **P3 small binary** | llama.cpp + GGML backends are statically linked into the main binary — tens of MB, always shipped. | `llama-server` is a separate `externalBin` that can be an **optional post-install download**, alongside the GGUF (which must be a download anyway — a 0.6–1.7B Q4 model is 400 MB–1.1 GB). Base installer stays ~20–30 MB. |
| **P5 easy integration** | Must reimplement grammar/JSON-schema plumbing, sampler chains, KV-cache management, and slot handling against a C++ API whose own maintainers warn about UB. | HTTP + JSON. Schema-constrained decoding is a request field. Any language can drive it. Testable with a stub HTTP server — **the pipeline's LLM calls become trivially mockable in `cargo test`, which matters enormously for a deterministic pipeline.** |
| **P6 cross-platform** | Requires `clang`/bindgen + cmake in *your* build, and a CI matrix for AVX/AVX2/AVX512/NEON variants. | Vendor upstream's official release binaries per target triple. No C++ in your build at all. |
| **P7 maintainability** | Your app's memory safety is coupled to a C++ API that can produce UB. A model-level crash takes down the user's in-progress conversion. | **Process isolation.** A segfault, OOM or bad GGUF kills only the sidecar; the pipeline catches the error, reports it, and falls back to the deterministic path. For a local-first tool this is the difference between "the AI step failed" and "the app crashed and lost your work." |

**Cost of the sidecar approach:** ~1 ms HTTP round-trip overhead per call (irrelevant — a book is a few hundred short structured calls, each dominated by tens of ms of decoding), child-process lifecycle management, and a port to bind.

**Operational recipe:** bind `127.0.0.1` on an OS-assigned free port (probe a free port in Rust, then pass it to `--port`), pass a per-launch random `--api-key`, `-np 1`, small `-c` (the LLM's job is per-block classification and heading/footnote disambiguation, not long-context reasoning), spawn lazily, health-check `/health`, terminate after idle timeout and on app exit (Tauri's shell plugin ties child lifetime to the app). Scope `shell:allow-execute` to the exact sidecar name with argument validators so the webview can never spawn anything else.

**Keep `llama-cpp-2` on the shelf.** If profiling later shows per-call latency actually matters, swapping an in-process backend behind the same `LlmBackend` trait is a contained change — but do not start there.

---

## 11. What the stack implies for rendering QA

Tauri 2 cannot screenshot a hidden webview (§3.5). Design around it — the result is *better* than what Electron would have given you.

### Layer 1 — In-app: structural and textual invariants, no pixels (Rust, ships)
This is what actually catches conversion bugs, and none of it needs a renderer:
- **EPUB conformance:** OCF zip structure (`mimetype` first, stored, uncompressed), `container.xml`, OPF manifest/spine completeness, every manifest item resolvable, `nav.xhtml` well-formed with correct `epub:type`, XHTML well-formedness and namespace correctness (`quick-xml`), no dangling internal hrefs, images referenced and present, CSS parses.
- **Content reconciliation against the source PDF:** character-retention ratio vs the pdfium text layer (flag < 98 %), no duplicated runs, monotonic reading order, heading hierarchy has no level skips, every footnote marker has a matching backlink and vice versa, table cell counts consistent per row.
- **Deterministic-output check:** convert twice, assert byte-identical EPUBs (excluding the timestamp field). Cheap, and catches every accidental HashMap-iteration-order bug.
- Surface all of the above as a per-conversion "quality report" in the UI. **This is a shippable product feature, not just a test.**

### Layer 2 — CI: pixel regression with Playwright, **Chromium *and* WebKit** (not shipped)
Playwright ships its own Chromium **and its own WebKit** build. Load the generated XHTML+CSS at fixed viewports (e.g. 600×800 e-reader, 390×844 phone, 1024×768 tablet), screenshot, and diff against golden images with a perceptual threshold. Store goldens in the repo; a change to the CSS or the layout engine that shifts pixels fails the build until a human approves the new goldens.

**This is strictly better than Electron's `capturePage`:** Electron gives you Chromium only, but your users read EPUBs in **Apple Books, Kobo and iBooks — all WebKit** — and Tauri's own macOS/Linux webviews are WebKit. Testing WebKit is the requirement; Electron cannot do it and Playwright can. It also directly mitigates the WebKitGTK-divergence risk from §3.1 by making engine differences a CI signal instead of a user bug report.

### Layer 3 — Human review: before/after contact sheets (Rust, dev + optional in-app)
Rasterise source PDF pages with pdfium (already linked, zero marginal cost) and pair them with the Layer-2 EPUB screenshots into side-by-side contact sheets. This is how you actually evaluate conversion quality on a new corpus, and it is the artifact you attach to a release.

### Layer 4 — In-app preview (Tauri webview, visible)
Render the generated XHTML in a normal, visible Tauri webview so the user can page through the result. Label it "approximate preview — your reader may differ", because it *is* WebView2 on Windows and WebKit elsewhere. Do **not** build automated QA on top of this window.

**Explicitly do not:** bundle a headless browser (undoes P3), depend on Blitz (pre-alpha), or try to screenshot an invisible Tauri window (unsupported — wry #1358, tao #289 both open).

---

## 12. Top 5 risks of the chosen stack, with mitigations

### Risk 1 — Rust learning curve for the single human maintainer *(highest probability, moderate impact)*
The maintainer is a CS student comfortable in Python/TS. Borrow checker, lifetimes, `Rc<RefCell<_>>` temptations in a tree-shaped document model, and async colouring are the classic time sinks.
**Mitigations:**
- **Architectural constraints that route around the hard parts:** no `async` anywhere in the pipeline core (it is CPU-bound; use `rayon` for per-page parallelism, `std::thread` for the sidecar supervisor); **no lifetimes in any public API** — the document model owns its data (`String`, `Vec`, arena indices via `slotmap`/`Vec<usize>` instead of references); `anyhow::Result` at the app boundary, `thiserror` in the library; `#![forbid(unsafe_code)]` in every crate except a thin `pdfium` wrapper module.
- Keep the **UI in TypeScript** — that is ~30–40 % of the code in the maintainer's existing skill set from day one, and it makes the Rust surface a well-defined command API rather than a whole application.
- `cargo clippy -- -D warnings`, `rustfmt`, `cargo deny check`, `insta` snapshots so aggressive refactors are safe to attempt.
- **Budget 4–6 weeks of reduced velocity.** Claude Code writes idiomatic Rust well and the compiler's error messages are the best teaching tool in the ecosystem; the curve is real but it is front-loaded and it flattens.

### Risk 2 — Cross-webview inconsistency (WebKitGTK 2.36 → 2.52 spread; WKWebView; WebView2) *(high probability, low impact if scoped)*
**Mitigations:** the app webview renders an internal control panel only — declare and enforce a conservative CSS baseline (no `:has()`, container queries, or subgrid); document **WebKitGTK ≥ 2.44** (Ubuntu 24.04 / Debian 13) as the supported floor; ship **Flatpak as the primary Linux artifact** (it pins its own WebKitGTK runtime, removing the spread entirely for most Linux users) with AppImage secondary; detect the WebView2 Evergreen runtime at install time and ship the ~2 MB bootstrapper; make **EPUB output** rendering a CI concern tested in both Playwright engines (§11), never something the app's own webview is trusted to validate.

### Risk 3 — PDFium supply chain and single-maintainer binding *(medium probability, high impact)*
`pdfium-render` is essentially a one-maintainer crate, and `libpdfium` is loaded at runtime from vendored per-platform binaries built by a third party.
**Mitigations:** vendor pinned `bblanchon/pdfium-binaries` releases with **SHA-256 verification in CI** (never download at app runtime); put every pdfium call behind an internal `PdfBackend` trait so `hayro` or `mupdf-rs` can be substituted; `pdfium-render` is MIT/Apache-2.0 and small enough to fork if abandoned; run **`hayro` as a differential oracle in tests** — when the two backends disagree on extracted text for a corpus PDF, that is a bug report; PDFium itself is BSD-3, Google-maintained, and the Chrome PDF engine — the *underlying* dependency is about as safe as software gets.

### Risk 4 — No in-process HTML rendering, so pixel QA cannot ship in the app *(certain, low impact once accepted)*
**Mitigations:** define the shipped quality gate as the structural/textual invariants of §11 Layer 1 (which are what actually find conversion defects); keep visual regression in CI with Playwright's Chromium **and** WebKit; provide a visible, explicitly-labelled preview window for humans; if a "deep visual check" is ever demanded in-app, shell out via CDP to a *system-installed* browser if one is detected, and never bundle one. Re-evaluate Blitz in 2027.

### Risk 5 — Code-signing cost and friction for an OSS project *(certain, moderate impact)*
Unsigned macOS builds are effectively unusable under Gatekeeper; unsigned Windows downloads trip SmartScreen.
**Mitigations:** budget the **$99/year Apple Developer Program** — it is mandatory for notarization and there is no free path; check the fee-waiver route if the project ever sits under a nonprofit. On Windows, **ship unsigned initially** with clear documentation of the SmartScreen bypass, then move to **Azure Artifact Signing** once there is a stable release cadence (cheaper than an EV cert; exact price requires a quote — **[UNVERIFIED]**). Linux needs no signing. Regardless of OS signing, **enable Tauri's updater plugin immediately** — its Ed25519 signing is mandatory, free, and gives update integrity from day one; host the static JSON manifest on GitHub Releases. Document that `.deb`/`.rpm`/Flatpak users update through their package manager, since the updater covers only AppImage/NSIS/MSI/`.app.tar.gz`.

### Risk 6 (bonus) — Licence contamination *(medium probability, high impact)*
This domain is littered with AGPL: `mupdf-rs`, PyMuPDF, `epubveri`, UniPDF. A single careless `cargo add` changes OpenConvert's licence obligations.
**Mitigation:** `cargo deny` with an explicit licence allow-list (MIT / Apache-2.0 / BSD / MPL-2.0 / ISC / Unicode) failing CI on anything else, from commit one. Decide the project licence deliberately: **Apache-2.0 or MIT** keeps every recommended dependency legal and keeps the option of a future permissive fork; choosing AGPL-3.0 would unlock `mupdf-rs` and `epubveri` but forecloses adoption by others. Recommend **Apache-2.0** (it also matches PdfPig, whose algorithms you are porting).

---

## 13. Concrete stack, for the record

| Layer | Choice | Licence |
|---|---|---|
| Shell / windowing | **Tauri 2.11.x** | MIT/Apache-2.0 |
| UI | TypeScript + a light framework (Svelte or vanilla), conservative CSS baseline | — |
| Pipeline | Rust workspace: `openconvert-pdf`, `-layout`, `-epub`, `-llm`, `-qa` | Apache-2.0 |
| PDF parse/render | **`pdfium-render` 0.9.x** + vendored `libpdfium` (bblanchon, SHA-pinned) | MIT/Apache-2.0 + BSD-3 |
| PDF cross-check | **`hayro`** (differential oracle in tests) | MIT/Apache-2.0 |
| PDF surgery | `lopdf` | MIT |
| Layout analysis | **Ported from PdfPig** (Docstrum, XY-cut, nearest-neighbour words, unsupervised reading order) + own heading/footnote heuristics | Apache-2.0, attributed |
| EPUB writing | Hand-rolled OPF/NAV/XHTML with `quick-xml` + `zip` (deterministic, `mimetype` stored-first) | — |
| EPUB validation | Own Rust conformance subset in-app; **real `epubcheck` (Java) in CI** | W3C epubcheck: BSD-3 |
| LLM | **`llama-server` sidecar** via `externalBin`, JSON-Schema-constrained, lazy spawn, idle kill | MIT (llama.cpp) |
| OCR | **Tesseract sidecar** (optional download); watch `ocrs` | Apache-2.0 |
| Updates | Tauri updater plugin, Ed25519-signed, static JSON on GitHub Releases | — |
| Testing | `cargo nextest` + `insta` + `proptest` + `criterion` + `cargo-fuzz` on the parser | — |
| Visual QA | **Playwright (Chromium + WebKit) in CI**, golden-image diffs; pdfium contact sheets | — |
| Eval harness | Separate Python tooling: `pypdfium2` + `pdfplumber` + pandas. **Never shipped, never a build dep, no AGPL.** | Apache/BSD/MIT |
| Linux dist | **Flatpak primary**, AppImage secondary, `.deb`/`.rpm` best-effort | — |
| Licence | **Apache-2.0**, enforced by `cargo deny` allow-list | — |

**Expected shipped installer:** ~20–30 MB (Tauri shell ~9 MB + libpdfium ~4 MB compressed + pipeline ~3–5 MB). LLM sidecar, GGUF model and OCR data are post-install downloads.
**Expected idle RAM:** ~50–90 MB (framework), rising to ~700 MB–1 GB only while the LLM sidecar is alive — and returning fully to baseline when it is killed.

---

## 14. Sources

**Tauri — official**
- Sidecar / external binaries — https://v2.tauri.app/develop/sidecar/
- App size & Cargo profile — https://v2.tauri.app/concept/size/
- Updater plugin — https://v2.tauri.app/plugin/updater/
- macOS signing & notarization — https://v2.tauri.app/distribute/sign/macos/
- Windows signing — https://v2.tauri.app/distribute/sign/windows/
- `WebviewWindow` / `onDragDropEvent` — https://v2.tauri.app/reference/javascript/api/namespacewebviewwindow/
- Releases (v2.11.5, 1 Jul 2026) — https://github.com/tauri-apps/tauri/releases

**Tauri — issues establishing limitations**
- wry #1358, screenshot capability (open, 9 Sep 2024) — https://github.com/tauri-apps/wry/issues/1358
- tao #289, offscreen rendering (open, 19 Jan 2022) — https://github.com/tauri-apps/tao/issues/289
- tauri #14373, `dragDropEnabled` semantics — https://github.com/tauri-apps/tauri/issues/14373
- tauri #9448, Windows drag-drop events — https://github.com/tauri-apps/tauri/issues/9448
- tauri #11763, webkit2gtk-4.1 on Ubuntu 22 (closed, premise incorrect) — https://github.com/tauri-apps/tauri/issues/11763
- tauri #12463, AppImage missing injected bundle — https://github.com/tauri-apps/tauri/issues/12463
- tauri #14087, WebKit2GTK path resolution — https://github.com/tauri-apps/tauri/issues/14087
- tauri #14796, linuxdeploy failures in CI — https://github.com/tauri-apps/tauri/issues/14796
- tauri #13599, Flatpak tray icon — https://github.com/tauri-apps/tauri/issues/13599
- `tauri-plugin-screenshots` (xcap-based, visible windows) — https://github.com/ayangweb/tauri-plugin-screenshots · https://crates.io/crates/tauri-plugin-screenshots

**Benchmarks**
- gethopp, measured, 9 Apr 2025 (8.6 MiB / 244 MiB; 172 MB / 409 MB) — https://www.gethopp.app/blog/tauri-vs-electron
- Oflight, cited ranges, 2026 — https://www.oflight.co.jp/en/columns/tauri-v2-vs-electron-comparison
- tech-insider.org, 2026 — **[UNVERIFIED, do not cite]** — https://tech-insider.org/tauri-vs-electron-2026/
- PyQt/PySide packaged installer sizes — https://www.pythonguis.com/faq/packaged-installer-file-sizes/

**Platform / runtime**
- WebView2 distribution & Windows 10/11 preinstall — https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution
- Ubuntu `libwebkit2gtk-4.1-0` availability by release — https://packages.ubuntu.com/search?keywords=libwebkit2gtk-4.1-0
- Apple Developer Program $99/yr, notarization gating — https://developer.apple.com/support/compare-memberships/
- Azure Artifact Signing overview — https://learn.microsoft.com/en-us/azure/trusted-signing/overview
- Azure Artifact Signing pricing (dollar amounts not published) — https://azure.microsoft.com/en-us/pricing/details/artifact-signing/

**PDF**
- `pdfium-render` 0.9.4, 6 Sep 2026 — https://docs.rs/crate/pdfium-render/latest
- `hayro` — https://github.com/LaurenzV/hayro
- `mupdf-rs` 0.8.0, AGPL — https://github.com/messense/mupdf-rs
- `lopdf` — https://github.com/J-F-Liu/lopdf
- `bblanchon/pdfium-binaries` (weekly since 2017, MIT) — https://github.com/bblanchon/pdfium-binaries
- `pypdfium2` (wheel sizes as pdfium size proxy) — https://github.com/pypdfium2-team/pypdfium2 · https://pypi.org/project/pypdfium2/#files
- PyMuPDF licence + benchmarks — https://pymupdf.readthedocs.io/en/latest/about.html
- PdfPig 0.1.14, Apache-2.0, layout algorithms — https://github.com/UglyToad/PdfPig
- pdfcpu — https://github.com/pdfcpu/pdfcpu
- pdfrx (Flutter) — https://github.com/espresso3389/pdfrx

**LLM**
- `llama-server` README (OpenAI API, JSON-Schema/GBNF, slots) — https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md
- `llama-cpp-2` v0.1.156, 2 Sep 2026 — https://crates.io/crates/llama-cpp-2 · https://lib.rs/crates/llama-cpp-2
- `node-llama-cpp` — https://www.npmjs.com/package/node-llama-cpp · https://github.com/withcatai/node-llama-cpp
- `llama-cpp-python` 0.3.35, 17 Aug 2026 — https://github.com/abetlen/llama-cpp-python · https://github.com/abetlen/llama-cpp-python/releases
- LLamaSharp v0.29.0 — https://github.com/SciSharp/LLamaSharp

**OCR / EPUB**
- `ocrs` (early preview, Latin only) — https://github.com/robertknight/ocrs
- `leptess` — https://github.com/houqp/leptess
- `epub-builder` (MPL-2.0) — https://github.com/lise-henry/epub-builder
- W3C `epubcheck` (Java) — https://github.com/w3c/epubcheck
- `epubveri` 0.4.4, AGPL/commercial, 98.8 % parity — https://github.com/veripublica/epubveri

**Electron / other frameworks**
- `webContents.capturePage` / `stayHidden` / `printToPDF` — https://www.electronjs.org/docs/latest/api/web-contents
- Electron offscreen rendering — https://www.electronjs.org/docs/latest/tutorial/offscreen-rendering
- `webUtils.getPathForFile` — https://www.electronjs.org/docs/latest/api/web-utils
- napi-rs — https://napi.rs/
- Wails (v2.15.0 stable, v3 beta) — https://wails.io/docs/introduction
- Blitz (pre-alpha Rust HTML/CSS renderer) — https://github.com/DioxusLabs/blitz
- Compose Multiplatform — https://kotlinlang.org/docs/multiplatform/compose-multiplatform-and-jetpack-compose.html **[status details UNVERIFIED]**

**Marked [UNVERIFIED] in this report:** tech-insider.org benchmark figures and its cited "Nickel"/"Open Web Foundation" suites; Avalonia WebView current production status; Dart llama.cpp binding maturity; pure-Rust UI (egui/iced/Slint) RAM/startup figures; Compose Multiplatform desktop stability and runtime size; exact Azure Artifact Signing dollar pricing.

**Research budget note:** the session's WebSearch quota (200 calls) was exhausted partway through; the remainder of the research was conducted by direct WebFetch against official documentation, repositories and package registries, which is the higher-quality source class anyway.
