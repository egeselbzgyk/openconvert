<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/openconvert-mark-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="docs/assets/openconvert-mark.svg">
    <img alt="OpenConvert" src="docs/assets/openconvert-mark.svg" width="96" height="96">
  </picture>
</p>

<h1 align="center">OpenConvert</h1>

<p align="center">Turn PDF books into reflowable EPUBs, on your own computer.</p>

<p align="center">
  <a href="https://github.com/egeselbzgyk/openconvert/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/egeselbzgyk/openconvert"></a>
  <a href="LICENSE"><img alt="License: Apache-2.0" src="https://img.shields.io/badge/license-Apache--2.0-blue"></a>
</p>

OpenConvert is a desktop app and a command-line tool that turns a PDF book into an EPUB 3.3 that
reflows on any e-reader. It rebuilds the book instead of copying pages: paragraphs, chapters, a
linked table of contents, footnotes and a cover. Everything runs on your computer. Nothing is
uploaded, and there is no telemetry.

[Install](#install) · [Desktop app](#desktop-app) · [Command line](#command-line) ·
[AI assistance](#ai-assistance-optional) · [Scanned books](#scanned-books) ·
[Privacy](#privacy-and-security) · [Limitations](#known-limitations) ·
[Build from source](#build-from-source)

## What's new in 1.1

- **Much better results on real books.** Paragraphs continue across page breaks, words hyphenated
  at line ends are joined again, page numbers no longer leak into the text, chapters are found more
  reliably, the book's printed contents page becomes clickable links, and the first page becomes the
  cover.
- **Front matter is recognised.** Title page, half-title, copyright page, dedication and epigraph are
  marked as what they are instead of being treated as ordinary text.
- **Not tied to particular languages.** The rules read each book's own layout, numbering and
  vocabulary instead of relying on word lists for a few languages, and the book's language is
  detected automatically.
- **More useful AI, with Fast and Quality modes.** When you turn it on, the model fills in a missing
  title and author and, in Quality mode, identifies opening pages the rules could not. Both modes
  work the same with the built-in model, Ollama or your own OpenAI-compatible server.
- **Desktop app:** a list of previous conversions, one folder for all your books with a button to
  open it, an adjustable time limit per step for large books, and the app's own icon.

## Install

Download the file for your system from the
[latest release](https://github.com/egeselbzgyk/openconvert/releases/latest):

| System | File |
|---|---|
| Windows (64-bit) | `OpenConvert_<version>_x64-setup.exe` (installs for your user, no admin rights), or the `.msi` |
| Linux (x86-64) | `OpenConvert_<version>_amd64.AppImage`: make it executable and run it |
| macOS | No download yet. You can [build it from source](#build-from-source). |

- **Windows will warn you once.** The installers are not code-signed, so Microsoft Defender
  SmartScreen shows "Windows protected your PC". Choose **More info**, then **Run anyway**. To check
  the file first, compare its SHA-256 with the one in the release notes.
- **Flatpak:** a Flathub package is prepared. [docs/INSTALL.md](docs/INSTALL.md) has its status,
  checksums, and what each package can and cannot do.
- The AI model is not in the installer. You download it from the app only if you want it.

## Desktop app

1. Drop one or more PDFs on the window, or click **Select PDF**.
2. Each book shows its progress. When it is done, open it in your e-reader, preview it, or read the
   conversion report.
3. If the title, author or chapter list is wrong, correct it and choose **Fix and rebuild**. This
   takes a few seconds.

By default every EPUB is saved in an `OpenConvert` folder in your Documents, and the folder button at
the top of the window opens it. Under **Settings › Output folder** you can pick another folder or save
each EPUB next to its PDF. An existing book is never overwritten. Your earlier books are listed under
**Previous conversions** on the main page.

If a very large book stops with a time-limit message, raise **Time limit per step** under
**Settings › Advanced** (30 minutes by default). The app is available in English, German and Turkish.

## Command line

`openconvert` is the engine the desktop app runs. There is no separate download for it yet, so
[build it from source](#build-from-source) to use it on its own.

```sh
openconvert convert book.pdf                  # writes book.epub and book.epub.report.json next to the PDF
openconvert convert book.pdf -o out.epub      # choose where the EPUB goes
openconvert convert book.pdf --lang de        # set the book's language instead of detecting it
openconvert validate book.epub                # check an EPUB with the built-in validator
openconvert inspect book.pdf                  # what the PDF contains, page by page
openconvert --help                            # every command and option
```

## AI assistance (optional)

AI assistance is **off by default**, and a conversion is complete without it. When you turn it on, a
language model helps with the few decisions the rules cannot make on their own:

- it fills in the title and author when the PDF file itself does not give a title, accepting only
  text that is actually printed on the first pages;
- in Quality mode, it identifies opening pages the rules could not (title page, copyright page,
  dedication and so on).

Every answer is checked before it is used. If it does not fit, the rule-based result stays. The
report lists each AI decision next to what the rules alone would have chosen.

**Where the model runs** (Settings › Provider):

- **Built-in:** the app runs a small model on your computer. The default is Qwen3 1.7B
  (Apache-2.0, a 1.28 GB download you start yourself).
- **Ollama** running on your computer.
- **Custom endpoint:** any OpenAI-compatible server. If it is not on your computer, the app asks for
  your consent before any text from your books is sent to it.

**AI mode** (Settings › AI assistance):

- **Quality** (default, slower): the model thinks each answer through and every question is asked
  twice. Only answers that agree are used.
- **Fast:** short answers and a shorter time budget.

From the command line:

```sh
openconvert model pull qwen3-1.7b-q4_k_m                          # download the default model once
openconvert convert book.pdf --ai                                 # Quality mode
openconvert convert book.pdf --ai --ai-mode fast
openconvert convert book.pdf --ai --llm-provider ollama --llm-model <name>
```

Further experimental tasks (heading levels, where front and back matter begin, verse or quotation)
run only with `--ai-all-tasks`.

## Scanned books

PDFs that already have a text layer use that text. Pages that are only an image are read with
[Tesseract](https://github.com/tesseract-ocr/tesseract) 5 if it is installed on your system.
OpenConvert does not include it. Without it, those pages stay images in the EPUB and the report says
so.

- **Linux (Debian/Ubuntu):** `sudo apt install tesseract-ocr`, plus the language data you need, for
  example `tesseract-ocr-deu`
- **macOS:** `brew install tesseract tesseract-lang`
- **Windows:** the [UB-Mannheim Tesseract installer](https://github.com/UB-Mannheim/tesseract/wiki);
  select the extra languages you need while installing

On the command line, `--ocr auto|never|always` and `--ocr-lang` (for example `deu+eng`) control it.

## Privacy and security

- No telemetry, no crash reporting, no account.
- Without AI assistance, a conversion never uses the network. With the built-in model or a local
  Ollama, nothing leaves your computer.
- The app goes online only when you download a model, when you press **Check for updates**, or when
  you use an AI server that is not on your computer and have agreed to send text to it.
  **Settings › Network log** lists every connection it made.
- Updates are installed only after their signature has been verified.

OpenConvert treats every PDF as untrusted: it checks size limits before doing the work, stops a step
that runs too long, and on recent Linux kernels confines the converter in a sandbox. Details are in
[docs/SECURITY.md](docs/SECURITY.md).
Please report vulnerabilities privately through GitHub Security Advisories.

## Known limitations

- No macOS download yet.
- The Windows installers are not code-signed.
- Not every book converts perfectly yet. Complex layouts and scans with a poor text layer can still
  lose some text or structure. Every conversion's report says how much of the text reached the EPUB,
  and what was removed and why.
- The AI tasks have not yet passed the project's formal accuracy evaluation. Every answer is still
  checked, and AI stays off unless you turn it on.
- The optional full EPUBCheck validation is not built into the app yet. The built-in validator checks
  every EPUB.

## Build from source

You need Rust 1.98 and Node 22, plus the [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/)
for the desktop app. On Debian or Ubuntu:

```sh
sudo apt-get install build-essential pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev \
  librsvg2-dev libayatana-appindicator3-dev
```

The command-line tool, from the repository root:

```sh
cargo run -p xtask -- vendor-pdfium            # the pinned PDFium build, checked against its SHA-256
cargo build -p openconvert --release
./target/release/openconvert convert book.pdf
```

The desktop app (Tauri 2 and Svelte 5, in `apps/desktop`):

```sh
cargo run -p xtask -- fetch-llama-server       # the model server the app bundles
cargo build -p openconvert
cargo run -p xtask -- stage-sidecars           # put both, with PDFium, where the bundler expects them
npm ci --prefix apps/desktop/ui
cargo install tauri-cli --version "=2.11.5" --locked
cd apps/desktop/src-tauri && cargo tauri dev
```

## Help and contributing

- **Problems and ideas:** open an [issue](https://github.com/egeselbzgyk/openconvert/issues). In the
  app, **Settings › About & updates › Report a problem** creates a diagnostic bundle that you review
  before sharing it. Nothing is sent automatically.
- **Documentation:** [installing](docs/INSTALL.md), [release notes](docs/CHANGELOG.md),
  [security](docs/SECURITY.md), [how the converter works](docs/ARCHITECTURE.md) and
  [its stages](docs/PIPELINE.md), [design decisions](docs/DECISIONS.md).
- **Contributing:** [CLAUDE.md](CLAUDE.md) describes how work is done here: test-first, with the
  rules CI enforces. Build the test PDFs with `cargo run -p xtask -- fixtures` and
  `cargo run -p xtask -- fixtures --keep-structtree`, then run
  `cargo nextest run --workspace --exclude openconvert-desktop` and `npm test --prefix apps/desktop/ui`.

## License

Apache-2.0: see [LICENSE](LICENSE) and [NOTICE](NOTICE). Third-party licenses are listed in
[docs/LICENSE_AND_DEPENDENCIES.md](docs/LICENSE_AND_DEPENDENCIES.md).
