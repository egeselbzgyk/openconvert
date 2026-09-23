# OpenConvert — Security testing

What `SECURITY.md` promises, the test that proves each promise, and how to run it. Written in
Phase 14 (`IMPLEMENTATION_PLAN.md` PHASE 14). The threat is a hostile PDF from an untrusted source;
the dominant real bug class is denial of service. Every row below is a named test, a CI job, or a
command. Where this machine could not run something, the row says so.

## 1. Resource caps, checked before the work

A cap that is enforced after the allocation it bounds is not a cap. Each cap is read from
`thresholds.toml` (`[limits.*]`) and checked **before** the expensive operation, from what the file
*declares*. The structured refusal is `oc_core::limits::CapViolation`; `cap()` names the threshold.

| Cap (`thresholds.toml`) | Default | Checked before | Where | Tests |
|---|---|---|---|---|
| `limits.max_image_pixels` | 100 MP | any decoder allocation: `/Width × /Height` read from the image dictionary | `oc_pdf::limits::check_image_before_decode` | 14.1 (spy decoder never entered); `hardening::a_gigapixel_claim_is_refused_from_the_dictionary` (A14.1) |
| `limits.max_decompressed_stream_bytes` | 256 MiB | the output buffer grows past it; `/Length` is ignored | `oc_pdf::limits::BoundedInflate`, `oc_pdf::filters::decode_stream` (one budget for the whole filter chain); `lopdf` loads with `max_decompressed_size` | 14.2, 14.3, `a_filter_chain_shares_one_budget`; `hardening::a_decompression_bomb_fails_closed_through_convert` (A14.2) |
| `limits.max_xref_chain` | 128 | PDFium or `lopdf` opens the file | `oc_pdf::prescan` walks `/Prev`, `/XRefStm` and ObjStm nesting with a depth counter **and** a visited set | 14.4 (depth), 14.5 (cycle, reported as a cycle) |
| `limits.max_pages` | 3 000 | the first page object loads: the catalogue's `/Count` | `oc_pdf::prescan` | 14.6; `hardening::a_declared_5000_page_pdf_exits_1_with_a_report` |
| `limits.max_page_glyphs` | 1 000 000 (provisional) | PDFium loads a page: text-operator string bytes counted from the content stream | `oc_pdf::glyph_budget` | 14.9 (A14.3), `every_committed_fixture_is_far_under_the_page_cap` |
| `limits.max_object_nesting` | 64 (provisional) | the pre-walk's own parser recurses past it | `oc_pdf::prescan` | covered by the mutated crash corpus (`objstm_nest`) |
| `limits.max_memory_bytes` / `--max-memory` | 4 GiB | the PDF opens: `RLIMIT_AS` (Unix); Windows: job object, no memory limit yet (provisional) | `oc_core::sandbox::rlimit`, `jobobject` | 14.7 (`getrlimit` read inside the engine at open, reported as `sandbox.memory.in_force_at_open`) |
| `limits.stage_deadline_secs` | 300 s | every `cancel.is_cancelled()` poll | `oc_core::deadline` on `Cancel` | 14.8; `hardening::a_stage_deadline_aborts_through_the_cancel_path` |

**One abort path.** A deadline sets the same flag as `{"t":"cancel"}`, with `AbortCause::Deadline`
(first cause wins); `Scratch::clean_up` is the one cleanup. The cancel tests and the deadline tests
exercise the same code (14.8).

**How a cap ends.** `convert` (flags or job spec) exits **1** with a failure report —
`status: "failed"`, `failure.{code, message, cap}` — and never leaves a file at the output path:
output is written to a temporary file beside the destination and renamed only on success
(`deliver::write_atomically`). Row 14.19 injects 1 000 violations across every cap and asserts the
destination never exists. `inspect` and `dump-stage` still exit 2 on a cap.

Run: `cargo nextest run -p oc-pdf -p oc-core` (unit rows) and
`cargo nextest run -p openconvert -E 'binary(hardening)'` (the engine rows).

## 2. Process containment

| Mechanism | Platform | What it does | Test |
|---|---|---|---|
| `RLIMIT_AS` | Unix | the engine caps its own address space before the PDF opens; the report records it (`sandbox.memory`) | 14.7 |
| Job object, `KILL_ON_JOB_CLOSE` | Windows | a standalone engine adopts itself into a job, so children die with it | type-checked only; **unverified here** (no Windows machine) |
| `PR_SET_PDEATHSIG` via an exec trampoline | Linux | every child (`llama-server`, `tesseract`) is started as `<engine> __oc-exec-child <pid> -- <program>`, which sets the death signal and `exec`s; a SIGKILLed engine takes its children with it | `ocr_child_does_not_outlive_a_sigkilled_engine`, `owned_server_does_not_outlive_a_sigkilled_engine` |
| Landlock | Linux ≥ 5.13 | after argument and job-spec validation, before the first PDF byte: read the input, models, tessdata and system program dirs; write the output directory and the temp directory; on ABI ≥ 4, TCP `connect` only to the AI endpoint's loopback port | 14.10, 14.12 (`oc-sandbox-probe`), `a_normal_conversion_succeeds_inside_landlock`, 14.11 (the recorded skip) |

A missing sandbox never fails a conversion: an unsupported kernel, or `OC_LANDLOCK=off`
(provisional), is recorded in the report as `sandbox.landlock.status = "unsupported"` with
`sandbox.landlock.reason`, and the conversion proceeds. The desktop app never restricts its own
process.

The engine must apply Landlock before it starts any thread (Landlock binds the calling thread and
its later threads). On macOS there is no self-restriction in v1 (SECURITY §11).

## 3. Network

- **No socket on the conversion path.** `oc-core` cannot reach a socket crate
  (`oc_core_has_no_net_dependency`, and `cargo tree -p oc-core -i ureq|rustls|oc-net` in the
  `no-network` job). On Landlock ABI ≥ 4 the engine also forbids TCP `connect` except to the AI
  endpoint it opened itself.
- **`unshare -n`.** The `no-network` CI job runs the conversion and inspection tests, the `--ai` path
  on cassettes (row 14.20 — `unshare_n_covers_the_ai_cassette_path` first proves the namespace is
  empty when `OC_EXPECT_NO_NETWORK=1`), and the `oc-ai` suite in an empty network namespace. Locally
  (as root): `unshare -n -- env OC_EXPECT_NO_NETWORK=1 cargo nextest run -p openconvert -E
  'binary(ai_pipeline) or binary(ai)'`.
- **Audit log.** Every outbound connection made by `oc-net` appends one JSON line
  `{ts, host, purpose, bytes, outcome, loopback}` to `<data_dir>/openconvert/network-audit.log`,
  rotated at `net.audit_log_rotate_bytes` (1 MiB; one previous generation, `.1`). Purposes:
  `download`, `llm-request`, `llm-probe`, and `update` for the desktop app's update check (Phase 15,
  `every_update_connection_is_in_the_network_audit_log`). `model pull` appends exactly one line; a
  conversion appends none (14.21, `a_conversion_appends_nothing_to_the_network_audit_log`). The
  desktop app installs the same log and shows it in Settings › Network log.

## 4. Crash-regression corpus

`corpus/fixtures/crash/` holds files that once crashed, hung or could have: each keyed
`(name, sha256)` in `corpus/fixtures/crash/manifest.json` (row 14.18 fails on an unkeyed or changed
file). The assertion is weak on quality and strong on behaviour: exit 0 or 1, a report written,
within the stage deadline, never 101, never a partial file at the output path.

| Set | Source | Test |
|---|---|---|
| `mutated/` (29 files) | `python -m oc_eval.mutate.crash` — xref byte flips, truncated streams, cyclic `/Prev`, nested ObjStm, `/Pages` loops, over the committed handmade fixtures; regenerates byte-identically (`--check`) | 14.17 |
| `fuzz/` | minimised crash inputs from the nightly `fuzz` job (`cargo fuzz tmin`), committed with their fix; none so far | 14.18 keys them |
| Isartor | fetched by `cargo run -p xtask -- fetch-isartor` from `xtask/isartor.lock` (pinned commit and per-file SHA-256), never committed | 14.16, behind `--features isartor`, nightly `isartor` job |

**The Isartor lock is not pinned** (the source was unreachable from the machine it was written on;
PROVISIONAL, `PROGRESS.md` › Blocked). `fetch-isartor` refuses until it is, and row 14.16 is
unverified until then.

**Adding a crash file:** put it under `mutated/` (through a generator) or `fuzz/`, run
`python -m oc_eval.mutate.crash` to re-key the manifest, and commit it with the fix.

## 5. Fuzzing

Three `cargo-fuzz` targets on the parsers OpenConvert owns (PDFium is fuzzed by OSS-Fuzz):

| Target | Property | Suite row |
|---|---|---|
| `ir_deserialize` | no panic; anything that deserialises re-serialises to byte-identical canonical JSON | 14.13 |
| `job_spec` | no panic; every accepted spec names only absolute, non-`..` paths (RT B15) | 14.14 |
| `xhtml_opf_roundtrip` | an IR drawn from the bytes → typed builder → XHTML/OPF re-parses and passes Tier-1 with zero repairs | 14.15 |

The properties live in `oc_testkit::fuzz_props`; each target is one line, and the ordinary suite runs
the same properties over `fuzz/corpus/` and generated inputs, so they stay green between nights.
`fuzz/` is its own workspace (nightly toolchain only there).

```sh
cargo run -p xtask -- fuzz-seeds                       # re-seed fuzz/corpus/ from the fixtures
cd fuzz && CARGO_TARGET_DIR=<scratch> cargo +nightly fuzz run -O job_spec <copy of corpus/job_spec> \
  -- -max_total_time=900 -rss_limit_mb=2048
```

Measured on the Phase 14 machine, 120 s each: 1 692 734 / 2 632 543 / 21 243 runs, zero crashes.
The nightly 15-minute campaigns are **unverified here** (GitHub Actions is disabled).

## 6. `unsafe`

`#![forbid(unsafe_code)]` at every crate root except the PDFium binding. `cargo run -p xtask --
ci-lint` (row 14.22) allows `unsafe` only under `crates/oc-pdf/src/pdfium/` and the three syscall
modules `crates/oc-core/src/sandbox/{landlock,rlimit,jobobject}.rs` — which today contain none:
they call `landlock`, `rustix` and `win32job`. `fuzz/` is exempt from the root rule
(`fuzz_target!` expands to a `#[no_mangle]` entry point).

## 7. `--isolate-parser`

A time-boxed spike (row 14.23, `cargo run --release -p xtask -- isolate-parser-spike`): one child per
8-page range, CBOR glyph batches over a pipe. Output identical; overhead +60.8 % on a 300-page book
and 24–100 % on text fixtures. **No-go** for v1 (`docs/DECISIONS_LOG.md`); v1's crash-isolation unit
stays the job.

## 8. Not verified on the Phase 14 machine

macOS and Windows at run time (job object, memory cap, orphan handling), Linux kernels older than
5.13 (A14.5 is shown by the recorded-skip path, not on an old kernel), the Isartor suite, the nightly
fuzz campaigns and every CI job.
