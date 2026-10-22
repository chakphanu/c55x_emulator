# Python -> Rust test migration (branch rust-tests)

Owner decision: every test moves to inline Rust. The end state is `cargo test` (plus
c55x-run) only; after that c55x_py and every Python test are deleted. This file is
phase A: the mapping, the fixture scheme, the 11 tests that need no execution (done in
61f1f24), and the execution harness design (implemented once NativeHost executes).

## 1. Counts (253 Python tests at ec5709c)

| Class | Count | Destination |
|---|---|---|
| keep | 146 | Rust integration tests in rust/c55x_core/tests/*.rs, same inputs and expected values |
| convert | 11 | done: rust/c55x_core/src/frontend/tests.rs (no execution) |
| rewrite | 10 | Rust tests with a changed setup (audio clock as data, a core-native port, Rust caps) |
| delete | 50 | deleted with the Zen Touch board (owner decision), nothing to port |
| infra | 36 | 35 launcher tests stay Python launcher tests; native_policy becomes plain cargo test |

Naming: the Rust test is the Python method name without `test_`. A file with several
Python classes gets one `mod` per class (snake case, `Tests` dropped).

## 2. Mapping

Format: Python file::Class.method | class | Rust target (path::[module::]name) or reason.

| Python test | Class | Rust target |
|---|---|---|
| test_c55x_emu::ParseTests.test_numbers | convert | rust/c55x_core/src/frontend/tests.rs::parse::numbers |
| test_c55x_emu::ParseTests.test_memory_operands | convert | rust/c55x_core/src/frontend/tests.rs::parse::memory_operands |
| test_c55x_emu::ParseTests.test_multiply_is_not_a_memory_operand | convert | rust/c55x_core/src/frontend/tests.rs::parse::multiply_is_not_a_memory_operand |
| test_c55x_emu::SemanticTests.test_sign_and_zero_extension | keep | rust/c55x_core/tests/semantics.rs::sign_and_zero_extension |
| test_c55x_emu::SemanticTests.test_long_constant_and_logical_shift | keep | rust/c55x_core/tests/semantics.rs::long_constant_and_logical_shift |
| test_c55x_emu::SemanticTests.test_post_increment_of_long_access_steps_two_words | keep | rust/c55x_core/tests/semantics.rs::post_increment_of_long_access_steps_two_words |
| test_c55x_emu::SemanticTests.test_ar_write_keeps_the_high_part | keep | rust/c55x_core/tests/semantics.rs::ar_write_keeps_the_high_part |
| test_c55x_emu::SemanticTests.test_dual_push_pop_order | keep | rust/c55x_core/tests/semantics.rs::dual_push_pop_order |
| test_c55x_emu::SemanticTests.test_call_and_return_restore_the_stacks | keep | rust/c55x_core/tests/semantics.rs::call_and_return_restore_the_stacks |
| test_c55x_emu::SemanticTests.test_repeat_and_localrepeat_counts | keep | rust/c55x_core/tests/semantics.rs::repeat_and_localrepeat_counts |
| test_c55x_emu::SemanticTests.test_goto_out_of_a_loop_ends_it | keep | rust/c55x_core/tests/semantics.rs::goto_out_of_a_loop_ends_it |
| test_c55x_emu::SemanticTests.test_conditional_execute | keep | rust/c55x_core/tests/semantics.rs::conditional_execute |
| test_c55x_emu::SemanticTests.test_parallel_parts_read_before_writing | keep | rust/c55x_core/tests/semantics.rs::parallel_parts_read_before_writing |
| test_c55x_emu::SemanticTests.test_parallel_mar_runs_in_the_address_phase | keep | rust/c55x_core/tests/semantics.rs::parallel_mar_runs_in_the_address_phase |
| test_c55x_emu::SemanticTests.test_parallel_mar_leaves_the_partner_address_alone | keep | rust/c55x_core/tests/semantics.rs::parallel_mar_leaves_the_partner_address_alone |
| test_c55x_emu::SemanticTests.test_standalone_conditional_execute_skips_the_next_instruction | keep | rust/c55x_core/tests/semantics.rs::standalone_conditional_execute_skips_the_next_instruction |
| test_c55x_emu::SemanticTests.test_multiply_uses_the_high_part_of_an_accumulator | keep | rust/c55x_core/tests/semantics.rs::multiply_uses_the_high_part_of_an_accumulator |
| test_c55x_emu::SemanticTests.test_long_result_keeps_its_width | keep | rust/c55x_core/tests/semantics.rs::long_result_keeps_its_width |
| test_c55x_emu::SemanticTests.test_field_extract | keep | rust/c55x_core/tests/semantics.rs::field_extract |
| test_c55x_emu::SemanticTests.test_unsupported_mode_raises | keep | rust/c55x_core/tests/semantics.rs::unsupported_mode_raises |
| test_c55x_emu::LcdTests.test_page_column_and_data | delete | deleted with board, done in 69f2583 |
| test_c55x_emu::HelloLcdTests.test_draws_the_test_picture | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_acov::AcovTests.test_flags_are_clear_after_reset | keep | rust/c55x_core/tests/acov.rs::acov::flags_are_clear_after_reset |
| test_c55x_emu_acov::AcovTests.test_in_range_count_leaves_the_flags_clear | keep | rust/c55x_core/tests/acov.rs::acov::in_range_count_leaves_the_flags_clear |
| test_c55x_emu_acov::AcovTests.test_count_above_31_sets_the_destination_flag | keep | rust/c55x_core/tests/acov.rs::acov::count_above_31_sets_the_destination_flag |
| test_c55x_emu_acov::AcovTests.test_count_below_minus_32_sets_the_destination_flag | keep | rust/c55x_core/tests/acov.rs::acov::count_below_minus_32_sets_the_destination_flag |
| test_c55x_emu_acov::AcovTests.test_clamped_count_result_is_confined_to_the_working_width | keep | rust/c55x_core/tests/acov.rs::acov::clamped_count_result_is_confined_to_the_working_width |
| test_c55x_emu_acov::ShiftM40Tests.test_m40_off_extracts_bits_shifted_past_31 | keep | rust/c55x_core/tests/acov.rs::shift_m40::m40_off_extracts_bits_shifted_past_31 |
| test_c55x_emu_acov::ShiftM40Tests.test_m40_off_with_satd_saturates | keep | rust/c55x_core/tests/acov.rs::shift_m40::m40_off_with_satd_saturates |
| test_c55x_emu_acov::ShiftM40Tests.test_m40_on_keeps_the_guard_bits | keep | rust/c55x_core/tests/acov.rs::shift_m40::m40_on_keeps_the_guard_bits |
| test_c55x_emu_acov::ShiftM40Tests.test_step_and_run_agree_on_immediate_shifts | keep | rust/c55x_core/tests/acov.rs::shift_m40::step_and_run_agree_on_immediate_shifts |
| test_c55x_emu_acov_mac::MacOverflowTests.test_m40_off_accumulate_overflow_reports_and_saturates | keep | rust/c55x_core/tests/acov_mac.rs::m40_off_accumulate_overflow_reports_and_saturates |
| test_c55x_emu_acov_mac::MacOverflowTests.test_m40_off_negative_overflow_saturates | keep | rust/c55x_core/tests/acov_mac.rs::m40_off_negative_overflow_saturates |
| test_c55x_emu_acov_mac::MacOverflowTests.test_m40_on_accumulate_uses_40_bits | keep | rust/c55x_core/tests/acov_mac.rs::m40_on_accumulate_uses_40_bits |
| test_c55x_emu_acov_mac::MacOverflowTests.test_in_range_product_and_sum_report_nothing | keep | rust/c55x_core/tests/acov_mac.rs::in_range_product_and_sum_report_nothing |
| test_c55x_emu_acov_mac::MacOverflowTests.test_smul_reports_the_multiplier_overflow | keep | rust/c55x_core/tests/acov_mac.rs::smul_reports_the_multiplier_overflow |
| test_c55x_emu_acov_mac::MacOverflowTests.test_unsigned_product_reports_nothing | keep | rust/c55x_core/tests/acov_mac.rs::unsigned_product_reports_nothing |
| test_c55x_emu_acov_mac::MacOverflowTests.test_step_and_run_agree_on_mac | keep | rust/c55x_core/tests/acov_mac.rs::step_and_run_agree_on_mac |
| test_c55x_emu_addressing::CircularTests.test_index_wraps_in_the_buffer | keep | rust/c55x_core/tests/addressing.rs::circular::index_wraps_in_the_buffer |
| test_c55x_emu_addressing::CircularTests.test_linear_pointer_ignores_the_buffer | keep | rust/c55x_core/tests/addressing.rs::circular::linear_pointer_ignores_the_buffer |
| test_c55x_emu_addressing::CircularTests.test_zero_buffer_size_raises | keep | rust/c55x_core/tests/addressing.rs::circular::zero_buffer_size_raises |
| test_c55x_emu_addressing::ShortOperandTests.test_parse | convert | rust/c55x_core/src/frontend/tests.rs::parse::short_operand |
| test_c55x_emu_addressing::ShortOperandTests.test_dsp_mode_uses_t1 | keep | rust/c55x_core/tests/addressing.rs::short_operand::dsp_mode_uses_t1 |
| test_c55x_emu_addressing::LongStackTests.test_mmr_block_save_and_restore | keep | rust/c55x_core/tests/addressing.rs::long_stack::mmr_block_save_and_restore |
| test_c55x_emu_addressing::LongStackTests.test_long_push_order | keep | rust/c55x_core/tests/addressing.rs::long_stack::long_push_order |
| test_c55x_emu_addressing::M40Tests.test_m40_off_keeps_32_bit_semantics | keep | rust/c55x_core/tests/addressing.rs::m40::m40_off_keeps_32_bit_semantics |
| test_c55x_emu_addressing::M40Tests.test_m40_on_uses_the_guard_bits | keep | rust/c55x_core/tests/addressing.rs::m40::m40_on_uses_the_guard_bits |
| test_c55x_emu_addressing::M40Tests.test_sign_extended_low_byte | keep | rust/c55x_core/tests/addressing.rs::m40::sign_extended_low_byte |
| test_c55x_emu_addressing::CarryTests.test_64_bit_add | keep | rust/c55x_core/tests/addressing.rs::carry::_64_bit_add |
| test_c55x_emu_addressing::CarryTests.test_64_bit_subtract | keep | rust/c55x_core/tests/addressing.rs::carry::_64_bit_subtract |
| test_c55x_emu_addressing::CarryTests.test_product_leaves_carry | keep | rust/c55x_core/tests/addressing.rs::carry::product_leaves_carry |
| test_c55x_emu_addressing::BitFieldTests.test_band_sets_tc_on_any_bit | keep | rust/c55x_core/tests/addressing.rs::bit_field::band_sets_tc_on_any_bit |
| test_c55x_emu_addressing::MmrLoadTests.test_mmr_load_sign_extends_like_memory | keep | rust/c55x_core/tests/addressing.rs::mmr_load::mmr_load_sign_extends_like_memory |
| test_c55x_emu_audio::DmaTests.test_block_copy_raises_the_channel_interrupt | keep | rust/c55x_core/tests/dma.rs::block_copy_raises_the_channel_interrupt |
| test_c55x_emu_audio::DmaTests.test_double_index_interleaves_two_buffers | keep | rust/c55x_core/tests/dma.rs::double_index_interleaves_two_buffers |
| test_c55x_emu_audio::McBspStreamTests.test_samples_follow_the_table | rewrite | rust/c55x_core/tests/mcbsp.rs::samples_follow_the_table (audio clock as data, not a callback) |
| test_c55x_emu_audio::McBspStreamTests.test_block_interrupts_and_rate | rewrite | rust/c55x_core/tests/mcbsp.rs::block_interrupts_and_rate (audio clock as data, not a callback) |
| test_c55x_emu_audio::McBspStreamTests.test_no_clock_no_samples | rewrite | rust/c55x_core/tests/mcbsp.rs::no_clock_no_samples (audio clock as data, not a callback) |
| test_c55x_emu_audio::HiLoadTests.test_hi_takes_the_sign_into_the_guard_bits | keep | rust/c55x_core/tests/semantics.rs::hi_takes_the_sign_into_the_guard_bits |
| test_c55x_emu_audio::CodecTests.test_usb_mode_rates | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_audio::CodecTests.test_clock_needs_master_and_active | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_audio::CodecTests.test_reset | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_audio::CodecTests.test_headphone_gain | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_audio::HelloAudioTests.test_one_kilohertz_tone | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_containment::RejectedOutside.test_direct_construction_is_refused | keep | rust/c55x_core/tests/containment.rs::outside::direct_construction_is_refused (#[ignore], run outside the launcher) |
| test_c55x_emu_containment::RejectedOutside.test_fake_environment_marker_alone_is_refused | keep | rust/c55x_core/tests/containment.rs::outside::fake_environment_marker_alone_is_refused (#[ignore], run outside the launcher) |
| test_c55x_emu_containment::InsideJob.test_core_constructs_and_runs | keep | rust/c55x_core/tests/containment.rs::core_constructs_and_runs |
| test_c55x_emu_containment::InsideJob.test_kernel_values_seen_by_the_job | keep | rust/c55x_core/tests/containment.rs::kernel_values_seen_by_the_job |
| test_c55x_emu_containment::InsideJob.test_policy_is_revalidated_when_the_job_ceiling_is_relaxed | keep | rust/c55x_core/tests/containment.rs::policy_is_revalidated_when_the_job_ceiling_is_relaxed |
| test_c55x_emu_containment::InsideJob.test_policy_is_revalidated_when_the_marker_disappears | keep | rust/c55x_core/tests/containment.rs::policy_is_revalidated_when_the_marker_disappears |
| test_c55x_emu_containment::InsideJob.test_every_execution_entry_of_an_existing_core_is_guarded | keep | rust/c55x_core/tests/containment.rs::every_execution_entry_of_an_existing_core_is_guarded |
| test_c55x_emu_containment::InsideJob.test_descendants_share_the_task_limit | keep | rust/c55x_core/tests/containment.rs::descendants_share_the_task_limit |
| test_c55x_emu_containment::InsideJob.test_small_tmpfs_allocation_is_charged_to_the_job | keep | rust/c55x_core/tests/containment.rs::small_tmpfs_allocation_is_charged_to_the_job |
| test_c55x_emu_containment::InsideJob.test_trace_length_cap_is_an_explicit_error | keep | rust/c55x_core/tests/containment.rs::trace_length_cap_is_an_explicit_error |
| test_c55x_emu_controls::ControlTests.test_buttons_hit_their_key_codes | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_controls::ControlTests.test_touch_and_lock_events | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_controls::LanguageMenuTests.test_navigation | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_controls::LanguageMenuTests.test_power_off_and_on | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_disk::AtaTests.test_identify_reports_the_capacity | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_disk::AtaTests.test_write_then_read_multiple | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_disk::AtaTests.test_unknown_command_aborts | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_disk::ImageTests.test_partition_table_layout | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_disk::ImageTests.test_minifs_file_round_trip | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_disk::ImageTests.test_synthetic_disk_without_files | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_dsp::MantExpTests.test_spru375g_examples | keep | rust/c55x_core/tests/dsp.rs::mant_exp::spru375g_examples |
| test_c55x_emu_dsp::MantExpTests.test_zero | keep | rust/c55x_core/tests/dsp.rs::mant_exp::zero |
| test_c55x_emu_dsp::MantExpTests.test_split_indexed_coefficient | convert | rust/c55x_core/src/frontend/tests.rs::dual::split_indexed_coefficient |
| test_c55x_emu_dsp::MantExpTests.test_split | convert | rust/c55x_core/src/frontend/tests.rs::dual::split |
| test_c55x_emu_dsp::DualMultiplyTests.test_shared_coefficient_read_once | keep | rust/c55x_core/tests/dsp.rs::dual_multiply::shared_coefficient_read_once |
| test_c55x_emu_dsp::DualMultiplyTests.test_fractional_mode_doubles_products | keep | rust/c55x_core/tests/dsp.rs::dual_multiply::fractional_mode_doubles_products |
| test_c55x_emu_dsp::SaturationTests.test_satd_clamps_at_32_bits | keep | rust/c55x_core/tests/dsp.rs::saturation::satd_clamps_at_32_bits |
| test_c55x_emu_dsp::SaturationTests.test_satd_with_m40_clamps_at_40_bits | keep | rust/c55x_core/tests/dsp.rs::saturation::satd_with_m40_clamps_at_40_bits |
| test_c55x_emu_dsp::SaturationTests.test_rnd_and_saturate | keep | rust/c55x_core/tests/dsp.rs::saturation::rnd_and_saturate |
| test_c55x_emu_dsp::ExecuteAfterPartnerTests.test_partner_runs_and_next_is_conditional | keep | rust/c55x_core/tests/dsp.rs::execute_after_partner::partner_runs_and_next_is_conditional |
| test_c55x_emu_dsp::ExecuteDUnitTests.test_next_data_operation_is_conditional | keep | rust/c55x_core/tests/dsp.rs::execute_d_unit::next_data_operation_is_conditional |
| test_c55x_emu_dsp::ExecuteDUnitTests.test_address_phase_always_runs | keep | rust/c55x_core/tests/dsp.rs::execute_d_unit::address_phase_always_runs |
| test_c55x_emu_dsp::AddressAndShiftTests.test_mar_arithmetic_is_circular | keep | rust/c55x_core/tests/dsp.rs::address_and_shift::mar_arithmetic_is_circular |
| test_c55x_emu_dsp::AddressAndShiftTests.test_shift_count_in_tx_is_signed_without_sxmd | keep | rust/c55x_core/tests/dsp.rs::address_and_shift::shift_count_in_tx_is_signed_without_sxmd |
| test_c55x_emu_dsp::AddressAndShiftTests.test_store_of_shifted_accumulator | keep | rust/c55x_core/tests/dsp.rs::address_and_shift::store_of_shifted_accumulator |
| test_c55x_emu_dsp::AddressAndShiftTests.test_popboth_sign_extends | keep | rust/c55x_core/tests/dsp.rs::address_and_shift::popboth_sign_extends |
| test_c55x_emu_dsp::AddressAndShiftTests.test_port_qualifier_covers_read_modify_write | rewrite | rust/c55x_core/tests/dsp.rs::address_and_shift::port_qualifier_covers_read_modify_write (core-native port instead of a Python hook) |
| test_c55x_emu_dsp::AddressAndShiftTests.test_reserved_mmr_reads_zero | keep | rust/c55x_core/tests/dsp.rs::address_and_shift::reserved_mmr_reads_zero |
| test_c55x_emu_gc::MachinesAreFreed.test_dropped_machines_are_collected | keep | rust/c55x_core/tests/lifecycle.rs::dropped_cores_are_collected (Drop and RSS instead of the Python collector) |
| test_c55x_emu_gc::MachinesAreFreed.test_rss_does_not_grow_with_the_number_of_machines | keep | rust/c55x_core/tests/lifecycle.rs::rss_does_not_grow_with_the_number_of_cores (Drop and RSS instead of the Python collector) |
| test_c55x_emu_idle::IdleTests.test_cpu_domain_active_runs_on | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_idle::IdleTests.test_enabled_interrupt_wakes_with_intm_set | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_idle::IdleTests.test_clock_stopped_waits_for_an_external_interrupt | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_idle::IdleTests.test_interrupt_ends_idle_and_returns_after_it | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_irq::DispatchTests.test_vector_addresses | keep | rust/c55x_core/tests/irq.rs::vector_addresses |
| test_c55x_emu_irq::DispatchTests.test_context_is_saved_and_restored | keep | rust/c55x_core/tests/irq.rs::context_is_saved_and_restored |
| test_c55x_emu_irq::DispatchTests.test_priority_and_masking | keep | rust/c55x_core/tests/irq.rs::priority_and_masking |
| test_c55x_emu_irq::DispatchTests.test_disabled_interrupt_stays_pending | keep | rust/c55x_core/tests/irq.rs::disabled_interrupt_stays_pending |
| test_c55x_emu_irq::TimerTests.test_auto_reload_period | delete | deleted with board (timer.py), done in 69f2583 |
| test_c55x_emu_jukebox2::Jukebox2BootTests.test_boot_reaches_mcu_init | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_jukebox2::Jukebox2BootTests.test_cfs_format_with_the_original_code | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_long_words::LongWordAddressOrderTests.test_memory_load_both_parities_and_postincrement | keep | rust/c55x_core/tests/long_words.rs::memory_load_both_parities_and_postincrement |
| test_c55x_emu_long_words::LongWordAddressOrderTests.test_memory_store_both_parities_and_postincrement | keep | rust/c55x_core/tests/long_words.rs::memory_store_both_parities_and_postincrement |
| test_c55x_emu_long_words::LongWordAddressOrderTests.test_mmr_load_both_parities | keep | rust/c55x_core/tests/long_words.rs::mmr_load_both_parities |
| test_c55x_emu_long_words::LongWordAddressOrderTests.test_mmr_store_both_parities | keep | rust/c55x_core/tests/long_words.rs::mmr_store_both_parities |
| test_c55x_emu_manual_review::SignedShiftSignTests.test_negative_left_shift_without_satd | keep | rust/c55x_core/tests/manual_review.rs::signed_shift_sign::negative_left_shift_without_satd |
| test_c55x_emu_manual_review::SignedShiftSignTests.test_negative_left_shift_with_satd | keep | rust/c55x_core/tests/manual_review.rs::signed_shift_sign::negative_left_shift_with_satd |
| test_c55x_emu_manual_review::SignedShiftSignTests.test_shift_by_zero_sign_extends | keep | rust/c55x_core/tests/manual_review.rs::signed_shift_sign::shift_by_zero_sign_extends |
| test_c55x_emu_manual_review::SignedShiftSignTests.test_positive_tx_count_on_a_negative_value | keep | rust/c55x_core/tests/manual_review.rs::signed_shift_sign::positive_tx_count_on_a_negative_value |
| test_c55x_emu_manual_review::SignedShiftSignTests.test_add_with_a_shifted_negative_accumulator | keep | rust/c55x_core/tests/manual_review.rs::signed_shift_sign::add_with_a_shifted_negative_accumulator |
| test_c55x_emu_manual_review::OverflowAttributionTests.test_dual_mac_second_destination_overflows | keep | rust/c55x_core/tests/manual_review.rs::overflow_attribution::dual_mac_second_destination_overflows |
| test_c55x_emu_manual_review::OverflowAttributionTests.test_dual_mac_first_destination_overflows | keep | rust/c55x_core/tests/manual_review.rs::overflow_attribution::dual_mac_first_destination_overflows |
| test_c55x_emu_manual_review::OverflowAttributionTests.test_dual_mac_both_destinations_overflow | keep | rust/c55x_core/tests/manual_review.rs::overflow_attribution::dual_mac_both_destinations_overflow |
| test_c55x_emu_manual_review::OverflowAttributionTests.test_store_shift_overflow_is_not_reported_on_the_partner | keep | rust/c55x_core/tests/manual_review.rs::overflow_attribution::store_shift_overflow_is_not_reported_on_the_partner |
| test_c55x_emu_manual_review::OverflowAttributionTests.test_store_does_not_erase_the_partner_overflow | keep | rust/c55x_core/tests/manual_review.rs::overflow_attribution::store_does_not_erase_the_partner_overflow |
| test_c55x_emu_manual_review::StoreShiftTests.test_right_shift_brings_guard_bits_down | keep | rust/c55x_core/tests/manual_review.rs::store_shift::right_shift_brings_guard_bits_down |
| test_c55x_emu_manual_review::StoreShiftTests.test_store_shift_does_not_saturate_with_satd | keep | rust/c55x_core/tests/manual_review.rs::store_shift::store_shift_does_not_saturate_with_satd |
| test_c55x_emu_manual_review::StoreShiftTests.test_saturate_keyword_clamps_the_shifted_value | keep | rust/c55x_core/tests/manual_review.rs::store_shift::saturate_keyword_clamps_the_shifted_value |
| test_c55x_emu_manual_review::ProductSignTests.test_smem_and_accumulator_forms_agree | keep | rust/c55x_core/tests/manual_review.rs::product_sign::smem_and_accumulator_forms_agree |
| test_c55x_emu_manual_review::UnsignedProductTests.test_unsigned_mac_example | keep | rust/c55x_core/tests/manual_review.rs::unsigned_product::unsigned_mac_example |
| test_c55x_emu_manual_review::SxmdIndependentOperandTests.test_multiply_operands_ignore_sxmd | keep | rust/c55x_core/tests/manual_review.rs::sxmd_independent_operand::multiply_operands_ignore_sxmd |
| test_c55x_emu_manual_review::SxmdIndependentOperandTests.test_mac_operands_ignore_sxmd | keep | rust/c55x_core/tests/manual_review.rs::sxmd_independent_operand::mac_operands_ignore_sxmd |
| test_c55x_emu_manual_review::SxmdIndependentOperandTests.test_unsigned_operands_stay_zero_extended | keep | rust/c55x_core/tests/manual_review.rs::sxmd_independent_operand::unsigned_operands_stay_zero_extended |
| test_c55x_emu_manual_review::SxmdIndependentOperandTests.test_byte_load_to_ta_register_ignores_sxmd | keep | rust/c55x_core/tests/manual_review.rs::sxmd_independent_operand::byte_load_to_ta_register_ignores_sxmd |
| test_c55x_emu_manual_review::SxmdIndependentOperandTests.test_byte_load_to_accumulator_follows_sxmd | keep | rust/c55x_core/tests/manual_review.rs::sxmd_independent_operand::byte_load_to_accumulator_follows_sxmd |
| test_c55x_emu_manual_review::StatusFlagTests.test_acov_is_part_of_st0 | keep | rust/c55x_core/tests/manual_review.rs::status_flag::acov_is_part_of_st0 |
| test_c55x_emu_manual_review::StatusFlagTests.test_bclr_acov0_clears_the_flag | keep | rust/c55x_core/tests/manual_review.rs::status_flag::bclr_acov0_clears_the_flag |
| test_c55x_emu_manual_review::StatusFlagTests.test_accumulator_move_reports_overflow | keep | rust/c55x_core/tests/manual_review.rs::status_flag::accumulator_move_reports_overflow |
| test_c55x_emu_manual_review::StatusFlagTests.test_sat_reports_overflow | keep | rust/c55x_core/tests/manual_review.rs::status_flag::sat_reports_overflow |
| test_c55x_emu_manual_review::StatusFlagTests.test_satr_clears_the_low_word_after_saturation | keep | rust/c55x_core/tests/manual_review.rs::status_flag::satr_clears_the_low_word_after_saturation |
| test_c55x_emu_manual_review::StatusFlagTests.test_subc_reports_overflow_without_saturation | keep | rust/c55x_core/tests/manual_review.rs::status_flag::subc_reports_overflow_without_saturation |
| test_c55x_emu_manual_review::StatusFlagTests.test_add_k16_to_memory_saturates_and_reports_acov0 | keep | rust/c55x_core/tests/manual_review.rs::status_flag::add_k16_to_memory_saturates_and_reports_acov0 |
| test_c55x_emu_manual_review::StatusFlagTests.test_add_k16_carry_and_one_address_update | keep | rust/c55x_core/tests/manual_review.rs::status_flag::add_k16_carry_and_one_address_update |
| test_c55x_emu_manual_review::StatusFlagTests.test_add_k16_negative_constant_saturates_to_8000h | keep | rust/c55x_core/tests/manual_review.rs::status_flag::add_k16_negative_constant_saturates_to_8000h |
| test_c55x_emu_manual_review::StatusFlagTests.test_accumulator_move_saturates_and_sets_st0_bit_9 | keep | rust/c55x_core/tests/manual_review.rs::status_flag::accumulator_move_saturates_and_sets_st0_bit_9 |
| test_c55x_emu_manual_review::StatusFlagTests.test_accumulator_move_with_m40_does_not_overflow | keep | rust/c55x_core/tests/manual_review.rs::status_flag::accumulator_move_with_m40_does_not_overflow |
| test_c55x_emu_manual_review::StatusFlagTests.test_repeated_subc_example | keep | rust/c55x_core/tests/manual_review.rs::status_flag::repeated_subc_example |
| test_c55x_emu_manual_review::BitCountTests.test_manual_example | keep | rust/c55x_core/tests/manual_review.rs::bit_count::manual_example |
| test_c55x_emu_manual_review::BitCountTests.test_even_count_clears_tcx | keep | rust/c55x_core/tests/manual_review.rs::bit_count::even_count_clears_tcx |
| test_c55x_emu_manual_review::DualArithmeticTests.test_add_at_even_address | keep | rust/c55x_core/tests/manual_review.rs::dual_arithmetic::add_at_even_address |
| test_c55x_emu_manual_review::DualArithmeticTests.test_add_at_odd_address_takes_the_previous_word | keep | rust/c55x_core/tests/manual_review.rs::dual_arithmetic::add_at_odd_address_takes_the_previous_word |
| test_c55x_emu_manual_review::DualArithmeticTests.test_low_part_overflow_sets_acov | keep | rust/c55x_core/tests/manual_review.rs::dual_arithmetic::low_part_overflow_sets_acov |
| test_c55x_emu_manual_review::DualArithmeticTests.test_high_part_overflow_sets_acov | keep | rust/c55x_core/tests/manual_review.rs::dual_arithmetic::high_part_overflow_sets_acov |
| test_c55x_emu_manual_review::DualArithmeticTests.test_high_part_carry | keep | rust/c55x_core/tests/manual_review.rs::dual_arithmetic::high_part_carry |
| test_c55x_emu_manual_review::DualArithmeticTests.test_subtract_reports_the_high_part_borrow | keep | rust/c55x_core/tests/manual_review.rs::dual_arithmetic::subtract_reports_the_high_part_borrow |
| test_c55x_emu_manual_review::ConditionalRepeatTests.test_true_condition_runs_k8_plus_1_times | keep | rust/c55x_core/tests/manual_review.rs::conditional_repeat::true_condition_runs_k8_plus_1_times |
| test_c55x_emu_manual_review::ConditionalRepeatTests.test_false_condition_runs_no_iteration | keep | rust/c55x_core/tests/manual_review.rs::conditional_repeat::false_condition_runs_no_iteration |
| test_c55x_emu_manual_review::ConditionalRepeatTests.test_negated_condition | keep | rust/c55x_core/tests/manual_review.rs::conditional_repeat::negated_condition |
| test_c55x_emu_manual_review::ExecuteConditionallyTests.test_manual_examples | keep | rust/c55x_core/tests/manual_review.rs::execute_conditionally::manual_examples |
| test_c55x_emu_manual_review::ExecuteConditionallyTests.test_after_a_partner | keep | rust/c55x_core/tests/manual_review.rs::execute_conditionally::after_a_partner |
| test_c55x_emu_manual_review::ExecuteConditionallyTests.test_parallel_store_keeps_the_pointer_update | keep | rust/c55x_core/tests/manual_review.rs::execute_conditionally::parallel_store_keeps_the_pointer_update |
| test_c55x_emu_manual_review::SquareDistanceTests.test_manual_example | keep | rust/c55x_core/tests/manual_review.rs::square_distance::manual_example |
| test_c55x_emu_manual_review::SquareDistanceTests.test_no_borrow_sets_carry | keep | rust/c55x_core/tests/manual_review.rs::square_distance::no_borrow_sets_carry |
| test_c55x_emu_mcu::I2cMcuTests.test_register_read | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_mcu::I2cMcuTests.test_command_with_argument | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_mcu::I2cMcuTests.test_events_queue_and_empty_answer | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_mcu::I2cMcuTests.test_absent_slave_nacks | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_mcu::I2cMcuTests.test_key_codes | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_mcu::McuPowerTests.test_only_power_wakes_after_suspend | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_mcu::HelloMcuTests.test_start_up_and_event_display | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_mcu::HelloMcuTests.test_timer_tick_interrupt | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_nor::NorTests.test_software_id_and_exit | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_nor::NorTests.test_cfi_size_exponent | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_nor::NorTests.test_program_only_clears_bits | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_nor::NorTests.test_sector_erase | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_nor::NorTests.test_bad_unlock_is_counted | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_nor::NorImageTests.test_table_and_checksum | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_nor::NorImageTests.test_oversized_area_raises | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_oracle_v2::K16ConstantTests.test_mpyk_negative_print_sign_extends | keep | rust/c55x_core/tests/oracle_v2.rs::k16_constant::mpyk_negative_print_sign_extends |
| test_c55x_emu_oracle_v2::K16ConstantTests.test_k16_constant_sign_extends | keep | rust/c55x_core/tests/oracle_v2.rs::k16_constant::k16_constant_sign_extends |
| test_c55x_emu_oracle_v2::K16ConstantTests.test_positive_k16_unchanged | keep | rust/c55x_core/tests/oracle_v2.rs::k16_constant::positive_k16_unchanged |
| test_c55x_emu_oracle_v2::K16ConstantTests.test_pre_negated_constant_unchanged | keep | rust/c55x_core/tests/oracle_v2.rs::k16_constant::pre_negated_constant_unchanged |
| test_c55x_emu_oracle_v2::RepeatBodyTraceTests.test_rpt_body_pcs_appear_in_the_trace | keep | rust/c55x_core/tests/oracle_v2.rs::repeat_body_trace::rpt_body_pcs_appear_in_the_trace |
| test_c55x_emu_oracle_v2::RepeatBodyTraceTests.test_tracing_changes_no_value | keep | rust/c55x_core/tests/oracle_v2.rs::repeat_body_trace::tracing_changes_no_value |
| test_c55x_emu_oracle_v2::K8ConstantTests.test_mpyk_k8_negative | keep | rust/c55x_core/tests/oracle_v2.rs::k8_constant::mpyk_k8_negative |
| test_c55x_emu_oracle_v2::K8ConstantTests.test_macmk_smem_k8_negative | keep | rust/c55x_core/tests/oracle_v2.rs::k8_constant::macmk_smem_k8_negative |
| test_c55x_emu_oracle_v2::K8ConstantTests.test_positive_k8_unchanged | keep | rust/c55x_core/tests/oracle_v2.rs::k8_constant::positive_k8_unchanged |
| test_c55x_emu_pacing::PacerTests.test_follows_the_wall_clock | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_pacing::PacerTests.test_slow_emulator_lags_at_most_max_lag | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_pacing::PacerTests.test_rebase | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_resource_caps::BusCaps.test_address_space_is_the_architectural_16_mib | rewrite | rust/c55x_core/tests/resource_caps.rs::address_space_is_the_architectural_16_mib (NativeHost memory and loader) |
| test_c55x_emu_resource_caps::BusCaps.test_load_outside_the_address_space_is_refused | rewrite | rust/c55x_core/tests/resource_caps.rs::load_outside_the_address_space_is_refused (NativeHost memory and loader) |
| test_c55x_emu_resource_caps::BusCaps.test_many_small_loads_stay_fast_and_keep_no_history | rewrite | rust/c55x_core/tests/resource_caps.rs::many_small_loads_stay_fast_and_keep_no_history (NativeHost memory and loader) |
| test_c55x_emu_resource_caps::ImageCaps.test_image_info_reports_name_size_sections_and_digest | rewrite | image loader of step 7 (c55x_core or c55x_run)::image_info_reports_name_size_sections_and_digest |
| test_c55x_emu_resource_caps::ImageCaps.test_an_image_larger_than_the_address_space_is_refused_before_allocation | rewrite | image loader of step 7 (c55x_core or c55x_run)::an_image_larger_than_the_address_space_is_refused_before_allocation |
| test_c55x_emu_resource_caps::CodeCacheCap.test_runaway_decode_stops_with_an_explicit_error | rewrite | rust/c55x_core/tests/resource_caps.rs::runaway_decode_stops_with_an_explicit_error (core code cache cap) |
| test_c55x_emu_sftl::SftlTests.test_manual_example_4_37_3 | keep | rust/c55x_core/tests/sftl.rs::manual_example_4_37_3 |
| test_c55x_emu_sftl::SftlTests.test_right_by_one_clears_the_guards | keep | rust/c55x_core/tests/sftl.rs::right_by_one_clears_the_guards |
| test_c55x_emu_sftl::SftlTests.test_register_destination_shifts_16_bits | keep | rust/c55x_core/tests/sftl.rs::register_destination_shifts_16_bits |
| test_c55x_emu_sftl::SftlTests.test_zero_count_clears_carry | keep | rust/c55x_core/tests/sftl.rs::zero_count_clears_carry |
| test_c55x_emu_status::StatusTests.test_duration | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_status::StatusTests.test_time_follows_the_clock | delete | deleted with board, done in 69f2583 |
| test_c55x_emu_status::StatusTests.test_rtc_second_follows_the_clock | delete | deleted with board, done in 69f2583 |
| test_c55x_frontend::FrontendTest.test_listing_readers_agree | convert | rust/c55x_core/src/frontend/tests.rs::synthetic::listing_readers_agree |
| test_c55x_frontend::FrontendTest.test_listing_errors_agree | convert | rust/c55x_core/src/frontend/tests.rs::synthetic::listing_errors_agree |
| test_c55x_frontend::FrontendTest.test_synthetic_listing | convert | rust/c55x_core/src/frontend/tests.rs::synthetic::synthetic_listing |
| test_c55x_frontend::FrontendTest.test_rejections | convert | rust/c55x_core/src/frontend/tests.rs::synthetic::rejections |
| test_c55x_frontend::FrontendTest.test_fuzz | convert | rust/c55x_core/src/frontend/tests.rs::synthetic::fuzz |
| test_c55x_launcher::LauncherTests.test_kernel_values_for_both_job_classes | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::LauncherTests.test_argv_cwd_streams_stdin_and_exit_code_are_preserved | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::LauncherTests.test_environment_threads_and_tmpdir | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::LauncherTests.test_excessive_limits_cannot_widen_the_hard_policy | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::LauncherTests.test_low_memory_headroom_refuses_before_the_payload | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::LauncherTests.test_tmpfs_work_dir_is_rejected_before_anything_is_created | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::LauncherTests.test_unavailable_manager_refuses_without_fallback | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::LauncherTests.test_reserved_exit_codes_from_the_payload_are_not_mistaken_for_outcomes | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::LauncherTests.test_children_below_individual_limits_exceed_the_aggregate_budget | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::LauncherTests.test_tmpfs_allocation_is_contained_by_the_cgroup | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::LauncherTests.test_wall_timeout_stops_a_term_ignoring_child | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::LauncherTests.test_cpu_limit_is_reported_as_a_timeout | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::LauncherTests.test_signal_death_is_reported_as_128_plus_n | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::LauncherTests.test_concurrent_emulator_jobs_get_distinct_slots_until_all_are_busy | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::LauncherTests.test_eight_slots_admit_eight_jobs_and_refuse_a_ninth | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::LauncherTests.test_orphaned_job_keeps_its_limits_blocks_only_its_slot_and_ends_by_itself | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::LauncherTests.test_launcher_interruption_stops_only_its_own_job | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::LauncherTests.test_status_reports_every_slot | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::LauncherTests.test_work_class_job_cannot_construct_a_core | infra | stays a launcher test (python3, no core import); payload becomes the Rust test binary or c55x-run instead of c55x_emu.testutil |
| test_c55x_launcher::LauncherTests.test_core_runs_in_a_slot_other_than_zero | infra | stays a launcher test (python3, no core import); payload becomes the Rust test binary or c55x-run instead of c55x_emu.testutil |
| test_c55x_launcher::LauncherTests.test_similarly_named_group_outside_the_shared_slice_is_refused | infra | stays a launcher test (python3, no core import); payload becomes the Rust test binary or c55x-run instead of c55x_emu.testutil |
| test_c55x_launcher::MockedLauncherTests.test_slot_names | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::MockedLauncherTests.test_slot_hook_can_only_lower_the_count | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::MockedLauncherTests.test_first_free_slot_is_taken_and_a_freed_slot_is_reused | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::MockedLauncherTests.test_active_unit_blocks_only_its_slot_and_its_lock_is_not_kept | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::MockedLauncherTests.test_unit_states_other_than_inactive_or_failed_keep_a_slot_busy | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::MockedLauncherTests.test_no_free_slot_names_every_slot | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::MockedLauncherTests.test_launcher_exits_75_when_every_slot_is_busy | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::MockedLauncherTests.test_a_unit_that_systemd_run_could_not_start_is_not_classified | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::MockedLauncherTests.test_a_unit_that_was_started_is_classified | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::MockedLauncherTests.test_status_is_read_only_and_lists_every_slot | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::MockedLauncherTests.test_job_side_verification_execs_only_when_values_match | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::MockedLauncherTests.test_job_side_verification_checks_the_slot_unit_and_the_shared_values | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::MockedLauncherTests.test_job_side_verification_checks_the_address_space_of_each_class | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_launcher::MockedLauncherTests.test_launch_asks_systemd_for_the_address_space_of_its_class | infra | stays a launcher test (python3, tests bin/c55x-contained only) |
| test_c55x_mtpscript::ParserTest.test_data_of_unknown_length_ends_at_its_response | delete | deleted with board, done in 69f2583 |
| test_c55x_mtpscript::ParserTest.test_prop_list | delete | deleted with board, done in 69f2583 |
| test_c55x_mtpscript::ParserTest.test_session_ends_with_reset_device | delete | deleted with board, done in 69f2583 |
| test_c55x_mtpscript::ParserTest.test_u32_array | delete | deleted with board, done in 69f2583 |
| test_c55x_mtpscript::ScriptTest.test_next_step_waits_for_the_answers_and_sees_them | delete | deleted with board, done in 69f2583 |
| test_c55x_native_policy::NativePolicyTests.test_policy_unit_tests_pass | infra | `cargo test -p c55x_core policy::` (policy.rs unit tests run directly; Python wrapper deleted) |


Infra decisions in detail:
- **test_c55x_native_policy** (1): it only compiles rust/c55x_core/src/policy.rs with
  rustc --test. `cargo test -p c55x_core policy::` runs the same unit tests, so the Python
  wrapper is deleted.
- **test_c55x_launcher** (35): these test bin/c55x-contained, which is a Python program
  and stays one. The tests stay Python and run with plain python3, because they test the
  launcher, not the emulator. 32 never touch the core. 3 tests
  (test_work_class_job_cannot_construct_a_core, test_core_runs_in_a_slot_other_than_zero,
  test_similarly_named_group_outside_the_shared_slice_is_refused) run a payload that
  builds a core through c55x_emu.testutil. Their payload changes to a Rust binary: the
  harness test binary with a filter, or c55x-run. They then work without c55x_py.
- **test_c55x_emu_containment** (10) moves to Rust (rust/c55x_core/tests/containment.rs):
  the guard is Rust. RejectedOutside (2) become #[ignore] tests run by hand outside the
  launcher, as they are skipped inside it today.

## 3. Fixture scheme (implemented: rust/fixtures, rust/c55x_fixtures)

- One directory per synthetic program: rust/fixtures/NAME/
  - prog.asm (hand-written source)
  - fixture.txt (`origin 0x......`)
  - prog.lst (dis55 -qq -r -g)
  - prog.hex (loaded sections: `NAME 0xADDR SIZE`, then hex lines)
  - prog.sym (`NAME 0xVALUE`; labels, from a second assembly with --output_all_syms, so
    the listing keeps numeric branch targets like a runtime listing)
  - frozen expected results (*.tsv, with a header naming their origin)
- `c55x-fixtures check|write [--ti DIR] [NAME...]` (std-only workspace bin) rebuilds
  prog.lst / prog.hex / prog.sym with asm55 + lnk55 + dis55 in a work job, and `check`
  fails when a committed file differs. It verifies that the code is identical with and
  without --output_all_syms. The tool directory comes from --ti, $C55X_TI_BIN or PATH and
  is never written into the repository.
- cargo test reads fixtures with include_str!(concat!(env!("CARGO_MANIFEST_DIR"),
  "/../fixtures/...")) and needs no TI tools.
- Program layout for execution tests (one fixture per Python test module):
  - Each test is a labeled snippet `t_<name>:` ending in `t_<name>_end: goto t_<name>_end`.
    A goto to itself is the idle loop that Core::run detects, as in testutil today.
  - The fixture starts with the mode directives of the runtime decoder: .mmregs,
    .cpl_on, .arms_on, .c54cm_off.
  - Data the Python test writes with bus.write is written by the Rust test before
    running. Constant tables can live in the fixture as .word data in their own section.
- Difference from testutil: testutil places every instruction at 2-byte steps with fake
  bytes. Assembled fixtures have real sizes, so a test that checks a pc, a return address
  or a trace takes the address from prog.sym instead of computing 0x20000 + 2n. Cycle
  counts do not change (one cycle per instruction).
- Forms asm55 does not accept. Seen so far: `bit(AR3, #2) = #1`, `cbit(T0, #4)`,
  `AC0 = pair(*AR3)`, `AC0 = HI(AC1 << T2)`, AR0 as an index with ARMS, `*SP(#k)` with
  CPL=0, `@k` with CPL=1.
  - The test takes the spelling asm55 accepts and dis55 prints back. The tool proves it.
  - Where no assembler spelling exists, a text-only fixture (prog.lst written by hand, no
    prog.asm, `kind text` in fixture.txt) is the exception. Each one is listed in this file
    with the reason and needs your approval.
- No firmware code, addresses, symbols or listings anywhere in rust/fixtures.

## 4. Execution harness (design; implement when NativeHost executes)

1. **Test layout**: rust/c55x_core/tests/<module>.rs, one test binary per Python module,
   plus tests/common/mod.rs. The helpers:
   - `Fixture::load(name)`: parses prog.lst with frontend::listing::parse, prog.hex into
     sections, prog.sym into a map.
   - `Fixture::core(entry_label) -> Core`: memory from the sections, a pc -> Decoded
     table built with frontend::build and frontend::resolver(symbols), NativeHost::core.
     The initial state is that of Cpu.__init__ + testutil: ST1 = CPL|SXMD|INTM,
     ST2 = ARMS, IVPD = IVPH = 0xFFFF, XSP = 0x1000, SSP = 0x0800, pc = the label.
   - `run(&mut Core)`: Core::run(10_000, no stop, no limit) must return true (idle
     reached), as testutil.run asserts.
   - Register and memory setup / checks go through core.st.* and Core::read / write /
     read32 / write32 (the core pairing, word ^ 1).
2. **Guard**:
   - The core must not run outside the launcher, and the harness never relaxes that.
     There is no cfg(test) path in policy.rs / guard.rs and no fake environment.
   - Constructing a core goes through the same guard as every host, `guard::guard()`.
     This ideally happens inside NativeHost::core / the native constructor that emu-fix
     adds; until then the helper calls it and panics with the policy message.
   - A test binary started outside an emulator job therefore fails at once with the
     policy error, and a work job is refused too ("job class work may not run the
     emulator core").
3. **Build and run** (a script, scripts/cargo-test-contained, POSIX sh):
   - **Build**: `bin/c55x-contained --job work --env RUSTC_WRAPPER --env CARGO_BUILD_JOBS
     -- cargo test --workspace --no-run --message-format=json` with RUSTC_WRAPPER= and
     CARGO_BUILD_JOBS=1. The script extracts the `"executable"` paths into
     build/test-bins.txt.
   - **Run**: one emulator job (one slot) runs every binary in turn:
     `bin/c55x-contained --env RUST_TEST_THREADS -- scripts/cargo-test-contained run`,
     with RUST_TEST_THREADS=2. libtest otherwise starts one thread per CPU and would hit
     the 32-task job limit.
   - The containment binary runs with --test-threads=1, because it changes the job's
     memory.max and the marker variable.
   - Memory: a core is 16 MiB of memory plus caches, so 2 threads stay far below the
     1 GiB default.
   - Exit status: the first failing binary fails the job (the launcher passes the payload
     status, 1-63, through).
   - Tests that never build a core (frontend, policy) run in the same job, so there is one
     command for everything.
4. **Order once NativeHost executes**:
   1. A pilot: tests/common + semantics.rs (17) with its fixture.
   2. Then one commit per module (acov, acov_mac, addressing, long_words, manual_review,
      oracle_v2, sftl, dsp, dma, irq, lifecycle, containment, then the rewrites).
   - Each Rust test copies the Python inputs and expected values literally.
   - Proposal: the Python test file is deleted in the same commit as its Rust
     replacement, so no behavior has two live test suites. The mapping table above is
     the record.

## 5. Open items

- The idle instruction has no model once idlectl goes with the board. The 4 idle tests are
  in "delete"; a 34-line ICR/ISTR model in c55x_core would make them rewrites instead.
- McBSP / take_audio need the audio clock as plain data in NativeHost (set_audio_clock),
  or the 3 McBSP tests and the feature go.
- ImageCaps (2) depend on where step 7 puts the image loader (c55x_core or c55x_run).
- `'NoneType' object has no attribute 'startswith'` (resolver, target missing) is frozen
  in rejected.tsv for equality with ir.py. Once Python is gone it can become a Rust
  message, changing the fixture in the same commit.

## 6. Decisions (lead, after phase A)

1. Each Python test file is deleted in the same commit as its passing Rust replacement.
2. Idle: the 4 idle tests are deleted. There is no ICR/ISTR model.
3. McBSP and DMA are on-chip and stay. The native host gets set_audio_clock as plain
   data, so the 3 McBSP tests become Rust tests; take_audio stays.
4. ImageCaps go with the image loader, wherever step 7 put it (check after the rebase).
5. Text-only fixtures are allowed only for a form that dis55 prints in real listings but
   asm55 will not re-assemble. Each one is listed here with the reason.
6. The Python-specific rejection text is replaced with a neutral Rust message when Python
   goes; the frozen tsv changes in that commit.
7. Launcher (owner decision): bin/c55x-contained and its 35 launcher tests stay Python as
   an explicit exception. They are machine-safety infrastructure, not the emulator.
   AGENTS.md records the exception when the Python emulator goes. Their payloads must not
   import c55x_emu or the c55x_core Python module: the 3 core payloads
   (test_work_class_job_cannot_construct_a_core, test_core_runs_in_a_slot_other_than_zero,
   test_similarly_named_group_outside_the_shared_slice_is_refused) switch to a Rust
   binary. Everything else becomes Rust.
8. The fork is never pushed by agents; commits stay local.

## 7. Progress and program adaptations

The `.byte` encoding below is a fourth kind of adaptation besides the three named in
section 3 (asm55 spelling, symbol addresses, text-only fixtures): it keeps the listing
produced by the tools. There is no text-only fixture so far.

Pilot (rust/c55x_core/tests/common + semantics.rs, fixture semantics): 18 tests
(SemanticTests 17 + HiLoadTests 1) pass in an emulator job via scripts/cargo-test-contained.
The same binary is refused in a work job and outside the launcher (guard message).

Where the Python test text is not what asm55 assembles / dis55 prints, the fixture uses the
tool spelling. The expected values are unchanged:

| Test | Python text | Fixture | Why |
|---|---|---|---|
| semantics::long_constant_and_logical_shift | `AC1 = AC0 >>> #1` | `AC1 = AC0 <<< #-1` | asm55 E0003: `>>>` needs the same register on both sides; the two-register logical shift is `<<< #-k` |
| semantics::standalone_conditional_execute_skips_the_next_instruction | `TC1 = #0` | `bit(ST0, #13) = #0` | no `TC1 = #0` spelling in asm55; dis55 prints the BCLR as bit(ST0, #13) |
| semantics::goto_out_of_a_loop_ends_it | 1-instruction localrepeat body | body `T1 = T1 + #1`, `nop` | asm55 E0800: localrepeat body under the minimum size |
| semantics::goto_out_of_a_loop_ends_it | `if (TC1) goto` out of a localrepeat | `.byte 0x61, 0x64` (its encoding, assembled outside the loop at the same address) | asm55 E0800 refuses a branch out of a localrepeat; real listings have it (mtp_proplist_count). dis55 lists the bytes back as the branch, so the listing stays tool output and no text-only fixture is needed |
| acov::{count_below_minus_32_sets_the_destination_flag, clamped_count_result_is_confined_to_the_working_width} | `ACy = AC0 >> T0` with T0 = -33 | `ACy = AC0 << T0` | there is no `>> Tx` form: the shift by Tx is `<< Tx`, a negative count shifts right (the case the comment describes) |
| acov::shift_m40::*, acov_mac::* | `AC0 = #0x10000`, `AC0 = #0x7FFFFFFF`, `AC0 = #-0x7FFFFFFF` | the test presets AC0 | asm55 E0003: constant out of range (no 32-bit load) |
| acov::step_and_run_agree_on_immediate_shifts, acov_mac::step_and_run_agree_on_mac | `ACy = ACx >> #4` | `ACy = ACx << #-4` | asm55 E0003: `>> #k` needs the same register on both sides |
| acov_mac::m40_off_negative_overflow_saturates | `AC0 = AC0 - (T1 * #0x7FFF)` | `AC0 = AC0 - (T1 * *AR3)`, *AR3 = 0x7FFF | asm55: no MAS with a constant |
| long_words::{memory_load_both_parities_and_postincrement, mmr_load_both_parities} | `AC2 = uns(dbl(...))` | `bit(ST1, #8) = #0`, then `AC2 = dbl(...)` | asm55 has no uns(dbl()) load; SXMD = 0 gives the same zero-extended long load |
| oracle_v2::k16_constant::*, k8_constant::* | `#0xffffa99a`, `#0xffffe200`, `#0xffffff80`, `#0xffffffad`, `#0x8000` (dis55 -e prints) | `#-22118`, `#-7680`, `#-128`, `#-83`, `#-32768` | the runtime listing prints negative constants as such; same encodings |
| oracle_v2::k8_constant::mpyk_k8_negative | `AC1 = #0x10000` | the test presets AC1 | asm55 E0003: constant out of range |
| lifecycle::* (test_c55x_emu_gc) | weakref / gc.collect | build, touch all 16 MiB, drop; RSS must not grow (freed memory reused) | Rust frees on drop; the allocator may keep freed memory for reuse, so return to the OS is not required |
| addressing::m40::* | `TC1 = AC0 == #0` | `TC1 = AC0 == AC1` (AC1 = 0 at that point) | asm55 has no compare with a constant into TCx |
| addressing::carry::product_leaves_carry | `AC0 = AC0 + (T0 * T1)` | `AC0 = AC0 + (AC1 * T1)` | asm55 has no Tx * Ty MAC; the test only needs a MAC after the borrow |
| addressing::carry::_64_bit_add / _64_bit_subtract | test_64_bit_add / test_64_bit_subtract | leading underscore | a Rust name cannot start with a digit |
| dsp::address_and_shift::port_qualifier_covers_read_modify_write | port 0x1234 hooked from Python; `*AR3 = *AR3 \| #1 \|\| writeport()` | port 0x0c04 (DMA channel 0 CSSA_L, a core register) set by the program; the instruction followed by `.byte 0x9a` (the writeport() qualifier) | no Python hooks in Rust (rewrite class); asm55 E5369 refuses the qualified read-modify-write (a silicon exception) though compiled code has it, and dis55 lists the bytes back as the bundle |
| manual_review::status_flag::{acov_is_part_of_st0, bclr_acov0_clears_the_flag} | `AC0 = #0x7FFFFFFF` | the test presets AC0 | asm55 E0003: constant out of range |
| containment::inside::* | Python host entries (PyCore enter() calls guard_entry) | harness helpers call guard::guard_entry() before Core::run / step / call / ret / advance | the core entries do not check the guard themselves yet (asked: guard_entry inside Core); test_trace_length_cap_is_an_explicit_error stays Python until the cap moves into the core |
| containment::outside::* | RejectedOutside (skipped in a job) | #[ignore], run by hand outside the launcher (`--ignored outside`) | they must run without the launcher |

## 8. Mutation spot checks

Each mutation was applied to the core in the worktree (not committed). The Rust tests
were run with scripts/cargo-test-contained, then the file was restored and touched, so
cargo rebuilds. Every mutation was caught:

| Mutation | Failing Rust tests |
|---|---|
| mem.rs read32 pairs `word & !1` / `word \| 1` instead of the addressed word and `word ^ 1` | long_words::memory_load_both_parities_and_postincrement |
| parts.rs: SUBC does not set the ACOV of its destination | manual_review::status_flag::subc_reports_overflow_without_saturation |
| parts.rs: ADD K16 to memory does not set ACOV0 | manual_review::status_flag::add_k16_to_memory_saturates_and_reports_acov0, ::add_k16_negative_constant_saturates_to_8000h |
| frontend ir.rs: the first statement of a dual multiply keeps its coef() pointer update | dsp::dual_multiply::shared_coefficient_read_once, ::fractional_mode_doubles_products, and the frontend tests builds_like_ir_py, dual::split_indexed_coefficient, synthetic::synthetic_listing, synthetic::fuzz |

Trap: restoring a file with `mv` keeps an mtime older than the mutated build, and cargo
then keeps the mutated binary. Touch the file after restoring it.

## 9. Status at ea461a1

The Python files left in tests/:
- test_c55x_emu_containment.py: the trace cap test, until Core::set_trace carries the cap.
- test_c55x_emu_resource_caps.py: BusCaps, ImageCaps and CodeCacheCap, until the core exposes
  its load bounds and caps.
- test_c55x_launcher.py: stays Python by the owner exception.
- test_c55x_run.py: tests the CLI, not covered by this migration's mapping.

The 50 board tests went in 69f2583. The McBSP tests are tests/mcbsp.rs, and
test_c55x_emu_audio.py is deleted.
