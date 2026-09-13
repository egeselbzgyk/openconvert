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
| 1.10 | `limits::image_pixel_bomb_is_refused_before_decode` | `oc-pdf` | fixture (h11) | `test` | green |
| 1.11 | `limits::decompression_bomb_is_bounded` | `oc-pdf` | fixture (h12) | `test` | green |
| 1.12 | `encrypt::encrypted_empty_user_password_opens` | `oc-pdf` | fixture (mutation) | `test` | green |
| 1.13 | `cli::encrypted_with_password_requires_flag` | `openconvert` | integration | `test` | green |
| 1.14 | `encrypt::owner_password_permissions_recorded_not_enforced` | `oc-pdf` | fixture (mutation) | `test` | green |
| 1.15 | `outline::outline_is_read_depth_first` | `oc-pdf` | fixture (h13) | `test` | green |
| 1.16 | `fuzz_lite::prop_never_panics_on_arbitrary_bytes` | `oc-pdf` | property | `test`, `proptest-deep` | green |
| 1.16a | `fuzz_lite::prop_never_panics_on_truncated_fixtures` | `oc-pdf` | property | `test`, `proptest-deep` | green |
| 1.16b | `fuzz_lite::prop_never_panics_on_corrupted_fixtures` | `oc-pdf` | property | `test`, `proptest-deep` | green |
| 1.16c | `fuzz_lite::corruption_reaches_the_extraction_paths` | `oc-pdf` | unit | `test` | green |
| 1.17 | `dump::dump_stage_ingest_snapshot_h01` | `oc-pdf` | snapshot | `test` | green |
| 1.17a | `dump::char_histogram_serialises_only_present_characters` | `oc-pdf` | unit | `test` | green |
| 1.17b | `cli::dump_stage_ingest_streams_one_object_per_line` | `openconvert` | integration | `test` | green |
| 1.17c | `cli::dump_stage_rejects_an_unimplemented_stage` | `openconvert` | integration | `test` | green |
| 1.18 | `oracle::differential_pdftotext_coverage_f01` | `oc-pdf` | integration (oracle) | `poppler-oracle` | green |
| 1.18a | `oracle::differential_pdftotext_coverage_f02` | `oc-pdf` | integration (oracle) | `poppler-oracle` | green |
| 1.9a | `scaling::per_page_extraction_cost_does_not_grow_with_page_count` | `oc-pdf` | perf regression | `test` | green |
| VD-d.1 | `smask_spike::processed_image_applies_the_soft_mask` | `oc-pdf` | known-answer | `test` | green |
| VD-d.2 | `smask_spike::processed_image_applies_a_stencil_mask` | `oc-pdf` | known-answer | `test` | green |
| VD-d.3 | `smask_spike::processed_image_resolves_an_indexed_palette` | `oc-pdf` | known-answer | `test` | green |
| VD-d.4 | `smask_spike::device_gray_is_expanded_to_rgb` | `oc-pdf` | known-answer | `test` | green |
| VD-d.5 | `smask_spike::an_unmasked_image_comes_back_fully_opaque` | `oc-pdf` | known-answer | `test` | green |
| VD-d.6 | `smask_spike::an_inline_image_decodes_like_any_other` | `oc-pdf` | known-answer | `test` | green |
| VD-d.7 | `smask_spike::decoding_a_pixel_bomb_is_refused` | `oc-pdf` | unit | `test` | green |
| A1.1 | `acceptance::every_glyph_carries_thirteen_real_signals` | `oc-pdf` | acceptance | `test` | green |
| 1.19 | `cmd_dump_stage::cancel_is_observed_inside_page_loop` | `openconvert` | integration | `test` | green |
| 1.19a | `cmd_dump_stage::an_uncancelled_loop_writes_every_page` | `openconvert` | integration | `test` | green |
| 1.19b | `control::a_cancel_on_the_control_channel_sets_the_flag` | `openconvert` | unit | `test` | green |
| 1.19c | `control::other_control_traffic_does_not_cancel` | `openconvert` | unit | `test` | green |
| 1.19d | `control::a_closed_control_channel_does_not_cancel` | `openconvert` | unit | `test` | green |
| 1.19e | `control::control_messages_parse_and_unknown_ones_are_ignored` | `openconvert` | unit | `test` | green |
| 1.19f | `cancel::cancel_is_shared_by_clones` | `oc-core` | unit | `test` | green |
| 1.19g | `cancel::cancel_crosses_threads` | `oc-core` | unit | `test` | green |
| 1.20 | `limits::max_pages_refuses_at_the_door` | `oc-pdf` | integration | `test` | green |

## Phase 2

| # | Test | Crate | Kind | CI job | Status |
|---|---|---|---|---|---|
| 2.16 | `ledger_check::conservation_i3_conserving_stage_has_empty_ledger` | `oc-core` | unit | `test` | green |
| 2.17 | `ledger_check::conservation_i4_budget_exceeded_is_fatal` | `oc-core` | unit | `test` | green |
| 2.16a | `ledger_check::conservation_i1_unexplained_removal_is_fatal` | `oc-core` | unit | `test` | green |
| 2.16b | `ledger_check::conservation_i1_balances_a_ligature_expansion` | `oc-core` | unit | `test` | green |
| 2.16c | `ledger_check::conservation_i2_undeclared_reason_is_fatal` | `oc-core` | unit | `test` | green |
| 2.16d | `ledger_check::c_of_counts_only_non_whitespace` | `oc-core` | unit | `test` | green |
| 2.17a | `ledger_check::budgets_accumulate_across_stages` | `oc-core` | unit | `test` | green |
| 2.17b | `ledger_check::budget_charges_net_loss_not_churn` | `oc-core` | unit | `test` | green |
| 2.1 | `normalize::normalize_expands_ligatures_and_ledgers_both_sides` | `oc-text` | unit | `test` | green |
| 2.2 | `normalize::normalize_strips_soft_hyphen_with_reason` | `oc-text` | unit | `test` | green |
| 2.3 | `normalize::normalize_is_idempotent` | `oc-text` | property | `test`, `proptest-deep` | green |
| 2.4 | `normalize::normalize_never_applies_nfkc` | `oc-text` | unit | `test` | green |
| 2.4a | `normalize::normalize_composes_to_nfc` | `oc-text` | unit | `test` | green |
| 2.4b | `normalize::normalize_expands_the_whole_ligature_block` | `oc-text` | unit | `test` | green |
| 2.7a | `normalize::normalize_never_changes_case` | `oc-text` | property | `test`, `proptest-deep` | green |
| 2.6 | `fold::fold_key_is_turkish_aware` | `oc-text` | unit | `test` | green |
| 2.6a | `fold::fold_key_lowercases_the_rest_of_the_world_normally` | `oc-text` | unit | `test` | green |
| 2.6b | `fold::fold_key_is_nfc` | `oc-text` | unit | `test` | green |
| 2.6c | `lang::a_tag_is_case_insensitive_and_keeps_its_region` | `oc-model` | unit | `test` | green |
| 2.6d | `lang::only_turkish_and_azerbaijani_pair_the_dotless_i` | `oc-model` | unit | `test` | green |
| 2.7 | `fold::text_is_never_case_folded_in_output` | `oc-text` | property | `test`, `proptest-deep` | green |
| 2.4c | `hyphen_marker::a_line_break_hyphen_is_a_hyphen_not_a_control_character` | `oc-pdf` | fixture | `test` | green |
| 2.4d | `hyphen_marker::no_extracted_glyph_is_an_undecodable_control` | `oc-pdf` | fixture | `test` | green |
| 2.4e | `hyphen_marker::c_raw_counts_the_hyphen_the_page_prints` | `oc-pdf` | fixture | `test` | green |
| 2.5 | `text_assembly::superscript_flag_survives_normalization` | `openconvert` | fixture (h16) | `test` | green |
| 2.8 | `text_assembly::words_split_on_bimodal_gap` | `openconvert` | fixture (h17) | `test` | green |
| 2.9 | `text_assembly::lines_cluster_by_baseline_tolerance` | `openconvert` | fixture (h18) | `test` | green |
| 2.8a | `text_assembly::a_space_is_inserted_where_the_document_only_left_a_gap` | `openconvert` | fixture (h16) | `test` | green |
| 2.9a | `lines::a_superscript_joins_the_line_it_is_raised_from` | `oc-text` | unit | `test` | green |
| 2.9b | `lines::a_heading_does_not_join_the_body_below_it` | `oc-text` | unit | `test` | green |
| 2.9c | `lines::lines_come_back_top_to_bottom_whatever_order_they_arrived_in` | `oc-text` | unit | `test` | green |
| 2.9d | `lines::nothing_in_nothing_out` | `oc-text` | unit | `test` | green |
| 2.8b | `words::uniform_tracking_has_no_space_to_find` | `oc-text` | unit | `test` | green |
| 2.8c | `words::prose_gaps_threshold_between_the_two_modes` | `oc-text` | unit | `test` | green |
| 2.8d | `words::a_single_gap_cannot_be_clustered` | `oc-text` | unit | `test` | green |
| 2.8e | `words::a_flat_distribution_is_rejected_even_when_tight` | `oc-text` | unit | `test` | green |
