# PROGRESS — OpenConvert

<!-- Machine-readable state. Claude Code reads this first and rewrites it after every completed work item. -->

STATUS: IN_PROGRESS
CURRENT_PHASE: 12
CURRENT_ITEM: Phase 12 — Desktop UI (being built on `phase/12-desktop-ui`). Phase 11 is complete
              and merged (2026-09-23); what Phase 12 must wire from it is in the Phase 11 section.
              Phase 7.5 is still parked.
LAST_UPDATED: 2026-09-23

---

## How to use this file

- `STATUS` is one of `IN_PROGRESS` · `BLOCKED` · `COMPLETE`.
- Set `STATUS: BLOCKED` **only** when a decision is needed that `docs/DECISIONS.md` does not settle.
  Write the question under `## Blocked` and stop.
- Tick a phase box only when its full Definition of Done (`IMPLEMENTATION_PLAN.md` §0.3) passes.
- Keep `## Notes` short: what a fresh session needs in order to resume, nothing else.

---

## Phases

- [x] **Phase 0** — Repo, workspace, CI, thresholds, hello-Tauri, first Typst fixtures
      *(First Milestone met; opened the Verification-debt table VD-a…VD-g)*
- [x] **Phase 1** — PDF inspection and ingestion  *(VD-d closed)*
- [x] **Phase 2** — Text assembly and normalization  *(all 22 named tests green; EN frequency list ships, DE/TR blocked on a D15 licence decision)*
- [x] **Phase 3** — Layout  *(all 17 named tests green; VD-b closed. A3.2 and A3.3 partial — both want the Phase 7 corpus)*
- [x] **Phase 4** — Structure  *(all 21 named tests green, plus about fifty additions; A4.3 partial — heading F1 needs the Phase 7 corpus)*
- [x] **Phase 5** — EPUB generation, Tier-1 validator, EPUBCheck CI gate
      *(all 20 named tests green, plus about sixty additions. **EPUBCheck 5.3.0 reports 0 errors
      and 0 warnings on all ten fixtures.** A5.3 is a CI job that cannot run on one machine and is
      unverified until the first CI run.)*
- [x] **Phase 6** — Structural validation, repair loop, report, CI DOM checks
      *(all 16 named tests green, plus about sixty additions. **I-7 holds on all ten fixtures, zero
      repairs fire, and 198 Chromium assertions pass at three viewports.** The two rows that could
      not be verified on one machine are **cashed**: CI run 35462059355 is green on
      ubuntu/macos/windows and `dom-checks` passes. Both of them failed first, along with five other
      real defects the first CI run found — `docs/DECISIONS_LOG.md`, 2026-09-19. VD-f deferred to
      Phase 15 with its reason.)*
- [x] **Phase 7** — Corpus v1, eval harness, benchmarks, real-world holdout
      *(all 14 named tests green, plus about 180 additions. **104 frozen holdout documents
      across 17 291 pages and seven producer strata, `ours(*)` at 0.103, corpus lint clean,
      and 0.0217 s/page against D13.11's 0.5 budget.** The CI jobs this phase wrote cannot be
      verified on one machine and are unverified until the branch merges.)*
- [ ] **Phase 7.5** — Reading corpus and conservation defect closure  *(**parked 2026-09-23**
      by the maintainer's decision, to resume once every phase is implemented. Parked at: 79 of
      104 corpus documents clean, 2 869 characters lost across 13, duplication zero, 11 not
      finishing in 90 s. Six defect classes closed, each with an invariant.)*
- [x] **Phase 8** — AI abstraction (no real model yet)
      *(all 16 named tests green, plus 52 additions; built on `worktree-phase8` and merged into
      `main` 2026-09-23. The four gates, the call budget, the cache, one OpenAI-compatible client
      over a `Transport`, cassettes and the six escalation predicates. `ai.enabled` stays `false`
      and nothing in the pipeline calls a model yet. The CI steps it added are unverified until
      CI runs.)*
- [x] **Phase 9** — Local model integration: sidecar lifecycle, model manager, promotion gate
      *(all 20 named tests exist and pass, 9.15/9.16 behind `live-llm`; 33 Rust and 20 Python
      tests added; built on `phase/09-local-model` and merged into `main` 2026-09-23. No real model
      could be fetched here, because huggingface.co is refused by the sandbox's egress policy. So
      the registry pins, the live tests and every gate run are unverified and listed in the Blocked
      section. A follow-up (`fix/phase-09-llama-pins`) filled the llama.lock digests and checked
      them against downloads.)*
- [x] **Phase 10** — AI-assisted decisions (the four tasks)
      *(all 22 named tests exist and pass, 10.17/10.18 as pytest functions; 40 Rust and 6 Python
      tests added, plus one live test behind `live-llm`; built on `phase/10-ai-decisions` and merged
      into `main` 2026-09-23. `--no-ai` output is byte-identical to the pre-phase snapshot. No model
      is reachable here, so A10.4/A10.5 — McNemar and the false-repair rate — are unmeasured, the
      language maps ship empty, and every provisional decision is in the Blocked section.)*
- [x] **Phase 11** — BYO providers
      *(all 10 named tests exist and pass, plus 18 additions and one live test behind `live-llm`;
      built on `phase/11-byo-providers` and merged into `main` 2026-09-23. Consent that names the
      host, enforced in `oc-net`; Ollama through `/api/chat` with `num_ctx` always set; the
      cassette contract through every adapter; `openconvert provider detect|check|probe`. No real
      provider is reachable here: a live Ollama and a remote endpoint are unverified, and the
      provisional decisions are in the Blocked section.)*
- [ ] **Phase 12** — Desktop UI  *(includes the early signing/notarization dry run)*
- [x] **Phase 13** — OCR  *(VD-g closed. All 22 named tests green, plus 27 additions (21 Rust, 6 Python); built on
      `phase/13-ocr` and merged into `main` 2026-09-23. Tesseract 5.3.4 was on this machine, so the
      real-engine tests ran: synthetic-scan CER 0.0007 against 0.03. The real-scan stratum, macOS
      and Windows are unverified here; two provisional decisions are in the Blocked section.)*
- [ ] **Phase 14** — Security hardening
- [ ] **Phase 15** — Packaging & release  *(then check Appendix D: Definition of Done for v1.0)*

## Phase 14 — on branch `phase/14-security-hardening`

Built in the worktree `/home/user/wt/phase13` while Phase 12 is finished on its own branch. Work
items, in order, with the plan's test rows against each:

- [x] **P14.1** `CapViolation`; the image cap read from the dictionary, the decoder behind it — row 14.1
- [x] **P14.2** `BoundedInflate` and our own filter chain; `lopdf` loads bounded — rows 14.2, 14.3
- [ ] **P14.3** the xref/ObjStm pre-walk: depth counter and visited set — rows 14.4, 14.5
- [ ] **P14.4** the page cap from the catalogue's `/Count`, before any page object — row 14.6
- [ ] **P14.5** one abort path: `AbortCause`, `DeadlineGuard`, one cleanup — row 14.8
- [ ] **P14.6** `--max-memory`: `RLIMIT_AS` / nested job object before the PDF opens — row 14.7
- [ ] **P14.7** children never outlive a killed engine: PDEATHSIG trampoline, job object (Phases 9, 13)
- [ ] **P14.8** Landlock: `ScopeSet`, self-restriction, recorded skip — rows 14.10–14.12
- [ ] **P14.9** `oc-net` audit log — row 14.21
- [ ] **P14.10** caps end in exit 1 with a report and no output; the 40 M-glyph PDF — rows 14.9, 14.19
- [ ] **P14.11** crash corpus: `oc-eval mutate`, `corpus/fixtures/crash/`, Isartor fetch — rows 14.16–14.18
- [ ] **P14.12** `fuzz/`: three targets, seeded corpora — rows 14.13–14.15
- [ ] **P14.13** `unshare -n` over the AI cassette path — row 14.20
- [ ] **P14.14** `unsafe` confined to declared modules — row 14.22
- [ ] **P14.15** `--isolate-parser` spike, go/no-go — row 14.23
- [ ] **P14.16** `docs/SECURITY_TESTING.md`, Definition of Done, CHANGELOG, merge

What a fresh session needs:

- `oc_core::limits::CapViolation` is the structured Phase 14 refusal (one variant per cap, `cap()`
  names it); Phase 1's flat `LimitExceeded` stays for the page count. `PdfError::cap()` answers
  "was this a cap?" for both.
- The pixel cap counts **pixels**, not pixels × components: D13.2 says "max image pixels (100 MP
  declared)", which outranks the plan's detail 1.
- `oc_pdf::filters::decode_stream` is our own chain (Flate, LZW, RunLength, ASCII85, ASCIIHex,
  PNG/TIFF predictors), every layer read through `limits::BoundedInflate`, **one budget shared by
  the whole chain**. Page content and XMP go through it; `lopdf` now loads with
  `max_decompressed_size` (it decodes ObjStm/xref streams itself, and its default was unbounded).
  `our_filter_chain_agrees_with_lopdf_on_every_fixture` holds it to `lopdf`'s answers. The dev
  profile builds `miniz_oxide`/`adler2` at `opt-level = 3` so the 256 MiB ceiling tests take ~2 s.

## Phase 13 — on branch `phase/13-ocr`

Work items, in order, with the plan's test rows against each:

- [x] **P13.1** `[ocr.*]` thresholds and the TSV parser (`oc_core::ocr::tsv`) — rows 13.5–13.8
- [x] **P13.2** language selection and the `W_OCR_*` warning codes — row 13.9
- [x] **P13.3** system-Tesseract discovery, VD-g — rows 13.1–13.4, A13.7 *(VD-g closed, DECISIONS_LOG 2026-09-23)*
- [x] **P13.4** invocation: fixed argv, deadline, process ownership, the fake engine — rows 13.10, 13.18 (invocation half), 13.19
- [x] **P13.5** `oc-pdf` rasterization (`render_region`) — rows 13.23, 13.23a, 13.23b (additions)
- [x] **P13.6** merge, the `ingest` declaration, region-scoped I-6, retention — rows 13.13, 13.15
- [x] **P13.7** OCR routing in `ingest`, the `convert` flags, degradation — rows 13.11, 13.12, 13.14, 13.16, 13.17, 13.18, 13.22
- [x] **P13.8** scanned fixtures, `.assert.json`, CER per stratum — rows 13.20, 13.21 *(synthetic CER 0.0007; real stratum unverified here)*
- [x] **P13.9** `docs/OCR_PACK_SPIKE.md`, CI job, Definition of Done, CHANGELOG, merge

What a fresh session needs:

- Tesseract 5.3.4 with `eng`, `deu`, `tur`, `osd` is installed here at `/usr/bin/tesseract`. Tests that
  need it are behind the `openconvert` feature `tesseract`; nothing else may depend on it being
  present, so the shared test helpers convert with OCR off.
- Process-level tests (discovery, argv, deadline, teardown) use `oc_testkit::fake_tesseract`, a POSIX
  shell script, and live in `crates/oc-testkit/tests/` (not `oc-core/tests` as the plan's file list
  says): `oc-core` cannot dev-depend on `oc-testkit` without putting `oc-net` into the graph
  `oc_core_has_no_net_dependency` walks. They are `#[cfg(unix)]`; Windows is unverified here.
- OCR routing is `openconvert::ocr::ocr_stage`, inside `convert`'s `ingest`; tests use the in-process
  `tests/common/ocr.rs::ScriptedEngine`. `common::build*` convert with `OcrOptions::off()`. New
  fixture `f11_mixed_plate` (Typst, `mixed`); `h05_invisible_layer` is the sandwich.
- Real-engine tests: `cargo nextest run -p openconvert --features tesseract -E 'binary(ocr_tesseract)'`.
  Scanned fixtures: `PYTHONPATH=eval/src eval/.venv/bin/python -m oc_eval.generate.scan_sim
  --scanned-fixtures [--check]` (the shared `eval/.venv` has the main checkout's `oc_eval` installed,
  so the worktree's source must be put first on the path).
- Disk: build with `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0` and run the suite per package with
  `scratchpad/p13_test_all.sh` (deletes each package's test binaries after it runs) — the whole
  workspace's test binaries at once filled the disk.
- VD-g is closed: UB-Mannheim installs to `%ProgramFiles%\Tesseract-OCR` (all users) or
  `%LOCALAPPDATA%\Programs\Tesseract-OCR` (one user), does not touch `PATH`, ships 5.5.3, and prints
  `tesseract v5.5.3.20260724` — the parser reads that form.

### Phase 13 — Definition of Done

`IMPLEMENTATION_PLAN.md` §0.3, row by row. Checked on this machine unless the row says otherwise.

| Row | State |
|---|---|
| Every named test exists and passes | **Yes.** All 22 rows, 13.1–13.22, under their names, plus 27 additions (`docs/TEST_MATRIX.md`). 13.22 runs a scanned book with `--ai --ai-all-tasks` (Phase 10's `convert_with_ai`). 13.20 and 13.21 and A13.1/A13.3's real-engine tests are behind `--features tesseract` and pass here against Tesseract 5.3.4. The process-level tests (13.1–13.4, 13.10, 13.18, 13.19) are `#[cfg(unix)]`. |
| `cargo nextest run --workspace` green | **Yes** on the merge commit: 672 tests (38 new in the default suite); with `--features tesseract` the 5 real-engine tests pass too. eval: 224 pytest tests (+ 6). |
| Green on Linux/macOS/Windows CI | **Unverified here:** GitHub Actions is disabled. Windows/macOS discovery, invocation and teardown have no machine here. |
| clippy `-D warnings` clean | **Yes**, workspace, all targets, all features (including `tesseract`). |
| `cargo fmt --check` clean | **Yes.** ruff, ruff format and mypy clean on `eval/`. |
| `cargo deny check` clean | **Yes.** No new crate: `image` was already in the graph (`oc-epub`, `pdfium-render`). |
| `cargo xtask thresholds-lint` clean | **Yes.** Seven `ocr.*` entries. |
| Every Given/When/Then demonstrated | **A13.1, A13.2, A13.3, A13.4, A13.7 yes** (Linux). **A13.5 yes on Linux** (13.18, 13.19); an engine killed outright is Phase 14's. **A13.6 partial:** synthetic CER 0.0007 ≤ 0.03; the real stratum is unverified here (no scan with ground truth on this machine). |
| `docs/CHANGELOG.md` entry | **Yes.** |
| No `TODO`/`FIXME` without an issue number | **Yes**, `xtask ci-lint` clean. |
| VD-g closed | **Yes**, `docs/DECISIONS_LOG.md` 2026-09-23. |

## Phase 9 — on branch `phase/09-local-model`

Work items, in order, with the plan's test rows against each:

- [x] **P9.1** the registry: `oc_net::registry` — rows 9.1, 9.2
- [x] **P9.2** the downloader: allowlist, streaming SHA-256, LICENSE/NOTICE, atomic — rows 9.3–9.6
- [x] **P9.3** the store (`list`, `remove`) and `HttpTransport` — row 9.19
- [x] **P9.4** `oc-core` has no net dependency — row 9.7
- [x] **P9.5** sidecar arguments and port picking — rows 9.12, 9.14
- [x] **P9.6** the owned server's lifecycle — rows 9.8–9.11, 9.13
- [x] **P9.7** `openconvert model pull|list|remove` — row 9.18
- [x] **P9.8** live tests behind `live-llm`, `W_LLM_PREFIX_COLD` — rows 9.15, 9.16
- [x] **P9.9** `eval/model_gate.py`, the default-model gate, `docs/MODEL_GATE.md` — rows 9.17, 9.20
- [x] **P9.10** the Definition of Done, CHANGELOG, merge

What a fresh session needs:

- **This sandbox's egress policy refuses `huggingface.co` (HTTP 403 on CONNECT, checked again
  2026-09-23 after the merge).** The proxy's README says not to route around a blocked host, so this
  phase does not read Hugging Face's API or fetch any model file. That means the registry fill (real
  `revision`/`sha256`/`size_bytes`), the A9.1 download and the live tests 9.15/9.16 are
  **unverified here**. Tests use synthetic hashes. github.com release downloads were refused during
  the phase but went through for the post-merge llama.lock follow-up: all four `b10456` assets
  hashed to the lock's digests, and `cargo run -p xtask -- fetch-llama-server` stages a working
  `vendor/llama-server/b10456/llama-b10456/llama-server` (gitignored).
- **TLS roots are option (a):** `ureq` with `rustls-no-provider` + `platform-verifier`, and
  `rustls` with the `ring` provider. `cargo deny --all-features check` is clean and `webpki-roots`
  is in no target's graph.
- **The downloader follows redirects itself** and checks every hop against the allowlist before
  fetching it. Tests run the real `ureq` client over loopback: `tests/common/mod.rs` maps
  `https://<host>/<path>` to `http://127.0.0.1:<port>/<host>/<path>`. Only Apache-2.0 models install,
  because that is the only licence text bundled.
- **`oc-core` opens no socket, and a test says so** (`tests/no_net.rs`, row 9.7): it walks
  Cargo.lock and scans `oc-core/src` for `std::net`. So the sidecar supervisor in `oc-core` reaches
  its server only through a health probe and a port its caller supplies (the caller links `oc-net`).
  The conversion suite passes here under `unshare -n` (5 tests, CI's filter).
- **The key travels in `LLAMA_API_KEY`**, never in argv (`oc_core::sidecar::llama::command`).
  `--cache-reuse` comes from the registry's `cache_reuse`, its chunk from
  `llm.cache_reuse_min_chunk` (new, provisional). `oc_net::loopback::free_port` picks the port:
  the plan's `oc-core/src/sidecar/portpick.rs` moved to `oc-net` because binding is opening a socket.
- **`oc_core::sidecar`**: `llama` (argv), `server` (`OwnedServer`: spawn, `wait_healthy(probe)`,
  idle policy, `Drop` kills), `supervise` (the child registry, panic hook, `ctrlc` handler, exit
  code 3 on a signal), `endpoint` (`LlmEndpoint::choose`: an external endpoint spawns nothing).
  The lifecycle tests are in `crates/oc-testkit/tests/sidecar.rs` with two dev-only binaries,
  `oc-stub-llama-server` and `oc-sidecar-engine`. PDEATHSIG and Windows job objects need `unsafe`
  and are deferred to Phase 14 (PROVISIONAL, DECISIONS_LOG 2026-09-23). The idle-kill *loop* is
  Phase 10's.
- **`openconvert model`** is `crates/openconvert/src/cmd_model.rs`, with the registry compiled
  in. The bundled `models.toml` still has `TODO_` pins, so on it `list`/`pull` exit 2. That is
  correct until the fill (Blocked).
- **Live tests** are `crates/oc-testkit/tests/live_llm.rs` (`--features live-llm`, env
  `OC_LLAMA_SERVER`, `OC_LIVE_MODEL`), run by the nightly `live-llm-cassette-refresh` job after
  `xtask fetch-llama-server` and `model pull`. `xtask/llama.lock`'s `b10456` digests are filled and
  download-checked, so that job now fails at `model pull` until `models.toml` is filled (Blocked).
- **The promotion gate** is `eval/model_gate.py` → `oc_eval.model_gate`. Probes and prompt
  fixtures are generated (`python -m oc_eval.model_gate.{probes,fixtures} --write`) and held equal to
  the committed files. `docs/MODEL_GATE.md` is rendered (`--render-table`), and no run is recorded:
  G3's reference tokens, G7's pairs and G9's conversion are all missing inputs (`not_run`).
- **Disk is shared with a parallel worker** (`/home/user/wt/phase12`, ~11 GB). Build with
  `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0` (env only, no repo change); prune stale duplicates in `target/debug/deps` when free space drops under
  ~5 GB (keep the newest artefact per crate name).

### Phase 9 — Definition of Done

`IMPLEMENTATION_PLAN.md` §0.3, row by row. Checked on this machine unless the row says otherwise.

| Row | State |
|---|---|
| Every named test exists and passes | **Yes, with two unverified here.** All 20 rows exist under their names. 9.17 and 9.20 are pytest functions, so they carry pytest's required `test_` prefix. 9.15 and 9.16 are behind `--features live-llm`: they compile, and they fail loudly without a server and model. **Unverified here:** no GGUF could be downloaded (huggingface.co egress 403). The pinned llama-server became fetchable and verified after the merge. Against the stub server, 9.15 passes and 9.16 fails with `W_LLM_PREFIX_COLD`, which exercises the harness and says nothing about a model. |
| `cargo nextest run --workspace` green | **Yes**, 594 tests (561 + 33). eval: 214 pytest tests (+ 20). |
| Green on Linux/macOS/Windows CI | **Unverified here:** GitHub Actions is disabled. `cargo check -p oc-core` passes for `x86_64-pc-windows-msvc` and `aarch64-apple-darwin`. `oc-net` cannot be cross-checked here because `ring` needs the MSVC C toolchain. |
| clippy `-D warnings` clean | **Yes**, workspace, all targets, all features (including `live-llm`). |
| `cargo fmt --check` clean | **Yes.** ruff, ruff format and mypy clean on `eval/`. |
| `cargo deny check` clean | **Yes.** New: `ureq` (platform-verifier TLS, **no `webpki-roots`**), `rustls` (ring), `secrecy`, `ctrlc`, `getrandom`. `deny.toml` is unchanged, `exceptions = []`. |
| `cargo xtask thresholds-lint` clean | **Yes.** Ten thresholds added, each with source, evidence, owner and `review_by`. |
| Every Given/When/Then demonstrated | **A9.2, A9.3 yes. A9.1, A9.4, A9.6 partially. A9.5 unverified here** (below). |
| `docs/CHANGELOG.md` entry | **Yes.** |
| No `TODO`/`FIXME` without an issue number | **Yes**, `xtask ci-lint` clean. (`TODO_` placeholders are the registry's named-slot convention.) |

- **A9.1** — `download_writes_license_and_notice`: through the real `ureq` client over loopback,
  a verified GGUF, `LICENSE` and `NOTICE` land in the store. **Unverified here:** a real
  `model pull qwen3-1.7b-q4_k_m` from huggingface.co (egress 403, and `models.toml` has no pins).
- **A9.2** — `download_refuses_host_off_allowlist` counts zero requests before a refusal and one
  (the allowlisted hop) before an off-list redirect is refused. `model_pull_refuses_a_host_off_the_allowlist`
  shows the same at the CLI.
- **A9.3** — `oc_core_has_no_net_dependency`. The conversion suite passes here under `unshare -n`
  (5 tests, CI's filter). The CI step is unverified until CI runs.
- **A9.4** — exit, panic and SIGTERM each leave no server within 2 s **on Linux**
  (`owned_server_is_killed_on_engine_{exit,panic,sigterm}`, mutation-checked, 25 runs green).
  **Unverified here:** macOS and Windows. **Deferred to Phase 14 (PROVISIONAL):** an engine killed
  outright (SIGKILL, segfault), which needs PDEATHSIG or a job object.
- **A9.5** — **unverified here:** no machine L, no model, no server. The harness is tested
  (`test_every_gate_passes_against_a_well_behaved_server`,
  `test_one_failing_gate_fails_the_verdict_and_the_exit_code`).
- **A9.6** — the default stays Qwen3-1.7B (`test_default_stays_qwen3_1_7b_until_gate_passes`), and
  a gate that did not run is never a pass (`test_a_gate_that_did_not_run_is_never_a_pass`). No run
  of Qwen3.5-2B exists; nothing promotes it.

## Current work item

**Phase 12 — Desktop UI**, built concurrently on `phase/12-desktop-ui`. Phase 11 is merged; the
list of what Phase 12 must wire from it is at the end of the Phase 11 section below.

## Phase 11 — built on `phase/11-byo-providers`, merged 2026-09-23

Work items, in order, with the plan's test rows against each:

- [x] **P11.1** `oc-net::consent`: `requires_consent`, `authorize`, `ConsentRecord`; `HttpTransport`
      cannot be built to a host off this machine without consent naming it — row 11.6
- [x] **P11.2** `oc-ai::provider`: the adapter modules, `ProviderKind`, schema-in-prompt and
      `W_LLM_UNCONSTRAINED` when a provider constrains nothing — row 11.4
- [x] **P11.3** `oc-ai::provider::ollama`: native `/api/chat`, `format`, `options.num_ctx`,
      `keep_alive`, `think: false` — rows 11.2, 11.3
- [x] **P11.4** `oc-net::detect`: `Transport::get`, `detect_ollama`, the capability probe — row 11.1
- [x] **P11.5** the Phase-8 cassettes through every adapter — row 11.10
- [x] **P11.6** `openconvert`: provider resolution, `--llm-provider`/`--llm-model`/`--llm-allow-host`,
      `E_CONSENT_REQUIRED` — rows 11.5, 11.8
- [x] **P11.7** consent in the report; a failing provider degrades — rows 11.7, 11.9
- [x] **P11.8** `openconvert provider detect|check|probe` (what Phase 12's settings page calls)
- [x] **P11.9** the Definition of Done, CHANGELOG, merge; A11.1 live behind `live-llm`

What a fresh session needs:

- **Consent is enforced in `oc-net`** (`consent::authorize`): loopback (`localhost`, 127/8, `::1`,
  `::ffff:127.x`) needs nothing; any other host needs a `ConsentRecord` naming it, and `https://`
  (plain http off the machine is refused even with consent — PROVISIONAL, DECISIONS_LOG
  2026-09-23). `HttpTransport::new` is loopback-only; `HttpTransport::with_consent` takes the record.
  URLs with user-info, `%`, `\`, `?`, `#` or whitespace are refused, never interpreted.
- **The adapters are `oc_ai::provider::{local_sidecar, openai_compatible, ollama}`** (`oc_ai::openai`
  is gone — moved to `provider::openai_compatible`). `custom_endpoint` takes probed `ProviderCaps`;
  with `ProviderCaps::neither()` the task's `schema.json` is appended to the user message and the
  `Session` raises `W_LLM_UNCONSTRAINED` once. `ProviderKind` names the adapter.
- **Ollama is `/api/chat`, not `/v1`** (PROVISIONAL, DECISIONS_LOG 2026-09-23): Ollama's `/v1`
  layer drops `num_ctx`/`format`/`keep_alive`/`think`. Every request sets `options.num_ctx` ≥
  `llm.ollama_num_ctx` and ≥ prompt bytes + overhead + `max_tokens`, `truncate: false`,
  `shift: false`. `tests/common/cassette_server.rs` answers both wire formats from the cassettes.
- **`oc_net::detect`**: `detect_ollama` (`GET /api/tags`), `probe` (`/props` → llama-server,
  `/api/tags` → Ollama, `/v1/models` → generic, which is `ProviderCaps::neither` — PROVISIONAL),
  `api_root` strips a trailing `/v1`. `Transport::get` exists (default 404).
- **Row 11.10** (`oc-ai/tests/contract.rs`) asks every committed cassette through five adapter
  configurations over `tests/common/cassette_server.rs` and compares with `Replay`: re-recording
  cassettes needs no per-adapter work.
- **`convert --ai` flags**: `--llm-provider builtin|ollama|openai-compatible`, `--llm-model`,
  `--llm-allow-host <HOST>` (the consent; must equal the endpoint's host). No consent → exit 2,
  `fatal{E_CONSENT_REQUIRED}` naming the host, zero connections. `ai_endpoint::open_with` takes a
  `Connector` (tests use an in-process one); `Opened { provider, server, kind, consent }`. The
  probe picks the adapter; a model is never guessed. `openconvert/tests/common/endpoint.rs` is a
  loopback model server for binary tests.
- **The report**: top-level `consent {host, granted_at, scope}` only when a remote endpoint was
  opened under consent; `ai.provider` names the adapter. `ReportInput` gained `provider` and
  `consent`. `report__report_f07.snap` moved only in its threshold count (205 → 209).
- Build with `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0`. **Disk is tight** (~5 GB free while
  three worktrees build).

### Phase 11 — Definition of Done

`IMPLEMENTATION_PLAN.md` §0.3, row by row. Checked on this machine unless the row says otherwise.

| Row | State |
|---|---|
| Every named test exists and passes | **Yes.** All 10 rows under their names: 11.1 and 11.6 in `oc-net` (`tests/detect.rs`, `tests/consent.rs`); 11.2, 11.3 (`tests/ollama.rs`), 11.4 (`tests/providers.rs`) and 11.10 (`tests/contract.rs`) in `oc-ai`; 11.5, 11.7, 11.8, 11.9 in `openconvert` (`tests/providers.rs`). Plus 18 additions, and `ai_against_a_live_ollama_converts_every_book` behind `--features live-llm` (fails loudly without `OC_LIVE_OLLAMA_MODEL` — **unverified here**: Ollama is not installed and no model can be fetched). |
| `cargo nextest run --workspace` green | **Yes**, 663 tests on the branch (635 + 28); **700** after merging Phase 13's `main`. |
| Green on Linux/macOS/Windows CI | **Unverified here:** GitHub Actions is disabled; no macOS or Windows machine. |
| clippy `-D warnings` clean | **Yes**, workspace, all targets, all features (including `live-llm`). |
| `cargo fmt --check` clean | **Yes.** |
| `cargo deny check` clean | **Yes.** No new external crate: `oc-net → time` and `openconvert → secrecy` are existing workspace dependencies. |
| `cargo xtask thresholds-lint` clean | **Yes.** 4 thresholds added (`llm.ollama_{num_ctx, template_overhead_tokens, keep_alive_secs}`, `llm.provider_probe_timeout_millis`), each provisional with owner and `review_by`. |
| Every Given/When/Then demonstrated | **A11.2, A11.3, A11.4 yes; A11.1 yes against a stub Ollama, live unverified here** (below). |
| `docs/CHANGELOG.md` entry | **Yes.** |
| No `TODO`/`FIXME` without an issue number | **Yes**, `xtask ci-lint` clean. |

- **A11.1** — `ollama_detected_on_default_port` (detection on `localhost:11434`),
  `ollama_num_ctx_is_always_overridden` (every body sets `options.num_ctx` ≥ prompt bytes +
  overhead + `max_tokens`, `truncate`/`shift` false), `ollama_uses_format_schema` (the task's
  schema in `format`, the cassette's answer back), `ollama_is_found_on_localhost_and_its_model_is_never_guessed`
  and `the_probe_decides_the_adapter`. **Unverified here:** the same against a real Ollama — the
  live test and the nightly `live-ollama` job exist and have not run.
- **A11.2** — `non_loopback_requires_consent`: `https://example.com/v1` without consent (and with
  consent for another host) is exit 2, `fatal{E_CONSENT_REQUIRED}` naming `example.com`, no book,
  no report — and the in-process connector records **zero connections**. The check also lives in
  `HttpTransport`'s constructors (`a_host_off_this_machine_needs_consent_that_names_it`).
- **A11.3** — `same_cassettes_pass_on_all_providers`: every committed cassette through the sidecar
  adapter, a JSON-schema endpoint, an unconstrained one, a Qwen one with `/no_think`, and Ollama;
  each answer equals `Replay`'s, and the seed canaries pass gate S identically.
- **A11.4** — `provider_failure_degrades_to_deterministic` (the binary): a 500 on every question
  through the `llama-server`, Ollama and generic adapters, and an endpoint that answers nothing —
  each book is the `--no-ai` book byte for byte, exit 0, `W_LLM_UNAVAILABLE` with the reason.
- **Regression artefacts:** `tests/common/cassette_server.rs` (both wire formats from the committed
  cassettes); the report snapshot moved only in its threshold count (205 → 209).

### What Phase 12 must wire from Phase 11

Phase 11 did not touch `apps/desktop` (`routes/settings/providers.svelte` is Phase 12's).

- **The engine is the only thing that connects.** The webview needs no network for providers:
  keep `connect-src 'none'`.
- **Provider radio (UI_UX §2.4)** — *Built-in*: the job spec's `ai.endpoint` is the app-owned
  `llama-server` URL, with `ai.api_key_file` and `ai.model_id`; the engine's probe recognises
  `llama-server` and uses GBNF + `chat_template_kwargs`. *Ollama*: `openconvert provider detect
  --json` → `{"ollama": {"url": "http://localhost:11434", "models": [...]}}` or `{"ollama": null}`
  (the "Detected on this computer" tag and the model list); the job spec's endpoint is that URL and
  `model_id` the chosen model. *Custom endpoint*: base URL (with or without `/v1`), model name,
  API key file.
- **Consent dialog (strings `consent.title/body/allow`)** — `openconvert provider check <URL>
  --json` → `{url, host, loopback, requires_consent, usable, reason}`, sending nothing. Show the
  dialog naming `host` when `requires_consent`; `usable: false` means plain http off the machine,
  and `reason` says https is required. On *Allow*, write `ai.non_loopback_consent: true`: the engine
  reads it as consent to that endpoint's own host (`AiArgs::consenting_to_the_endpoint`); the CLI
  equivalent is `--llm-allow-host <host>`. The engine remembers nothing (`scope: "run"`): the app
  keeps the user's choice per configuration and writes it into every job.
- **Optional "Test connection"** — `openconvert provider probe <URL> [--llm-model M]
  [--llm-allow-host H] [--llm-api-key-file P] --json` → `{available, provider, constraint,
  thinking, model, models, consent}` (exit 0), `{available: false, reason}` (exit 1), or exit 2
  `fatal{E_CONSENT_REQUIRED}`.
- **Job spec → engine**: `endpoint` → `--llm-endpoint`, `api_key_file` → `--llm-api-key-file`,
  `model_path` → `--model-path`, `model_id` → `--llm-model`, `non_loopback_consent` →
  `--llm-allow-host <endpoint host>`. Job-spec v1 has no provider-kind field; the engine probes.
  (The engine's `--job` reader does not exist yet.)
- **Events and warnings**: `fatal{code: "E_CONSENT_REQUIRED"}` (exit 2) — show the consent dialog
  again, never a generic error. `W_LLM_UNAVAILABLE {reason}` and the new `W_LLM_UNCONSTRAINED
  {model}` have en/de/tr templates. No new NDJSON event type.
- **Report page**: top-level `consent {host, granted_at, scope}` ("text from this book was sent to
  {host} at {granted_at}"), and `ai.provider`.
- **Network log (Settings › Network log)**: no event feeds it yet — it is PHASE 14 detail 12
  (`oc-net/src/audit.rs`, `<data_dir>/network-audit.log`).

## Phase 10 — built on `phase/10-ai-decisions`, merged 2026-09-23

Work items, in order, with the plan's test rows against each:

- [x] **P10.1** one definition of the verse band (`oc_core::escalation::line_band`), used by
      `oc-structure::quotes`; the pre-phase `--no-ai` EPUB hashes pinned — the open finding of
      2026-09-22, and the byte-identity artefact the rest of the phase is held to
- [x] **P10.2** `oc-structure::escalate`: the four predicates over the stage's evidence, the
      `EscalationRecord`, in `Conversion` and the report with AI off — rows 10.1, 10.2
- [x] **P10.3** `oc-ai::task::metadata`: the verbatim-substring check — rows 10.3, 10.4
- [x] **P10.4** `oc-ai::task::heading_roles`: pre-gate, held-out check, label ≠ deletion — rows 10.5–10.8
- [x] **P10.5** `oc-ai::task::book_structure`: boundaries, chunking with overlap — rows 10.9–10.11
- [x] **P10.6** `oc-ai::task::verse_quote`: counter-evidence, the 30-block cap — rows 10.12, 10.13
- [x] **P10.7** the plan: degradation order, language gate, wall-clock meter — rows 10.21, 10.22
- [x] **P10.8** `openconvert`: the AI step in the pipeline — rows 10.14, 10.15, 10.20
- [x] **P10.9** `convert --ai` and the endpoint flags; a missing sidecar degrades — rows 10.16, 10.19
- [x] **P10.10** `eval/compare`: McNemar, false repair, gold sets, `docs/AI_EVALUATION.md` — rows 10.17, 10.18
- [x] **P10.11** the Definition of Done, CHANGELOG, merge

What a fresh session needs:

- **No model is reachable here** (huggingface.co and GitHub release downloads: 403 on CONNECT). Every
  live measurement is unverified here; tests use cassettes recorded through the stub and synthetic
  data. `ai.enabled = false` stays the default.
- **`--no-ai` output is pinned**: `crates/openconvert/tests/snapshots/ai__no_ai_epub_sha256.snap`
  was written at `8f045a1` (Phase 9's merge), before any Phase 10 change. It must not move.
- **The tagged fixtures** are a separate invocation: `cargo run -p xtask -- fixtures` and
  `cargo run -p xtask -- fixtures --keep-structtree` (oc-pdf's `struct_tree_is_read_from_the_catalogue`
  needs the second).
- **Escalation records** (`oc_structure::escalate`) are gathered after `structure` on every
  conversion and land in `Conversion.escalations` and the report's `escalations`, AI on or off.
  `report__report_f07.snap` gained its one record (the ambiguous block); the EPUB did not move.
- **Phase 10 cassettes** are scripted answers recorded through the stub (`model_id = "stub"`),
  written by `crates/oc-ai/tests/tasks.rs::replayed` under `OC_AI_RECORD_SEEDS=1` — record with
  `-j 1` (nextest runs tests as parallel processes and the index is read-modify-write). A cassette is
  keyed by its *question*, so two scripted answers need two different payloads.
- **A task runs through an `Asker`** (`oc_ai::session`): `task::<name>::run` pre-gates, asks, gates
  S, validates, and returns `TaskResult::{Refused, Unasked, Rejected, Admitted}` with the edit.
  Admitted edits are applied by **re-running `structure`** — `oc_structure::stage::structure_with`
  with `StructureEdits` — never by patching output. `openconvert::convert::prepare` gives a test the
  stage's input.
- **`oc_ai::session::Session`** is the production `Asker`: stop → wall-clock share → budget →
  cache → provider, in that order; `oc_ai::plan` decides the grant before any call (language gate,
  degradation order). **The language maps ship empty** (nothing evaluated): `--ai` alone asks
  nothing; `--ai-all-tasks` runs unproven tasks (DECISIONS_LOG 2026-09-23, PROVISIONAL).
- **The AI step is `openconvert::ai::run`**, called from `convert::convert_prepared` when an
  `AiContext` is given (`convert` passes none). Tests drive it with in-process providers
  (`crates/openconvert/tests/ai_pipeline.rs`: `Echo` answers every task from its payload).
- **`convert --ai`** opens a provider in `openconvert::ai_endpoint` (loopback endpoint, or an
  owned `llama-server` from `OC_LLAMA_SERVER`/beside the binary with `--model-path` or the store's
  default); a non-loopback endpoint is exit 2 until Phase 11; anything else unavailable is
  `W_LLM_UNAVAILABLE`, exit 0. The answer cache is `openconvert::data_dir::llm_cache()`.
- **The evaluation** is `eval/src/oc_eval/compare` (`python -m oc_eval.compare --render|--check|--gate`,
  run from `eval/` with `PYTHONPATH=$PWD/src` in a worktree — the shared venv's `.pth` points at
  main's sources). `eval/data/ai_eval/outcomes.jsonl` is empty: no evaluation has run.
- **Task validations are gate failures with codes**: `V.verbatim`, `S.range`, `S.order`,
  `S.overlap`, `S.holdout`, `S.roles` (`oc_ai::gates::GateFailure`).
- **The pinned llama-server is fetchable and verified** since main's `fix/phase-09-llama-pins`
  (`cargo run -p xtask -- fetch-llama-server`); a model still cannot be (huggingface.co refused).
- Build with `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0` (disk is shared with two other
  worktrees; the whole workspace is ~3.6 GB that way).

### Phase 10 — Definition of Done

`IMPLEMENTATION_PLAN.md` §0.3, row by row. Checked on this machine unless the row says otherwise.

| Row | State |
|---|---|
| Every named test exists and passes | **Yes.** All 22 rows under their names: 10.1 in `oc-structure`, 10.3–10.13 and 10.21–10.22 in `oc-ai`, 10.2, 10.8, 10.14–10.16, 10.19, 10.20 in `openconvert`, 10.17/10.18 as pytest functions (`test_` prefix). Plus 24 Rust and 4 Python additions, and `ai_against_a_live_model_conserves_every_book` behind `--features live-llm` (fails loudly without a server and model — **unverified here**: no model can be fetched). |
| `cargo nextest run --workspace` green | **Yes**, 635 tests (595 + 40). eval: 220 pytest tests (+ 6). |
| Green on Linux/macOS/Windows CI | **Unverified here:** GitHub Actions is disabled. The cross-`cargo check` Phase 9 used no longer gets past `blake3`'s C build here (`ml64.exe` / Apple `cc` missing) — a toolchain limit of this box, not a change of this phase. |
| clippy `-D warnings` clean | **Yes**, workspace, all targets, all features (including `live-llm`). |
| `cargo fmt --check` clean | **Yes.** ruff, ruff format and mypy clean on `eval/`. |
| `cargo deny check` clean | **Yes.** No new external crate; `openconvert → oc-ai` and `oc-structure → blake3` are workspace edges. |
| `cargo xtask thresholds-lint` clean | **Yes.** 27 thresholds added, each with source, evidence, owner and `review_by`; string arrays are a new value type. |
| Every Given/When/Then demonstrated | **A10.1, A10.3, A10.6 yes; A10.2 yes with an in-process model, live unverified; A10.4, A10.5 unverified here** (below). |
| `docs/CHANGELOG.md` entry | **Yes.** |
| No `TODO`/`FIXME` without an issue number | **Yes**, `xtask ci-lint` clean. |

- **A10.1** — `ai_default_is_off` (the binary: no `llm` event, no `ai` report section, no model
  trace; the escalations are still recorded) and `no_ai_output_is_byte_identical_to_the_pre_phase_snapshot`
  (all ten fixtures' `--no-ai` EPUB hashes, pinned at `8f045a1` before any change).
- **A10.2** — `a_book_with_no_outline_and_boilerplate_metadata` (≤ 8 calls, the model's title in
  `dc:title` with `source = llm`), `wallclock_share_hard_stop` (injected clock),
  `task_priority_order_on_budget_overflow`; every admitted edit passes S, the task validation, L
  and V, and every escalation ends in a `Decision`. **Unverified here:** the same against a real
  model (the live test).
- **A10.3** — `ai_edits_are_conserving_end_to_end`: every fixture, as filed and with outline and
  title removed, with a cooperative in-process model whose answers are applied (all four tasks are
  applied somewhere across the twenty variants): I-7 holds on every one.
- **A10.4 / A10.5 — unverified here.** No model, no corpus: `eval/data/ai_eval/outcomes.jsonl` is
  empty and `docs/AI_EVALUATION.md` says no evaluation has run. The harness is tested
  (`test_mcnemar_and_false_repair_reported_per_category`, `test_false_repair_rate_under_one_percent`),
  and with no task enabled the gate passes vacuously and says so.
- **A10.6** — `running_head_label_never_deletes_text`: 2 000 generated mappings, outlines removed
  so the mappings act; mutation-checked (emptying a demoted block fails it). The edit type has no
  removal variant.
- **Regression artefacts:** 12 scripted cassettes recorded through the stub beside the four seeds;
  the four gold sets (47 seed items). "One cassette per task per gold fixture" needs a model's
  answers — **unverified here**.

Phase 7.5 is **parked, not done**. Its section below is the resume point. The deterministic
baseline Phase 10 will be compared against is the parked one: 79 of 104 corpus documents clean,
2 869 characters lost across 13, 11 not finishing in 90 s.

## Phase 7.5 — parked 2026-09-23, resume here

**Parked by the maintainer's decision** until every phase is implemented (`docs/DECISIONS_LOG.md`, 2026-09-23). Everything below is the state it was parked in.

**Phase 7.5 is in progress, and its conservation gate is close.** Everything below is measured on
the full 104-document corpus with one binary, via `openconvert diff-stage structure`.

```
run          clean  conservation  timeout  text-I1       lost      dup
2026-09-20     35        58           6*       4    1 421 777   30 579
2026-09-22 a   72        14          17        0        3 415        0
2026-09-22 b   79        13          11        0        2 869        0
                              (* 300 s per document; the later runs allowed 90 s)
```

**Loss down 99.8 %, duplication gone, clean documents 35 → 79.** The 2026-09-20 baseline has
mixed provenance: its loop kept running after its wrapper was reaped, and its last nine rows came
from whichever binary was built at that moment.

### Classes closed, in the order they were worked

Each one: admitted at ≥ 3 documents across ≥ 2 strata, *how else could this arise?* written in
`docs/DECISIONS_LOG.md` before the fix, closed by an architectural change, left an invariant test.

| class | mechanism | invariant |
|---|---|---|
| `structure/lost/claim-without-emission` | a claim computed from a predicate over the input (a line range, a bbox) rather than from what the builder emitted | `a_claimed_block_must_be_accounted_for_by_its_claimant` |
| `text/substituted/nfc-singleton` → ADR ruling | `N`'s NFC changed `C` and D13.4's closed `Reason` enum could not say so. **Ruled 2026-09-20: `C(·)` is taken after canonical *de*composition** — NFD, because composition depends on adjacency and a multiset must not | `c_of_is_invariant_under_canonical_equivalence_across_unicode`, `c_of_does_not_depend_on_where_the_text_was_cut` |
| `structure/lost/orphaned-claimant` — **73 % of all loss** | a list entered the flow only if its first item's block was itself claimed; 13 of 13 orphans had an unclaimed first block | `no_claimant_is_orphaned` |
| `structure/appeared/contested-claim` | four detectors ran independently over every block; now notes → tables → captions → lists, each built from what the ones before left | `no_text_has_two_owners` |
| (found inside the above) caption claimed by **text** | a running head repeated on 15 pages, one bound as a caption, all 15 claimed | covered by the two above |
| `structure/lost/unattached-note-text` — **the page-42 defect this phase opened on** | a note-zone line arriving with no note open was dropped; `8Cornelia` is not a marker to the rule | `note_assembly_keeps_every_character_of_the_zone` |

**One shape, found five times:** a second predicate standing in for a decision that had already
been made — a claim re-derived from geometry, from a line range, from a text search, from a
trigger block, from a walk over items. Every fix was to record the decision where it is made and
read that record. It is worth looking for first in any new class.

### Found and fixed on the way

- **A fatal reached nobody without `--progress json`.** `inspect`, `validate`, `dump-stage` and
  `diff-stage` exited non-zero printing nothing, which invented a "timeout class" of 13 documents
  that were failing in under a second.
- **Hashing decoded every image** although only small ones are ever compared: a 131-page scan
  went 101 s → 42.5 s, and six documents left the timeout list.
- **Unreferenced notes are in the book** — `epub` emits them as asides — so the diagnostic's
  reachability rule for notes was stricter than the output and reported losses that were not.
- Three quadratic or cubic lookups this phase itself introduced, all replaced by indexes.

### Open, in the order it matters

1. **The timeout class — 11 documents, all long books — is image decoding, not `structure`.**
   An earlier note here said `structure` > 280 s; timed directly, `structure` takes **167 ms**
   on that 368-page volume and every detector is under 40 ms. The volume draws **10 613 images,
   5 846 of them small enough to be ornament candidates**, and hashing costs **~237 ms per image**
   — per *call*, not per pixel. Next on resuming: time a page load, `get_processed_image` and the
   hash separately on one heavy page. A batched decoder (one page load per page) was written and
   **reverted unmeasured** — its one run was slower than the old code's, on a machine too loaded
   to tell a regression from noise. `get_raw_image` is the candidate if the render is the cost,
   and changes hash values, so the ornament fixtures must be re-measured with it.
   D13.2's `limits.stage_deadline_secs` should make this fail legibly and does not.
2. **13 documents still lose 2 869 characters**, a mechanism not yet named: three Turkish Word
   articles (612, 612, 368) and `oapen-117097`, `-117206` (672, 420), then small ones. None of it
   is unattached note text.
3. **`emitted_text` is still what I-1 is computed against.** `reachable_text` exists, is now
   aligned with what `epub` emits, and is asserted on the fixtures; the pipeline has not been
   switched.
4. `dergipark-1113748` is refused by I-4 (furniture over budget) — one document, one stratum.
5. **Item 7.5.0, the reading corpus.** Candidates for en/de/tr are committed at
   `corpus/reading/candidates_*.json` and verified; the `PD-old-work` admission path is written in
   the plan and not in `stratify.py`. Turkish is structurally short (life + 70, alphabet 1928):
   source it from Sabahattin Ali and Sait Faik and report it short rather than pad it.
6. **`docs/LLM_BOUNDARY.md`** and its property test (item 7.5.7) are not written.

Recorded as **quality**, deliberately not fixed in a conservation phase: the runaway list (a
margin-level marker swallows pages of prose, because `x0 >= marker.indent − tol` is always true
at the margin); the running head that reached `structure` (a chapter title on fifteen pages, below
the whole-book repetition ratio `furniture` uses); a debug-assert panic in `para_of` on arXiv's
vertical datestamp (one stratum, not admitted).

### Running the corpus on this machine

System memory has been at 1.5–4.5 GB free of 15.7 GB, and background shells are reaped while the
session is idle. Run the inventory **in the foreground, in chunks** under the 10-minute limit,
packed by each document's previous run time; strip `\r` from Python-written chunk files (Windows
writes CRLF, and every name carries it), and give the child `< /dev/null` or it swallows the list.
Check `rc` and output sizes before believing a run: one "complete" run here had never executed.

## Phase 8 — built on `worktree-phase8`, merged 2026-09-23

Phase 8 (AI abstraction, no real model) was built on its own branch while Phase 7.5 ran on
`main`, and merged into `main` on 2026-09-23 after 7.5 was parked. It is almost entirely new code
in `oc-ai`, which was an empty crate; outside it, it adds threshold entries, one additive field on
`oc_model::decision::Decision` (P8.2), warning templates, and the escalation predicates in
`oc-core` (P8.9). The merge conflicted only in this file's header and the end of
`docs/DECISIONS_LOG.md`, both resolved by keeping both sides.

Work items, in order, with the plan's test rows against each:

- [x] **P8.1** prompt artifacts, the GBNF parser, the request — rows 8.1, 8.14 (+ 19)
- [x] **P8.2** gate S, and gate D's record — rows 8.2, 8.3, 8.4, 8.9, 8.12 (+ 6)
- [x] **P8.3** gate L — rows 8.5, 8.6 (+ 3)
- [x] **P8.4** gate V — rows 8.7, 8.8 (+ 5)
- [x] **P8.5** the call budget — row 8.13 (+ 2)
- [x] **P8.6** the cache key and the file cache — row 8.10 (+ 5)
- [x] **P8.7** transport, the OpenAI-compatible client, the stub server — A8.1 end to end (+ 7)
- [x] **P8.8** cassettes and replay — row 8.11 (+ 5)
- [x] **P8.9** the six escalation predicates, in `oc-core` — row 8.16
- [x] **P8.10** no socket dependency, the CI wiring, the Definition of Done — row 8.15

What a fresh session needs, in the order it matters:

- **`oc-ai` depends on `oc-model` alone** (DECISIONS.md Appendix A; `oc-net → oc-ai` must not
  reach `oc-core`). Every number it needs is a parameter its caller reads from `T`; `oc-core` is a
  *dev*-dependency so the tests use the real values. Gate V's statistics will therefore live in
  `oc-ai` rather than be borrowed from `oc-text` — `docs/DECISIONS_LOG.md`, 2026-09-22.
- **Prompt v1 is frozen.** `crates/oc-ai/prompts/v1.sha256` pins all sixteen artifacts; an edit is
  a `v2` directory and a `PROMPT_VERSION` bump, because the cache key carries the version and not
  the system prefix's hash.
- **Appendix A is not followed in three places**, each logged 2026-09-22: A.2's grammar does not
  load in llama.cpp (a top-level rule ends at its newline, so the role list is parenthesised); the
  held-out probe rides in the `heading_roles` call as ARCHITECTURE §9.6 and PIPELINE §8 say, not in
  a second call as A.3 says; and `system.md` is four byte-identical files, per ARCHITECTURE §9.2.
- **Gate S judges the whole answer** and deserialises straight into each task's wire type — never
  through `serde_json::Value`, which would let a key said twice silently win. Task-level semantics
  (metadata's verbatim check, book-structure index order, verse's line count) are Phase 10's, run
  after gate S. **`Decision.fallback`** is the new field that records why the deterministic answer
  stood — a gate code such as `S.enum`, never the failure's text (D13.9).
- **Gate L compares `C` without the ledger, then reading order.** A ledgered deletion balances
  I-1 and is still a deletion no task may make; a rename cannot reorder or re-encode text either.
  Test 8.5 runs 5 000 generated edits and has a converse, `gate_l_admits_every_rename`, so the
  gate cannot pass by refusing everything. `tests/common/book.rs` builds small `Document`s.
- **Gate V restates oc-text's Gopher statistics** (dup-line, top-2/3-gram, non-alpha words) plus
  heading-tree violations, as ARCHITECTURE §6.2's fixed tuple; undefined is `None` and skipped.
  `gate_v_statistics_are_oc_texts` (oc-text is a dev-dependency) holds the two definitions equal.
  `gates::gate_edit` runs L then V over an applied edit; the caller keeps `before` on `Err`.
  Epsilons: `llm.gate_v_ratio_eps = 0.01` (provisional), `llm.gate_v_violations_eps = 0`.
- **The budget counts requests, cached or not**, so a warm and a cold cache decide the same things
  (D13.8). One budget for all tasks; the degradation order is Phase 10's (test 10.22). A refusal
  is `W_LLM_BUDGET_EXHAUSTED` (registered, en/de/tr templates) and, via `fallback::unasked`, a
  `Decision` with `fallback = "budget.calls"` and no trace.
- **The cache key length-prefixes the model id** — ARCHITECTURE's bare `‖` is ambiguous — and
  the worked example's key is pinned in `the_key_is_the_documented_layout`, because cassettes are
  named by it. `FileCache` reports a damaged or misfiled entry as an error, never as a miss.
- **One client, `openai::OpenAiCompatible<T: Transport>`**, is every model provider: the grammar
  goes out as `grammar` (GBNF), `response_format` (JSON Schema) or not at all, and thinking is
  turned off by `chat_template_kwargs`, `think: false` or `/no_think` on the prefix (D10). A
  separated `reasoning_content` still fails gate S (`gate_response`). The stub in
  `tests/stub_server.rs` answers in all six adversarial ways the plan names, plus that one.
  `llm.temperature = 0.0` (binary) is new.
- **Cassettes replay at the provider seam** — `cassette::Replay` is an `LlmProvider` with no
  transport — because the key carries the prompt version, which never travels on the wire.
  The four committed seeds (`tests/cassettes/<task>/`) are A.3's answers recorded through the
  stub, `model_id = "stub"`; regenerate with `OC_AI_RECORD_SEEDS=1 cargo nextest run -p oc-ai -E
  'test(seeds)'` only after a prompt-version bump, and review the diff. A miss names the nearest
  recording and the byte where the questions diverge.
- **The six escalation predicates are `oc_core::escalation`** (pure: evidence struct + `T` →
  `Verdict::{Fires, Abstains}`), where `oc-structure` and `oc-text` can call them in Phase 10.
  **Open, for Phase 10:** `oc-structure::quotes::classify_indented` widens the `f32` short-line
  ratio to `f64` and so reads a block exactly on `verse.short_line_ratio_min` as a quotation
  instead of ambiguous (`docs/DECISIONS_LOG.md`, 2026-09-22).
- **Test 8.15 walks `Cargo.lock`** from `oc-ai` (a superset of every build's graph), holds its
  socket-crate list equal to `deny.toml`'s `oc-net`-only bans, and scans `oc-ai`'s own sources for
  `std::net` — which no dependency ban can see. CI's `no-network` job runs the whole `oc-ai` suite
  under `unshare -n`.

### Phase 8 — Definition of Done

`IMPLEMENTATION_PLAN.md` §0.3, row by row. Checked on this machine unless the row says otherwise.

| Row | State |
|---|---|
| Every named test exists and passes | **Yes.** All 16 rows, 8.1–8.16, plus 52 additions. 8.16 is in `oc-core`, the other fifteen in `oc-ai`. |
| `cargo nextest run --workspace` green | **Yes**, 559 tests. |
| Green on Linux/macOS/Windows CI | **Not verifiable here.** Windows only; the branch is not pushed. |
| clippy `-D warnings` clean | **Yes**, workspace, all targets, all features. |
| `cargo fmt --check` clean | **Yes.** |
| `cargo deny check` clean | **Yes** — advisories, bans, licences, sources, and the tooling config. No new dependency in a shipped crate's normal graph: `oc-ai`'s is `oc-model` and four crates already in the workspace; `toml`, `proptest`, `oc-core` and `oc-text` are dev-dependencies. |
| `cargo xtask thresholds-lint` clean | **Yes.** Four thresholds added, each with source, evidence, owner and `review_by`. |
| Every Given/When/Then demonstrated | **A8.1–A8.4 yes** (below). A8.3's `unshare -n` step is CI's and unverified until it runs. |
| `docs/CHANGELOG.md` entry | **Yes.** |
| No `TODO`/`FIXME` without an issue number | **Yes**, `xtask ci-lint` clean. |

- **A8.1** — `every_adversarial_answer_leaves_the_deterministic_answer_standing`: the stub's six
  adversarial answers and a separated `reasoning_content`, through the real client and the real
  gates, each leave the deterministic answer and a `Decision` naming the gate. Rows 8.2–8.4, 8.9
  and 8.12 state the same per gate.
- **A8.2** — `gate_l_rejects_any_character_change` over 5 000 generated edits, with its converse
  `gate_l_admits_every_rename`.
- **A8.3** — `cassette_replay_is_offline`: the replay provider holds no transport, so the test
  tiers cannot make a live call; the `no-network` CI job runs the suite where no socket opens.
- **A8.4** — `oc_ai_has_no_socket_dependency`, and `cargo deny check bans` clean.
- **The worked examples of A.3 are `crates/oc-ai/tests/common/mod.rs`** and are the seeds for the
  committed cassettes (P8.8). "One cassette per task per fixture" is read as one per task per
  *named payload*: a payload rendered from a Typst fixture needs Phase 10's inventory builders.

## Phase 6 — what it built

**Phase 6 is complete.** The structural validator (invariant I-7 end to end), the validate→repair
loop, the conversion report, and the CI DOM checks. It is the first phase whose subject is *what to
do when the output is wrong*.

Work items, in order, with the plan's test rows against each:

- [x] **6.1** I-7 and retention — `oc-validate::structural` (rows 6.1, 6.2, 6.3 + 6.1a/6.1b/6.2a/6.3a)
- [x] **6.2** the rest of the structural checks: image parity, note bijection, heading-tree
      sanity, duplicate and quality statistics (row 6.16 + additions)
- [x] **6.3** the repair loop: measure, static table, plan, loop (rows 6.4, 6.5, 6.6, 6.7, 6.9)
- [x] **6.4** the loop wired into the pipeline, fire rate zero on the fixtures (row 6.10)
- [x] **6.5** `report.json`, `--report`, and the post-cap policy (rows 6.8, 6.12)
- [x] **6.6** warning codes and the en/de/tr templates (row 6.11)
- [x] **6.7** Playwright DOM checks (rows 6.13, 6.14, 6.15)
- [x] **6.8** the Tier-3 Ace runner and the nightly `ace-a11y` job

**`oc_validate::structural::validate_structural` is the structural validator**, and its report
holds on all ten fixtures: I-7, image parity, the note bijection, resolving hrefs, a sane heading
tree. `crates/openconvert/tests/snapshots/structural__structural_report_per_fixture.snap` is the
measurement; three findings are carried out of it, all three in `docs/DECISIONS_LOG.md`, 2026-09-18:

- **Retention is a flag, not a gate.** Four fixtures land at 0.968–0.973 because furniture removal
  is part of `C_0`, and `validate.min_char_retention = 0.98` is jointly unsatisfiable with the 0.04
  furniture budget. Appendix D's v1.0 retention gate needs Phase 7's strata to be stated at all.
- **The Gopher n-gram statistics cannot gate output text.** `f09` scores `top_3gram` 0.8008 against
  a 0.18 bound, and is correct: its printed contents page is hundreds of `. . .` leaders. The nine
  statistics are recorded; what warns is `DuplicateStats` over emitted **blocks**, at
  `validate.dup_block_frac = 0.02`.
- **The h1-count range is scoped by page count** (`validate.h1_count_min_pages = 20`), so every
  fixture answers `None` and the check is first exercised on the Phase 7 corpus.

**`oc-validate` gained `oc-text` and `oc-core` as dependencies**, which ARCHITECTURE §3.1's table
does not list; the argument is in the same log entry and in `crates/oc-validate/Cargo.toml`.

**`oc_validate::repair` is the loop.** A repair edits the `Document` and the book is re-emitted; it
never patches the zip. All four control rules are in place and each was mutation-tested — deleting
any one of strict decrease, the new-id rule or the hash rule turns exactly one of rows 6.4/6.5/6.6
red. The loop takes an `Emit` trait rather than a concrete emitter, because the emitter passes
EPUBCheck clean on every fixture and therefore cannot be made to oscillate: the cases that matter
only exist against a double. `Emit::apply` is provided (it calls `apply_fix`) so that the control
flow and the table's edits are testable apart.

**The repair table has three `AutoFix` entries, not thirty.** `ACC-001` describes a figure whose
`alt` is empty; `RSC-012` demotes a note reference whose target does not exist to plain text;
`OPF-003` drops a figure nothing references and that carries no caption. All three are Conserving:
`alt` is an attribute, a `noteref` is a link, and an uncaptioned figure carries no characters. Six
more ids are `WarnUser` — understood, and with no fix that would not change the book. Everything
else is unmapped and reported verbatim. The plan's "~30 ids" would have been thirty untested paths
for defects this emitter has never produced.

**The loop is on the real path and fires zero times on all ten fixtures** (row 6.10, A6.2). The loop
owns the emission: `epub_stage` is split into `build` and `epub_check`, because a `convert` that
built the book once for the conservation check and again for the loop would re-encode every image
twice, and PIPELINE §12 budgets one regeneration *per iteration*. `Conversion` now carries `tier1`,
`structural` and `repair` alongside the document and the bytes.

**`validate` and `repair` are declared stages and are in the ledger**, both Conserving. `repair`'s
check is the one that earns its keep: it compares `C` of the document the loop was given against `C`
of the document it settled on with an empty ledger, so a repair that changed one character of the
book fails I-1 and the conversion stops — PIPELINE §12's "none of them may change the character
content of the book", stated as an invariant instead of as a property of three functions. `repair`
is Conserving with an *empty* reason set: PIPELINE §12 words it "except `UserOverride`", and
declaring a reason a Conserving stage may never cite (I-3) would be a contract contradicting itself.
The stage becomes `Budgeted` when the review UI arrives in Phase 12.

**A defect found on the way: the `document` stage's conservation check was never recorded.** It ran
— a violation returns `DocumentError` — but `convert` dropped the `StageCheck`, so the ledger named
seven stages where the pipeline had checked eight, and every phase's "checked after every stage"
claim was one stage short in its evidence. Fixed, and `repair::the_ledger_records_validate_and_repair_as_conserving_stages`
asserts the whole list in order.

**`report.json` is written on every conversion**, versioned `openconvert.report/1`, to
`<output>.report.json` or wherever `--report` says, and **before** the atomic rename so a report
exists even for a conversion whose output could not be placed. It lives in `openconvert::report`
rather than in `oc-core` as the plan's file list has it: assembling it needs `Tier1Report`,
`StructuralReport` and `RepairOutcome`, which are `oc-validate`'s, and `oc-validate` depends on
`oc-core`.

`PROVENANCE` now carries `owner` and `review_by` as well as `source` and `evidence` — "this number
is provisional" is only actionable with "owned by whom, revisit by when" beside it — so
`build.rs` emits a `Provenance` struct instead of a 3-tuple. `convert` times each stage and carries
the producer family and the page-class histogram. The NDJSON `warning` event carries `args` now;
Phase 5 emitted the code with an empty object.

**Row 6.8's cap is demonstrated at two levels and neither is a whole-pipeline run**, because it
cannot be: the emitter passes EPUBCheck clean on all ten fixtures, so a real conversion reports
`Clean` before any repair runs. `repair_loop::the_cap_stops_a_loop_that_would_otherwise_keep_going`
exercises the cap against a scripted emitter, and `report::repair_cap_writes_epub_and_marks_invalid`
asserts the policy — report `invalid`, remaining ids verbatim, the EPUB still there. Forcing three
failing iterations out of a correct emitter would make the assertion about the break.

**Twenty-six warning codes, three locales, and a lint that keeps them total.** `oc-core::warnings`
holds the registry (`codes.rs`: the code and the argument names it carries) and the three template
files; `render` fills `{slot}`s and leaves an unfilled one *visible*, because a warning missing its
number has stopped being a factual claim and a brace is a bug report where a blank is a mystery.

The gate has four parts and each was mutation-tested: every code has a template in every locale, no
locale has a template for a code the registry does not know, every `{slot}` is an argument the code
declares, and the English text uses every argument its code carries. The registry cannot be derived
— `oc-structure` owns `W_TABLE_AS_IMAGE` and `oc-core` cannot depend on it — so
**`xtask ci-lint` holds the registry and the tree in agreement in both directions**: a `W_…`
constant anywhere with no entry fails, and an entry nothing defines fails.

`--locale en|de|tr` is new and is not in §2.1's flag list; §2.2's job spec has the field. Without it
the engine's own localisation would be unreachable from the command line and the three template files
would be readable only by a GUI that arrives in Phase 12. The CLI prints the rendered sentences to
stderr only when stderr is *not* the NDJSON channel.

**The DOM checks run, and the `dom-checks` CI job is on.** 198 assertions across three Chromium
viewports (600×800, 390×844, 1024×768), all green, no screenshots anywhere. WebKit fills the
nightly `webkit-dom` job. Each of the three specs was mutation-tested: a 3000px block trips the
overflow check, two swapped nav entries trip the order check, a `display: none` footnote trips the
note check.

`cargo run -p xtask -- dom-fixtures` converts every fixture through the one pipeline the CLI drives
and unpacks the containers into `target/dom/<fixture>/`, with a manifest holding the **spine** and a
**note-reference inventory** — the two things a browser cannot enumerate from inside one document.
The nav order is read *in the browser*, deliberately: a spec that read one side of the heading-order
comparison out of JSON our own Rust wrote could not fail when the nav itself was wrong.

**A nav entry is not always a heading**, and the first draft of the order spec assumed it was. The
unheaded preamble becomes a front-matter section so nothing is lost (PIPELINE §8), and the nav names
that section, which has no heading in it — f02, f03 and f07 all failed until the claim was restated
over nav *targets*.

Two emitter properties the specs found already true, and worth knowing: `pre { white-space: pre-wrap;
overflow-wrap: break-word }` means a 400-character unbroken line does not overflow a phone, and the
table rules keep a wide table inside its column. The plan named both as the cases row 6.13 would
fail on.

**Tier 3 is wired.** `oc_validate::ace` runs Ace by DAISY and reads its report; the nightly
`ace-a11y` job installs `@daisy/ace` and turns the `ace` cargo feature on. Both halves of
PIPELINE §11's gate are asserted — zero serious violations, **and** every required accessibility
metadata field present — because a book with no `schema:accessMode` passes every structural check
and leaves a screen-reader user unable to decide about it before opening it. `critical` counts as
serious: a gate written against the word alone would pass a book with a critical violation in it.

The metadata half is also asserted **without Node**, on every fixture, by reading the package
document: what the gate really claims about those properties is that the emitter writes them, and
that claim should not be testable only in a nightly job.

**What Phase 5 hands it**, in the order it will be wanted:

1. **`openconvert::convert::convert` is the one pipeline.** It runs every stage, checks each under
   the conservation law, and returns `Conversion { document, built, extracted_images }`. The CLI,
   the tests and (from Phase 12) the desktop app all take that path. `document.ledger` carries
   every stage's `StageCheck`, including `epub`'s.
2. **`oc_validate::validate_tier1` is the issue source the repair loop consumes.** Its `Finding`
   carries an EPUBCheck message id where one exists (`RSC-005`, `RSC-007`, `RSC-012`, `OPF-014`,
   `OPF-003`, `OPF-012`, `OPF-030`, `OPF-060`, `PKG-007`, `PKG-008`, `RSC-002`, `ACC-001`) and an
   `OC-…` id where it does not (`OC-SCRIPT`, `OC-REMOTE`, `OC-ENTITY`, `OC-NOTE-BIJECTION`,
   `OC-IMAGE-PARITY`). `Severity` is already the `(fatal, error, warning)` the repair loop's
   lexicographic measure needs (D13.7).
3. **I-7 is one function call away.** `document.ledger.removed_all()` / `added_all()` are the two
   halves, `ledger.c_0` is the baseline, and `oc_epub::textcontent::body_text` is how `C(EPUB)` is
   measured — by parsing the emitted documents, not by asking the emitter.
4. **The repair-fire rate is a release metric with target zero.** Every repair that fires is an
   emitter bug, so Phase 6 starts from an emitter that EPUBCheck already passes clean; a repair
   that fires on a fixture means something regressed.
5. **`Document.warnings` is where the report's issue list comes from.** Phase 5 added
   `W_EPUB_LARGE`, `W_XHTML_OVERSIZE`, `W_PAGE_BREAK_UNPLACED`, `W_NO_TEXT_EXTRACTED`.
   `BuiltEpub.warnings` carries codes rather than `Warning`s — `oc-epub` does not depend on the
   pipeline — and nothing yet attaches them to the document; that is Phase 6's report to do.
6. **Fixture numbers.** Taken: **f01–f10**, **h01–h29**. Next free: **f11**, **h30**.
   No new fixtures in Phase 5: the emitter's subject is the ten documents that already exist.
7. **Still open, and cheap:** CI's `test` job runs `xtask fixtures` but never `handmade-fixtures`
   or `mutations`, so a builder change that no longer reproduces the committed fixtures is not
   caught. A `--check` mode on those two tasks would close it.

## Notes

Carried forward, in the order a fresh session needs them:

- **`furniture` recovers no folio from a book that changes numbering system.** `f09` paginates
  `i, ii` then `1, 2, 3`; digit masking puts the three arabic folios in one group covering 3 of 5
  pages, a repetition ratio of 0.6, inside the grey zone where the detector abstains. The folios
  stay in the flow as one-character paragraphs and every `PageRef.label` is `None` — which also
  removes the arabic-1 reset PIPELINE §9 step 1 calls a hard boundary signal. The fix is a change
  to how a folio group is scoped (per numbering system, or per pagination run) and wants the
  Phase 7 corpus to choose between them. `docs/DECISIONS_LOG.md`, 2026-09-14.
- **Two of four real books outside the corpus are refused by the conservation law**, both in
  `structure`, and both are Phase 7's to calibrate rather than Phase 5's to guess at:
  - *AI Engineering* (O'Reilly, ~500 pp): `structure` emits 21 526 characters twice. The ruled-
    table detector finds **365 tables** in a book that has perhaps twenty — it fires on figure
    boxes and code blocks — and `tables.consumed` does not cover every block whose text it
    claimed, so the same lines are in a table's cells *and* in the flow. Attributed by source,
    every duplicate but three involves a table (`list+table` 84, `caption+table` 47,
    `heading+table` 16). `table.{min_row_rules, min_column_rules, grid_snap_pt, rule_overlap_min}`
    are all `provisional` and have never met a real book.
  - *Aus dem Leben eines Taugenichts* (Project Gutenberg): 66 730 characters **lost** with no
    ledger entry — the other direction, and a different bug.
  - *Tschick* and *O Crime do Padre Amaro* convert clean, and EPUBCheck reports 0 errors and
    0 warnings on both. The law refusing two books rather than shipping duplicated or missing
    paragraphs is it working.
- **A fallback table is emitted as a grid, not as an image plus `<details>`.** PIPELINE §8.7 wants
  the image; rasterising a vector region needs a page renderer in `oc-pdf` that does not exist.
  No text is lost either way, and `El::details` is written and tested for when it does.
- **A tightly set ruled table falls back to an image.** When a cell gutter is narrower than
  `text.line_split_gap_em`, `words` keeps two cells in one run and a `Run` carries a box and its
  text but not its glyphs' positions, so nothing in `structure` can split it. The long-term fix is
  to split runs at vertical rules in `layout`, which needs the rules to reach `layout`.
- **A contents page with no drawn leader is not parsed.** `"Preface    i"` reaches the parser as
  `"Preface i"` — the gap is geometry and a line's text is not.
- **`Note.body` is one paragraph.** A note that runs to several paragraphs is one paragraph here.
- **`RunId` is page-local**, whatever IR_SKETCH calls it. Every map from a run is keyed on
  `(page, RunId)`; `oc_structure::build::NoteRefRuns` names the pair once.
- **Language detection is not wired into the driver.** `convert` uses the configured tag and falls
  back to `LangTag::UND`. `whatlang` is a Phase 2 capability that the stage driver never calls.
- **EPUBCheck and its corpus are fetched, never committed**: `cargo run -p xtask -- fetch-epubcheck`
  and `fetch-epubcheck-corpus` put them under `vendor/`, which `.gitignore` covers.
- Local tool versions: rustc 1.98.1, cargo-nextest 0.9.143, cargo-deny 0.20.2, EPUBCheck 5.3.0,
  Temurin-compatible JVM 23 locally / Temurin 21 in CI, Python 3.13.7 + uv 0.11.28.
- **The Python side runs out of `eval/.venv`** (`uv venv eval/.venv && uv pip install --python
  eval/.venv -e "eval[dev]"`). On the maintainer's Windows box `python3` exists only because a
  copy of `python.exe` was placed beside it under that name; CI's Linux runners have the real one.

## Phase 7 — what it has built so far

- **`oc_eval.corpus.download`** fetches mirror-first, verifies the manifest's sha256 before the
  bytes are placed, and deletes a file that fails rather than quarantining it. The opener is a
  parameter, so no test needs a connection; the default refuses any URL that is not https.
- **`oc_eval.corpus.manifest`** is the vocabulary — licence allowlist and blocklist markers, the
  nine producer strata plus `page-level-layout`, and the document-vs-page unit.
  **`oc_eval.corpus.lint`** is fourteen rules over it, one stable slug each, and the slugs are
  the contract the tests assert on.
- **`oc_eval.thresholds`** reads `thresholds.toml`, so the corpus gates are the same numbers in
  Python and in Rust rather than two that agree today.
- Two rules needed a judgement the plan does not make. `tagged-share-off-target` is skipped when
  the synthetic bucket is too small to land within five points of 0.126 at all — a five-file
  bucket can be 0.0 or 0.2 tagged and nothing between, and reporting that is reporting
  arithmetic. `is_page_level` deliberately does not trust the `unit` field alone: a DocLayNet
  entry is page-level whether or not it says so, and `page-unit-undeclared` separately requires
  it to say so, which is what closes §7.6's per-page shortcut.
- **`mypy` is clean on everything Phase 7 has written** and reports 13 errors in two Phase 2
  files (`generate/scan_sim.py`, `train/hyphen_clf.py`). Item 7.10 owns them, because that is
  where `mypy` becomes a CI gate.

### Item 7.3 — how the corpus was assembled

- **Five adapters, each a pure parser plus a thin fetch loop.** OAPEN's DSpace
  `/rest/filtered-items` filters on `dc.rights.uri` and `dc.language`, so the CC-BY subset and
  the German slice are selected by the catalogue rather than by downloading and hoping.
  Internet Archive filters on `licenseurl` and then picks the scan out of an item's files,
  avoiding the `_text.pdf` and `_djvu.pdf` sidecars. arXiv reads OAI-PMH `arXivRaw`, because
  the Atom search API reports no licence at all and a paper with none is under arXiv's own
  distribution licence, not ours. US-Gov is NASA's NTRS, admitting only
  `copyright.determinationType == "GOV_PUBLIC_USE_PERMITTED"`.
- **The Turkish slice was rewritten mid-item.** Reading the licence off a DergiPark article
  page admitted one article in ten — most DergiPark journals are CC BY-NC-ND. Selecting
  **journals** from DOAJ by their curated `license.type` and then taking a few articles from
  each admits at the journal's rate instead, and DOAJ's `fulltext` link for those journals is
  the direct PDF, so nothing scrapes a page any more.
- **Nothing enters the manifest on a promise.** Every entry's `sha256`, `pages`, `tagged` and
  `producer_raw` come from opening the file that actually arrived. 45 candidates were rejected
  with a named reason across the four runs — duplicates, landing pages that were not PDFs,
  files over the 40 MB per-file budget, documents under four pages.
- **A top-up harvest needed a fix to work at all.** A source adapter re-walks its catalogue
  from the start, so `usgov=2` against thirteen NASA reports already held yielded thirteen
  duplicates and nothing else. `admit(want=…)` caps what one call admits and `ask_for` clears
  the held count, so the generator is drawn further down the catalogue and stops as soon as it
  has enough.

### Item 7.4 — the mutation catalogue

Ten recipes in `oc-testkit::mutate`, listed as data in `xtask::mutations::catalogue()` so that
row 7.6's "every recipe" has one place to be asked. Five are new: `strip_structtree`,
`double_draw`, `ocr_sandwich`, `jitter_spacing`, `damage_xref`.

Each recipe declares two things the tests hold it to.

- **`Reproducibility`.** Running `xtask mutations` twice showed the three encrypted mutants
  rewritten with different bytes every time — 902 bytes on one run, 903 on the next — because
  AES-128 draws a fresh initialisation vector per string and stream. Row 7.6's byte-for-byte
  assertion holds for the seven `Deterministic` recipes; the three `Randomised` ones are
  asserted on the property that makes byte-equality impossible, that two applications differ.
  `xtask mutations` now leaves an existing randomised mutant alone, because regenerating it
  replaced a regression artefact with noise.
- **`Effect`** — what the recipe does to the characters on the page, checked through PDFium
  against the parent. `Preserves` (cropbox offset, struct-tree strip, jitter, damaged xref),
  `Duplicates` (double draw, OCR sandwich — exactly twice the characters, the original first),
  `BreaksTextMapping` (ToUnicode strip), `Unopenable` (encrypted with a user password).
  Flipping one declared effect turns the test red, which is how it is known not to be vacuous.

**Type 3 re-encoding is the one item of PHASE 7 §4's list that is not done.** Re-encoding an
embedded font as Type 3 while keeping its outlines needs a glyph-outline extractor `lopdf` does
not have, and a Type 3 font whose CharProcs draw rectangles would change what the page looks
like rather than only how it is encoded. It belongs with the handmade fixtures — a small PDF
authored as Type 3 with a `/ToUnicode` map — and is a gap, not a mutation.
`docs/DECISIONS_LOG.md`, 2026-09-20.

### Item 7.5 — ground truth

Three sources, one type. `oc_eval.ground_truth.schema.GroundTruth` is TEST_CORPUS §5.1's shape
— headings, paragraphs, footnote pairs, figures — whatever produced it, so the scorer has one
thing to compare against and a fourth source would not touch it.

**The assertions are the engine's own vocabulary.** `to_assertions` emits `heading_tree`,
`text_present`, `block_count`, `image_count`, `note_bijection`, `lang_tag` and `text_order` —
the same kinds `oc_testkit::assertions` reads and the committed `.assert.json` fixtures are
written in — so one runner checks a generated expectation and a hand-written one.
`test_every_generated_assertion_kind_is_one_the_engine_knows` reads the Rust enum rather than
keeping a second copy of the list, the same trick `xtask ci-lint` uses for the warning registry.

**A ground truth never invents what its source does not say.** A tagged PDF's `/H1` points at
marked content, not at characters, so a struct-tree heading has a level and no text — and
`to_assertions` emits no `heading_tree` or `heading_level` for it, because asserting on an
empty string would score every document as wrong. The same rule drops a noteref whose endnote
is in a file that was not read, and withholds `note_bijection` when the notes do not pair.

- `from_xhtml` reads Standard Ebooks markup. Heading levels come from `<section>` nesting, not
  from the tag number: SE writes a chapter title as `<h2>` because the book's `<h1>` is its
  title page, and comparing `h2` against `h1` would score a correct conversion as wrong.
- `from_structtree` reads a tagged PDF's own tree. On the tagged fixture it finds one `/H1`
  and four `/P`, which is the document.
- `from_latex` reads arXiv sectioning, and `looks_parseable` is §5.2's "curated
  parses-cleanly subset" as a predicate — a paper that pulls its sections in through `\input`
  is refused **before** it is scored against rather than after it has quietly scored zero.

`corpus/gt/se_sample/` is a small SE-shaped book written for this repository: a chapter with
two heading levels, four paragraphs, two noteref/endnote pairs and a captioned figure.

### Item 7.6 — the metric suite

Eight modules under `oc_eval.metrics`, TEST_STRATEGY §8.1's table one function at a time:
`cer` (text NED after D13.4's normalisation), `reading_order` (edit similarity plus Kendall
tau), `toc_f1` (heading P/R/F1 and outline edit distance), `footnotes`, `images`, `teds`
(TEDS and TEDS-S), `prf` (the shape the four set-metrics share) and `report`.

Two decisions carry the phase's weight.

- **A pass rate is an interval, not a number.** 94 of 100 and 940 of 1000 are the same rate
  and not the same evidence. `assertions.PassRate.interval()` is Wilson's, pinned against the
  published value for 95/100 — (0.8882, 0.9785) — because the normal approximation is wrong
  exactly where this gate lives, at p near 1. The gate is last-green minus the half-width, and
  a suite below `eval.assertion_min_instances` **fails and says why** rather than passing on
  an interval wide enough to admit any regression.
- **There is no aggregate row and there never will be.** `report.build` emits `per_file`,
  `per_stratum` and `ours_vs_real`, and a test asserts no `aggregate` key appears — an average
  across strata is precisely the number that lets a synthetic win mask a real-book regression,
  which is what D18 exists to prevent. A report with no real strata states no gap rather than
  inventing one.

The metrics normalise before they measure. `cer.ned("ﬁre", "fire")` is 0.0 and
`cer.ned("pipe-
line", "pipeline")` is 0.0, because the pipeline is *supposed* to fold those
(D13.4's `N`) and a metric that charges for them scores a correct conversion as wrong — R9
§A.10's gap in Nougat's metric. What is not folded is anything the pipeline may not change: a
hyphen inside a line is content and still costs.

Two new thresholds: `eval.assertion_confidence` (0.95, binary — the plan's level) and
`eval.assertion_min_instances` (30, provisional — where the Wilson half-width at p ≈ 0.95
falls under eight points).

### Item 7.7 — the trend

`oc_eval.trend` keeps the `ours(*)`-versus-real gap in `eval/out/trend.json`, in the
repository, because TEST_STRATEGY §8 wants a regression to be a diff against the
immediately-prior committed baseline. One entry per commit: re-running the nightly on an
unchanged tree corrects the record rather than doubling it.

The point is the **direction**, not the number. A gap of 0.06 is fine or alarming depending on
whether last week's was 0.05 or 0.09, so `widening()` reports first-to-last and returns None
when the history holds fewer than two runs that stated a gap at all — a run with no real strata
states no gap, and is not evidence about widening either. `plot()` draws it to a PNG under a
headless backend.

### Item 7.8 — the holdout refusal

TEST_CORPUS §7.1(c) as a mechanism rather than a sentence. Every fit goes through
`calibrate.fit`, which loads the manifest and raises `HoldoutLeak` on any id marked `holdout`.
Two details make it hold:

- **an unknown id is refused too** (`UnknownFile`). An id the manifest cannot account for is
  not evidence that it is not holdout, and "I could not check" must not read as "it is fine";
- **one offending file stops the whole fit.** Dropping it and carrying on would produce a
  number that looks fitted on what was asked for and was not.

`risk_coverage` is where an escalation threshold actually comes from (D17): sort by
confidence, and report the widest coverage whose risk is still under the target — returning
None when nothing meets it, rather than the best available dressed up as a hit. `reliability`
is the ECE and the diagram rows behind it.

### Item 7.9 — the performance budget

**Measured: 0.0217 s/page over 300 pages**, against D13.11's 0.5 — a 23x margin, in an
unoptimised `test` profile on the maintainer's machine. The gate was confirmed to fail when the
budget was lowered below the measurement, so it is not vacuous. Machine L's number will differ;
this is a floor on the headroom, not the reference measurement.

`oc_testkit::handmade::reference_book(pages)` builds the input rather than committing it: three
hundred pages of prose is a megabyte of fixture nobody would regenerate or review. It is
deterministic, and it carries what makes a book *expensive* rather than merely long — a running
head and a folio on every page for furniture detection to find, a chapter opening every twenty
pages, body lines at a real leading.

The split: the **arithmetic** runs on every PR — that row 7.12's five stage budgets sum to
`perf.seconds_per_page_max`, that each is a positive share of it, that the reference book is the
300 pages D13.11 states the budget for — because that is where the mistake that actually happens
gets caught, a stage quietly given room the whole does not have. The **timed** assertions are
behind the `bench` cargo feature, which the nightly job turns on: a wall-clock assertion on a
shared CI runner measures the runner, and a gate that fails for that reason is one people learn
to re-run until it passes.

`oc_eval.bench.peak_rss` measures the **whole process tree** — `/proc`'s `VmHWM` on Linux, a Job
Object on Windows, `psutil` polling otherwise — because D13.11's 500 MB is for the converter and
whatever it spawns, and a figure that counts only the parent stops being true the moment Phase
9's sidecar exists. It says which mechanism it used and whether the answer is exact, and a
sampled floor is printed as a floor.

Six new thresholds: the five per-stage budgets and `perf.bench_reference_pages`.

## Blocked

STATUS stays IN_PROGRESS: each item below was decided in the most conservative way consistent with
DECISIONS.md, logged in `docs/DECISIONS_LOG.md` (2026-09-23) as **PROVISIONAL — needs maintainer
ratification**, and worked around. None of them blocks Phase 11's work.

1. **Registry pins** — `models.toml` still has `TODO_` `revision`/`sha256` and `size_bytes = 0` for
   all four entries: huggingface.co is refused by this sandbox's egress policy. Fill them on a
   machine that can reach it with `eval/model_gate.py --emit-registry`, which hashes the download
   itself. Until then `model list`/`pull` on the bundled registry exit 2.
2. **llama.cpp digests** — filled after the merge (`fix/phase-09-llama-pins`). They were read from
   GitHub's releases API and then checked by download: all four `b10456` assets match in digest and
   size. **PROVISIONAL — needs maintainer ratification: verify by downloading** (run
   `cargo run -p xtask -- fetch-llama-server` on a maintainer machine; DECISIONS_LOG 2026-09-23,
   "llama.cpp `b10456` digests filled, then checked by download"). The nightly live job now fails
   at `model pull` (item 1), not at the server fetch.
3. **`LLAMA_API_KEY`** — the sidecar's key goes in that environment variable, never in argv. The
   pinned build's `--help` lists `(env: LLAMA_API_KEY)` under `--api-key`. Whether a running server
   enforces the key still needs a model, which cannot be fetched here. `--api-key-file` is the
   fallback.
4. **The allowlist's CDN hosts** are the plan's three, and today's redirect target could not be
   observed. An off-list redirect fails closed and names the host.
5. **PDEATHSIG and Windows job objects** (crash and SIGKILL teardown) need `unsafe` and are
   deferred to Phase 14.
6. **G3/G7/G9 inputs, and G7's numbers** — no reference tokenizations, no paired answers, no own
   conversion. `model_gate.g7_alpha = 0.05` and `g7_noninferiority_margin = 0.02` are invented.
7. **`apps/desktop/src-tauri/src/llm.rs`** is deferred to Phase 12, which owns the desktop app and
   is being built concurrently.
8. **G8's probes are generated, not native-speaker authored**, and their labels follow from their
   templates.

Phase 10's, each logged in `docs/DECISIONS_LOG.md` (2026-09-23):

9. **The language maps ship empty** (`[ai.task.<task>.languages] = []`): no task has passed an
   evaluation, so `--ai` alone asks nothing, and `--ai-all-tasks` is the flag an unproven task stays
   behind. Enabling a language needs a McNemar run on the real strata (A10.4/A10.5).
10. **Book-structure boundaries are strictly increasing**, `front == parts[0]` included, although
    the frozen v1 prompt makes that a legitimate answer for a book whose body opens with a part.
    Ratify `≤`, or write a v2 prompt.
11. **What a heading mapping may change:** size-rank levels only; `body`/`epigraph` demote,
    `other`/`caption`/`running_head` change nothing; a mapping that demotes every heading is
    refused; no silhouette check (none is computed); run-in candidates do not ride along (no slot in
    the v1 payload); fewer than 8 held-out lines → no call; no size-rank heading → no call
    (`pregate.headings`).
12. ~~**A non-loopback `--llm-endpoint` is exit 2** until Phase 11 brings consent (D10).~~
    **Closed by Phase 11:** it is still exit 2 without consent, now `E_CONSENT_REQUIRED`; with
    `--llm-allow-host <HOST>` naming it (and `https://`) it is used, and the report records it.
13. **The wall-clock stop raises `W_LLM_TIME_EXHAUSTED`**, not detail 6's `W_LLM_BUDGET_EXHAUSTED`,
    whose template speaks of calls.
14. **Invented numbers**, each `provisional` with owner and `review_by`: the chunk overlap (20), the
    metadata size-category ratios and input cap, the deep-indent em, the centred-cluster ratio, the
    sidecar timeouts, and `ai_eval.{alpha, noninferiority_margin}`.

Phase 11's, each logged in `docs/DECISIONS_LOG.md` (2026-09-23):

15. **Plain `http://` to a host off this machine is refused even with consent** (`PlaintextRemote`):
    D10 is silent on the scheme; consent to a host reading the text is not consent to the path.
16. **Ollama speaks `/api/chat`**, although PHASE 11 detail 1 says every provider speaks `/v1`:
    Ollama's `/v1` layer silently drops the `num_ctx` and `format` D10 requires. Three invented
    thresholds: `llm.ollama_{num_ctx, template_overhead_tokens, keep_alive_secs}`.
17. **A generic OpenAI-compatible server is probed as constraining nothing** (schema in the prompt,
    `W_LLM_UNCONSTRAINED`): a `GET` cannot show that `response_format` is honoured.
18. **Consent at the command line is `--llm-allow-host <HOST>`**, naming the endpoint's host; the
    job spec's `non_loopback_consent: true` is consent to its own endpoint's host.

### Blocked — Phase 13 (each PROVISIONAL, logged in `docs/DECISIONS_LOG.md` 2026-09-23)

- **P13-a `BrokenText` pages are not OCR'd.** A visible broken layer cannot coexist with an `Ocr`
  region under I-6, and no declared `Reason` removes visible text for being unreadable. The page
  keeps its text and `W_BROKEN_TEXT_PAGES`. Ratify one of: a new `Reason`, widening
  `OcrLayerDuplicate`, or "v1 does not OCR broken-text pages".
- **P13-b re-OCR's `OcrLayerDuplicate` removal is not budget-charged.** `ingest` budgets are deferred
  to `text` (PIPELINE §3) and re-OCR replaces a whole layer by design; decide whether it needs its
  own budget.

## Phase 7 — Definition of Done

`IMPLEMENTATION_PLAN.md` §0.3, row by row. Checked on this machine unless the row says otherwise.

| Row | State |
|---|---|
| Every named test exists and passes | **Yes.** All 14 of PHASE 7's rows, plus ~180 additions. Rows 7.11 and 7.12 are behind the `bench` cargo feature and pass with it on. |
| `cargo nextest run --workspace` green | **Yes**, 466 tests. |
| Green on Linux/macOS/Windows CI | **Not verifiable here.** Windows only. |
| clippy `-D warnings` clean | **Yes**, workspace, all targets, all features. |
| `cargo fmt --check` clean | **Yes.** |
| `cargo deny check` clean | **Yes** — no new Rust dependency; `criterion` was already in the workspace manifest. |
| `cargo xtask thresholds-lint` clean | **Yes.** Eight thresholds added, each with source, evidence, owner and an unexpired `review_by`. |
| Every Given/When/Then demonstrated | **A7.1, A7.1b, A7.2 and A7.3 yes** (below). **A7.4 partially**: the gate is written and tested, and there is no "last green" until the nightly has run once. |
| `docs/CHANGELOG.md` entry | **Yes.** |
| No `TODO`/`FIXME` without an issue number | **Yes**, `xtask ci-lint` clean. |

### Acceptance criteria

- **A7.1** — `oc-eval corpus lint` is clean. `ours(*)` is 0.103 against the 0.40 cap; the
  holdout is 104 **documents**, counted with page-level entries excluded.
- **A7.1b** — the composition meets §7.6's target as written: OAPEN 44 (~40), Internet Archive
  20 (~20), arXiv 15 (~15), US-Gov 15 (~15), DergiPark 10 (~10), and the German (34) and
  Turkish (10) slices are non-empty.
- **A7.2** — `report.build` emits `per_file`, `per_stratum` and `ours_vs_real`, and
  `test_per_stratum_scores_are_reported_separately` asserts no `aggregate` key exists.
- **A7.3** — **0.0217 s/page over 300 pages**, unoptimised `test` profile, against 0.5. The
  gate fails when the budget is lowered below the measurement. Reference machine L will differ.
- **A7.4** — the gate is `last green - margin` where the margin is the Wilson half-width, and
  it refuses to gate at all below `eval.assertion_min_instances`. The first nightly records the
  baseline it will compare against.

### What Phase 7 was asked to settle, and did not

The seven items the phase was carrying are not all closed, and it is worth saying which.

1. **`validate.min_char_retention` is still not a gate.** The corpus now exists to choose
   between the two candidate definitions, and nothing has run against it yet. Still open.
2. **`validate.h1_count_min_pages = 20`** is first exercisable now — 104 real documents,
   most well over twenty pages — but the first exercise is the nightly's, not this branch's.
3. **`validate.dup_block_frac = 0.02` has still met no real book.** Same reason.
4. **The two real books the conservation law refuses** have not been re-run. They are in
   `example_pdfs/`, which is not corpus, and the table thresholds are still `provisional`.
5. **A2.3, A3.2, A3.3 and A4.3** are still partial. The corpus exists; the gold data for them
   does not, and `corpus/gt/` holds one hand-written sample rather than §5.5's ~50-file set.
6. **The benchmark harness is done**, and A1.6 is a measurement on this machine rather than
   on reference machine L.
7. **Repair fires per id per stratum** — `oc-eval run` records `repairs_fired` per file and
   the report groups per stratum, so RT A10.4 is answerable on the first nightly.

## Phase 6 — Definition of Done

Checked against `IMPLEMENTATION_PLAN.md` §0.3 on 2026-09-18, every gate run for real. **Two rows
cannot be verified on a single machine** and are named as such rather than counted as passes.

1. **Every named test exists and passes** — all sixteen rows of the Phase 6 table (6.1–6.16), each
   run individually by name, plus about sixty additions. `docs/TEST_MATRIX.md` lists every one and
   the CI job that runs it. Rows 6.13–6.15 are the Playwright specs (44 + 20 + 2 assertions at one
   viewport, 198 across three); row 6.25 is behind the `ace` cargo feature; neither is `#[ignore]`d,
   which `xtask ci-lint` enforces.
2. **`cargo nextest run --workspace` green on three OSes** — **cashed.** CI run 35462059355 on `main`:
   `test (ubuntu-latest)`, `test (macos-latest)` and `test (windows-latest)` all green, alongside
   every other job. 456 locally. It was **not** green on the first attempt: macOS failed
   `golden_epub_bytes_f01` and Ubuntu ran out of disk, both for real reasons, both fixed.
3. **`cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`** — clean.
4. **`cargo fmt --all --check`** — clean.
5. **`cargo deny --all-features check`** — advisories, bans, licences, sources ok.
6. **`xtask thresholds-lint`** — clean; four new entries, each with the five keys. **`ci-lint`** —
   clean, including its new rule that the warning registry and the tree agree in both directions.
7. **Every acceptance criterion demonstrated:**
   - **A6.1** (I-7 holds) — `i7_holds_end_to_end_on_all_fixtures` over all ten, measured over the
     **archive** through the package document's spine, plus
     `the_archive_and_the_emitter_agree_about_the_text` for the two measurements agreeing. "Every
     corpus file" is Phase 7's corpus; the same scope limit as A2.3 and A3.2, for the same reason.
   - **A6.2** (zero repairs fire) — `repair_fire_rate_is_zero_on_corpus`, against
     `repair.corpus_fire_rate_max = 0`. Every fixture reports `RepairStatus::Clean`.
   - **A6.3** (≤ 3 iterations, strictly decreasing `M`, or a named status) — the four control rules,
     each **mutation-tested**: deleting strict decrease, the new-id rule or the hash rule turns
     exactly one of rows 6.4/6.5/6.6 red, and nothing else.
   - **A6.4** (the report's contents) — `the_report_carries_every_part_the_plan_names` asserts each
     part the plan's detail 6 names, by name; `report_schema_is_valid_and_snapshotted` snapshots
     `f07` with the timings redacted.
   - **A6.5** (3 viewports, no overflow, nav/DOM agreement, noterefs resolve) — **cashed.** The
     `dom-checks` job passed on its first real run in CI 35462059355, and nightly `webkit-dom` passed
     too, so the assertions hold in both engines. 198 Chromium assertions, each spec
     mutation-tested (a 3000px block, two swapped nav entries, a `display: none` footnote).
8. **`docs/CHANGELOG.md`** — Phase 6 entry written.
9. **No `TODO`/`FIXME` without an issue number, no `#[ignore]`** — `xtask ci-lint` clean; zero
   `test.skip` or `.only` in the Playwright specs either.

**One file of the plan's Files list is elsewhere**, with the reason recorded: `report.rs` is in
`openconvert`, not `oc-core`. Assembling the report needs `Tier1Report`, `StructuralReport` and
`RepairOutcome`, which are `oc-validate`'s, and `oc-validate` depends on `oc-core` — the plan's
placement is a cycle.

**One deliverable is deliberately smaller than the plan asks.** The repair table has three `AutoFix`
entries where the plan says "the ~30 ids our own generator can plausibly trigger". This emitter
triggers none of them: EPUBCheck reports zero errors on all ten fixtures. Thirty speculative repairs
would be thirty untested paths, against the one gate (A6.2) that says every repair firing is an
emitter bug.

### What Phase 6 found that Phase 5 did not

- **The `document` stage's conservation check was never recorded in the ledger.** It ran — a
  violation returns `DocumentError` — but `convert` dropped the `StageCheck`, so every phase's
  "I-1 … I-4 were checked after every stage" rested on a record naming seven stages where the
  pipeline had checked eight.
- **`validate.min_char_retention` and `conservation.budget.furniture` are jointly unsatisfiable.**
  Four fixtures retain 0.968–0.973, all of it ledgered furniture inside its budget.
- **The Gopher n-gram statistics cannot gate output text.** `f09` scores `top_3gram` 0.8008 against
  a 0.18 bound and is correct: its contents page is hundreds of `. . .` leaders.

### What CI found that Phase 6's own testing did not

Seven defects, none a flake, none findable on one machine. The full argument for each is in
`docs/DECISIONS_LOG.md`, 2026-09-19; in one line each:

1. **The container's bytes differed on Windows.** `zip` fills the "version made by" host byte from
   the building platform; the field is fixed-width, so the EPUB came out the same length with
   different bytes. `zip.rs` already *claimed* the field was pinned.
2. **Ubuntu ran out of disk mid-link.** Phase 6's eight new integration-test binaries;
   `debug = "line-tables-only"` cut the workspace's test executables from 5.7 GB to 488 MB.
3. **`epub-pagesource`, serious.** A book publishing page numbers did not say where they came from.
4. **`metadata-accessmodesufficient`.** The condition was inverted — textual sufficiency claimed for
   books that *had* images and withheld from books that were nothing but text.
5. **`epub-type-has-matching-role`**, on every content document: no `role="doc-chapter"`.
6. **The Ace runner read `data.metadata`**, a key Ace does not write, so the metadata half of its own
   gate would have reported everything missing on every book. Its unit fixture had been composed from
   the documentation by the same hand as the parser.
7. **`xtask fetch-epubcheck` unpacked a zip with `tar`.** Works on Windows, where `tar` is
   libarchive; GNU tar refuses it. Phase 5 code, and `epubcheck` and `tier1-parity` had never once
   run in CI because `needs: test` had never passed.

Nightly is green too (run 35462081049): `webkit-dom`, and `ace-a11y` once Ace's Electron was given
a root-owned setuid `chrome-sandbox` and an Xvfb display.

## Phase 5 — Definition of Done

Checked against `IMPLEMENTATION_PLAN.md` §0.3 on 2026-09-14. **One row cannot be verified on a
single machine** and is named as such rather than counted as a pass.

1. **Every named test exists and passes** — all twenty rows of the Phase 5 table (5.1–5.20), plus
   about sixty additions. `docs/TEST_MATRIX.md` lists every one and the CI job that runs it.
   Row 5.17 is behind the `epubcheck` cargo feature and row 5.5 is a CI job; neither is
   `#[ignore]`d, which `xtask ci-lint` enforces.
2. **`cargo nextest run --workspace` green on three OSes** — green locally on Windows. The Linux
   and macOS legs are the `test` matrix job and are unverified until CI runs.
3. **`cargo clippy --workspace --all-targets -- -D warnings`** — clean.
4. **`cargo fmt --all --check`** — clean.
5. **`cargo deny check`** — clean (advisories, bans, licences, sources).
6. **`cargo xtask thresholds-lint`** — clean; four new entries, each with the five keys.
7. **Every acceptance criterion demonstrated:**
   - **A5.1** (every fixture, 0 EPUBCheck errors) — **met, and measured**: EPUBCheck 5.3.0 reports
     0 errors *and 0 warnings* on all ten fixtures. Row 5.17 is the standing gate.
   - **A5.2** (the same input twice is byte-identical modulo `dcterms:modified`) — row 5.4 over all
     ten fixtures, and `cli::convert_writes_a_valid_container_and_leaves_no_temporary` through the
     binary.
   - **A5.3** (identical sha256 on ubuntu/macos/windows) — **partial, and unverifiable here**: the
     `epub-bytes` matrix job and the `epub_is_byte_identical_across_os` job are written, and one
     machine cannot run them. Everything they depend on — pure-Rust codecs, a fixed JPEG quality, a
     fixed resampling filter, sorted entries, fixed timestamps, `--modified` — is in place and
     tested on this machine.
   - **A5.4** (Tier 1: bijection, page-list, alt, no-script) — `tier1_passes_on_every_fixture`,
     plus one test per check against a container broken in exactly that way.
   - **A5.5** (a footnote inside a paragraph does not compile) — `phrasing_cannot_contain_figure`
     is the same claim about the same trait bound; `El<Phrasing>` has neither `figure` nor
     `aside_footnote`, both being `FlowContext` methods.
   - **A5.6** (per-message-id parity recorded and non-decreasing) — `docs/TIER1_PARITY.md` is
     generated by `xtask epubcheck-parity` over EPUBCheck's own corpus, and `--check` is the CI
     gate. Expanded publications are zipped by this project's writer on the way in, so an
     OCF-level defect in one of those cases is repaired before Tier 1 sees it; the 25 packaged
     `.epub` files are the ones whose container bytes are measured.
8. **`docs/CHANGELOG.md`** — Phase 5 entry written.
9. **No `TODO`/`FIXME` without an issue number** — `xtask ci-lint` clean.

### What EPUBCheck found that the tests did not

Two real defects, both fixed, both now with a Tier-1 check of their own:

- Image `src` was written package-root-relative from a document in `text/`, so every figure
  resolved to `text/images/…` and was missing (`RSC-007`). Tier 1 checked fragments and not
  resources; it checks both now.
- A document that yielded no text produced an empty spine, an empty nav `<ol>` and an empty
  `navMap` — three `RSC-005`s and not a book. A book with no text now carries its pages as
  figures (PIPELINE §10).

That is the whole argument for D6's Tier 2 being a hard gate rather than a nice-to-have.

## Phase 4 — Definition of Done

Checked against `IMPLEMENTATION_PLAN.md` §0.3 on 2026-09-14. **One row is partial** and it is
blocked on the corpus, which is Phase 7's; it is marked as such rather than counted as a pass.

1. **Every named test exists and passes** — all twenty-one rows of the Phase 4 table (4.1–4.21),
   plus about fifty additions. `docs/TEST_MATRIX.md` lists every one and the CI job that runs it.
   The plan's fixture numbers were all spent in Phases 2 and 3, so `f06`–`f08` there are
   `f08`–`f10` here and `h15`–`h20` are `h24`–`h29`; the test *names* are unchanged.
2. **`cargo nextest run --workspace`** — 307 passed, 0 skipped, 0 ignored, locally. The three-OS
   claim is CI's and is made when this branch merges, as Phases 2 and 3 cashed theirs.
3. **clippy** `--workspace --all-targets --all-features --locked -- -D warnings` — clean.
4. **`cargo fmt --all --check`** — clean.
5. **`cargo deny check`** — advisories, bans, licenses, sources ok; `deny.tools.toml` ok. `uuid`
   (MIT OR Apache-2.0) is the only new shipped dependency, and the plan names it.
6. **`cargo run -p xtask -- thresholds-lint`** — clean. **`ci-lint`** — clean.
7. **Acceptance criteria A4.1–A4.6.**
   - **A4.1** (outline → headings 1:1) — `outline_is_used_as_heading_ground_truth` on `f09`, in
     both directions: every outline entry binds and no heading is emitted that the outline did
     not name.
   - **A4.2** (noteref↔footnote bijection total) — `footnote_marker_body_bijection` on `f08`
     (`match_rate == 1.0`, no anchor shared) and `footnote_symbol_cycle_resets_per_page` on `h24`.
   - **A4.3** (heading F1 ≥ 0.75 against ground truth on the corpus) — **PARTIAL.** There is no
     corpus and no heading ground truth to score against; both arrive in Phase 7. What exists is
     the *exactness* of the two fast paths on `f09` (outline and contents page, 7/7 each), size
     rank on `f10` (4/4), and `heading_tree_has_no_level_skips` over eight fixtures under all
     three sources. The same shape of partial as Phase 2's A2.3 and Phase 3's A3.2, and the same
     cause.
   - **A4.4** (cell multiset equals source, or the table becomes an image) — enforced as the gate
     itself in `tables::build_table`, demonstrated by `ruled_table_becomes_html_table` on `f10`
     for the markup path and `borderless_table_falls_back_to_image_with_details` on `h26` for the
     other one.
   - **A4.5** (ledger empty) — `structure_stage_is_conserving` over nine documents, through the
     real `check_invariants` with `StageKind::Conserving`.
   - **A4.6** (identical `dc:identifier` across reconversions) —
     `identifier_is_stable_across_reconversions`, as a unit test on the function and end to end on
     `h28`, including that the filename does not enter it and the hash does.
8. **`docs/CHANGELOG.md`** — Phase 4 entry written.
9. **No unnumbered TODO/FIXME** — `xtask ci-lint` clean.

**Three defects in earlier phases were found and fixed here**, each recorded in
`docs/DECISIONS_LOG.md`:

- `read_outline` expanded every `/Next` chain at every node, so a five-entry chain came back with
  thirty-two entries and `f09`'s seven headings with thirty-four. The outline is heading ground
  truth, so every duplicate would have become a heading. `h13` hid it (its chains are two long)
  and `f01` hid it (one bookmark).
- Docstrum merged every heading into the paragraph beneath it, and `paragraphs` merged them back
  when the barrier was added to `layout` alone.
- A superscript set with an OpenType `sups` glyph was read as ordinary text, so footnote markers
  were swallowed into the middle of body runs.

**Known gaps carried out of the phase**, each with a named cause, are in the Notes below.

## Phase 3 — Definition of Done

Checked against `IMPLEMENTATION_PLAN.md` §0.3 on 2026-09-13. **Two rows are partial and both are
blocked on work outside this phase**; they are marked as such rather than counted as passes.

1. **Every named test exists and passes** — all seventeen rows of the Phase 3 table (3.1–3.17), plus
   about sixty additions. `docs/TEST_MATRIX.md` lists every one and the CI job that runs it. Three of the
   plan's fixture numbers were already spent, so test 3.5 uses `h22_false_gutter`, test 3.7 uses
   `h23_paragraph_across_pages` and test 3.9 uses `f06_hyphenation_de`; the test *names* are unchanged.
2. **`cargo nextest run --workspace`** — 240 passed, 0 skipped, 0 ignored, locally. The three-OS claim
   is CI's and is made when this branch merges; Phase 2's was cashed the same way.
3. **clippy** `--workspace --all-targets --all-features --locked -- -D warnings` — clean.
4. **`cargo fmt --all --check`** — clean.
5. **`cargo deny check`** — advisories, bans, licenses, sources ok; `deny.tools.toml` ok. Both configs
   now ban `hyphenation` (VD-b).
6. **`cargo run -p xtask -- thresholds-lint`** — clean. **`ci-lint`** — clean: no `#[ignore]`, no
   unnumbered `TODO`.
7. **Acceptance criteria A3.1–A3.5.**
   - **A3.1** (`f02`, left column before right) — `two_column_reading_order_is_left_then_right`.
     Cashed properly only because the fixture was rewritten: it had never had two columns.
   - **A3.2** (100 % of gold block orders on any Manhattan corpus file) — **PARTIAL.** There is no
     corpus and no gold block order to reproduce; both arrive in Phase 7. What exists is the order
     itself, on `f01`, `f02` and `h22`, plus `prop_single_column_order_is_monotone_in_y` over generated
     pages and `prop_page_permutation_metamorphic` over three documents. The same shape of partial as
     Phase 2's A2.3, and the same cause.
   - **A3.3** (keep-hyphen recall ≥ 0.80 on the holdout, for the DE fixture) — **PARTIAL**, and split in
     two. The recall is measured and passes — 0.912 over 239 held-out keeps
     (`hyphen_classifier_keep_recall_on_holdout`) — but the holdout is **English**, because D15 has no
     German source that may be redistributed and so there is no German training data. The German fixture
     itself is checked in both directions by `dehyphenate_keeps_german_real_hyphen`, on the rule that
     needs no lexicon. Closing this row properly needs the DE list, which is the open D15 question from
     item 2.9.
   - **A3.4** (layout ledger empty) — `layout_stage_is_conserving`, and
     `layout_stage_conservation_violation_errors` for the other direction.
   - **A3.5** (I-5: exactly one hyphen, nothing else) — `dehyphenate_i5_removes_exactly_one_hyphen`
     (5,000 generated joins) for the operation, `check_invariants` for the ledger, and
     `dehyphenation_is_ledgered_one_hyphen_at_a_time` end to end on `h23`.
8. **`docs/CHANGELOG.md`** — Phase 3 entry written.
9. **No unnumbered TODO/FIXME** — `xtask ci-lint` clean.

**VD-b closed** (2026-09-13): the `hyphenation` crate ships the `hyph-utf8` pattern files with their
licence headers stripped and disclaims them; Turkish is LPPL-1.0+ upstream and the compiled dictionaries
fold in GPL/LGPL/MPL extended data. It is banned in both `deny` configs. v1 needs no patterns.

**One file from the plan's Files list is deliberately absent:** `f05_verse_and_quote.typ` (which would be
`f07` here). No Phase 3 test names it — verse and block quotes are PIPELINE §8.6, and their tests are
Phase 4's. It is written when the test that needs it is.

**Known gaps carried out of the phase**, each with a named cause:

- `f01`'s committed `pipeline` assertion is not met. The classifier reads `pipe-line` as a real compound,
  which it was in the nineteenth-century register its training corpus is written in. Fails in the safe
  direction; the fix is a modern corpus (Phase 7).
- One block of `f02` is flagged low-confidence: the cover cuts a paragraph's last line off because a line
  with no ascenders has a shorter inked box. Over-flagging a confidence signal is the safe direction, and
  the flag changes no segmentation.
- A `Dehyphenate` budget is a fraction, so a document of a hundred characters breaches it on one
  legitimate hyphen. A floor on the denominator belongs with the rest of the breach policy, in Phase 6.

## Phase 2 — Definition of Done

Checked against `IMPLEMENTATION_PLAN.md` §0.3 on 2026-09-13:

1. **Every named test exists and passes** — all twenty-two rows of the Phase 2 table (2.1–2.22),
   plus about forty additions, each of which exists because something was measured and was not
   what the plan assumed. `docs/TEST_MATRIX.md` lists every one and the CI job that runs it.
2. **`cargo nextest run --workspace`** — 156 passed, 0 skipped, 0 ignored, **on all three
   operating systems**. CI run 34758095844 on `main`, 2026-09-13: `test (ubuntu-latest)`,
   `test (macos-latest)` and `test (windows-latest)` all green, alongside `lint`, `deny`,
   `desktop`, `no-network`, `poppler-oracle` and `ui`. This is the first time that claim has
   been true rather than deferred — see the Notes below.
3. **clippy** `--workspace --all-targets --all-features --locked -- -D warnings` — clean.
4. **`cargo fmt --all --check`** — clean.
5. **`cargo deny check`** — advisories, bans, licenses, sources ok; `deny.tools.toml` ok.
6. **`cargo run -p xtask -- thresholds-lint`** — clean.
7. **Acceptance criteria A2.1–A2.6.**
   - A2.1 (I-1…I-4 hold, a violation is fatal) by `conservation_i1_holds_across_text_and_furniture`
     over `f01`/`f02` through the real checker, `conservation_i1_holds_over_generated_documents`
     over 200 generated documents, and `a_budget_breach_stops_the_stage`.
   - A2.2 (`"The Test Book"` and the page numbers absent from flow, present in the ledger) by
     tests 2.10 and 2.11.
   - A2.3 (furniture ≤ 4 % of `|C_0|`) enforced by `check_invariants` on every stage run and
     demonstrated on `f01` and `f02`; **"any corpus file" is Phase 7's corpus**, which does not
     exist yet, so this is demonstrated on the fixtures rather than at the stated scope.
   - A2.4 (10 000 random strings: idempotent, NFC, no NFKC, no case folding) by
     `normalize_is_idempotent` (10 000 cases), `normalize_never_applies_nfkc`,
     `normalize_composes_to_nfc` and `text_is_never_case_folded_in_output`.
   - A2.5 (EN/DE/TR `dc:language`) by test 2.19 over `f01`, `f04` and `f05`.
   - A2.6 (Turkish folding, emitted text unchanged) by tests 2.6 and 2.7.
8. **`docs/CHANGELOG.md`** — Phase 2 entry written.
9. **No unnumbered TODO/FIXME** — `xtask ci-lint` clean.

**One deliverable is deliberately partial, with the maintainer's agreement.** PLAN Phase 2
detail 5 asks for EN/DE/TR word-frequency lists. English ships (20 000 words from twelve CC0
Standard Ebooks, with a source manifest). German and Turkish do not: the plan names DTA plain
text and Wikisource-TR as "CC0/PD" and their transcriptions are CC-BY-SA, which is not on D15's
allow-list for a shipped artefact. The generator refuses them mechanically. `dict_hit_rate`
returns `None` for both — not zero — so nothing downstream misreads the absence as evidence.
**Open for `DECISIONS.md`: which sources build the DE and TR lists.**

## Phase 1 — Definition of Done

Checked against `IMPLEMENTATION_PLAN.md` §0.3 on 2026-09-10:

1. **Every named test exists and passes** — all twenty rows of the Phase 1 table (1.1–1.20), plus
   about twenty more, each of which exists because something was measured and was not what the
   plan assumed. `docs/TEST_MATRIX.md` lists every one and the CI job that runs it.
2. **`cargo nextest run --workspace`** — 83 passed, 0 skipped, 0 ignored; with
   `--features poppler-oracle`, 55 passed in `oc-pdf`. Verified on Windows at the time; the
   three-OS claim was cashed on 2026-09-13, when CI first ran (see Notes), and it needed two
   fixes in this phase's code to hold — the tagged-fixture dependency and h01's host-dependent
   glyph boxes.
3. **clippy** `--workspace --all-targets --all-features --locked -- -D warnings` — clean.
4. **`cargo fmt --all --check`** — clean.
5. **`cargo deny check`** — advisories, bans, licenses, sources ok; `deny.tools.toml` ok.
6. **`cargo run -p xtask -- thresholds-lint`** — clean.
7. **Acceptance criteria A1.1–A1.6.** A1.1 by `acceptance::every_glyph_carries_thirteen_real_
   signals` over all eleven fixtures; A1.2 by test 1.5; A1.3 by tests 1.10 and VD-d.7; A1.4 by
   test 1.8; A1.5 by 1.16. **A1.6 measured** on a 301-page Typst book: 10.2 ms/page against a
   150 ms budget, 98 MB peak RSS against 250 MB — but **not on D9's reference machine L**, so it
   is indicative rather than signed off. Phase 7's benchmark harness measures it properly.
8. **`docs/CHANGELOG.md`** — Phase 1 entry written.
9. **No unnumbered TODO/FIXME** — `xtask ci-lint` clean.

**VD-d closed**: PDFium's `get_processed_image()` composites soft masks, stencil masks, indexed
palettes and DeviceGray correctly, so the image policy uses it and writes no compositing of its
own. CMYK JPEG, 1-bit CCITT and JPX are uncovered — no encoder exists to build those fixtures
honestly — and are deferred to real samples in the Phase 7 corpus.

## Phase 0 — Definition of Done

Checked against `IMPLEMENTATION_PLAN.md` §0.3 on 2026-09-09:

1. **Every named test exists and passes** — all 23 rows of the Phase 0 table, plus four additions
   (0.8a, 0.12a, 0.23a, and the committed-assertion-file test), each with its reason in
   `docs/DECISIONS_LOG.md`. `docs/TEST_MATRIX.md` lists every one and the CI job that runs it.
2. **`cargo nextest run --workspace`** — green, 0 skipped, 0 ignored. Verified on Windows at the
   time, and the entry said plainly that Linux and macOS "have not run yet". They ran for the
   first time on 2026-09-13 and are green (see Notes). The caveat was correct and it stood for
   four days longer than anyone noticed, because `ci` triggers on `main` and this phase was
   never on it.
3. **clippy** `--workspace --all-targets --all-features --locked -- -D warnings` — clean.
4. **`cargo fmt --all --check`** — clean.
5. **`cargo deny check`** — advisories, bans, licenses, sources all ok; plus `deny.tools.toml`
   licenses/bans/sources ok.
6. **`cargo run -p xtask -- thresholds-lint`** — clean.
7. **Acceptance criteria** A0.1–A0.9: A0.1 (minus the two other OSes, see 2), A0.2, A0.3, A0.4, A0.5,
   A0.7, A0.8 and A0.9 are demonstrated by named tests. **A0.6 is partly open**: the app compiles, the
   handshake and the rendered version string are unit-tested, but no one has watched a real window open.
   Phase 12 owns the UI; a screenshot at that point closes it.
8. **`docs/CHANGELOG.md`** — Phase 0 entry written.
9. **No unnumbered TODO/FIXME** — `xtask ci-lint` clean.

## Completed items log

<!-- one line per finished work item: `YYYY-MM-DD  P<phase>.<item>  <what>  <commit sha>` -->
2026-09-09  P0.setup  Cargo workspace, thresholds.toml, deny.toml, CI matrix, eval skeleton (§1.1-§1.9)  e6d02df
2026-09-09  P0.1      oc-model: BlockId derivation + Rect (tests 0.1, 0.2)                          1d32410
2026-09-09  P0.2      oc-model: canonical JSON + IR_VERSION (tests 0.3, 0.4)                        2cd8f76
2026-09-09  P0.3      oc-core: thresholds codegen + provenance lint (tests 0.5, 0.6)                3843abb
2026-09-09  P0.4      oc-pdf: page-space normalisation (test 0.8 + corner unit test)              79ac71f
2026-09-09  P0.5      xtask vendor-pdfium + oc-pdf PDFium binding and probe (test 0.7)             5059a98
2026-09-09  P0.6      oc-pdf: page classification (tests 0.9-0.12 + mixed/blank/dict test)        97ddfd8
2026-09-09  P0.7      oc-pdf: producer-family detection (test 0.13)                              4788213
2026-09-09  P0.8      xtask fixtures + audit-surface split resolving Q1 (test 0.20)              57df06e
2026-09-09  P0.9      oc-pdf: inspect report + PdfDoc over PDFium (tests 0.14-0.16)             3ec6f70
2026-09-09  P0.10     openconvert: CLI, NDJSON events, exit codes (tests 0.17-0.19)             931cc0a
2026-09-09  P0.11/12  oc-testkit assertions + xtask ci-lint/thresholds-lint (0.21, 0.23)        de4298c
2026-09-09  P0.13     apps/desktop hello-Tauri + handshake + LICENSE (test 0.22)                52a4071
2026-09-09  PHASE 0   COMPLETE - Definition of Done checked, one item partly open (A0.6)
2026-09-09  P1.1      oc-testkit handmade fixtures + the PDFium overdraw finding           15d0dbe
2026-09-09  P1.2      oc-model extract/ledger + oc-pdf glyph extraction (tests 1.1-1.4)    f962f00
2026-09-09  P1.3      oc-pdf metamorphic invariants + oc-testkit mutate (tests 1.5-1.7)   bd96e3a
2026-09-09  P1.4      oc-pdf broken-text: control-char counter + strip_tounicode (test 1.8)  928f9d7
2026-09-09  P1.5      oc-pdf images: ImageRef, DPI, kind, smask/inline via lopdf (test 1.9)  053ad54
2026-09-09  P1.6      oc-core/oc-pdf resource limits + --max-pages (tests 1.10, 1.11, 1.20)  13fce0b
2026-09-09  P1.7      oc-pdf encryption: permissions recorded not enforced (tests 1.12-1.14)  a825fc5
2026-09-10  P1.8      oc-pdf outline walk + meta from the object tree (test 1.15)  d815ad3
2026-09-10  P1.9      oc-pdf fuzz-lite: random, truncated and corrupted inputs (test 1.16)  d9f18aa
2026-09-10  P1.10     oc-pdf dump + openconvert dump-stage ingest (test 1.17)  0ccbd1b
2026-09-10  P1.11     oc-pdf differential pdftotext oracle behind a feature (test 1.18)  2a9aa8f
2026-09-10  P1.perf   oc-pdf: page ids read once, not per page (O(n^2) fix)                6908601
2026-09-10  P1.12     oc-core cancel/progress + openconvert control channel (test 1.19)  69734e5
2026-09-10  P1.13     oc-pdf image_bytes + VD-d known-answer spike (VD-d closed)             c73476e
2026-09-10  PHASE 1   COMPLETE - Definition of Done checked; A1.6 measured off reference machine L
2026-09-13  P2.1      oc-core ledger_check: I-1..I-4 + stage declarations (2.16, 2.17 + 6)  fad617e
2026-09-13  P2.2      oc-text normalize: N, ledgered on both sides (2.1-2.4 + 3)             b7a822c
2026-09-13  P2.3      oc-text fold_key + oc-model LangTag (2.6, 2.7 + 4)                     b5e08ad
2026-09-13  P2.4      oc-pdf: decode the U+0002 hyphen marker at extraction (3 tests)        e5ada15
2026-09-13  P2.5      oc-text words/lines + oc-model text layer, h16-h18 (2.5, 2.8, 2.9 + 9) c49ef42
2026-09-13  P2.6      oc-layout furniture + h19-h21 (2.10-2.14 + 3)                          787002f
2026-09-13  P2.7      openconvert lib: text+furniture under check_invariants (2.15 + 4)      9ce1585
2026-09-13  P2.8      oc-text stats: the nine Gopher numbers + verdict (2.18 + 5)            b20c293
2026-09-13  P2.doc    example_pdfs recorded in TEST_CORPUS 7.5a as the local smoke set       b7f2037
2026-09-13  P2.9      oc-text freq + wordfreq.py; EN ships, DE/TR blocked on D15 (2.22 + 7)  bb11165
2026-09-13  P2.10     oc-text lang + f04/f05 fixtures (2.19, 2.20 + 7)                       a184c12
2026-09-13  P2.11     openconvert dump-stage text + snapshot; min_space_ratio (2.21)         b3949e9
2026-09-13  PHASE 2   COMPLETE - Definition of Done checked; DE/TR frequency lists open (D15)
2026-09-13  P3.1      VD-b closed: hyphenation banned, not depended on                            059c00d
2026-09-13  P3.2      oc-layout blocks: Docstrum + whitespace cover + layout stage (3.1, 3.13 + 6) 36874a1
2026-09-13  P3.3      oc-layout columns + reading order; f02 rewritten (3.2-3.4 + 7)               adb50d9
2026-09-13  P3.4      oc-layout continuity + h22; the retry compares hypotheses (3.5 + 7)          7fa85aa
2026-09-13  P3.5      oc-layout paragraphs: convention, unwrap factor, cross-page merge (3.6 + 8)  0ad7b6a
2026-09-13  P3.6      oc-text dehyphen tiers + paragraphs stage + I-5 + h23 (3.7/3.8/3.10/3.11/3.17) 7979668
2026-09-13  P3.7      oc-text compound_de + f06; the capital after the hyphen (3.9 + 8)            f417036
2026-09-13  P3.8      oc-text classifier + hyphen_clf.py + holdout; keep-recall 0.912 (3.12 + 6)   650a3e6
2026-09-13  P3.9      openconvert dump_layout + anchor + drop caps; the cover fixed (3.14-3.16 + 12) 0dfd45c
2026-09-13  PHASE 3   COMPLETE - Definition of Done checked; A3.2 and A3.3 partial, both named
2026-09-14  P4.1      oc-model doc: the semantic layer + 5 id types; Para grows its half (3)     ac2218b
2026-09-14  P4.2      oc-pdf VectorRegion + is_rule + h24; the rule predicate (3)                316611f
2026-09-14  P4.3      oc-structure cluster + validity gate; text gets a font table (4.3/4.6 + 4) 5b047af
2026-09-14  P4.4      oc-pdf: an outline entry is read once; f07-f10 land (1)                    697099b
2026-09-14  P4.5      oc-structure headings: outline/TOC/size-rank + the style barrier (4.1-4.5) 6340775
2026-09-14  P4.6      oc-structure notes + the raised-ink superscript rule (4.7, 4.8)            d00443d
2026-09-14  P4.7      oc-structure figures: captions, and the abstention (4.9, 4.10)             a434a4c
2026-09-14  P4.8      oc-structure lists + tables, and the honest fallback (4.11-4.14 + 5)       9b2b2ad
2026-09-14  P4.9      oc-structure quotes + meta; XMP over boilerplate (4.16-4.18 + 4)           4076f40
2026-09-14  P4.10     openconvert structure stage under the conservation law (4.15, 4.19-4.21)   70439b6
2026-09-14  P5.1      oc-model Document + the document stage; page breaks, class, preset (10)   5d3717a
2026-09-14  P5.2      oc-epub: the typed XHTML builder; two compile-fail rows (5.1, 5.2 + 14)    b100485
2026-09-14  P5.3      oc-epub: the deterministic OCF container (5.3 + 3)                          b57bfad
2026-09-14  P5.4      oc-epub: one stylesheet that names no typeface (5.12 + 2)                   c7cd1e5
2026-09-14  P5.5      oc-structure: spans carry their styles and their note references (2)        ea69e20
2026-09-14  P5.6      oc-epub: content documents, package, nav, ncx, images, container (5.4-5.14, 5.20 + 14)  24087ae
2026-09-14  P5.7      oc-validate: Tier 1, against real and crafted output (5.15, 5.16 + 6)       a4b8e7b
2026-09-14  P5.8      openconvert: convert + validate; the pipeline moved into the library (5.19 + 5)  19c31b1
2026-09-18  P6.1      oc-validate: I-7 over the archive, and retention as a flag (6.1-6.3 + 4)     c5a5627
2026-09-18  P6.2      oc-validate: the structural report - headings, duplicates, quality (6.16 + 15)  3bca6fd
2026-09-18  P6.3      oc-validate: the repair loop, its measure and its table (6.4-6.7, 6.9 + 12)  7b35583
2026-09-18  P6.4      openconvert: the loop on the real path, fire rate zero (6.10 + 5)              9889827
2026-09-18  P6.5      openconvert: report.json, --report, the post-cap policy (6.8, 6.12 + 4)        e445783
2026-09-18  P6.6      oc-core: the warning registry, en/de/tr templates, the registry lint (6.11 + 7)   167227d
2026-09-18  P6.7      tests/dom: the Playwright DOM checks, three viewports, CI on (6.13-6.15 + 4)    4a71fc0
2026-09-18  P6.8      oc-validate: the Tier-3 Ace runner and the nightly ace-a11y job (5 tests)      5855c53
2026-09-19  P6.ci     seven defects CI found: cross-OS bytes, Ace a11y x3, disk, tar/zip     23c7064
2026-09-20  P7.1      corpus: sha256 before use, mirror-then-source, the LOCAL_EVAL boundary (9)   3bf9c1c
2026-09-20  P7.2      corpus: the manifest vocabulary and fourteen lint rules (7.1, 7.3b + 21)     5281b83
2026-09-20  P7.3      corpus: the frozen holdout, 104 real documents, five sources (7.2, 7.3 + 50)  86848f8
2026-09-20  P7.4      oc-testkit: the mutation catalogue, ten recipes with declared effects (7.6 + 3)  3b55dbb
2026-09-20  P7.5      oc-eval: ground truth from XHTML, struct trees and LaTeX (7.7 + 16)     36b6384
2026-09-20  P7.6      oc-eval: metrics, the Wilson gate, the per-stratum report (7.8, 7.9 + 21)  ee146db
2026-09-20  P7.7      oc-eval: the ours-vs-real gap recorded and plotted (7.10 + 7)      8a056c0
2026-09-20  P7.8      oc-eval: calibration refuses the holdout; risk-coverage (7.4 + 11)  bb253f3
2026-09-20  P7.9      openconvert: the perf budget, 0.0217 s/page on 300 pages (7.11, 7.12 + 14)  1f94be4
2026-09-20  P7.10     ci: the python job, corpus lint, and four nightly bodies (7.13, 7.14 + 16)  61edb1d
2026-09-22  P8.1      oc-ai: v1 prompts, a llama.cpp GBNF parser, the request (8.1, 8.14 + 19)  42807c2
2026-09-22  P8.2      oc-ai: gate S, and gate D's record in Decision.fallback (8.2-8.4, 8.9, 8.12 + 6)  6c47025
2026-09-22  P8.3      oc-ai: gate L - C unchanged without the ledger, then reading order (8.5, 8.6 + 3)  7b83a2a
2026-09-22  P8.4      oc-ai: gate V - the fixed tuple, undefined skipped, oc-text's statistics (8.7, 8.8 + 5)  e635630
2026-09-22  P8.5      oc-ai: one call budget per book, W_LLM_BUDGET_EXHAUSTED, unasked decisions (8.13 + 2)  3af6cef
2026-09-22  P8.6      oc-ai: the cache key - one rule for cache and cassettes - and the file cache (8.10 + 5)  e1a7b8a
2026-09-22  P8.7      oc-ai: one OpenAI-compatible client over a Transport; the adversarial stub (+ 7)  cc95e87
2026-09-22  P8.8      oc-ai: cassettes at the provider seam, replay exact, four seeds (8.11 + 5)  6294cc6
2026-09-22  P8.9      oc-core: the six escalation predicates, pure, table-tested (8.16)  1074970
2026-09-22  P8.10     oc-ai: no socket by dependency or by std; CI unshare step; the DoD (8.15)  e5ad4ef
2026-09-22  PHASE 8   COMPLETE on worktree-phase8 - Definition of Done checked; Linux/macOS CI and the unshare step unverified until merge
2026-09-23  P9.1      oc-net: models.toml registry refuses TODO_ pins and non-commit revisions (9.1, 9.2)  fe32587
2026-09-23  P9.2      oc-net: pinned download, allowlist per hop, SHA-256 while streaming, atomic (9.3-9.6)  5ffd509
2026-09-23  P9.3      oc-net: store list/remove, HttpTransport (9.19 + 4)  5086e76
2026-09-23  P9.4      oc-core: no net dependency, walked from Cargo.lock; CI step on (9.7)  1774052
2026-09-23  P9.5      oc-core: llama-server argv from the registry, key never in argv (9.12, 9.14 + 1)  595be0b
2026-09-23  P9.6      oc-core: OwnedServer, supervise, endpoint; teardown on exit/panic/signal (9.8-9.11, 9.13 + 4)  6ac5582
2026-09-23  P9.7      openconvert: model pull|list|remove, ModelReadiness (9.18 + 4)  1a8c4ac
2026-09-23  P9.8      oc-testkit: live tests behind live-llm; W_LLM_PREFIX_COLD; fetch-llama-server (9.15, 9.16 + 5)  9f223a0
2026-09-23  P9.9      eval: model_gate.py, probes, fixtures, MODEL_GATE.md (9.17, 9.20 + 17)  b9c0ab4
2026-09-23  PHASE 9   COMPLETE on phase/09-local-model - DoD checked; live model, gate runs, macOS/Windows and CI unverified here
2026-09-23  P9.fix    xtask: llama.lock b10456 digests pinned, all four checked by download (+1)  2432458
2026-09-23  P10.1     oc-structure: the verse band read through oc_core::escalation; --no-ai EPUB hashes pinned (+ 2)  33b00ce
2026-09-23  P10.2     oc-structure: escalate.rs, EscalationRecord in Conversion and the report (10.1, 10.2 + 2)  d91276e
2026-09-23  P10.3     oc-ai: task 1, the verbatim-substring check and apply_metadata (10.3, 10.4 + 1)  85f2e3a
2026-09-23  P10.4     oc-ai: task 2, pre-gate, held-out check, role rules; structure_with (10.5-10.8 + 4)  d8e4567
2026-09-23  P10.5     oc-ai: task 3, strict boundaries, chunks agreeing on the overlap (10.9-10.11 + 2)  ea56fb9
2026-09-23  P10.6     oc-ai: task 4, batches of ten, the 30-block cap, counter-evidence (10.12, 10.13)  b914848
2026-09-23  P10.7     oc-ai: plan (language gate, degradation order) and Session (10.21, 10.22 + 3)  56cc540
2026-09-23  P10.8     openconvert: the AI step, applied through structure, gated, recorded (10.14, 10.15, 10.20 + 3)  ffb529d
2026-09-23  P10.9     openconvert: convert --ai, endpoint flags, missing model degrades (10.16, 10.19 + 3)  1a506e2
2026-09-23  P10.10    eval: McNemar and false repair per task/category/language, the gate (10.17, 10.18 + 4)  203533e
2026-09-23  P10.11    openconvert: live convert --ai behind live-llm; CHANGELOG; the DoD  7659dae
2026-09-23  PHASE 10  COMPLETE on phase/10-ai-decisions - DoD checked; A10.4/A10.5, live model, macOS/Windows and CI unverified here
2026-09-23  P11.1     oc-net: consent names the host; HttpTransport refuses any other (11.6 + 4)  a1515b6
2026-09-23  P11.2     oc-ai: provider adapters; an unconstrained provider warns (11.4 + 3)  ad6cea3
2026-09-23  P11.3     oc-ai: Ollama through /api/chat, num_ctx always set, format schema (11.2, 11.3 + 1)  e50805a
2026-09-23  P11.4     oc-net: detect Ollama on localhost:11434; probe what an endpoint is (11.1 + 3)  e7f7945
2026-09-23  P11.5     oc-ai: the cassette contract through every adapter (11.10)  933dd2b
2026-09-23  P11.6     openconvert: providers by probe, consent by name, E_CONSENT_REQUIRED (11.5, 11.8 + 5)  525fc0e
2026-09-23  P11.7     openconvert: the report records consent; a failing provider degrades (11.7, 11.9)  4d1bd4a
2026-09-23  P11.8     openconvert: provider detect, check and probe (+ 3)  4450002
2026-09-23  P11.9     openconvert: A11.1 live behind live-llm; CHANGELOG; the DoD  b8184e1
2026-09-23  PHASE 11  COMPLETE on phase/11-byo-providers - DoD checked; live Ollama/remote endpoint, macOS/Windows and CI unverified here
