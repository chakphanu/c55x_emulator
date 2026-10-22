; Programs of rust/c55x_core/tests/acov.rs (from tests/test_c55x_emu_acov.py).
; Each test starts at t_NAME and ends in the idle loop e_NAME (a goto to itself).
; Accumulator values asm55 cannot load in one instruction are preset by the test.
	.mmregs
	.cpl_on
	.arms_on
	.c54cm_off
	.text

t_flags_are_clear_after_reset:
	AC0 = #0
e_flags_are_clear_after_reset:
	goto e_flags_are_clear_after_reset

t_in_range_count_5:
	T0 = #5
	AC1 = AC0 << T0
e_in_range_count_5:
	goto e_in_range_count_5

t_in_range_count_m4:
	T0 = #-4
	AC1 = AC0 << T0
e_in_range_count_m4:
	goto e_in_range_count_m4

t_count_above_31_sets_the_destination_flag:
	T0 = #40
	AC1 = AC0 << T0
	AC0 = #7
e_count_above_31_sets_the_destination_flag:
	goto e_count_above_31_sets_the_destination_flag

t_count_below_minus_32_sets_the_destination_flag:
	T0 = #-33
	AC2 = AC0 << T0
e_count_below_minus_32_sets_the_destination_flag:
	goto e_count_below_minus_32_sets_the_destination_flag

t_clamped_left:
	T0 = #40
	AC1 = AC0 << T0
e_clamped_left:
	goto e_clamped_left

t_clamped_right:
	T0 = #-33
	AC1 = AC0 << T0
e_clamped_right:
	goto e_clamped_right

t_m40_off_extracts_bits_shifted_past_31:
	AC0 = AC0 << #16
e_m40_off_extracts_bits_shifted_past_31:
	goto e_m40_off_extracts_bits_shifted_past_31

t_m40_off_with_satd_saturates_1:
	bit(ST1, #9) = #1
	AC0 = AC0 << #16
e_m40_off_with_satd_saturates_1:
	goto e_m40_off_with_satd_saturates_1

t_m40_off_with_satd_saturates_2:
	bit(ST1, #9) = #1
	AC0 = #-4
	AC0 = AC0 << #30
e_m40_off_with_satd_saturates_2:
	goto e_m40_off_with_satd_saturates_2

t_m40_on_keeps_the_guard_bits:
	bit(ST1, #10) = #1
	AC0 = AC0 << #16
e_m40_on_keeps_the_guard_bits:
	goto e_m40_on_keeps_the_guard_bits

t_step_and_run_agree_on_immediate_shifts:
	AC0 = #0x1234
	AC0 = AC0 << #8
	AC1 = AC0 << #24
	T1 = #4
	AC2 = AC0 << T1
	AC3 = AC0 << #-4
e_step_and_run_agree_on_immediate_shifts:
	goto e_step_and_run_agree_on_immediate_shifts
