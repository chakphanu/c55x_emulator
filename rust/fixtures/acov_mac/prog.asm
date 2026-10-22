; Programs of rust/c55x_core/tests/acov_mac.rs (from tests/test_c55x_emu_acov_mac.py).
; Each test starts at t_NAME and ends in the idle loop e_NAME (a goto to itself).
; Accumulator values asm55 cannot load in one instruction are preset by the test.
	.mmregs
	.cpl_on
	.arms_on
	.c54cm_off
	.text

t_m40_off_accumulate_overflow_1:
	T1 = #0x7FFF
	AC0 = AC0 + (T1 * #0x7FFF)
e_m40_off_accumulate_overflow_1:
	goto e_m40_off_accumulate_overflow_1

t_m40_off_accumulate_overflow_2:
	bit(ST1, #9) = #1
	T1 = #0x7FFF
	AC0 = AC0 + (T1 * #0x7FFF)
e_m40_off_accumulate_overflow_2:
	goto e_m40_off_accumulate_overflow_2

t_m40_off_negative_overflow_saturates:
	bit(ST1, #9) = #1
	T1 = #0x7FFF
	XAR3 = mar(*(#01600h))
	AC0 = AC0 - (T1 * *AR3)
e_m40_off_negative_overflow_saturates:
	goto e_m40_off_negative_overflow_saturates

t_m40_on_accumulate_uses_40_bits:
	bit(ST1, #10) = #1
	T1 = #0x7FFF
	AC0 = AC0 + (T1 * #0x7FFF)
e_m40_on_accumulate_uses_40_bits:
	goto e_m40_on_accumulate_uses_40_bits

t_in_range_product_and_sum_report_nothing:
	T1 = #0x40
	AC0 = #0x1000
	AC0 = AC0 + (T1 * #0x40)
e_in_range_product_and_sum_report_nothing:
	goto e_in_range_product_and_sum_report_nothing

t_smul_reports_the_multiplier_overflow:
	bit(ST1, #6) = #1
	bit(ST1, #9) = #1
	bit(ST3, #1) = #1
	T1 = #0x8000
	AC0 = AC1 + (T1 * #-0x8000)
e_smul_reports_the_multiplier_overflow:
	goto e_smul_reports_the_multiplier_overflow

t_unsigned_product_reports_nothing:
	XAR3 = mar(*(#01600h))
	T1 = #0xFFFF
	AC0 = uns(T1 * *AR3)
e_unsigned_product_reports_nothing:
	goto e_unsigned_product_reports_nothing

t_step_and_run_agree_on_mac:
	T1 = #0x7FFF
	AC0 = AC0 + (T1 * #0x7FFF)
	AC1 = AC0 << #-4
e_step_and_run_agree_on_mac:
	goto e_step_and_run_agree_on_mac
