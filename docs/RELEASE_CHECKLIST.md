# Release checklist

PHASE 15 detail 9. Every item is binary. Where a machine can check an item, `.github/workflows/
release.yml` does, in a step named for the plan's row; this list says which, and what a person does
when there is nothing to automate. A solo maintainer's memory is not a release process.

Run top to bottom for every release. A red item blocks the release; it is fixed, not waived.

## Before tagging

- [ ] **Every phase's Definition of Done is ticked** in `PROGRESS.md`, and Appendix D of
      `IMPLEMENTATION_PLAN.md` is evaluated item by item. *(manual)*
- [ ] **No placeholder remains.** `cargo run -p xtask -- ci-lint --release-branch` is clean: no
      `TODO_` in `models.toml`, `packs.toml`, `thresholds.toml` or `tauri.conf.json`, and no
      `thresholds.toml` entry whose `review_by` has passed. *(release job: `gates`, row 15.18)*
- [ ] **The version bumps are right.** `cargo run -p xtask -- bump-rules-check --tag vX.Y.Z` is clean
      (`docs/VERSIONING.md`); `Cargo.toml` and `tauri.conf.json` carry `X.Y.Z`. *(release job:
      `gates`, rows 15.16/15.17)*
- [ ] **`docs/CHANGELOG.md` has a `## [X.Y.Z]` section**, which becomes the release notes and the
      updater's `notes`; it lists every `thresholds.toml` value change (D17: `PROVENANCE` is in every
      report). *(release job: `publish` refuses an empty section)*
- [ ] **`docs/MODEL_GATE.md` is regenerated** from `eval/results/model_gate/` and G1–G9 are recorded
      for the default model. *(manual; `eval/`)*
- [ ] **`cargo deny --all-features check` is clean.** *(CI: `deny`)*
- [ ] **EPUBCheck reports zero errors on the corpus, Ace zero serious violations.** *(CI:
      `epubcheck`; nightly: `full-corpus`, `ace-a11y`)*
- [ ] **The conversion suite is green under `unshare -n`.** *(CI: `no-network`)*

## The release job (`git tag vX.Y.Z && git push origin vX.Y.Z`)

- [ ] **macOS: every nested Mach-O signed with one Team ID, the deep strict verification passes,
      Gatekeeper accepts the app, and the app and the dmg carry a stapled ticket.** *(rows 15.1–15.4)*
- [ ] **Windows: NSIS and MSI produced and hashed.** *(row 15.6)*
- [ ] **Linux: the AppImage converts a book headless.** *(row 15.7)*
- [ ] **Every installer is within budget** (`release.max_installer_bytes`). *(row 15.15)*
- [ ] **The SBOM is valid CycloneDX 1.6 and lists every vendored native with its SHA-256.**
      *(rows 15.11, 15.12; attached to the release)*
- [ ] **`--no-ai` output is byte-identical on all three OSes.** *(row 15.13)*
- [ ] **The Flathub manifest passes `flatpak-builder-lint`** and has no network permission.
      *(`flatpak-lint`; row 15.8 in `gates`)*
- [ ] **The updater manifest is signed and verifies against the key the app ships.** *(`publish`,
      row 15.9 on the release key)*
- [ ] **Every artefact's SHA-256 is in the release body**, checked on the assets as downloaded back.
      *(row 15.20)*

## After the job, before publishing the draft

- [ ] **A fresh-VM install-and-convert smoke on all three OSes** (row 15.19): on a clean VM per OS,
      download the draft's asset and run `packaging/smoke/fresh-install.sh <AppImage|dmg> <book.pdf>
      <sha256 from the notes>` (Linux, macOS) or `packaging\smoke\fresh-install.ps1 -Installer …
      -Pdf … -Sha256 …` (Windows): it checks the hash, installs, converts through the installed app's
      `--smoke-convert` and checks the EPUB. Watch the Windows run for a console window flashing (there
      must be none), run the Windows installer by hand once to record what SmartScreen shows, and drop
      a PDF on the window by hand once on each OS. *(manual + scripted)*
- [ ] **A fresh install accepts the update**: install the *previous* release, let it find this one
      (Settings → check for updates), and see it verify, install and restart. *(manual)*
- [ ] **Publish the draft.**
- [ ] **Record what shipped**: `cargo run -p xtask -- bump-rules-check --record vX.Y.Z`, and commit
      `docs/releases/baseline.toml` together with `docs/releases/X.Y.Z/` — the three repro hash
      tables, the SBOM and the release manifests from the job's artefacts. *(manual; one commit)*

## Keys and signing — what exists, where, and what not to do

**Updater key (Ed25519, minisign format).** Generated once, by the maintainer, on their own machine:
`cargo tauri signer generate -w ~/.tauri/openconvert.key`. The private key and its password go only
into the repository secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; the
public key goes into `apps/desktop/src-tauri/tauri.conf.json` at `plugins.updater.pubkey`, replacing
`TODO_UPDATER_PUBKEY`.

- Public key fingerprint (the minisign key id): **not generated yet** — record it here when it is.
- **Rotating the key invalidates the update path for every existing install**: an installed app
  verifies updates against the key it was built with, so after a rotation it refuses every update and
  its users must reinstall by hand. Rotate only when the private key is compromised, and say so in
  the release notes of the last release signed with the old key.

**macOS (Developer ID Application + notarization).** Secrets `APPLE_CERTIFICATE` (base64 .p12),
`APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_TEAM_ID`,
`APPLE_APP_PASSWORD` (app-specific). `packaging/macos/sign_nested.sh` signs every Mach-O inside out;
`packaging/macos/entitlements.plist` stays empty — never add `disable-library-validation` or
`allow-jit` (SECURITY §5; row 15.5 fails the build if anyone does).

**Windows (unsigned in v1, D12).** SmartScreen warns on first run; `docs/INSTALL.md` says so and says
why. Adding Azure Artifact Signing later is **a workflow secret plus one signing step** after
`cargo tauri build` in the Windows leg of `release.yml` (sign the `.exe` and `.msi`, then re-sign the
NSIS updater payload with `cargo tauri signer sign`), and **requires no bundler change** — which is
recorded here so the change is not re-litigated when it comes.

## Known release blockers (as of 2026-09-23, Phase 15 part A)

- `models.toml` model pins are `TODO_` (huggingface.co is unreachable from the machine that built
  Phase 9), so `ci-lint --release-branch` fails.
- `packs.toml`'s validation pack is unbuilt (`TODO_`) and its JRE licence (VD-f) unverified.
- The updater keypair has not been generated (`TODO_UPDATER_PUBKEY`).
- The Linux AppImage is 112.7 MB against the 45 MB budget (WebKitGTK); a maintainer decision.
- Nothing in `release.yml` beyond its unit tests has run: GitHub Actions, macOS, Windows and the
  certificates were not available.
