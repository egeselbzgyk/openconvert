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

## Checklist run: 2026-09-23, v1.0.0 preparation (Phase 15, Linux build machine)

The list above, run once on the machine Phase 15 was built on — no GitHub Actions, no macOS or
Windows, no certificates, no VMs. `[x]` is a pass seen here; `[ ]` is not met, or not checkable here,
and says which. **v1.0.0 is not releasable from this state.**

Before tagging:
- [ ] Every phase's DoD ticked — **no:** Phase 7.5 is parked; Appendix D fails (PROGRESS.md).
- [ ] No placeholder — **no:** `ci-lint --release-branch` finds 15: 8 model pins in `models.toml`
      (huggingface.co unreachable here), 6 in `packs.toml` (validation pack unbuilt), 1 updater key.
      No lapsed `review_by`. *(Since then, 2026-09-23: `models.toml` is pinned
      (`fix/phase-09-registry-pins`), and the gate finds 7.)*
- [x] Version bumps — `bump-rules-check` clean against the unreleased baseline (re-recorded after
      Phase 14; unchanged). The tree is still 0.1.0: bump to 1.0.0 when tagging.
- [x] `## [1.0.0]` section — present, Security section written from Phase 14's evidence;
      `xtask release changelog --version v1.0.0` accepts it.
- [ ] `docs/MODEL_GATE.md` regenerated with G1–G9 — **no:** no model could be downloaded here.
- [x] `cargo deny --all-features check` clean (and the tooling policy).
- [ ] EPUBCheck zero errors on the corpus, Ace zero serious — **not run here on the corpus:** EPUBCheck
      is zero on the fixtures (Phase 5); the corpus and Ace runs are CI/nightly jobs, unverified here.
- [x] Conversion suite green under `unshare -n` — here, including the `--ai` cassette path (Phase 14,
      row 14.20); the CI job itself unverified.

The release job:
- [ ] macOS signing, notarization, Gatekeeper, stapling (rows 15.1–15.4) — **unverified here.**
- [ ] Windows NSIS + MSI produced and hashed (row 15.6) — **unverified here.**
- [x] Linux AppImage converts a book headless (row 15.7) — the AppImage built here, after Phase 14's
      merge, converts f01 under `xvfb-run` with Landlock applied (ABI 7) and `RLIMIT_AS` in force.
- [ ] Installers within budget (row 15.15) — **no:** the AppImage is 112 953 848 bytes against
      45 000 000.
- [x] SBOM valid CycloneDX 1.6 with every vendored native (rows 15.11, 15.12) — generated and
      validated here; attaching it is the release job's.
- [ ] `--no-ai` byte-identical on three OSes (row 15.13) — **Linux half only:** identical across
      working directory, time zone, locale and build profile; cross-OS unverified.
- [ ] `flatpak-builder-lint` — **unverified here** (no flatpak tooling); the manifest's no-network rule
      passes (row 15.8).
- [ ] Updater manifest signed with the release key and verified (row 15.9, real key) — **no:** the
      keypair has not been generated; the verification logic passes with an in-test key.
- [ ] Every artefact's SHA-256 in the release body (row 15.20) — **no release exists.**

After the job:
- [ ] Fresh-VM smoke on three OSes (row 15.19) — **not run:** no VMs; `fresh-install.sh` passes against
      the real AppImage on this machine.
- [ ] A fresh install accepts the update — **not run.**
- [ ] Publish; record what shipped — **not reached.**

## Known release blockers (as of 2026-09-23, after Phase 14 and Phase 15)

- `docs/MODEL_GATE.md` has no G1–G9 for the default model. (`models.toml` is pinned since 2026-09-23 and
  the default model's download was verified against its pin; the other three have not been
  downloaded.)
- `packs.toml`'s validation pack is unbuilt (`TODO_`) and its JRE licence (VD-f) unverified.
- The updater keypair has not been generated (`TODO_UPDATER_PUBKEY`).
- The Linux AppImage is 112.95 MB against the 45 MB budget (WebKitGTK); a maintainer decision.
- Nothing in `release.yml` beyond its unit tests has run: GitHub Actions, macOS, Windows and the
  certificates were not available.
- Phase 7.5 (the reading corpus and conservation defects) is parked: I-1…I-7 do not yet hold on the
  whole corpus.
- Two of SECURITY §4's caps are not enforced: there is no Windows memory cap (P14-b), and "max output
  size" has no value and no cap (a 50 MiB warning only).
- The whole list, against Appendix D of the plan, is in `PROGRESS.md` › Blocked › "v1.0 — Appendix D".
