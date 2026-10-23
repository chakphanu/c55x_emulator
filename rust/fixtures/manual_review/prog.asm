; Programs of rust/c55x_core/tests/manual_review.rs (from tests/test_c55x_emu_manual_review.py).
; Each test starts at t_NAME and ends in the idle loop e_NAME (a goto to itself).
	.mmregs
	.cpl_on
	.arms_on
	.c54cm_off
	.text
; AC0 = #0x7FFFFFFF of the Python status tests is preset by the Rust tests (no 32-bit load).

t_neg_left_shift:
	AC0 = #-4
	AC0 = AC0 << #1
e_neg_left_shift:
	goto e_neg_left_shift

t_neg_left_shift_satd:
	bit(ST1, #9) = #1
	AC0 = #-4
	AC0 = AC0 << #1
e_neg_left_shift_satd:
	goto e_neg_left_shift_satd

t_shift_by_zero:
	AC1 = AC0 << #0
e_shift_by_zero:
	goto e_shift_by_zero

t_shift_by_zero_satd:
	bit(ST1, #9) = #1
	AC1 = AC0 << #0
e_shift_by_zero_satd:
	goto e_shift_by_zero_satd

t_positive_tx_count:
	T0 = #3
	AC0 = #-4
	AC1 = AC0 << T0
e_positive_tx_count:
	goto e_positive_tx_count

t_add_shifted:
	AC0 = #-4
	AC1 = #100
	AC1 = AC1 + (AC0 << #2)
e_add_shifted:
	goto e_add_shifted

t_add_shifted_satd:
	bit(ST1, #9) = #1
	AC0 = #-4
	AC1 = #100
	AC1 = AC1 + (AC0 << #2)
e_add_shifted_satd:
	goto e_add_shifted_satd

t_dual_mac:
	XAR1 = mar(*(#01600h))
	XAR2 = mar(*(#01601h))
	XCDP = mar(*(#01602h))
	AC0 = AC0 + (*AR1 * coef(*CDP)), AC1 = AC1 + (*AR2 * coef(*CDP))
e_dual_mac:
	goto e_dual_mac

t_store_bundle_t2_2:
	XAR0 = mar(*(#01600h))
	XAR1 = mar(*(#01610h))
	T2 = #2
	AC3 = AC1 + (*AR0 << #16) || *AR1 = HI(AC2 << T2)
e_store_bundle_t2_2:
	goto e_store_bundle_t2_2

t_store_bundle_t2_0:
	XAR0 = mar(*(#01600h))
	XAR1 = mar(*(#01610h))
	T2 = #0
	AC3 = AC1 + (*AR0 << #16) || *AR1 = HI(AC2 << T2)
e_store_bundle_t2_0:
	goto e_store_bundle_t2_0

t_store_right:
	XAR0 = mar(*(#01600h))
	T1 = #1
	*AR0 = HI(AC0 << #-8)
e_store_right:
	goto e_store_right

t_store_satd:
	bit(ST1, #9) = #1
	XAR0 = mar(*(#01600h))
	T1 = #1
	*AR0 = HI(AC0 << #2)
e_store_satd:
	goto e_store_satd

t_store_saturate:
	XAR0 = mar(*(#01600h))
	T1 = #1
	*AR0 = HI(saturate(AC1 << T1))
e_store_saturate:
	goto e_store_saturate

t_smem_and_accumulator_forms_agree:
	bit(ST1, #6) = #1
	T1 = #-32768
	XAR1 = mar(*(#01600h))
	AC0 = T1 * *AR1
	AC1 = AC1 * T1
e_smem_and_accumulator_forms_agree:
	goto e_smem_and_accumulator_forms_agree

t_unsigned_mac_example:
	XAR1 = mar(*(#01600h))
	XAR3 = mar(*(#01602h))
	AC0 = (AC0 >> #16) + (uns(*AR1) * uns(*AR3))
e_unsigned_mac_example:
	goto e_unsigned_mac_example

t_sx_mpy:
	bit(ST1, #8) = #0
	XAR0 = mar(*(#01600h))
	XAR1 = mar(*(#01600h))
	XAR2 = mar(*(#01601h))
	AC1 = T1 * *AR1
e_sx_mpy:
	goto e_sx_mpy

t_sx_mac:
	bit(ST1, #8) = #0
	XAR0 = mar(*(#01600h))
	XAR1 = mar(*(#01600h))
	XAR2 = mar(*(#01601h))
	AC0 = AC0 + (T1 * *AR1)
e_sx_mac:
	goto e_sx_mac

t_sx_uns:
	bit(ST1, #8) = #0
	XAR0 = mar(*(#01600h))
	XAR1 = mar(*(#01600h))
	XAR2 = mar(*(#01601h))
	AC1 = uns(*AR1) * uns(*AR2)
e_sx_uns:
	goto e_sx_uns

t_sx_t_low:
	bit(ST1, #8) = #0
	XAR0 = mar(*(#01600h))
	XAR1 = mar(*(#01600h))
	XAR2 = mar(*(#01601h))
	T0 = low_byte(*AR0)
e_sx_t_low:
	goto e_sx_t_low

t_sx_ar_high:
	bit(ST1, #8) = #0
	XAR0 = mar(*(#01600h))
	XAR1 = mar(*(#01600h))
	XAR2 = mar(*(#01601h))
	AR2 = high_byte(*AR0)
e_sx_ar_high:
	goto e_sx_ar_high

t_sx_t_uns_low:
	bit(ST1, #8) = #0
	XAR0 = mar(*(#01600h))
	XAR1 = mar(*(#01600h))
	XAR2 = mar(*(#01601h))
	T0 = uns(low_byte(*AR0))
e_sx_t_uns_low:
	goto e_sx_t_uns_low

t_sx_ac_low:
	bit(ST1, #8) = #0
	XAR0 = mar(*(#01600h))
	XAR1 = mar(*(#01600h))
	XAR2 = mar(*(#01601h))
	AC0 = low_byte(*AR0)
e_sx_ac_low:
	goto e_sx_ac_low

t_acov_is_part_of_st0:
	AC0 = AC0 + #1
	.cpl_off
	T0 = @ST0_L || mmap()
	.cpl_on
e_acov_is_part_of_st0:
	goto e_acov_is_part_of_st0

t_bclr_acov0_clears_the_flag:
	AC0 = AC0 + #1
	bit(ST0, #10) = #0
e_bclr_acov0_clears_the_flag:
	goto e_bclr_acov0_clears_the_flag

t_accumulator_move:
	AC1 = AC0
e_accumulator_move:
	goto e_accumulator_move

t_sat_reports_overflow:
	AC1 = saturate(AC0)
e_sat_reports_overflow:
	goto e_sat_reports_overflow

t_satr_clears_the_low_word_after_saturation:
	AC1 = saturate(rnd(AC0))
e_satr_clears_the_low_word_after_saturation:
	goto e_satr_clears_the_low_word_after_saturation

t_subc_reports_overflow_without_saturation:
	bit(ST1, #8) = #0
	XAR1 = mar(*(#0300h))
	subc(*AR1, AC0, AC1)
e_subc_reports_overflow_without_saturation:
	goto e_subc_reports_overflow_without_saturation

t_add_k16_to_memory_saturates_and_reports_acov0:
	bit(ST1, #9) = #1
	XAR0 = mar(*(#01600h))
	*AR0 = *AR0 + #1
e_add_k16_to_memory_saturates_and_reports_acov0:
	goto e_add_k16_to_memory_saturates_and_reports_acov0

t_add_k16_carry_and_one_address_update:
	XAR0 = mar(*(#01600h))
	*AR0+ = *AR0+ + #1
e_add_k16_carry_and_one_address_update:
	goto e_add_k16_carry_and_one_address_update

t_add_k16_negative_constant_saturates_to_8000h:
	bit(ST1, #9) = #1
	XAR0 = mar(*(#01600h))
	*AR0 = *AR0 - #1
e_add_k16_negative_constant_saturates_to_8000h:
	goto e_add_k16_negative_constant_saturates_to_8000h

t_accumulator_move_saturates_and_sets_st0_bit_9:
	bit(ST1, #9) = #1
	AC1 = AC0
	.cpl_off
	T0 = @ST0_L || mmap()
	.cpl_on
e_accumulator_move_saturates_and_sets_st0_bit_9:
	goto e_accumulator_move_saturates_and_sets_st0_bit_9

t_accumulator_move_with_m40_does_not_overflow:
	bit(ST1, #10) = #1
	AC1 = AC0
e_accumulator_move_with_m40_does_not_overflow:
	goto e_accumulator_move_with_m40_does_not_overflow

t_repeated_subc_example:
	XAR1 = mar(*(#0200h))
	CSR = #1
	repeat(CSR)
	subc(*AR1, AC1, AC1)
e_repeated_subc_example:
	goto e_repeated_subc_example

t_count_tc1_0:
	bit(ST0, #13) = #0
	T1 = count(AC1, AC2, TC1)
e_count_tc1_0:
	goto e_count_tc1_0

t_count_tc1_1:
	bit(ST0, #13) = #1
	T1 = count(AC1, AC2, TC1)
e_count_tc1_1:
	goto e_count_tc1_1

t_dual_add_even:
	XAR0 = mar(*(#01600h))
	HI(AC1) = HI(*AR0) + HI(AC0), LO(AC1) = LO(*AR0) + LO(AC0)
e_dual_add_even:
	goto e_dual_add_even

t_dual_add_odd:
	XAR0 = mar(*(#01601h))
	HI(AC1) = HI(*AR0) + HI(AC0), LO(AC1) = LO(*AR0) + LO(AC0)
e_dual_add_odd:
	goto e_dual_add_odd

t_dual_add_satd_0:
	bit(ST1, #9) = #0
	XAR0 = mar(*(#01600h))
	HI(AC1) = HI(*AR0) + HI(AC0), LO(AC1) = LO(*AR0) + LO(AC0)
e_dual_add_satd_0:
	goto e_dual_add_satd_0

t_dual_add_satd_1:
	bit(ST1, #9) = #1
	XAR0 = mar(*(#01600h))
	HI(AC1) = HI(*AR0) + HI(AC0), LO(AC1) = LO(*AR0) + LO(AC0)
e_dual_add_satd_1:
	goto e_dual_add_satd_1

t_dual_sub:
	XAR0 = mar(*(#01600h))
	HI(AC1) = HI(AC0) - HI(*AR0), LO(AC1) = LO(AC0) - LO(*AR0)
e_dual_sub:
	goto e_dual_sub

t_rptcc_tc1_1:
	XAR1 = mar(*(#0200h))
	bit(ST0, #13) = #1
	while (TC1 && (RPTC < #3)) repeat
	subc(*AR1, AC1, AC1)
	T0 = RPTC
e_rptcc_tc1_1:
	goto e_rptcc_tc1_1

t_rptcc_tc1_0:
	XAR1 = mar(*(#0200h))
	bit(ST0, #13) = #0
	while (TC1 && (RPTC < #3)) repeat
	subc(*AR1, AC1, AC1)
	T0 = RPTC
e_rptcc_tc1_0:
	goto e_rptcc_tc1_0

t_rptcc_not_tc1_0:
	XAR1 = mar(*(#0200h))
	bit(ST0, #13) = #0
	while (!TC1 && (RPTC < #3)) repeat
	subc(*AR1, AC1, AC1)
	T0 = RPTC
e_rptcc_not_tc1_0:
	goto e_rptcc_not_tc1_0

t_xcc_ar0_3000:
	bit(ST1, #8) = #0
	XAR0 = mar(*(#03000h))
	XAR2 = mar(*(#0405h))
	if (AR0 != #0) execute(D_Unit)
	AC0 = AC0 + *AR2+
e_xcc_ar0_3000:
	goto e_xcc_ar0_3000

t_xcc_ar0_0:
	bit(ST1, #8) = #0
	XAR0 = mar(*(#00h))
	XAR2 = mar(*(#0405h))
	if (AR0 != #0) execute(D_Unit)
	AC0 = AC0 + *AR2+
e_xcc_ar0_0:
	goto e_xcc_ar0_0

t_xcc_partner_tc2_0:
	bit(ST0, #12) = #0
	XAR2 = mar(*(#01600h))
	T1 = #5 || if (TC2) execute(D_Unit)
	AC0 = AC0 + *AR2+
e_xcc_partner_tc2_0:
	goto e_xcc_partner_tc2_0

t_xcc_partner_tc2_1:
	bit(ST0, #12) = #1
	XAR2 = mar(*(#01600h))
	T1 = #5 || if (TC2) execute(D_Unit)
	AC0 = AC0 + *AR2+
e_xcc_partner_tc2_1:
	goto e_xcc_partner_tc2_1

t_xcc_store_tc1_0:
	bit(ST0, #13) = #0
	XAR0 = mar(*(#01600h))
	T0 = #9
	if (TC1) execute(D_Unit) || *AR0+ = T0
e_xcc_store_tc1_0:
	goto e_xcc_store_tc1_0

t_xcc_store_tc1_1:
	bit(ST0, #13) = #1
	XAR0 = mar(*(#01600h))
	T0 = #9
	if (TC1) execute(D_Unit) || *AR0+ = T0
e_xcc_store_tc1_1:
	goto e_xcc_store_tc1_1

t_sqdst:
	XAR0 = mar(*(#01600h))
	XAR1 = mar(*(#01601h))
	sqdst(*AR0,*AR1,AC0,AC1)
e_sqdst:
	goto e_sqdst

; XCCPART with a pointer partner that reads its Smem (SUB src, Smem, dst into T1); the
; tests set TC1, AR0, AR2, ST3 and the memory word
t_xcc_sub:
	if (TC1) execute(D_Unit) || T1 = *AR0- - AR2
e_xcc_sub:
	goto e_xcc_sub
