# TEST_MATRIX

Every test added by the project, the crate it lives in, and the CI job that runs it.
Required by the Phase 0 First Milestone (item 6) and by the Definition of Done (`IMPLEMENTATION_PLAN.md` §0.3 item 7).

`Status`: `written` = the test exists and is RED · `green` = it exists and passes · `—` = not yet written.
A test is never `#[ignore]`d; a test that cannot run everywhere is gated behind a cargo feature and the
CI job that enables that feature is named here (`IMPLEMENTATION_PLAN.md` §0.2).

## CI jobs (`.github/workflows/ci.yml`)

| Job | Runs |
|---|---|
| `lint` | `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `xtask ci-lint`, `xtask thresholds-lint` |
| `deny` | `cargo deny check --all-features` |
| `test` | `cargo nextest run --workspace --locked --profile ci` on ubuntu-latest, macos-latest, windows-latest |
| `no-network` | conversion + inspection tests under `unshare -n`; `xtask assert-no-net-deps` |
| `epubcheck` | `oc-validate --features epubcheck`, `xtask epubcheck-corpus --max-errors 0` |
| `dom-checks` | Playwright DOM assertions (Chromium) |
| `ui` | Vitest + lint for `apps/desktop/ui` |

`.github/workflows/nightly.yml` declares `full-corpus`, `webkit-dom`, `mutation-testing`,
`proptest-deep` (`PROPTEST_CASES=4096`), `bench`, `live-llm-cassette-refresh`, `ace-a11y`; each body is
replaced by the phase that owns it.

## Phase 0

| # | Test | Crate | Kind | CI job | Status |
|---|---|---|---|---|---|
| 0.1 | `ids::block_id_is_stable_for_same_inputs` | `oc-model` | unit | `test` | green |
| 0.2 | `ids::prop_block_id_collision_suffix_is_unique` | `oc-model` | property | `test`, `proptest-deep` | green |
| 0.3 | `canonical::canonical_json_sorts_keys_and_rounds_geometry` | `oc-model` | snapshot | `test` | green |
| 0.4 | `canonical::canonical_json_rejects_nan` | `oc-model` | unit | `test` | green |
| 0.5 | `thresholds::every_provisional_has_owner_and_future_review` | `oc-core` | CI-gate | `test`, `lint` | green |
| 0.6 | `thresholds::generated_constants_match_toml` | `oc-core` | unit | `test` | green |
| 0.7 | `pdfium::binds_and_reports_version` | `oc-pdf` | integration | `test` | — |
| 0.8 | `geom::prop_normalised_rects_are_inside_page` | `oc-pdf` | property | `test`, `proptest-deep` | — |
| 0.9 | `classify::classify_text_page` | `oc-pdf` | unit | `test` | — |
| 0.10 | `classify::classify_image_only_page` | `oc-pdf` | unit | `test` | — |
| 0.11 | `classify::classify_ocr_sandwich_page` | `oc-pdf` | unit | `test` | — |
| 0.12 | `classify::classify_broken_text_page` | `oc-pdf` | unit | `test` | — |
| 0.13 | `producer::producer_family_table` | `oc-pdf` | unit (table-driven) | `test` | — |
| 0.14 | `inspect::inspect_f01_prose_single_column` | `oc-pdf` | snapshot (insta) | `test` | — |
| 0.15 | `inspect::inspect_f02_two_column` | `oc-pdf` | snapshot | `test` | — |
| 0.16 | `inspect::inspect_f03_image_only` | `oc-pdf` | snapshot | `test` | — |
| 0.17 | `cli::inspect_json_is_valid_and_stable` | `openconvert` | integration | `test`, `no-network` | — |
| 0.18 | `events::hello_is_first_stderr_line` | `openconvert` | integration | `test` | — |
| 0.19 | `cli::exit_code_2_on_bad_args` | `openconvert` | integration | `test` | — |
| 0.20 | `fixtures::typst_fixtures_are_reproducible` | `xtask` | fixture/CI | `test` | — |
| 0.21 | `assertions::assertion_runner_understands_all_kinds` | `oc-testkit` | unit | `test` | — |
| 0.22 | `engine::spawn_receives_hello` | `apps/desktop/ui` | integration (Vitest + Tauri mock) | `ui` | — |
| 0.23 | `no_ignored_tests` | `xtask` | CI-gate | `lint` | — |
