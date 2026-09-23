<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/openconvert-mark-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="docs/assets/openconvert-mark.svg">
    <img alt="OpenConvert" src="docs/assets/openconvert-mark.svg" width="96" height="96">
  </picture>
</p>

<h1 align="center">OpenConvert</h1>

<p align="center">Convert PDF books to reflowable EPUB 3.3, on your own computer.</p>

<p align="center">
  <a href="https://github.com/egeselbzgyk/openconvert/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/egeselbzgyk/openconvert/actions/workflows/ci.yml/badge.svg?branch=main"></a>
  <a href="LICENSE"><img alt="License: Apache-2.0" src="https://img.shields.io/badge/license-Apache--2.0-blue"></a>
  <a href="https://github.com/egeselbzgyk/openconvert/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/egeselbzgyk/openconvert"></a>
</p>

OpenConvert is a desktop app and a command-line tool that turns a PDF book into an EPUB 3.3 that
reflows on any e-reader. It runs entirely on your computer: nothing is uploaded, there is no
telemetry, and a conversion never uses the network.

## What it does, and how it differs

Most converters copy the text out of a PDF page by page. OpenConvert rebuilds the book's structure:

- **Deterministic structure inference.** Columns and reading order, paragraphs rejoined across lines
  and pages, running headers, footers and page numbers removed, headings and chapters recovered (one
  file per chapter, a table of contents and a page list that point at them), footnotes linked both
  ways, and verse, quotations, lists, tables and figures kept as what they are. Without AI, the same
  PDF gives the same EPUB, byte for byte.
- **Character conservation.** Every stage of the pipeline must keep every character of the book's
  text or declare why it removed it (a running header, a soft hyphen, …). The report says what was
  removed, why, and how much of the source text reached the EPUB.
- **Validate, then repair.** Every EPUB is checked as it is written — package structure, navigation,
  footnote links, the page list, image descriptions, no scripts or remote resources — and anything
  the checks find is repaired before the file is saved. CI holds the project's test books to zero
  EPUBCheck errors.
- **Optional local AI, off by default.** A small model on your computer can be asked four narrow
  questions per book; every answer is checked before it is used, and the conversion is complete
  without it.
- **Private by construction.** No telemetry and no crash reporting. The converter's core contains no
  network code; the app connects only when you download a model, check for updates, or use an AI
  server off your computer after consenting to its host.

## Download and install

OpenConvert 1.0 is for **Windows and Linux**. A macOS version comes in a later 1.x release, once the
app can be signed and notarized; until then it can be built from source (below).

- **Windows:** the NSIS installer (per user, no administrator rights) or the MSI. The installers are
  not code-signed in 1.0, so Microsoft Defender SmartScreen warns the first time; choose *More info*
  → *Run anyway*, or first compare the file's SHA-256 with the one in the release notes.
- **Linux:** the AppImage, which updates itself when you ask it to. A Flatpak for Flathub (no network
  access, updated by Flathub) is prepared in `packaging/linux/flatpak/` and is listed on Flathub once
  Flathub accepts it.

Downloads are on the [Releases](https://github.com/egeselbzgyk/openconvert/releases) page, with every
file's SHA-256 and a CycloneDX SBOM. Checksums, the SmartScreen warning and what each package can
and cannot do: [docs/INSTALL.md](docs/INSTALL.md).

## Using it

**Desktop app.** Drop PDFs on the window. Each book shows its progress; when it is done you can read
its report, correct the title, author or chapter list and rebuild, and preview the result. The app is
in English, German and Turkish.

**Command line.** The app runs the same engine, `openconvert`:

```sh
openconvert convert book.pdf                   # book.epub and book.epub.report.json beside the PDF
openconvert convert book.pdf -o out.epub --lang de --preset novel
openconvert validate book.epub                 # the built-in validator's findings
openconvert validate book.epub --json          # the same, as JSON on stdout
openconvert inspect book.pdf --json            # the PDF's pages, how each is classified, its producer
openconvert --help                             # every command and flag
```

`--progress json` prints progress as NDJSON events on stderr; stdout carries data only. Exit codes:
0 ok, 1 failed (a report is still written), 2 usage, 3 cancelled.

## Optional AI assistance

Off by default. With it on, the converter may ask a local model four once-per-book questions — the
title and author, the heading levels, where the front and back matter begin, and whether an indented
passage is verse or a quotation — and uses an answer only after checking it. The model is a download
you choose; it is never in the installer:

```sh
openconvert model list                         # the models this version knows, with sizes and licences
openconvert model pull qwen3-1.7b-q4_k_m       # the default: Qwen3 1.7B, Apache-2.0, 1.28 GB
openconvert convert book.pdf --ai
```

It can also use Ollama or another OpenAI-compatible server you run (`--llm-provider`,
`--llm-endpoint`); a server off your computer needs your consent to its host (`--llm-allow-host`).
In 1.0 no task has yet passed its accuracy evaluation for any language, so `--ai` (and the app's AI
switch) asks the model nothing; `--ai-all-tasks` runs the tasks without that evaluation.

## OCR

Scanned pages, and scanned regions of otherwise digital pages, are read with
[Tesseract](https://github.com/tesseract-ocr/tesseract) 5 when it is installed on your system
(English, German and Turkish are tested). Without it, scanned pages stay images and the report says
so. `--ocr never|auto|always` and `--ocr-lang` control it.

## Privacy and security

The threat OpenConvert defends against is a hostile PDF. Resource limits (pages, image size,
decompressed data, per-stage deadlines) are checked before the work they bound; on Linux the
converter caps its own memory and confines itself with Landlock before it reads the PDF; the parsers
the project writes are fuzzed. What is and is not enforced on each platform:
[docs/SECURITY.md](docs/SECURITY.md) and the Security section of the
[1.0.0 release notes](docs/CHANGELOG.md). Please report vulnerabilities privately through GitHub
Security Advisories.

## Known limitations in 1.0

- No macOS build yet; it comes in a later 1.x.
- The Windows installers are not code-signed.
- The optional validation pack (the full EPUBCheck inside the app) is not offered yet; the built-in
  validator checks every EPUB.
- On the project's real-world test corpus, 79 of 104 documents convert with every character
  accounted for; the others lose some text, do not finish within the time limit, or stop at a
  conservation check. That work continues after 1.0.
- No cap on the size of the EPUB written (a book over 50 MiB gets a warning), and no memory cap on
  Windows.
- AI assistance is off by default, no task is enabled for any language yet, and the default model has
  not been through its promotion gates.

The full list is in [docs/CHANGELOG.md](docs/CHANGELOG.md).

## Building from source

You need Rust 1.98 and Node 22. On Linux, with Debian or Ubuntu package names:

```sh
sudo apt-get install build-essential pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev \
  librsvg2-dev libayatana-appindicator3-dev
```

On Windows: the Microsoft C++ Build Tools and the WebView2 runtime (part of Windows 10 and 11), as
for any [Tauri 2](https://v2.tauri.app/start/prerequisites/) app.

The converter, from the repository root (it finds PDFium in `vendor/`, or set `OC_PDFIUM_PATH`):

```sh
cargo run -p xtask -- vendor-pdfium            # the pinned PDFium build, checked against its SHA-256
cargo build -p openconvert --release
./target/release/openconvert convert book.pdf
```

The desktop app (Tauri 2 and Svelte 5, in `apps/desktop`):

```sh
cargo run -p xtask -- fetch-llama-server       # the pinned llama.cpp server the app bundles
cargo build -p openconvert                     # the engine the app runs
cargo run -p xtask -- stage-sidecars           # both, with PDFium, where the bundler expects them
npm ci --prefix apps/desktop/ui
cargo install tauri-cli --version "=2.11.5" --locked
cd apps/desktop/src-tauri && cargo tauri dev
```

Tests: `cargo run -p xtask -- fixtures` and `cargo run -p xtask -- fixtures --keep-structtree`
compile the test PDFs, then `cargo nextest run --workspace --exclude openconvert-desktop`; the UI's
tests are `npm test --prefix apps/desktop/ui`.

## Contributing and project documents

- [docs/DECISIONS.md](docs/DECISIONS.md) — the architecture decisions and their reasons; the
  highest authority when documents disagree.
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — processes, the intermediate representation, the
  conservation law, the validation gates.
- [docs/PIPELINE.md](docs/PIPELINE.md) — each stage's algorithm and parameters.
- [docs/IMPLEMENTATION_PLAN.md](docs/IMPLEMENTATION_PLAN.md) and [PROGRESS.md](PROGRESS.md) — the
  phase plan and where the work stands.
- [CLAUDE.md](CLAUDE.md) — the repository's working agreement: test-first work items and the rules
  CI enforces (no ignored tests, no numeric literals outside `thresholds.toml`, deterministic
  output).

## License

Apache-2.0: see [LICENSE](LICENSE) and [NOTICE](NOTICE). Third-party licences are listed in
[docs/LICENSE_AND_DEPENDENCIES.md](docs/LICENSE_AND_DEPENDENCIES.md) and
`licenses/third-party-rust.txt`.
