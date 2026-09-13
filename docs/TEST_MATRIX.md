# TEST_MATRIX

Every test added by the project, the crate it lives in, and the CI job that runs it.
Required by the Phase 0 First Milestone (item 6) and by the Definition of Done (`IMPLEMENTATION_PLAN.md` §0.3 item 7).

A row numbered `N.Na` is an addition to the plan's table, with its reason in `docs/DECISIONS_LOG.md`.

`Status`: `written` = the test exists and is RED · `green` = it exists and passes · `—` = not yet written.
A test is never `#[ignore]`d; a test that cannot run everywhere is gated behind a cargo feature and the
CI job that enables that feature is named here (`IMPLEMENTATION_PLAN.md` §0.2).

## CI jobs (`.github/workflows/ci.yml`)

`ci` runs on every pull request and on pushes to `main`.

| Job | Runs | On |
|---|---|---|
| `lint` | `cargo fmt --all --check`, `cargo clippy --workspace --exclude openconvert-desktop --all-targets --all-features -- -D warnings`, `xtask ci-lint`, `xtask thresholds-lint` | now |
| `deny` | `cargo deny --all-features check`, and the same licence/ban/source policy over `deny.tools.toml` | now |
| `test` | `cargo nextest run --workspace --exclude openconvert-desktop --locked --profile ci` on ubuntu-latest, macos-latest, windows-latest | now |
| `desktop` | GTK/WebKit, `ui/dist`, a staged sidecar, then clippy over `openconvert-desktop` | now |
| `no-network` | conversion + inspection tests under `unshare -n` | now |
| `poppler-oracle` | `oc-pdf --features poppler-oracle`, the differential `pdftotext` tests | now |
| `ui` | Vitest + lint for `apps/desktop/ui` | now |
| `epubcheck` | `oc-validate --features epubcheck`, `xtask epubcheck-corpus --max-errors 0` | **Phase 5** |
| `dom-checks` | Playwright DOM assertions (Chromium) | **Phase 6** |
| `no-network` → `assert-no-net-deps` | `xtask assert-no-net-deps` (one step, not a job) | **Phase 14** |

**The last three are `if: false`**, because the commands they call do not exist yet. A job that
reports red for a reason unrelated to the code under review teaches everyone to ignore the colour,
which is worse than an absent job; the comment above each one names the phase that turns it on.
Until Phase 14, the socket ban is enforced by `deny.toml`'s `wrappers` rule inside `deny`, which is
a build-time property rather than a weaker check.

**Why the engine jobs exclude `openconvert-desktop`:** the Tauri crate has no Rust tests — test 0.22
is a Vitest test in `apps/desktop/ui`, in the `ui` job — so its only assertion is that it compiles,
and buying that inside `--workspace` costs every job on every OS a GUI toolchain, a built
`ui/dist`, and a staged sidecar that `tauri-build` resolves at build time. It is bought once, on
Linux, in `desktop`. `cargo fmt --all` and `cargo deny` still cover the crate. See
`docs/DECISIONS_LOG.md`, 2026-09-13.

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
| 2.10 | `furniture::furniture_removes_repeating_header_f01` | `openconvert` | fixture (f01) | `test` | green |
| 2.11 | `furniture::furniture_detects_page_numbers_by_progression` | `openconvert` | fixture (f01) | `test` | green |
| 2.12 | `furniture::furniture_keeps_chapter_number_that_is_not_a_progression` | `openconvert` | fixture (h19) | `test` | green |
| 2.13 | `furniture::furniture_respects_parity` | `openconvert` | fixture (h20) | `test` | green |
| 2.14 | `furniture::furniture_never_removes_sole_page_content` | `openconvert` | fixture (h21) | `test` | green |
| 2.10a | `furniture::digits_mask_so_page_twelve_and_page_one_hundred_match` | `oc-layout` | unit | `test` | green |
| 2.11a | `furniture::roman_numerals_are_page_numbers_too` | `oc-layout` | unit | `test` | green |
| 2.10b | `furniture::edit_distance_is_normalised_by_length` | `oc-layout` | unit | `test` | green |
| 2.15 | `conservation::conservation_i1_holds_across_text_and_furniture` | `openconvert` | fixture (f01, f02) | `test` | green |
| 2.15a | `conservation::conservation_i1_holds_over_generated_documents` | `openconvert` | property (200) | `test`, `proptest-deep` | green |
| 2.15b | `conservation::an_inferred_space_is_never_ledgered` | `openconvert` | property | `test`, `proptest-deep` | green |
| 2.15c | `conservation::a_ligature_reaches_the_flow_expanded_and_balanced` | `openconvert` | fixture (h04) | `test` | green |
| 2.15d | `conservation::a_budget_breach_stops_the_stage` | `openconvert` | fixture (h20) | `test` | green |
| 2.18 | `stats::quality_stats_match_datatrove_thresholds` | `oc-text` | unit | `test` | green |
| 2.18a | `stats::clean_prose_is_ok` | `oc-text` | unit | `test` | green |
| 2.18b | `stats::a_repeated_phrase_shows_up_in_the_top_ngram` | `oc-text` | unit | `test` | green |
| 2.18c | `stats::replacement_characters_are_counted_as_a_share_of_the_text` | `oc-text` | unit | `test` | green |
| 2.18d | `stats::a_page_of_glyph_indices_is_broken_by_both_signals` | `oc-text` | unit | `test` | green |
| 2.18e | `stats::empty_text_is_not_broken_it_is_empty` | `oc-text` | unit | `test` | green |
| 2.22 | `broken_text::dict_hit_rate_feeds_broken_text_classification` | `openconvert` | fixture (mutation) | `test` | green |
| 2.22a | `broken_text::the_unmutated_fixture_passes_both_signals` | `openconvert` | fixture (f01) | `test` | green |
| 2.22b | `freq::the_english_blob_is_well_formed` | `oc-text` | unit | `test` | green |
| 2.22c | `freq::common_words_are_in_and_glyph_indices_are_not` | `oc-text` | unit | `test` | green |
| 2.22d | `freq::a_language_with_no_list_answers_none_not_zero` | `oc-text` | unit | `test` | green |
| 2.22e | `freq::prose_scores_high_and_glyph_indices_score_zero` | `oc-text` | unit | `test` | green |
| 2.22f | `freq::an_empty_text_is_unmeasured_not_zero` | `oc-text` | unit | `test` | green |
| 2.22g | `freq::tokens_are_classified_before_they_are_counted` | `oc-text` | unit | `test` | green |
| 2.19 | `language::language_detected_en_de_tr` | `openconvert` | fixture (f01, f04, f05) | `test` | green |
| 2.19a | `language::the_turkish_page_keeps_its_dotted_and_dotless_i` | `openconvert` | fixture (f05) | `test` | green |
| 2.19b | `lang::every_language_whatlang_knows_has_a_two_letter_tag` | `oc-text` | unit | `test` | green |
| 2.19c | `lang::the_three_languages_v1_claims_are_named_correctly` | `oc-text` | unit | `test` | green |
| 2.19d | `lang::a_document_with_no_text_falls_back_and_says_so` | `oc-text` | unit | `test` | green |
| 2.20 | `lang::block_lang_override_capped` | `oc-text` | unit | `test` | green |
| 2.20a | `lang::a_single_foreign_block_is_tagged` | `oc-text` | unit | `test` | green |
| 2.20b | `lang::a_short_block_is_never_tagged` | `oc-text` | unit | `test` | green |
| 2.20c | `lang::a_block_in_the_documents_own_language_gets_no_attribute` | `oc-text` | unit | `test` | green |
| 2.21 | `dump_text::dump_stage_text_snapshot_f01` | `openconvert` | snapshot | `test` | green |

## Phase 3

| # | Test | Crate | Kind | CI job | Status |
|---|---|---|---|---|---|
| 3.1 | `layout::blocks_docstrum_and_whitespace_agree_on_f01` | `openconvert` | fixture (f01) | `test` | green |
| 3.1a | `blocks::blank_line_separates_two_blocks` | `oc-layout` | unit | `test` | green |
| 3.1b | `blocks::a_gutter_is_not_crossed_by_a_block` | `oc-layout` | unit | `test` | green |
| 3.1c | `blocks::a_single_line_page_is_one_block` | `oc-layout` | unit | `test` | green |
| 3.1d | `blocks::disagreement_flags_the_block_low_confidence` | `oc-layout` | unit | `test` | green |
| 3.1e | `blocks::every_block_gets_its_own_id` | `oc-layout` | unit | `test` | green |
| 3.2 | `layout::two_column_reading_order_is_left_then_right` | `openconvert` | fixture (f02) | `test` | green |
| 3.3 | `layout::floating_title_is_premasked_not_split` | `openconvert` | fixture (f02) | `test` | green |
| 3.4 | `reading_order::prop_single_column_order_is_monotone_in_y` | `oc-layout` | property | `test` | green |
| 3.4a | `reading_order::a_two_column_page_is_read_down_then_across` | `oc-layout` | unit | `test` | green |
| 3.4b | `reading_order::a_masked_block_with_nothing_below_it_goes_last` | `oc-layout` | unit | `test` | green |
| 3.2a | `columns::a_wide_valley_between_two_bodies_of_text_is_a_gutter` | `oc-layout` | unit | `test` | green |
| 3.2b | `columns::the_blank_half_of_a_short_column_is_not_a_gutter` | `oc-layout` | unit | `test` | green |
| 3.2c | `columns::a_single_column_page_has_no_gutter` | `oc-layout` | unit | `test` | green |
| 3.3a | `columns::a_crossing_title_does_not_destroy_the_gutter` | `oc-layout` | unit | `test` | green |
| 3.3b | `columns::a_block_that_crosses_the_gutter_is_marked_a_floating_title` | `oc-layout` | unit | `test` | green |
| 3.5 | `layout::cross_page_continuity_downgrades_column_count` | `openconvert` | fixture (h22) | `test` | green |
| 3.5a | `layout::the_false_gutter_is_found_before_continuity_rejects_it` | `openconvert` | fixture (h22) | `test` | green |
| 3.5b | `continuity::a_sentence_that_crosses_a_page_boundary_is_continuous` | `oc-layout` | unit | `test` | green |
| 3.5c | `continuity::a_page_that_ends_its_sentence_is_not_evidence_of_continuity` | `oc-layout` | unit | `test` | green |
| 3.5d | `continuity::an_uppercase_start_breaks_continuity_even_without_punctuation` | `oc-layout` | unit | `test` | green |
| 3.5e | `continuity::a_single_page_has_no_boundary_to_measure` | `oc-layout` | unit | `test` | green |
| 3.5f | `continuity::a_blank_page_is_skipped_not_counted_as_a_break` | `oc-layout` | unit | `test` | green |
| 3.5g | `columns::a_capped_page_reports_one_column` | `oc-layout` | unit | `test` | green |
| 3.13 | `layout::layout_stage_is_conserving` | `openconvert` | fixture (f01) | `test` | green |
| 3.13a | `layout::layout_stage_conservation_violation_errors` | `openconvert` | unit | `test` | green |
