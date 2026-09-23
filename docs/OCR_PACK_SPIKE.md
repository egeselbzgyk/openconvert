# OCR pack — spike checklist (D4, PHASE 13 detail 14)

**Status: deferred, not started.** v1 uses the *user's* Tesseract 5 (Homebrew, apt, UB-Mannheim),
found by `oc_core::ocr::discover`, and degrades to page images with a copy-pasteable install hint
when there is none (`W_OCR_ENGINE_MISSING`). This document is the commitment D4 asks for: what the
signed OCR pack is, how it would be built and shipped, and the go/no-go criteria that decide whether
it ships. Nothing here has been built or measured; every number below is a target, not a result.

## What the pack is

One download per platform, fetched on the user's request by the same mechanism that fetches a model
(Phase 9: `oc-net`'s allowlisted, SHA-256-verified, atomically-placed downloader), containing:

- a statically linked `tesseract` 5.x executable (Leptonica linked in, nothing else dynamic beyond
  the OS's own C runtime);
- `tessdata_fast` traineddata for `eng`, `deu`, `tur` and `osd` (~1.5 MB each for the three
  languages; Apache-2.0, redistributable);
- `LICENSE` (Apache-2.0, Tesseract) and the Leptonica BSD-2 notice, and a `NOTICE` naming both.

The engine treats a pack install as one more discovery source, tried **after** `--ocr-path` and
before `PATH`, so a user's own Tesseract still wins when they point at it.

## Build recipe (to be proven)

- [ ] **vcpkg static triplets**, one CI job per OS:
      `x64-windows-static-md`, `x64-linux`, `arm64-osx` (plus `x64-osx` if Intel macOS is still
      supported at release time).
- [ ] Ports: `leptonica` and `tesseract` only. Disabled: `libcurl` (Tesseract's URL input — the pack
      must not be able to open a socket, D13.9), `libarchive`, the training tools, OpenCL, and every
      image codec Tesseract does not need for PNG input (the engine only ever hands it a PNG).
- [ ] The versions pinned in a lock file beside `xtask/llama.lock`, bumped only by a reviewed commit.
- [ ] Reproducibility: two CI runs of the recipe on the same pin produce byte-identical archives, or
      the difference is explained and accepted in `docs/DECISIONS_LOG.md`.
- [ ] Smoke test in the recipe's own job: `tesseract --version` reports the pinned version,
      `--list-langs` lists exactly `deu eng osd tur`, and `ocr_tesseract.rs` passes against the
      packed binary (`--ocr-path`).

## Declaration and download

- [ ] `packs.toml`: per OS, the archive URL on an allowlisted host (the project's GitHub Releases),
      `sha256`, `size_bytes`, the Tesseract and Leptonica versions, and the licence ids — the
      `models.toml` shape, read by the same registry code.
- [ ] Download, verification and placement reuse Phase 9's `oc_net::download` **verbatim**: no second
      downloader, no second verifier.
- [ ] `openconvert ocr pull|list|remove`, mirroring `openconvert model`.
- [ ] Discovery's trust rules apply unchanged to the unpacked binary: absolute path, named
      `tesseract[.exe]`, not group- or world-writable, major version ≥ 5.

## Signing and platform exposure

- [ ] **macOS:** the packed Mach-O is signed with the same Developer ID as the app and notarized. A
      downloaded, unsigned binary will not execute under the Hardened Runtime (SECURITY §5), so this
      is not optional there. The notarization step runs in CI with no manual action per release.
- [ ] **Windows:** SmartScreen reputation. An unsigned `tesseract.exe` spawned by the engine is not
      shown SmartScreen's download prompt (it is not the user launching it), but it is exposed to
      antivirus heuristics; record what Defender does with it on a clean VM. Signing follows the
      app's own decision (D12: v1 ships unsigned and says so).
- [ ] **Linux:** none beyond the SHA-256 pin.

## Go / no-go criteria

The pack ships only when **all three** hold:

1. **≤ 25 MB per OS**, compressed, languages included.
2. **A reproducible CI build recipe**: the checklist above, green on all target triplets, from a
   pinned lock, with the smoke test passing against the packed binary.
3. **A signing step that costs no manual work per release** (macOS notarization automated in CI).

If any one fails, the pack stays deferred and v1's behaviour — the user's own Tesseract, or page
images and an install hint — stands. The decision, with the measured size and the CI run that
proved (or failed) the recipe, is recorded in `docs/DECISIONS_LOG.md`.

## Explicitly not in the pack

- Any other OCR engine (PP-OCR, `ocrs`, platform OCR): post-v1 extension points behind
  `oc_core::ocr::invoke::OcrEngine`.
- `tessdata_best` models (≈ 10× larger; no accuracy evidence that justifies the size for book type).
- LLM post-correction of OCR output — never (D16, R10 §6.14).
