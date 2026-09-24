# Installing OpenConvert

OpenConvert 1.0 is available for **Windows and Linux**. A macOS version comes in a later 1.x
release: it needs an Apple Developer ID to be signed and notarized, and the project does not have one
yet. Until then, macOS users can build the converter and the app from source (`README.md`), unsigned.

Download the file for your system from the project's GitHub Releases page. Every file's SHA-256 is
listed at the end of the release notes; to check a download, compare it with

- Linux: `sha256sum OpenConvert_*.AppImage`
- Windows (PowerShell): `Get-FileHash .\OpenConvert_*-setup.exe -Algorithm SHA256`

The release also carries an SBOM (`openconvert-<version>.cdx.json`, CycloneDX 1.6) listing every
component inside the app, including PDFium and llama.cpp with their exact builds.

What you install is the app, the converter and the libraries it needs. The AI model is **not** in the
installer: it is an optional download you choose in the app, with its size and licence shown first.
Scanned pages are read with Tesseract when your system has it installed. The optional validation
pack (the full EPUBCheck) is not offered in 1.0 and comes in a later release; the built-in
validator checks every EPUB either way. Converting a book needs none of these and never uses the
network.

## Windows

Run `OpenConvert_<version>_x64-setup.exe` (installs for your user only, no administrator rights) or
use the `.msi`.

**Windows will warn you.** This version of OpenConvert is not code-signed, so the first time you run
the installer Microsoft Defender SmartScreen shows "Windows protected your PC". That warning means
"Microsoft does not recognise who published this file", not "this file is harmful". To continue,
choose **More info**, then **Run anyway**. If you would rather check first, compare the file's
SHA-256 with the one in the release notes (above). Code signing is planned once releases are regular;
until then we say so rather than work around it.

## macOS

Not in 1.0: a signed and notarized macOS app (a `.dmg` for Apple silicon and Intel) comes in a later
1.x release.

## Linux

**AppImage (recommended).** Make it executable and run it:

```sh
chmod +x OpenConvert_<version>_amd64.AppImage
./OpenConvert_<version>_amd64.AppImage
```

If your system has no FUSE (some containers and minimal installs), run it with
`APPIMAGE_EXTRACT_AND_RUN=1`. The AppImage updates itself when you ask it to (Settings), and only
after the update's signature has verified.

**Flatpak (Flathub).** Once Flathub has accepted it (the manifest is
`packaging/linux/flatpak/io.github.egeselbzgyk.OpenConvert.yml`):
`flatpak install flathub io.github.egeselbzgyk.OpenConvert`. Flathub keeps it up to date; the in-app updater is not part of this build. The Flatpak runs without network access — a
conversion never needs it — so inside the Flatpak the app cannot download an AI model, and
AI assistance through Ollama on your computer is not reachable either. Use the AppImage if you want
those.

The AppImage is larger than the Windows installer (about 133 MB) because it carries its own copy of
the WebKitGTK web engine, so it runs the same on every distribution.

`.deb` and `.rpm` packages are not provided for this version.

## Checking that it works

Every build can convert a book without its window, which is also how releases are tested:

```sh
./OpenConvert_<version>_amd64.AppImage --smoke-convert /path/to/book.pdf
```

writes `book.epub` beside the PDF (never over an existing file) and exits with 0 on success.

## Your data

OpenConvert keeps its settings and temporary files in your user's application-data folder and
nothing anywhere else. There is no telemetry and no crash reporting. Network access happens only when
you download a model, when you choose an AI provider off your computer (after you consent
to that host), and when you check for updates.
