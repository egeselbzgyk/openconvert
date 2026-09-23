# Release checklist

PHASE 15 detail 9. Every item is binary. Where a machine can check an item, `.github/workflows/
release.yml` does, in a step named for the plan's row; this list says which, and what a person does
when there is nothing to automate. A solo maintainer's memory is not a release process.

Run top to bottom for every release. A red item blocks the release; it is fixed, not waived.

**Platforms.** v1.0.0 ships for **Windows and Linux**. macOS comes in a later 1.x, once there is an
Apple Developer ID (maintainer decision 2026-09-23; D12 amendment): until then `release.yml` has no
macOS leg, and the macOS items below are marked *(macOS, later 1.x)* and are not part of a release.

## Before tagging

- [ ] **Every phase's Definition of Done is ticked** in `PROGRESS.md`, and Appendix D of
      `IMPLEMENTATION_PLAN.md` is evaluated item by item. *(manual)*
- [ ] **No placeholder remains.** `cargo run -p xtask -- ci-lint --release-branch` is clean: no
      `TODO_` in `models.toml`, `packs.toml`, `thresholds.toml` or `tauri.conf.json`, and no
      `thresholds.toml` entry whose `review_by` has passed. *(release job: `gates`, row 15.18)* The
      last one before 1.0.0 is the updater public key — see "Keys and signing" below.
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

- *(macOS, later 1.x)* **macOS: every nested Mach-O signed with one Team ID, the deep strict
  verification passes, Gatekeeper accepts the app, and the app and the dmg carry a stapled ticket.**
  *(rows 15.1–15.4; not in `release.yml` until macOS ships — the scripts are in `packaging/macos/`)*
- [ ] **Windows: NSIS and MSI produced and hashed.** *(row 15.6)*
- [ ] **Linux: the AppImage converts a book headless.** *(row 15.7)*
- [ ] **Every installer is within its budget**: the Linux AppImage within
      `release.max_linux_installer_bytes` (120 MB — it carries WebKitGTK; maintainer decision
      2026-09-23), every other installer within `release.max_installer_bytes` (45 MB, D12).
      *(row 15.15)*
- [ ] **The SBOM is valid CycloneDX 1.6 and lists every vendored native with its SHA-256.**
      *(rows 15.11, 15.12; attached to the release)*
- [ ] **`--no-ai` output is byte-identical on every OS the release ships** (Linux and Windows for
      1.0; `ci.yml` still tests macOS on every push to `main`). *(row 15.13)*
- [ ] **The Flathub manifest passes `flatpak-builder-lint`** and has no network permission.
      *(`flatpak-lint`; row 15.8 in `gates`)*
- [ ] **The updater manifest is signed and verifies against the key the app ships.** *(`publish`,
      row 15.9 on the release key)*
- [ ] **Every artefact's SHA-256 is in the release body**, checked on the assets as downloaded back.
      *(row 15.20)*

## After the job, before publishing the draft

- [ ] **A fresh-VM install-and-convert smoke on every shipped OS** (row 15.19; Windows and Linux for
      1.0): on a clean VM per OS,
      download the draft's asset and run `packaging/smoke/fresh-install.sh <AppImage> <book.pdf>
      <sha256 from the notes>` (Linux) or `packaging\smoke\fresh-install.ps1 -Installer …
      -Pdf … -Sha256 …` (Windows): it checks the hash, installs, converts through the installed app's
      `--smoke-convert` and checks the EPUB. Watch the Windows run for a console window flashing (there
      must be none), run the Windows installer by hand once to record what SmartScreen shows, and drop
      a PDF on the window by hand once on each OS. *(manual + scripted)* For 1.0.0 there is no
      previous release, so the update item below is first checked at 1.0.1.
- [ ] **A fresh install accepts the update**: install the *previous* release, let it find this one
      (Settings → check for updates), and see it verify, install and restart. *(manual)*
- [ ] **Publish the draft.**
- [ ] **Record what shipped**: `cargo run -p xtask -- bump-rules-check --record vX.Y.Z`, and commit
      `docs/releases/baseline.toml` together with `docs/releases/X.Y.Z/` — the repro hash table of
      each shipped OS, the SBOM and the release manifests from the job's artefacts. *(manual; one
      commit)*

## Keys and signing — what exists, where, and what not to do

**Updater key (Ed25519, minisign format).** Generated once, by the maintainer, on their own machine
(maintainer decision 2026-09-23) — never in CI, never by anyone else, never in this repository:

1. In a directory **outside** the repository clone (the file must never be committed):
   `npx @tauri-apps/cli signer generate -w openconvert.key` — it asks for a password; choose a
   strong one. It writes the private key `openconvert.key` and the public key `openconvert.key.pub`.
   Back both up somewhere safe and offline: losing the private key ends the update path (below).
2. In the GitHub repository, *Settings → Secrets and variables → Actions → New repository secret*:
   - `TAURI_SIGNING_PRIVATE_KEY` = the whole content of `openconvert.key`;
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` = the password from step 1.
   `release.yml`'s build job fails by name, before building, when the first is missing.
3. The public key goes into **`apps/desktop/src-tauri/tauri.conf.json`, field
   `plugins.updater.pubkey`**: set it to the whole content of `openconvert.key.pub` (one base64
   line; it replaced the placeholder `TODO_UPDATER_PUBKEY`), and commit that on `main`. Nothing else holds the key:
   the app verifies updates against it, and `xtask release verify-latest` (row 15.9) reads it from
   there. Then `cargo run -p xtask -- ci-lint --release-branch` is clean.
4. Record the key id here: it is the last word of the first line of
   `base64 -d openconvert.key.pub` ("untrusted comment: minisign public key: <KEY ID>").

- Public key fingerprint (the minisign key id): **`0C6C69CA122C11B0`** (Tauri prints it as
  `C6C69CA122C11B0`). Generated by the maintainer on their own machine, 2026-09-23; the public key is
  in `tauri.conf.json` (step 3 done), and `the_shipped_updater_key_is_the_maintainers_minisign_key`
  fails if it changes. Steps 1 and 2 — the private key and its password as repository secrets — are
  the maintainer's, and `release.yml` fails by name without the first.
- **Rotating the key invalidates the update path for every existing install**: an installed app
  verifies updates against the key it was built with, so after a rotation it refuses every update and
  its users must reinstall by hand. Rotate only when the private key is compromised, and say so in
  the release notes of the last release signed with the old key.

**macOS (Developer ID Application + notarization) — for the later 1.x that ships macOS; not used by
1.0.0.** Secrets `APPLE_CERTIFICATE` (base64 .p12),
`APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_TEAM_ID`,
`APPLE_APP_PASSWORD` (app-specific). `packaging/macos/sign_nested.sh` signs every Mach-O inside out;
`packaging/macos/entitlements.plist` stays empty — never add `disable-library-validation` or
`allow-jit` (SECURITY §5; row 15.5 fails the build if anyone does).

**Windows (unsigned in v1, D12).** SmartScreen warns on first run; `docs/INSTALL.md` says so and says
why. Adding Azure Artifact Signing later is **a workflow secret plus one signing step** after
`cargo tauri build` in the Windows leg of `release.yml` (sign the `.exe` and `.msi`, then re-sign the
NSIS updater payload with `cargo tauri signer sign`), and **requires no bundler change** — which is
recorded here so the change is not re-litigated when it comes.

## Checklist run: 2026-09-23, v1.0.0 release preparation (`release/v1.0.0-prep`, Linux)

After the maintainer's release decisions of 2026-09-23 (Windows + Linux only; validation pack deferred;
AppImage budget 120 MB; updater key generated by the maintainer) — `docs/DECISIONS_LOG.md`. Run on the
Linux build machine; GitHub Actions now runs `ci.yml` on pushes to `main`, and `release.yml` runs when the
maintainer tags. **The maintainer has chosen to ship 1.0.0 with the gaps marked "known limitation"
open**; each is in the 1.0.0 notes' "Known limitations" and in `PROGRESS.md` › Blocked.

Before tagging:
- [ ] Every phase's DoD ticked — **no, a known limitation:** Phase 7.5 is parked (79 of 104 corpus
      documents clean); Appendix D does not fully pass (`PROGRESS.md`).
- [x] No placeholder — `ci-lint --release-branch` clean: `models.toml` pinned, the validation pack a
      `[[deferred]]` entry without pins, the updater public key the maintainer's (key id
      `0C6C69CA122C11B0`). No lapsed `review_by`.
- [x] Version bumps — the tree is 1.0.0 (`Cargo.toml`, `tauri.conf.json`); `bump-rules-check --tag
      v1.0.0` clean (drift is a note until the release records the baseline).
- [x] `## [1.0.0]` section — with "Known limitations"; `xtask release changelog --version v1.0.0`
      accepts it and it holds no `TODO_`.
- [ ] `docs/MODEL_GATE.md` regenerated with G1–G9 — **no, a known limitation:** AI is off by default and
      no task is enabled for any language; the gates need reference machines L and M.
- [x] `cargo deny --all-features check` clean, and the tooling policy.
- [ ] EPUBCheck zero errors on the corpus, Ace zero serious — **CI/nightly**; zero errors on the
      fixtures is a `ci.yml` job, green on `main` (run #44).
- [x] Conversion suite green under `unshare -n` — here (Phase 14); `ci.yml`'s `no-network` job on `main`.

The release job — **not run until the maintainer tags** (it needs `TAURI_SIGNING_PRIVATE_KEY` and its
password as repository secrets):
- *(macOS, later 1.x)* rows 15.1–15.4 — not part of 1.0.0.
- [ ] Windows NSIS + MSI produced and hashed (row 15.6) — the release job's.
- [x] Linux AppImage converts a book headless (row 15.7) — here, Phase 15 (P15.20).
- [x] Installers within budget (row 15.15) — the AppImage measured 112 953 848 bytes against its
      120 000 000 budget; the Windows installers are measured by the release job against 45 000 000.
- [x] SBOM valid CycloneDX 1.6 with every vendored native (rows 15.11, 15.12) — here, Phase 15.
- [ ] `--no-ai` byte-identical on Linux and Windows (row 15.13) — the release job's, over the fast
      corpus; `ci.yml`'s `epub-bytes` (one fixture, three OSes) is green on `main` (run #44), and
      under Wine all ten fixture hashes matched Linux.
- [ ] `flatpak-builder-lint` — the release job's.
- [ ] Updater manifest signed and verified against the shipped key (row 15.9) — the release job's; the
      shipped key parses (`the_shipped_updater_key_is_the_maintainers_minisign_key`).
- [ ] Every artefact's SHA-256 in the release body (row 15.20) — the release job's.

After the job: the fresh-VM smoke on Windows and Linux (row 15.19), then publish and record — the
maintainer's. There is no previous release, so the update item is first checked at 1.0.1.

## Earlier checklist run: 2026-09-23, v1.0.0 preparation (Phase 15, Linux build machine)

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

## Known release blockers (as of 2026-09-23, release preparation)

Resolved by the maintainer's decisions of 2026-09-23: the updater keypair (generated; public key in
`tauri.conf.json`), the validation pack (deferred past 1.0), the AppImage budget (120 MB), macOS (later
1.x). What remains before `git tag v1.0.0`:

- The repository secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` must be set,
  or the release job's build legs stop at their first step.
- ~~The updater endpoint~~ — settled 2026-09-23: `tauri.conf.json` `plugins.updater.endpoints` and
  `Cargo.toml` `repository` name `github.com/egeselbzgyk/openconvert`, where the releases are published
  (DECISIONS_LOG 2026-09-23).
- Nothing in `release.yml` beyond its unit tests has run yet: the Windows installers, the cross-OS
  reproducibility comparison, `flatpak-builder-lint` and the publication run first on the tag.

Shipped as known limitations (the 1.0.0 notes): Phase 7.5 parked (I-1…I-7 not yet on the whole
corpus); no max-output-size cap and no Windows memory cap; AI off by default, no task enabled, G1–G9
not run; Isartor and the nightly fuzzing not yet run; Windows unsigned (D12). The whole list, against
Appendix D: `PROGRESS.md` › Blocked › "v1.0 — Appendix D".
