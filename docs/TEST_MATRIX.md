# TEST_MATRIX

Every test added by the project, the crate it lives in, and the CI job that runs it.
Required by the Phase 0 First Milestone (item 6) and by the Definition of Done (`IMPLEMENTATION_PLAN.md` §0.3 item 7).

A row numbered `N.Na` is an addition to the plan's table, with its reason in `docs/DECISIONS_LOG.md`.

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
| 0.7 | `pdfium::binds_and_reports_version` | `oc-pdf` | integration | `test` | green |
| 0.8 | `geom::prop_normalised_rects_are_inside_page` | `oc-pdf` | property | `test`, `proptest-deep` | green |
| 0.8a | `geom::normalises_corners_for_each_rotation` | `oc-pdf` | unit | `test` | green |
| 0.9 | `classify::classify_text_page` | `oc-pdf` | unit | `test` | green |
| 0.10 | `classify::classify_image_only_page` | `oc-pdf` | unit | `test` | green |
| 0.11 | `classify::classify_ocr_sandwich_page` | `oc-pdf` | unit | `test` | green |
| 0.12 | `classify::classify_broken_text_page` | `oc-pdf` | unit | `test` | green |
| 0.12a | `classify::classify_mixed_blank_and_dictionary_arm` | `oc-pdf` | unit | `test` | green |
| 0.13 | `producer::producer_family_table` | `oc-pdf` | unit (table-driven) | `test` | green |
| 0.14 | `inspect::inspect_f01_prose_single_column` | `oc-pdf` | snapshot (insta) | `test` | green |
| 0.15 | `inspect::inspect_f02_two_column` | `oc-pdf` | snapshot | `test` | green |
| 0.16 | `inspect::inspect_f03_image_only` | `oc-pdf` | snapshot | `test` | green |
| 0.17 | `cli::inspect_json_is_valid_and_stable` | `openconvert` | integration | `test`, `no-network` | green |
| 0.18 | `events::hello_is_first_stderr_line` | `openconvert` | integration | `test` | green |
| 0.19 | `cli::exit_code_2_on_bad_args` | `openconvert` | integration | `test` | green |
| 0.20 | `fixtures::typst_fixtures_are_reproducible` | `xtask` | fixture/CI | `test` | green |
| 0.21 | `assertions::assertion_runner_understands_all_kinds` | `oc-testkit` | unit | `test` | green |
| 0.22 | `engine > spawn_receives_hello` | `apps/desktop/ui` | integration (Vitest + Tauri mock) | `ui` | green |
| 0.23 | `ci::no_ignored_tests` | `xtask` | CI-gate | `test`, `lint` | green |
| 0.23a | `ci::thresholds_pass_their_own_provenance_rule` | `xtask` | CI-gate | `test`, `lint` | green |

## Phase 1

| # | Test | Crate | Kind | CI job | Status |
|---|---|---|---|---|---|
| 1.1 | `glyphs::glyphs_carry_all_verified_signals` | `oc-pdf` | fixture (h01) | `test` | green |
| 1.2 | `glyphs::generated_spaces_are_dropped_and_ledgered` | `oc-pdf` | fixture (h06) | `test` | green |
| 1.3 | `glyphs::invisible_render_mode_3_is_not_visible_text` | `oc-pdf` | fixture (h05) | `test` | green |
| 1.4 | `glyphs::overdraw_duplicate_glyphs_are_deduped` | `oc-pdf` | fixture (h07, h08) | `test` | green |
| 1.5 | `metamorphic::prop_rotate_invariance_of_extracted_text` | `oc-pdf` | metamorphic | `test`, `proptest-deep` | green |
| 1.6 | `metamorphic::cropbox_offset_does_not_lose_text` | `oc-pdf` | metamorphic (h03) | `test` | green |
| 1.7 | `metamorphic::prop_content_stream_reorder_invariance` | `oc-pdf` | metamorphic | `test`, `proptest-deep` | green |
| 1.8 | `inspect::stripped_tounicode_page_classifies_broken_text` | `oc-pdf` | fixture (mutation) | `test` | green |
| 1.9 | `images::image_only_page_extracts_one_image_with_dpi` | `oc-pdf` | fixture (f03) | `test` | green |
| 1.10 | `image_pixel_bomb_is_refused_before_decode` | `oc-pdf` | unit | `test` | — |
| 1.11 | `decompression_bomb_is_bounded` | `oc-pdf` | fixture | `test` | — |
| 1.12 | `encrypted_empty_user_password_opens` | `oc-pdf` | fixture | `test` | — |
| 1.13 | `encrypted_with_password_requires_flag` | `openconvert` | integration | `test` | — |
| 1.14 | `owner_password_permissions_recorded_not_enforced` | `oc-pdf` | fixture | `test` | — |
| 1.15 | `outline_is_read_depth_first` | `oc-pdf` | fixture | `test` | — |
| 1.16 | `prop_never_panics_on_arbitrary_bytes` | `oc-pdf` | property | `test`, `proptest-deep` | — |
| 1.17 | `dump_stage_ingest_snapshot_h01` | `oc-core` | snapshot | `test` | — |
| 1.18 | `differential_pdftotext_coverage_f01` | `oc-pdf` | integration (oracle) | `test` | — |
| 1.19 | `cancel_is_observed_inside_page_loop` | `oc-core` | integration | `test` | — |
| 1.20 | `max_pages_refuses_at_the_door` | `oc-core` | unit | `test` | — |
