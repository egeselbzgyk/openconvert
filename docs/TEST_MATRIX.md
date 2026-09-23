# TEST_MATRIX

Every test added by the project, the crate it lives in, and the CI job that runs it.
Required by the Phase 0 First Milestone (item 6) and by the Definition of Done (`IMPLEMENTATION_PLAN.md` §0.3 item 7).

A row numbered `N.Na` is an addition to the plan's table, with its reason in `docs/DECISIONS_LOG.md`.
Rows 3.18 and 3.19 are whole steps of PIPELINE §6 (image anchoring, drop caps) that the plan's Phase 3
table does not name a test for; they are numbered past the table rather than squeezed into it.

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
| `epubcheck` | `openconvert --features epubcheck -E 'test(epubcheck_zero_errors)'` — row 5.17, A5.1 | now |
| `tier1-parity` | `xtask epubcheck-parity --check` over EPUBCheck's own corpus — row 5.18 | now |
| `epub-bytes` + `epub_is_byte_identical_across_os` | each OS converts `f07` with `--modified` pinned; a fourth job asserts the three sha256s agree — row 5.5, A5.3 | now |
| `dom-checks` | Playwright DOM assertions (Chromium) | **Phase 6** |
| `no-network` → AI path | `binary(ai_pipeline) or binary(ai)` under `unshare -n` with `OC_EXPECT_NO_NETWORK=1` — row 14.20; the `cargo tree` fact is the step `assert oc-core cannot reach a socket crate` (row 9.7), not an xtask | now (Phase 14) |

**Rows marked with a phase were `if: false`** until that phase, because the commands they called did
not exist yet. A job that reports red for a reason unrelated to the code under review teaches
everyone to ignore the colour, which is worse than an absent job; the comment above each one names the phase that turns it on.
Until Phase 14, the socket ban is enforced by `deny.toml`'s `wrappers` rule inside `deny`, which is
a build-time property rather than a weaker check.

**Why the engine jobs exclude `openconvert-desktop`:** the Tauri crate has no Rust tests — test 0.22
is a Vitest test in `apps/desktop/ui`, in the `ui` job — so its only assertion is that it compiles,
and buying that inside `--workspace` costs every job on every OS a GUI toolchain, a built
`ui/dist`, and a staged sidecar that `tauri-build` resolves at build time. It is bought once, on
Linux, in `desktop`. `cargo fmt --all` and `cargo deny` still cover the crate. See
`docs/DECISIONS_LOG.md`, 2026-09-13.

`.github/workflows/nightly.yml` declares `full-corpus`, `webkit-dom`, `mutation-testing`,
`proptest-deep` (`PROPTEST_CASES=4096`), `bench`, `live-llm-cassette-refresh`, `ace-a11y`, and (Phase 14)
`isartor` and `fuzz`; each body is replaced by the phase that owns it.

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

**Fixture numbers.** The plan's Phase 3 fixture names collide with numbers earlier phases spent, so
`f04_hyphenation_de` is **`f06_hyphenation_de`**, test 3.5's `h13` is **`h22_false_gutter`**, and test
3.7's fixture is **`h23_paragraph_across_pages`** (the plan names `f01`, whose hyphenated break is
within a page rather than across one). The test *name* is the contract; the fixture number is
indicative (PROGRESS.md).

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
| 3.6 | `layout::paragraph_convention_indent_detected` | `openconvert` | fixture (f01) | `test` | green |
| 3.6a | `paragraphs::an_indent_starts_a_paragraph_when_nothing_else_marks_one` | `oc-layout` | unit | `test` | green |
| 3.6b | `paragraphs::a_short_line_ends_its_paragraph` | `oc-layout` | unit | `test` | green |
| 3.6c | `paragraphs::a_nearly_full_line_does_not_end_a_paragraph` | `oc-layout` | unit | `test` | green |
| 3.6d | `paragraphs::a_paragraph_merges_across_a_page_break` | `oc-layout` | unit | `test` | green |
| 3.6e | `paragraphs::a_finished_sentence_does_not_merge_across_a_page_break` | `oc-layout` | unit | `test` | green |
| 3.6f | `paragraphs::a_hyphen_at_a_page_break_merges_the_paragraph` | `oc-layout` | unit | `test` | green |
| 3.6g | `paragraphs::joining_lines_does_not_resolve_a_hyphen` | `oc-layout` | unit | `test` | green |
| 3.6h | `paragraphs::every_paragraph_gets_its_own_id` | `oc-layout` | unit | `test` | green |
| 3.7 | `layout::paragraph_merges_across_page_break` | `openconvert` | fixture (h23) | `test` | green |
| 3.7a | `layout::dehyphenation_is_ledgered_one_hyphen_at_a_time` | `openconvert` | fixture (h23) | `test` | green |
| 3.7b | `layout::the_lexicon_is_built_from_the_document` | `openconvert` | fixture (h23) | `test` | green |
| 3.8 | `dehyphen::dehyphenate_joins_when_indoc_evidence` | `oc-text` | unit | `test` | green |
| 3.8a | `dehyphen::dehyphenate_keeps_when_the_document_spells_it_with_a_hyphen` | `oc-text` | unit | `test` | green |
| 3.8b | `dehyphen::a_document_that_spells_it_both_ways_decides_nothing` | `oc-text` | unit | `test` | green |
| 3.8c | `lexicon::the_documents_own_words_answer_for_it` | `oc-text` | unit | `test` | green |
| 3.8d | `lexicon::a_hyphenated_word_is_held_whole` | `oc-text` | unit | `test` | green |
| 3.8e | `lexicon::a_line_break_is_not_a_word` | `oc-text` | unit | `test` | green |
| 3.8f | `lexicon::punctuation_is_stripped_from_the_ends_of_words` | `oc-text` | unit | `test` | green |
| 3.8g | `lexicon::folding_is_the_documents_own` | `oc-text` | unit | `test` | green |
| 3.10 | `dehyphen::dehyphenate_fails_closed_on_unknown` | `oc-text` | unit | `test` | green |
| 3.10a | `dehyphen::an_uppercase_continuation_is_not_a_candidate_in_english` | `oc-text` | unit | `test` | green |
| 3.10b | `dehyphen::an_uppercase_continuation_is_still_a_candidate_in_german` | `oc-text` | unit | `test` | green |
| 3.10c | `dehyphen::a_number_range_is_never_joined` | `oc-text` | unit | `test` | green |
| 3.10d | `dehyphen::a_line_without_a_hyphen_is_not_a_candidate` | `oc-text` | unit | `test` | green |
| 3.10e | `dehyphen::a_non_breaking_hyphen_is_not_a_line_break` | `oc-text` | unit | `test` | green |
| 3.10f | `dehyphen::two_attested_halves_that_also_form_a_word_go_to_the_classifier` | `oc-text` | unit | `test` | green |
| 3.11 | `dehyphen::dehyphenate_i5_removes_exactly_one_hyphen` | `oc-text` | property | `test` | green |
| 3.17 | `dehyphen::turkish_agglutinative_join_prefers_keep` | `oc-text` | unit | `test` | green |
| 3.9 | `layout::dehyphenate_keeps_german_real_hyphen` | `openconvert` | fixture (f06) | `test` | green |
| 3.9a | `layout::the_german_fixture_removes_exactly_one_hyphen` | `openconvert` | fixture (f06) | `test` | green |
| 3.9b | `compound_de::an_uppercase_continuation_is_a_real_hyphen` | `oc-text` | unit | `test` | green |
| 3.9c | `compound_de::a_lowercase_continuation_is_a_broken_word` | `oc-text` | unit | `test` | green |
| 3.9d | `compound_de::a_compound_of_two_attested_words_is_accepted` | `oc-text` | unit | `test` | green |
| 3.9e | `compound_de::a_linking_morpheme_at_the_seam_is_allowed` | `oc-text` | unit | `test` | green |
| 3.9f | `compound_de::the_longest_linking_morpheme_wins` | `oc-text` | unit | `test` | green |
| 3.9g | `compound_de::a_word_with_no_attested_parts_is_not_a_compound` | `oc-text` | unit | `test` | green |
| 3.9h | `compound_de::a_two_letter_part_is_not_a_compound_seam` | `oc-text` | unit | `test` | green |
| 3.12 | `hyphen_holdout::hyphen_classifier_keep_recall_on_holdout` | `oc-text` | golden-decision | `test` | green |
| 3.12a | `hyphen_holdout::the_training_manifest_matches_the_committed_model` | `oc-text` | unit | `test` | green |
| 3.12b | `classifier::the_shipped_model_is_readable` | `oc-text` | unit | `test` | green |
| 3.12c | `classifier::the_hash_is_the_one_the_trainer_uses` | `oc-text` | unit | `test` | green |
| 3.12d | `classifier::the_feature_set_matches_the_trainer` | `oc-text` | unit | `test` | green |
| 3.12e | `classifier::a_low_margin_is_undecided_and_therefore_keeps` | `oc-text` | unit | `test` | green |
| 3.10g | `dehyphen::an_unattested_pair_the_model_is_sure_about_is_decided` | `oc-text` | unit | `test` | green |
| 3.14 | `layout_metamorphic::prop_page_permutation_metamorphic` | `openconvert` | metamorphic | `test` | green |
| 3.15 | `dump_layout::dump_stage_layout_snapshot_f02` | `openconvert` | snapshot | `test` | green |
| 3.15a | `dump_layout::dump_stage_layout_header_f02` | `openconvert` | snapshot | `test` | green |
| 3.16 | `dump_layout::digest_f01_layout` | `openconvert` | digest snapshot | `test` | green |
| 3.16a | `dump_layout::digest_h22_layout` | `openconvert` | digest snapshot | `test` | green |
| 3.18 | `layout::an_image_only_page_keeps_its_image_in_the_flow` | `openconvert` | fixture (f03) | `test` | green |
| 3.18a | `anchor::an_image_is_anchored_before_the_block_below_it` | `oc-layout` | unit | `test` | green |
| 3.18b | `anchor::an_image_below_everything_goes_last` | `oc-layout` | unit | `test` | green |
| 3.18c | `anchor::an_image_on_a_page_with_no_text_is_still_anchored` | `oc-layout` | unit | `test` | green |
| 3.18d | `anchor::two_images_at_one_anchor_keep_their_order` | `oc-layout` | unit | `test` | green |
| 3.19 | `anchor::a_large_single_glyph_beside_text_is_a_drop_cap` | `oc-layout` | unit | `test` | green |
| 3.19a | `anchor::a_lone_large_glyph_is_not_a_drop_cap` | `oc-layout` | unit | `test` | green |
| 3.19b | `anchor::a_body_sized_single_glyph_is_not_a_drop_cap` | `oc-layout` | unit | `test` | green |
| 3.13 | `layout::layout_stage_is_conserving` | `openconvert` | fixture (f01) | `test` | green |
| 3.13a | `layout::layout_stage_conservation_violation_errors` | `openconvert` | unit | `test` | green |

## Phase 4

| # | Test | Crate | Kind | CI job | Status |
|---|---|---|---|---|---|
| 4.1 | `structure::outline_is_used_as_heading_ground_truth` | `openconvert` | fixture (f09) | `test` | green |
| 4.2 | `structure::toc_page_parsed_when_no_outline` | `openconvert` | fixture (f09) | `test` | green |
| 4.2a | `toc_page::a_dotted_leader_line_parses_into_title_and_folio` | `oc-structure` | unit | `test` | green |
| 4.2b | `toc_page::prose_that_ends_in_a_number_is_not_a_contents_line` | `oc-structure` | unit | `test` | green |
| 4.3 | `structure::style_clusters_identify_body_mode` | `openconvert` | fixture (f01) | `test` | green |
| 4.3a | `cluster::the_mode_is_body_and_the_larger_short_style_is_the_candidate` | `oc-structure` | unit | `test` | green |
| 4.3b | `cluster::sizes_within_one_quantum_cluster_together` | `oc-structure` | unit | `test` | green |
| 4.3c | `cluster::an_empty_document_has_no_body_cluster` | `oc-structure` | unit | `test` | green |
| 4.4 | `structure::heading_level_from_size_rank` | `openconvert` | fixture (f10) | `test` | green |
| 4.4a | `numbering::keywords_are_read_in_all_three_languages` | `oc-structure` | unit | `test` | green |
| 4.4b | `numbering::turkish_keywords_fold_under_turkish_rules` | `oc-structure` | unit | `test` | green |
| 4.4c | `numbering::a_dotted_number_gives_its_own_depth` | `oc-structure` | unit | `test` | green |
| 4.4d | `numbering::a_keyword_without_a_number_is_not_numbering` | `oc-structure` | unit | `test` | green |
| 4.4e | `numbering::roman_numerals_are_read_and_valued` | `oc-structure` | unit | `test` | green |
| 4.5 | `structure::heading_tree_has_no_level_skips` | `openconvert` | property (8 fixtures × 3 sources) | `test` | green |
| 4.5a | `levels::a_skipped_level_is_closed_and_a_descent_is_not` | `oc-structure` | unit | `test` | green |
| 4.5b | `levels::the_first_heading_is_always_level_one` | `oc-structure` | unit | `test` | green |
| 4.6 | `cluster::style_inventory_invalid_above_24_clusters` | `oc-structure` | unit | `test` | green |
| 4.6a | `cluster::style_inventory_invalid_when_no_cluster_is_the_body` | `oc-structure` | unit | `test` | green |
| 4.7 | `structure::footnote_marker_body_bijection` | `openconvert` | fixture (f08) | `test` | green |
| 4.8 | `structure::footnote_symbol_cycle_resets_per_page` | `openconvert` | fixture (h24) | `test` | green |
| 4.9 | `structure::caption_associated_to_nearest_figure` | `openconvert` | fixture (f10) | `test` | green |
| 4.9a | `figures::localized_prefixes_are_recognised_with_their_number` | `oc-structure` | unit | `test` | green |
| 4.9b | `figures::a_prefix_word_without_a_number_is_not_a_caption` | `oc-structure` | unit | `test` | green |
| 4.9c | `figures::edge_distance_is_zero_for_overlapping_boxes_and_grows_with_the_gap` | `oc-structure` | unit | `test` | green |
| 4.10 | `structure::ambiguous_caption_left_unassociated` | `openconvert` | fixture (h25) | `test` | green |
| 4.11 | `structure::ordered_list_numbering_is_contiguous` | `openconvert` | fixture (f10) | `test` | green |
| 4.11a | `lists::markers_of_every_kind_are_read_with_their_ordinal` | `oc-structure` | unit | `test` | green |
| 4.11b | `structure::prose_with_no_list_yields_no_list` | `openconvert` | fixture (f01) | `test` | green |
| 4.12 | `lists::year_paragraph_is_not_a_list_item` | `oc-structure` | unit | `test` | green |
| 4.13 | `structure::ruled_table_becomes_html_table` | `openconvert` | fixture (f10) | `test` | green |
| 4.13a | `tables::a_ladder_places_a_value_in_its_band` | `oc-structure` | unit | `test` | green |
| 4.13b | `tables::coordinates_within_the_snap_collapse_to_one` | `oc-structure` | unit | `test` | green |
| 4.13c | `tables::the_cell_multiset_check_counts_repeats` | `oc-structure` | unit | `test` | green |
| 4.14 | `structure::borderless_table_falls_back_to_image_with_details` | `openconvert` | fixture (h26) | `test` | green |
| 4.15 | `structure::ornament_repeated_on_most_pages_is_dropped` | `openconvert` | fixture (h27) | `test` | green |
| 4.15a | `images::a_large_repeated_image_is_never_an_ornament` | `oc-structure` | unit | `test` | green |
| 4.15b | `images::a_short_document_has_no_ornaments` | `oc-structure` | unit | `test` | green |
| 4.15c | `images::two_different_images_do_not_add_up_to_one_ornament` | `oc-structure` | unit | `test` | green |
| 4.15d | `images::the_perceptual_hash_separates_two_greys_and_joins_two_copies` | `oc-pdf` | unit | `test` | green |
| 4.16 | `structure::metadata_prefers_xmp_over_boilerplate_docinfo` | `openconvert` | fixture (h28) | `test` | green |
| 4.16a | `meta::the_boilerplate_blocklist_catches_what_producers_emit` | `oc-structure` | unit | `test` | green |
| 4.16b | `meta::a_filename_becomes_a_title_only_when_it_is_not_boilerplate` | `oc-structure` | unit | `test` | green |
| 4.16c | `meta::dublin_core_fields_are_read_out_of_a_packet` | `oc-pdf` | unit | `test` | green |
| 4.17 | `meta::identifier_is_stable_across_reconversions` | `oc-structure` | unit | `test` | green |
| 4.17a | `structure::identifier_is_stable_across_reconversions` | `openconvert` | fixture (h28) | `test` | green |
| 4.18 | `structure::verse_and_quote_ambiguity_recorded_not_guessed` | `openconvert` | fixture (f07) | `test` | green |
| 4.18a | `quotes::an_attribution_is_a_short_capitalised_tail_after_a_dash` | `oc-structure` | unit | `test` | green |
| 4.18b | `quotes::ambiguity_is_resolved_and_never_emitted` | `oc-structure` | unit | `test` | green |
| 4.19 | `structure::structure_stage_is_conserving` | `openconvert` | fixture (9 documents) | `test` | green |
| 4.20 | `structure::digest_f09_structure` | `openconvert` | digest snapshot | `test` | green |
| 4.20a | `structure::digest_f10_structure` | `openconvert` | digest snapshot | `test` | green |
| 4.20b | `structure::dump_stage_structure_header_f09` | `openconvert` | snapshot | `test` | green |
| 4.21 | `structure::drop_cap_is_not_a_one_char_paragraph` | `openconvert` | fixture (h29) | `test` | green |
| 4.22 | `structure::book_structure_runs_front_body_back` | `openconvert` | fixture (f09) | `test` | green |
| 4.22a | `book::a_roman_run_followed_by_arabic_one_is_the_body_boundary` | `oc-structure` | unit | `test` | green |
| 4.22b | `book::back_matter_keywords_are_read_in_all_three_languages` | `oc-structure` | unit | `test` | green |
| 4.22c | `book::front_matter_keywords_are_read_in_all_three_languages` | `oc-structure` | unit | `test` | green |
| 4.23 | `outline::every_outline_entry_is_read_exactly_once` | `oc-pdf` | fixture (f09) | `test` | green |
| 4.24 | `vectors::a_filled_hairline_is_read_as_a_horizontal_rule` | `oc-pdf` | fixture (h24) | `test` | green |
| 4.24a | `vectors::a_page_with_no_paths_has_no_vector_regions` | `oc-pdf` | fixture (h01) | `test` | green |
| 4.24b | `vectors::a_block_is_not_a_rule_however_it_is_proportioned` | `oc-pdf` | unit | `test` | green |
| 4.25 | `blocks::a_size_change_splits_a_block_where_distance_alone_cannot` | `oc-layout` | unit | `test` | green |
| 4.25a | `blocks::a_drop_cap_does_not_split_its_own_line_off` | `oc-layout` | unit | `test` | green |
| 4.26 | `doc::every_content_variant_is_listed_and_named_once` | `oc-model` | unit | `test` | green |
| 4.26a | `doc::heading_levels_are_clamped_into_the_xhtml_range` | `oc-model` | unit | `test` | green |
| 4.26b | `doc::the_three_zones_are_ordered_front_body_back` | `oc-model` | unit | `test` | green |
| 4.27 | `similarity::distance_is_zero_for_equal_strings_and_scaled_by_the_longer` | `oc-text` | unit | `test` | green |

## Phase 5 — EPUB generation, Tier-1 validator, EPUBCheck CI gate

| # | Test | Crate | Kind | CI job | Status |
|---|---|---|---|---|---|
| 5.1 | `compile_fail::phrasing_cannot_contain_figure` | `oc-epub` | compile-fail (`trybuild`) | `test` | green |
| 5.2 | `compile_fail::anchor_cannot_nest` | `oc-epub` | compile-fail (`trybuild`) | `test` | green |
| 5.3 | `zip::zip_mimetype_is_first_and_stored` | `oc-epub` | unit | `test` | green |
| 5.3a | `zip::the_same_entries_in_any_order_produce_the_same_bytes` | `oc-epub` | unit | `test` | green |
| 5.3b | `zip::the_container_reads_back_as_the_entries_it_was_given` | `oc-epub` | unit | `test` | green |
| 5.3c | `zip::entry_names_that_collide_case_insensitively_are_refused` | `oc-epub` | unit | `test` | green |
| 5.4 | `epub::zip_is_byte_identical_across_runs` | `openconvert` | fixture (10) | `test` | green |
| 5.5 | `epub_is_byte_identical_across_os` | CI | gate | `epub-bytes` then `epub_is_byte_identical_across_os` | green |
| 5.6 | `epub::opf_has_all_required_metadata` | `openconvert` | snapshot (f09) | `test` | green |
| 5.7 | `opf::manifest_properties_are_computed_from_bytes` | `oc-epub` | unit | `test` | green |
| 5.7a | `opf::a_url_in_the_text_is_not_a_remote_resource` | `oc-epub` | unit | `test` | green |
| 5.8 | `epub::nav_and_ncx_agree` | `openconvert` | fixture (10) | `test` | green |
| 5.8a | `nav::a_nav_item_is_an_anchor_and_at_most_a_nested_list` | `oc-epub` | unit | `test` | green |
| 5.8b | `nav::the_page_list_carries_the_printed_folios` | `oc-epub` | unit | `test` | green |
| 5.8c | `nav::an_empty_nav_is_absent_rather_than_empty` | `oc-epub` | unit | `test` | green |
| 5.8d | `ncx::play_order_counts_across_the_whole_document_depth_first` | `oc-epub` | unit | `test` | green |
| 5.8e | `ncx::the_navigation_map_keeps_the_trees_own_order` | `oc-epub` | unit | `test` | green |
| 5.9 | `epub::page_list_targets_all_resolve` | `openconvert` | fixture (10) | `test` | green |
| 5.10 | `epub::noteref_footnote_bijection_in_output` | `openconvert` | fixture (f08) | `test` | green |
| 5.10a | `tier1::tier1_reports_the_bijection_and_notices_when_it_is_broken` | `openconvert` | fixture (f08) | `test` | green |
| 5.11 | `epub::split_happens_on_paragraph_boundary` | `openconvert` | fixture (f09, split at 900 B) | `test` | green |
| 5.12 | `css::css_has_no_font_family_or_absolute_size` | `oc-epub` | unit | `test` | green |
| 5.12a | `css::every_class_the_builder_can_emit_has_a_rule` | `oc-epub` | unit | `test` | green |
| 5.12b | `css::a_chapter_breaks_the_page_before_it` | `oc-epub` | unit | `test` | green |
| 5.13 | `epub::img_alt_is_never_empty` | `openconvert` | fixture (10) | `test` | green |
| 5.13a | `flow::an_image_without_alt_text_cannot_be_built` | `oc-epub` | unit | `test` | green |
| 5.14 | `epub::no_script_no_remote_resources` | `openconvert` | fixture (10) | `test` | green |
| 5.15 | `tier1::tier1_catches_rsc005_malformed_xml` | `openconvert` | crafted container | `test` | green |
| 5.16 | `tier1::tier1_catches_pkg007_mimetype` | `openconvert` | crafted container | `test` | green |
| 5.17 | `epubcheck::epubcheck_zero_errors_on_all_fixtures` | `openconvert` (feature `epubcheck`) | gate | `epubcheck` | green |
| 5.18 | `epubcheck::tier1_parity_does_not_regress` plus `xtask epubcheck-parity --check` | `openconvert`, `xtask` | gate | `tier1-parity` | green |
| 5.19 | `fuzz_roundtrip::fuzz_xhtml_emitter_roundtrip` | `oc-epub` | property (`proptest`) | `test` | green |
| 5.20 | `epub::golden_epub_bytes_f01` | `openconvert` | snapshot (sha256) | `test` | green |
| 5.21 | `tier1::tier1_passes_on_every_fixture` | `openconvert` | fixture (10) | `test` | green |
| 5.21a | `tier1::tier1_catches_an_image_that_did_not_arrive` | `openconvert` | crafted | `test` | green |
| 5.21b | `tier1::tier1_catches_empty_alt_text` | `openconvert` | crafted | `test` | green |
| 5.21c | `tier1::tier1_catches_a_script_and_the_property_that_was_not_declared` | `openconvert` | crafted | `test` | green |
| 5.21d | `tier1::tier1_catches_missing_required_metadata` | `openconvert` | crafted | `test` | green |
| 5.21e | `tier1::tier1_catches_a_resource_reference_that_resolves_to_nothing` | `openconvert` | crafted | `test` | green |
| 5.22 | `epub::every_manifest_item_and_internal_href_resolves` | `openconvert` | fixture (10) | `test` | green |
| 5.22a | `epub::the_stylesheet_is_in_the_container_and_every_document_points_at_it` | `openconvert` | fixture (f07) | `test` | green |
| 5.23 | `escape::the_markup_characters_are_escaped_on_both_sides_of_the_tag` | `oc-epub` | unit | `test` | green |
| 5.23a | `escape::whitespace_in_an_attribute_survives_as_a_character_reference` | `oc-epub` | unit | `test` | green |
| 5.23b | `escape::a_character_xml_cannot_carry_is_refused_rather_than_dropped` | `oc-epub` | unit | `test` | green |
| 5.24 | `xhtml::a_page_serialises_as_the_markup_it_was_built_from` | `oc-epub` | unit | `test` | green |
| 5.24a | `xhtml::an_illegal_character_propagates_out_of_every_nesting_it_was_written_into` | `oc-epub` | unit | `test` | green |
| 5.24b | `phrasing::a_paragraph_may_carry_more_than_one_note_reference` | `oc-epub` | unit | `test` | green |
| 5.24c | `phrasing::a_link_may_hold_emphasis_but_its_content_model_is_not_phrasing` | `oc-epub` | unit | `test` | green |
| 5.24d | `phrasing::a_span_carries_a_class_from_the_stylesheet_and_nothing_else` | `oc-epub` | unit | `test` | green |
| 5.24e | `flow::a_page_break_marker_holds_no_text` | `oc-epub` | unit | `test` | green |
| 5.24f | `flow::a_list_that_kept_its_printed_markers_says_so` | `oc-epub` | unit | `test` | green |
| 5.24g | `flow::a_fallback_table_carries_its_text_beside_its_image` | `oc-epub` | unit | `test` | green |
| 5.24h | `sectioning::a_heading_level_is_clamped_into_the_range_xhtml_has` | `oc-epub` | unit | `test` | green |
| 5.24i | `sectioning::a_continuation_section_borrows_the_first_fragments_heading` | `oc-epub` | unit | `test` | green |
| 5.25 | `images::an_image_with_alpha_becomes_png_and_an_opaque_one_becomes_jpeg` | `oc-epub` | unit | `test` | green |
| 5.25a | `images::an_image_is_scaled_down_to_the_bound_and_never_up` | `oc-epub` | unit | `test` | green |
| 5.25b | `images::encoding_the_same_image_twice_produces_the_same_bytes` | `oc-epub` | unit | `test` | green |
| 5.25c | `images::a_buffer_that_is_not_rgba_is_refused` | `oc-epub` | unit | `test` | green |
| 5.26 | `textcontent::the_head_and_every_attribute_are_outside_the_text` | `oc-epub` | unit | `test` | green |
| 5.26a | `textcontent::entities_are_resolved_back_to_their_characters` | `oc-epub` | unit | `test` | green |
| 5.26b | `textcontent::nested_markup_contributes_its_text_in_order` | `oc-epub` | unit | `test` | green |
| 5.27 | `document::the_document_closes_every_reference_it_makes` | `openconvert` | fixture (10) | `test` | green |
| 5.27a | `document::page_breaks_open_every_page_the_flow_reaches` | `openconvert` | fixture (10) | `test` | green |
| 5.27b | `document::page_breaks_carry_the_printed_labels_furniture_recovered` | `openconvert` | fixture (f01) | `test` | green |
| 5.27c | `document::a_section_that_opens_a_page_breaks_before_its_heading` | `openconvert` | fixture (f09) | `test` | green |
| 5.27d | `document::every_page_break_anchors_on_a_block_the_flow_still_carries` | `openconvert` | fixture (10) | `test` | green |
| 5.27e | `document::a_book_is_classified_by_what_the_pipeline_measured` | `openconvert` | fixture (f01, f02) | `test` | green |
| 5.27f | `document::document_records_a_conserving_check_in_the_ledger` | `openconvert` | fixture (f09) | `test` | green |
| 5.27g | `tier1::a_document_with_no_text_still_carries_its_pages` | `openconvert` | fixture (f03) | `test` | green |
| 5.28 | `document::a_dangling_reference_is_found_at_every_nesting_depth` | `oc-model` | unit | `test` | green |
| 5.28a | `document::auto_resolves_to_a_concrete_preset_and_an_explicit_choice_survives` | `oc-model` | unit | `test` | green |
| 5.28b | `lang::the_undetermined_tag_is_a_tag` | `oc-model` | unit | `test` | green |
| 5.29 | `structure::a_paragraphs_spans_are_its_text_split` | `openconvert` | fixture (6) | `test` | green |
| 5.29a | `structure::a_note_marker_becomes_a_span_that_carries_the_note_it_refers_to` | `openconvert` | fixture (f08) | `test` | green |
| 5.30 | `cli::convert_writes_a_valid_container_and_leaves_no_temporary` | `openconvert` | binary | `test` | green |
| 5.30a | `cli::validate_reports_tier_one_and_exits_on_the_verdict` | `openconvert` | binary | `test` | green |
| 5.30b | `cli::validate_tier_two_without_a_jar_says_so` | `openconvert` | binary | `test` | green |
| 5.31 | `epubcheck::a_report_with_errors_is_not_read_as_a_clean_one` | `oc-validate` | unit | `test` | green |
| 5.31a | `epubcheck::an_unreadable_report_is_an_error_and_not_an_empty_one` | `oc-validate` | unit | `test` | green |
| 5.32 | `fetch_epubcheck::the_lock_pins_a_digest_and_a_size` | `xtask` | unit | `test` | green |
| 5.32a | `epubcheck_parity::the_recorded_number_round_trips_through_the_report` | `xtask` | unit | `test` | green |
| 5.32b | `epubcheck_parity::a_report_without_a_number_is_not_a_number` | `xtask` | unit | `test` | green |

## Phase 6 — Structural validation, repair loop, report, CI DOM checks

| # | Test | Crate | Kind | CI job | Status |
|---|---|---|---|---|---|
| 6.1 | `structural::i7_holds_end_to_end_on_all_fixtures` | `openconvert` | fixture (10) | `test` | green |
| 6.1a | `structural::the_archive_and_the_emitter_agree_about_the_text` | `openconvert` | fixture (10) | `test` | green |
| 6.1b | `structural::retention_per_fixture_is_recorded` | `openconvert` | snapshot (10) | `test` | green |
| 6.2 | `structural::i7_detects_injected_text_loss` | `oc-validate` | unit (mutation) | `test` | green |
| 6.2a | `structural::i7_accepts_a_ledgered_removal` | `oc-validate` | unit | `test` | green |
| 6.3 | `structural::retention_below_threshold_warns` | `oc-validate` | unit | `test` | green |
| 6.3a | `structural::a_book_with_no_source_text_has_no_retention_to_report` | `oc-validate` | unit | `test` | green |
| 6.16 | `duplicates::prop_duplicate_paragraph_detected` | `oc-validate` | property (`proptest`) | `test` | green |
| 6.16a | `duplicates::one_duplicated_paragraph_in_a_short_document_crosses_the_gopher_bound` | `oc-validate` | unit | `test` | green |
| 6.16b | `duplicates::one_duplicated_block_in_a_long_book_is_invisible_to_gopher_and_not_to_the_block_bound` | `oc-validate` | unit | `test` | green |
| 6.16c | `structural::a_repeated_block_is_counted_and_named` | `oc-validate` | unit | `test` | green |
| 6.17 | `structural::a_heading_level_skip_is_found_and_located` | `oc-validate` | unit | `test` | green |
| 6.17a | `structural::a_document_that_starts_below_h1_has_skipped_a_level` | `oc-validate` | unit | `test` | green |
| 6.17b | `structural::headings_out_of_page_order_are_reported` | `oc-validate` | unit | `test` | green |
| 6.17c | `structural::the_h1_count_range_says_nothing_about_a_document_too_short_to_be_a_book` | `oc-validate` | unit | `test` | green |
| 6.18 | `blocks::the_head_is_not_a_block` | `oc-validate` | unit | `test` | green |
| 6.18a | `blocks::a_nested_block_is_counted_once_at_its_innermost_level` | `oc-validate` | unit | `test` | green |
| 6.18b | `blocks::phrasing_and_entities_are_part_of_the_block` | `oc-validate` | unit | `test` | green |
| 6.18c | `blocks::whitespace_is_collapsed_so_wrapping_does_not_make_a_new_block` | `oc-validate` | unit | `test` | green |
| 6.18d | `blocks::a_heading_carries_its_level` | `oc-validate` | unit | `test` | green |
| 6.18e | `blocks::an_empty_block_is_not_reported` | `oc-validate` | unit | `test` | green |
| 6.19 | `structural::the_structural_report_holds_on_every_fixture` | `openconvert` | fixture (10) | `test` | green |
| 6.19a | `structural::the_structural_report_on_every_fixture_is_recorded` | `openconvert` | snapshot (10) | `test` | green |
| 6.4 | `repair_loop::repair_requires_strict_decrease` | `oc-validate` | unit | `test` | green |
| 6.5 | `repair_loop::repair_rejects_new_message_id` | `oc-validate` | unit | `test` | green |
| 6.6 | `repair_loop::repair_detects_oscillation_by_hash` | `oc-validate` | unit | `test` | green |
| 6.7 | `table::repair_at_most_one_per_file_node` | `oc-validate` | unit | `test` | green |
| 6.7a | `table::the_plan_is_in_severity_then_id_then_location_order` | `oc-validate` | unit | `test` | green |
| 6.7b | `table::the_table_is_sorted_and_names_each_id_once` | `oc-validate` | unit | `test` | green |
| 6.7c | `table::every_fix_has_its_own_id` | `oc-validate` | unit | `test` | green |
| 6.9 | `repair_loop::unmapped_epubcheck_id_is_logged_not_guessed` | `oc-validate` | unit | `test` | green |
| 6.9a | `table::an_unmapped_id_produces_no_action_and_is_collected` | `oc-validate` | unit | `test` | green |
| 6.9b | `table::a_warn_only_id_is_not_an_unmapped_one` | `oc-validate` | unit | `test` | green |
| 6.9c | `repair_loop::a_covered_id_with_no_safe_fix_says_so_rather_than_saying_nothing` | `oc-validate` | unit | `test` | green |
| 6.20 | `measure::the_measure_is_lexicographic_in_severity_order` | `oc-validate` | unit | `test` | green |
| 6.20a | `measure::the_measure_counts_a_finding_list_by_severity` | `oc-validate` | unit | `test` | green |
| 6.20b | `measure::a_new_message_id_is_the_difference_of_two_id_sets` | `oc-validate` | unit | `test` | green |
| 6.21 | `repair_loop::a_clean_container_is_not_repaired` | `oc-validate` | unit | `test` | green |
| 6.21a | `repair_loop::an_accepted_repair_is_counted_under_its_own_id` | `oc-validate` | unit | `test` | green |
| 6.21b | `repair_loop::the_cap_stops_a_loop_that_would_otherwise_keep_going` | `oc-validate` | unit | `test` | green |
| 6.10 | `repair::repair_fire_rate_is_zero_on_corpus` | `openconvert` | gate (10) | `test` | green |
| 6.10a | `repair::a_clean_conversion_is_the_same_bytes_the_emitter_produced` | `openconvert` | fixture (10) | `test` | green |
| 6.22 | `repair::the_ledger_records_validate_and_repair_as_conserving_stages` | `openconvert` | fixture (10) | `test` | green |
| 6.22a | `repair::the_documents_warnings_carry_what_validation_found` | `openconvert` | fixture (f01) | `test` | green |
| 6.23 | `host::the_modified_timestamp_is_not_part_of_the_content_hash` | `oc-validate` | unit | `test` | green |
| 6.23a | `host::everything_but_the_timestamp_is_still_hashed` | `oc-validate` | unit | `test` | green |
| 6.8 | `report::repair_cap_writes_epub_and_marks_invalid` | `openconvert` | integration | `test` | green |
| 6.12 | `report::report_schema_is_valid_and_snapshotted` | `openconvert` | snapshot (f07) | `test` | green |
| 6.12a | `report::the_report_carries_every_part_the_plan_names` | `openconvert` | fixture (f01) | `test` | green |
| 6.12b | `report::the_report_carries_every_thresholds_provenance` | `openconvert` | unit | `test` | green |
| 6.12c | `cli::convert_writes_a_report_beside_the_epub_and_where_asked` | `openconvert` | binary | `test` | green |
| 6.12d | `cli::convert_emits_warning_events_with_their_arguments` | `openconvert` | binary | `test` | green |
| 6.11 | `warnings::every_warning_code_has_all_locale_templates` plus `xtask ci-lint`'s registry rule | `oc-core`, `xtask` | gate | `test`, `lint` | green |
| 6.11a | `warnings::every_template_slot_is_an_argument_the_code_declares` | `oc-core` | unit | `test` | green |
| 6.11b | `warnings::the_english_template_uses_every_argument_its_code_carries` | `oc-core` | unit | `test` | green |
| 6.11c | `warnings::the_registry_is_sorted_and_names_each_code_once` | `oc-core` | unit | `test` | green |
| 6.11d | `warnings::a_slot_with_no_argument_stays_visible` | `oc-core` | unit | `test` | green |
| 6.11e | `warnings::every_locale_renders_a_warning_rather_than_its_code` | `oc-core` | unit | `test` | green |
| 6.11f | `warnings::an_unknown_locale_tag_falls_back_to_english` | `oc-core` | unit | `test` | green |
| 6.11g | `cli::convert_prints_localised_warnings_and_never_into_the_event_stream` | `openconvert` | binary | `test` | green |
| 6.13 | `overflow.spec.ts::dom_no_horizontal_overflow` | `tests/dom` | gate (Playwright, 3 viewports) | `dom-checks`, nightly `webkit-dom` | green |
| 6.13a | `overflow.spec.ts::dom_images_render_at_a_non_zero_size` | `tests/dom` | gate (Playwright) | `dom-checks`, nightly `webkit-dom` | green |
| 6.14 | `order.spec.ts::dom_heading_order_matches_nav` | `tests/dom` | gate (Playwright) | `dom-checks`, nightly `webkit-dom` | green |
| 6.14a | `order.spec.ts::dom_heading_levels_never_skip` | `tests/dom` | gate (Playwright) | `dom-checks`, nightly `webkit-dom` | green |
| 6.15 | `notes.spec.ts::dom_every_noteref_resolves` | `tests/dom` | gate (Playwright) | `dom-checks`, nightly `webkit-dom` | green |
| 6.15a | `notes.spec.ts::dom_every_footnote_is_reachable` | `tests/dom` | gate (Playwright) | `dom-checks`, nightly `webkit-dom` | green |
| 6.24 | `dom_fixtures::a_note_reference_is_resolved_against_the_document_it_is_in` | `xtask` | unit | `test` | green |
| 6.25 | `ace::ace_zero_serious_violations_on_all_fixtures` | `openconvert` (feature `ace`) | gate | nightly `ace-a11y` | green |
| 6.25a | `ace::every_fixture_carries_the_accessibility_metadata_ace_requires` | `openconvert` | fixture (10) | `test` | green |
| 6.25b | `ace::a_critical_violation_counts_as_a_serious_one` | `oc-validate` | unit | `test` | green |
| 6.25c | `ace::missing_accessibility_metadata_fails_the_gate_on_its_own` | `oc-validate` | unit | `test` | green |
| 6.25d | `ace::an_unreadable_report_is_an_error_and_not_an_empty_one` | `oc-validate` | unit | `test` | green |

## Phase 13 — OCR

Tests that need a real Tesseract are behind the `openconvert` feature `tesseract` and run in the
`ocr` CI job, which installs `tesseract-ocr` with `deu` and `tur`. Tests that drive a fake engine
by process (a shell script standing in for `tesseract`) are `#[cfg(unix)]`: Windows discovery and
invocation are unverified on this machine and have no CI runner until the maintainer turns Actions
back on.

| # | Test | Crate | Kind | Job | Status |
|---|---|---|---|---|---|
| 13.5 | `ocr_tsv::tsv_header_mismatch_is_an_error` | `oc-core` | unit | `test` | green |
| 13.6 | `ocr_tsv::tsv_parses_words_and_confidences` | `oc-core` | unit (golden TSV, a real Tesseract 5.3.4 capture) | `test` | green |
| 13.7 | `ocr_tsv::tsv_drops_non_word_and_negative_conf_rows` | `oc-core` | unit | `test` | green |
| 13.8 | `ocr_tsv::pixel_boxes_map_into_normalized_page_space` | `oc-core` | property (2 000 cases) | `test` | green |
| 13.9 | `ocr_lang::language_selection_maps_and_falls_back` | `oc-core` | unit (table) | `test` | green |
| 13.1 | `ocr_discovery::discovery_finds_tesseract_on_path` | `oc-testkit` | unit (temp `PATH`, fake engine, Unix) | `test` | green |
| 13.2 | `ocr_discovery::discovery_falls_back_to_well_known_dirs` | `oc-testkit` | unit (fake engine, Unix) | `test` | green |
| 13.3 | `ocr_discovery::discovery_rejects_version_below_5` | `oc-testkit` | unit (fake engine, Unix) — also A13.7's discovery half | `test` | green |
| 13.4 | `ocr_discovery::discovery_rejects_writable_binary` | `oc-testkit` | unit (Unix) | `test` | green |
| 13.3a | `discover::tests::every_platforms_version_banner_parses` | `oc-core` | unit — VD-g's banner forms | `test` | green |
| 13.3b | `discover::tests::the_well_known_lists_are_the_documented_ones` | `oc-core` | unit — VD-g's paths | `test` | green |
| 13.10 | `ocr_invoke::psm_follows_page_class` | `oc-testkit` | unit (argv spy, fake engine, Unix) | `test` | green |
| 13.18a | `ocr_invoke::a_hung_call_is_killed_at_its_deadline` | `oc-testkit` | integration (a fake that sleeps, Unix) — the invocation half of 13.18 | `test` | green |
| 13.19 | `ocr_invoke::ocr_child_dies_with_the_engine` | `oc-testkit` | integration (SIGTERM and panic; Linux here, macOS/Windows unverified) | `test` | green |
| 13.23 | `render::render_region_is_the_page_at_the_asked_resolution` | `oc-pdf` | fixture (f03) | `test` | green |
| 13.23a | `render::a_rotated_page_renders_in_normalised_space` | `oc-pdf` | fixture (h02) | `test` | green |
| 13.23b | `render::pixel_windows_are_clipped_to_the_page` | `oc-pdf` | unit | `test` | green |
| 13.13 | `ocr_merge::i6_region_scope_rejects_overlapping_text` | `oc-core` | unit | `test` | green |
| 13.15 | `ocr_merge::ocr_regions_excluded_from_source_retention` | `oc-core` | unit | `test` | green |
| 13.15a | `structural::retention_excludes_ocr_added_characters` | `oc-validate` | unit — 13.15 end to end, through I-7 | `test` | green |
| 13.24 | `ocr_merge::ocr_words_become_one_run_per_line_with_provenance_ocr` | `oc-core` | unit — detail 7 | `test` | green |
| 13.24a | `ocr_merge::region_confidence_is_the_mean_and_the_sub_floor_count` | `oc-core` | unit — detail 9 | `test` | green |
| 13.24b | `ocr_merge::a_full_page_region_is_cut_into_bands_around_existing_text` | `oc-core` | unit — I-6 on a scan with stray PDF glyphs | `test` | green |
| 13.11 | `ocr_e2e::ocr_runs_carry_provenance_ocr` | `openconvert` | fixture (f03, f11; in-process engine) | `test` | green |
| 13.12 | `ocr_e2e::ledger_ocr_entries_are_added_only` | `openconvert` | fixture (f03; in-process engine) | `test` | green |
| 13.14 | `ocr_e2e::mixed_page_ocrs_only_uncovered_regions` | `openconvert` | fixture (f11 mixed; in-process engine) | `test` | green |
| 13.16 | `ocr_e2e::re_ocr_replaces_sandwich_layer_conservingly` | `openconvert` | fixture (h05 sandwich; in-process engine) | `test` | green |
| 13.17 | `ocr_e2e::missing_engine_emits_install_hint_and_page_images` | `openconvert` | integration (binary) — A13.2 | `test` | green |
| 13.18 | `ocr_e2e::hung_tesseract_is_killed_at_deadline` | `openconvert` | integration (a fake that sleeps, Unix) — A13.5 | `test` | green |
| 13.22 | `ocr_e2e::ocr_never_calls_the_llm` | `openconvert` | unit — `--ai --ai-all-tasks` over a scanned book: no model call before OCR finishes; source and dependency scan | `test` | green |
| 13.25 | `ocr_e2e::a_doubtful_or_failed_region_keeps_its_picture` | `openconvert` | fixture — details 9 and 11 | `test` | green |
| 13.25a | `ocr_e2e::hello_reports_the_discovered_engine` | `openconvert` | binary (fake engine, Unix) — detail 1 | `test` | green |
| 13.20 | `ocr_tesseract::scanned_fixture_assertions_pass` | `openconvert` (feature `tesseract`) | fixture (4 synthetic scans, real Tesseract) | `ocr` | green |
| 13.21 | `ocr_tesseract::cer_per_stratum_within_budget` | `openconvert` (feature `tesseract`) | CI-gate — synthetic-scan CER, real stratum reported, gap printed; the nightly `full-corpus` report's `ocr_cer` section | `ocr`, nightly `full-corpus` | green (synthetic); real stratum unverified here |
| 13.21a | `ocr_tesseract::cer_is_levenshtein_over_the_truths_length` | `openconvert` (feature `tesseract`) | unit | `ocr` | green |
| 13.21b | `test_metrics::test_ocr_cer_is_reported_per_stratum_with_the_real_minus_synthetic_gap` | `eval` | unit | `python` | green |
| 13.21c | `test_metrics::test_cer_is_edit_distance_over_the_truths_length` | `eval` | unit | `python` | green |
| 13.21d | `test_run::test_a_committed_scanned_fixture_is_its_own_source_and_has_a_ground_truth` | `eval` | unit | `python` | green |
| 13.21e | `test_run::test_the_reading_text_of_an_epub_is_its_spine_a_block_per_line` | `eval` | unit | `python` | green |
| 13.26 | `ocr_tesseract::image_only_pdf_converts_with_system_tesseract` | `openconvert` (feature `tesseract`) | fixture (f03, real Tesseract) — A13.1 | `ocr` | green |
| 13.26a | `ocr_scanned::scanned_ground_truth_is_the_born_digital_text` | `openconvert` | fixture — the committed `.gt.txt` is this pipeline's text of the source | `test` | green |
| 13.26b | `ocr_scanned::scanned_fixtures_are_image_only_without_ocr` | `openconvert` | fixture | `test` | green |
| 13.26c | `ocr_merge::ocr_line_sizes_are_word_heights_snapped_to_one_body_size` | `oc-core` | unit | `test` | green |
| 13.26d | `tesseract::tests::every_call_caps_openmp_at_one_thread` | `oc-core` | unit | `test` | green |
| 13.26e | `ocr_tesseract::mixed_page_with_system_tesseract_reads_only_the_plate` | `openconvert` (feature `tesseract`) | fixture (f11, real Tesseract) — A13.3 | `ocr` | green |
| 13.26f | `ocr_e2e::an_old_tesseract_converts_as_if_none_existed` | `openconvert` | binary (a fake 4.1.1, Unix) — A13.7 | `test` | green |

## Phase 14 — Security hardening

All rows run in `test` on Linux here. The Landlock rows decide "should enforce" from the kernel
release (≥ 5.13; this machine is 6.18, ABI 7) and assert the recorded skip below it. Windows and
macOS are type-checked only: the job object, the memory cap (none on Windows yet, provisional) and
the orphan tests are unverified on those systems. The fuzz properties run in the ordinary suite over
`fuzz/corpus/`; the 15-minute `cargo fuzz` campaigns are the nightly `fuzz` job. `isartor` is a cargo
feature of `openconvert`, turned on only by the nightly `isartor` job.

| # | Test | Crate | Kind | Job | Status |
|---|---|---|---|---|---|
| 14.1 | `limits::image_pixel_cap_checked_before_decode` | `oc-pdf` | unit (spy decoder never entered) | `test` | green |
| 14.2 | `limits::bounded_inflate_stops_at_ceiling` | `oc-pdf` | unit | `test` | green |
| 14.3 | `limits::declared_length_is_not_trusted` | `oc-pdf` | unit | `test` | green |
| 14.3a | `filters::every_filter_decodes_its_known_vector` | `oc-pdf` | unit — detail 2's filters | `test` | green |
| 14.3b | `filters::a_filter_chain_shares_one_budget` | `oc-pdf` | unit | `test` | green |
| 14.3c | `filters::our_filter_chain_agrees_with_lopdf_on_every_fixture` | `oc-pdf` | fixture (differential) | `test` | green |
| 14.4 | `prescan::xref_chain_depth_is_capped` | `oc-pdf` | unit | `test` | green |
| 14.5 | `prescan::xref_cycle_terminates_via_visited_set` | `oc-pdf` | unit | `test` | green |
| 14.6 | `prescan::page_cap_checked_before_first_page_load` | `oc-pdf` | unit | `test` | green |
| 14.6a | `prescan::every_fixture_walks_and_agrees_with_pdfium_on_its_page_count` | `oc-pdf` | fixture (differential) | `test` | green |
| 14.6b | `hardening::a_declared_5000_page_pdf_exits_1_with_a_report` | `openconvert` | binary — 14.6's exit 1 | `test` | green |
| 14.7 | `hardening::memory_cap_is_applied_before_the_pdf_opens` | `openconvert` | binary (Unix; Windows has no memory cap yet, provisional) | `test` | green (Linux) |
| 14.7a | `sandbox::rlimit::the_memory_cap_limits_the_address_space` | `oc-core` | unit | `test` | green |
| 14.7b | `sandbox::byte_counts_parse_with_binary_units_only` | `openconvert` | unit — `--max-memory` | `test` | green |
| 14.8 | `deadline::deadline_and_cancel_share_one_abort_path` | `oc-core` | unit (clock-injected) | `test` | green |
| 14.8a | `deadline::a_dropped_deadline_never_fires` | `oc-core` | unit (clock-injected) | `test` | green |
| 14.8b | `hardening::a_stage_deadline_aborts_through_the_cancel_path` | `openconvert` | binary | `test` | green |
| 14.9 | `hardening::degenerate_40m_glyph_pdf_fails_cleanly` | `openconvert` | binary — A14.3, SECURITY §4 | `test` | green |
| 14.9a | `glyph_budget::text_operators_count_their_string_bytes` | `oc-pdf` | unit | `test` | green |
| 14.9b | `glyph_budget::every_committed_fixture_is_far_under_the_page_cap` | `oc-pdf` | fixture | `test` | green |
| 14.10 | `sandbox::landlock_applies_on_supported_kernel` | `oc-testkit` | integration (`oc-sandbox-probe`, Linux ≥ 5.13) — A14.4 | `test` | green |
| 14.10a | `hardening::a_normal_conversion_succeeds_inside_landlock` | `openconvert` | binary — the over-restriction half of 14.10 | `test` | green |
| 14.10b | `sandbox::a_scope_reads_the_input_and_writes_beside_the_output` | `openconvert` | unit | `test` | green |
| 14.11 | `hardening::landlock_skips_gracefully_when_unsupported` | `openconvert` | binary (`OC_LANDLOCK=off`) — A14.5 | `test` | green |
| 14.12 | `sandbox::landlock_blocks_tcp_connect_on_abi4` | `oc-testkit` | integration (`oc-sandbox-probe`) | `test` | green |
| 14.13 | `fuzz_props::fuzz_ir_deserialize_no_panic` | `oc-testkit` | property over `fuzz/corpus/`; target `ir_deserialize` | `test`, nightly `fuzz` | green; nightly unverified here |
| 14.14 | `fuzz_props::fuzz_job_spec_accepts_only_absolute_paths` | `oc-testkit` | property over `fuzz/corpus/`; target `job_spec` | `test`, nightly `fuzz` | green; nightly unverified here |
| 14.14a | `jobspec::tests::every_path_in_an_accepted_spec_is_absolute_and_never_climbs` | `oc-core` | unit — the hole 14.14 found | `test` | green |
| 14.15 | `fuzz_props::fuzz_xhtml_opf_roundtrip_fires_no_repair` | `oc-testkit` | property over `fuzz/corpus/`; target `xhtml_opf_roundtrip` | `test`, nightly `fuzz` | green; nightly unverified here |
| 14.15a | `test_ci_workflows::test_the_fuzz_job_runs_every_target_for_fifteen_minutes` | `eval` | CI-gate (workflow text) | `python` | green |
| 14.16 | `crash_corpus::isartor_corpus_terminates_cleanly` | `openconvert` (feature `isartor`) | integration — A14.6 | nightly `isartor` | **unverified here**: `xtask/isartor.lock` is not pinned |
| 14.16a | `test_ci_workflows::test_the_isartor_job_fetches_the_pinned_suite_and_turns_the_test_on` | `eval` | CI-gate (workflow text) | `python` | green |
| 14.16b | `fetch_isartor::an_unpinned_lock_refuses_before_fetching` | `xtask` | unit | `test` | green |
| 14.16c | `fetch_isartor::a_lock_path_cannot_escape_the_target` | `xtask` | unit | `test` | green |
| 14.17 | `crash_corpus::mutated_crash_corpus_terminates_cleanly` | `openconvert` | integration (29 files) — A14.6 | `test` | green |
| 14.18 | `crash_corpus::crash_fixtures_are_manifest_keyed` | `openconvert` | CI-gate | `test` | green |
| 14.18a | `test_crash_corpus::test_the_committed_crash_corpus_regenerates_byte_identically` | `eval` | CI-gate | `python` | green |
| 14.18b | `test_crash_corpus::test_the_manifest_keys_files_by_name_and_sha256_in_order` | `eval` | unit | `python` | green |
| 14.18c | `test_crash_corpus::test_every_generator_contributes_and_names_are_unique` | `eval` | unit | `python` | green |
| 14.19 | `hardening::no_partial_output_after_any_cap_violation` | `openconvert` | property (1 000 injected violations) | `test` | green |
| 14.19a | `hardening::a_gigapixel_claim_is_refused_from_the_dictionary` | `openconvert` | binary — A14.1 | `test` | green |
| 14.19b | `hardening::a_decompression_bomb_fails_closed_through_convert` | `openconvert` | binary — A14.2 | `test` | green |
| 14.20 | `ai_pipeline::unshare_n_covers_the_ai_cassette_path` | `openconvert` | CI-gate (`OC_EXPECT_NO_NETWORK=1` proves the namespace empty) — A14.7 | `no-network` | green here under `unshare -n`; the job unverified here |
| 14.20a | `test_ci_workflows::test_the_no_network_job_covers_the_ai_path` | `eval` | CI-gate (workflow text) | `python` | green |
| 14.21 | `audit::net_audit_log_records_downloads_and_nothing_else` | `oc-net` | integration (loopback server) | `test` | green |
| 14.21a | `hardening::a_conversion_appends_nothing_to_the_network_audit_log` | `openconvert` | binary | `test` | green |
| 14.21b | `audit::the_log_rotates_at_its_size` | `oc-net` | unit | `test` | green |
| 14.21c | `audit::hosts_are_read_without_port_or_credentials` | `oc-net` | unit | `test` | green |
| 14.21d | `netlog::tests::the_network_log_lists_the_audit_log_newest_first` | `openconvert-desktop` | unit — Settings › Network log | `desktop` | green |
| 14.22 | `ci::unsafe_is_confined_to_declared_modules` | `xtask` | CI-gate | `test` | green |
| 14.23 | `ci::isolate_parser_spike_overhead_is_recorded` | `xtask` | bench (spike): harness runs, output identical, verdict in `docs/DECISIONS_LOG.md` | `test` | green (NO-GO recorded) |
| 14.24 | `ocr_invoke::ocr_child_does_not_outlive_a_sigkilled_engine` | `oc-testkit` | integration (Linux; PDEATHSIG trampoline) | `test` | green |
| 14.24a | `sidecar::owned_server_does_not_outlive_a_sigkilled_engine` | `oc-testkit` | integration (Linux) | `test` | green |
| 14.24b | `sidecar::orphan::a_trampoline_command_names_its_target` | `oc-core` | unit | `test` | green |
| 14.24c | `sidecar::orphan::an_unguarded_command_is_the_program_itself` | `oc-core` | unit | `test` | green |

## Phase 15 — Packaging & release

Most of Phase 15's rows are gates over real release artefacts. They are tests behind the `xtask`
feature `release-artifacts`, which read what the release job produced from `OC_BUNDLE_DIR`,
`OC_SBOM`, `OC_REPRO_DIR`, `OC_RELEASE_BODY` and `OC_RELEASE_ASSETS`, fail when those are unset, and
run as steps of `.github/workflows/release.yml` named `row 15.N <test>`. Rows 15.1–15.4 are shell
steps of the macOS release leg. Each gate's logic also has a default-suite test on synthetic input.
`Status` "green here" means it ran against real artefacts on this Linux machine; "release job" means
it has not run anywhere yet.

| # | Test | Crate | Kind | Job | Status |
|---|---|---|---|---|---|
| 15.1 | `every_nested_macho_is_signed_with_one_team_id` | — | CI step (macOS) | `release`/`build` | release job |
| 15.2 | `codesign_verify_deep_strict_passes` | — | CI step (macOS) | `release`/`build` | release job |
| 15.3 | `spctl_assess_accepts_the_bundle` | — | CI step (macOS) | `release`/`build` | release job |
| 15.4 | `notarization_ticket_is_stapled` | — | CI step (macOS) | `release`/`build` | release job |
| 15.5 | `entitlements_do_not_disable_library_validation` | `xtask` | unit (plist parse) | `test` | green |
| 15.5a | `sign_nested_signs_every_macho_inside_out` | `xtask` | the script on fake Mach-O files, recording `codesign` (Unix) | `test` | green |
| 15.6 | `windows_installers_are_produced_and_hashed` | `xtask` (`release-artifacts`, Windows) | CI gate | `release`/`build` | release job |
| 15.6a | `release_manifest_requires_every_declared_installer` | `xtask` | unit | `test` | green |
| 15.7 | `appimage_launches_and_converts_headless` | `xtask` (`release-artifacts`, Linux) | integration (xvfb-run, the AppImage) | `release`/`build` | green here |
| 15.7a | `appimage_carries_the_bundle_layout` | `xtask` (`release-artifacts`, Linux) | integration | `release`/`build` | green here |
| 15.7b | `smoke::tests::only_the_smoke_flag_asks_for_a_smoke_conversion`, `a_smoke_job_ends_with_the_engines_exit_code` | `openconvert-desktop` | unit | `desktop` | green |
| 15.8 | `flatpak_manifest_has_no_network_finish_arg` | `xtask` | unit (YAML parse) | `test` | green |
| 15.9 | `updater_manifest_signature_verifies` | `oc-net` | integration (keypair made in the test) | `test` | green |
| 15.10 | `updater_rejects_tampered_payload` | `oc-net` | integration | `test` | green |
| 15.10a | `an_update_off_the_release_hosts_or_over_the_budget_is_refused` | `oc-net` | integration | `test` | green |
| 15.10b | `update::tests::{versions_order_as_semver_does, the_placeholder_key_verifies_nothing, this_build_looks_itself_up_most_specific_first}` | `oc-net` | unit | `test` | green |
| 15.10c | `updater::tests::the_updater_reads_the_key_and_endpoint_tauri_conf_carries` | `openconvert-desktop` | unit | `desktop` | green |
| 15.10d | `the_release_latest_json_is_what_the_updater_verifies` | `xtask` | unit (release tooling → the updater's parser and verifier) | `test` | green |
| 15.11 | `sbom_is_valid_cyclonedx_1_6` | `xtask` (`release-artifacts`) | CI gate (`OC_SBOM`) | `release`/`sbom` | green here |
| 15.11a | `the_merged_sbom_validates_offline_and_names_no_build_path`, `the_sbom_schema_check_rejects_an_invalid_document` | `xtask` | unit | `test` | green |
| 15.12 | `sbom_lists_every_vendored_native` | `xtask` (`release-artifacts`) | CI gate (`OC_SBOM`) | `release`/`sbom` | green here |
| 15.12a | `a_pinned_pack_joins_the_sbom_and_a_placeholder_does_not` | `xtask` | unit | `test` | green |
| 15.13 | `reproducible_no_ai_output_across_os` | `xtask` (`release-artifacts`) | CI gate (three OS tables) | `release`/`repro-compare` | release job (the Linux half ran here) |
| 15.13a | `repro_check_names_the_first_differing_zip_entry` | `xtask` | unit | `test` | green |
| 15.14 | `release_job_needs_no_python` | `xtask` | unit (YAML parse) + the containers' own assertion | `test`, `release` | green (YAML); release job |
| 15.14a | `every_release_gate_row_is_a_named_release_step` | `xtask` | unit (YAML parse) | `test` | green |
| 15.15 | `installer_size_within_budget` | `xtask` (`release-artifacts`) | CI gate | `release`/`build` | **red here, correctly: AppImage 112 953 848 B > 45 000 000** (rebuilt after Phase 14's merge) |
| 15.15a | `the_installer_budget_counts_installers_only` | `xtask` | unit | `test` | green |
| 15.16 | `ir_version_bump_is_enforced` | `xtask` | rehearsal on a copy of the tree | `test` | green |
| 15.17 | `protocol_bump_is_enforced` | `xtask` | rehearsal on a copy of the tree | `test` | green |
| 15.17a | `prompt_and_job_spec_changes_follow_their_rules`, `the_committed_baseline_describes_this_tree` | `xtask` | rehearsal / unit | `test` | green |
| 15.18 | `no_todo_placeholders_on_a_release_tag` | `xtask` | unit (scratch trees) + `ci-lint --release-branch` in `release` | `test`, `release` | green (logic); **the tree fails the gate today, correctly** |
| 15.19 | `fresh_install_converts_a_book` | — | manual + scripted VM (`--smoke-convert`) | — | not run (no VMs) |
| 15.20 | `release_artifacts_all_have_published_hashes` | `xtask` (`release-artifacts`) | CI gate | `release`/`publish` | release job |
| 15.20a | `a_release_body_missing_one_hash_is_refused` | `xtask` | unit | `test` | green |
| 15.21 | `no_sidecar_shares_a_name_with_a_workspace_binary` | `xtask` | unit — the stale-engine carry-over | `test` | green |
| 15.21a | `engine::tests::the_sidecar_the_app_runs_is_the_one_tauri_bundles` | `openconvert-desktop` | unit | `desktop` | green |
| 15.22 | `the_bundle_layout_is_the_same_on_every_os` | `xtask` | unit (merged Tauri configs) | `test` | green |
| 15.22a | `stage_sidecars::tests::{the_llama_runtime_is_the_server_and_what_it_loads_on_every_os, the_rpc_backend_and_other_tools_are_never_bundled}` | `xtask` | unit | `test` | green |
| 15.19a | `fresh_install_script_checks_the_hash_and_the_epub` | `xtask` | the Linux script against a stand-in AppImage (Unix) | `test` | green |
| 15.23 | `about & updates` × 3 (`settings.svelte.test.ts`) | `apps/desktop/ui` | component (Vitest) | `ui` | green |
| 15.23a | `config::tests::the_ui_is_told_whether_this_build_has_an_updater` | `openconvert-desktop` | unit (with and without `updater`) | `desktop` | green |
| 15.24 | `the_rust_notices_are_up_to_date`, `every_shipped_crate_and_only_those_has_a_licence_text` | `xtask` | generated file vs `cargo metadata` | `test` | green |
| 15.25 | `release_notes_come_from_the_changelog_and_refuse_a_placeholder` | `xtask` | unit | `test` | green |
| 15.26 | `every_update_connection_is_in_the_network_audit_log` | `oc-net` | integration (loopback server, the real `HttpFetch`) | `test` | green |
| 15.26a | "the network log says when nothing has connected, and lists what the audit log holds" (now with the `update` purpose and "when you check for updates") in `settings.svelte.test.ts` | `apps/desktop/ui` | component (Vitest) | `ui` | green |
| 15.27 | first run › "the first screen states no telemetry and no network while converting, beside the costs" (`models.svelte.test.ts`) — Appendix D, Product | `apps/desktop/ui` | component (Vitest) | `ui` | green |
