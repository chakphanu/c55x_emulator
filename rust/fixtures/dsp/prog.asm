; Programs of rust/c55x_core/tests/dsp.rs (from tests/test_c55x_emu_dsp.py).
; Each test starts at t_NAME and ends in the idle loop e_NAME (a goto to itself).
	.mmregs
	.cpl_on
	.arms_on
	.c54cm_off
	.text

t_mant_exp:
	AC1 = mant(AC0), T1 = exp(AC0)
e_mant_exp:
	goto e_mant_exp

t_dual:
	XAR0 = mar(*(#01600h))
	XAR1 = mar(*(#01610h))
	XCDP = mar(*(#01620h))
	AC0 = *AR0+ * coef(*CDP+), AC1 = *AR1+ * coef(*CDP+)
e_dual:
	goto e_dual

t_dual_frct:
	bit(ST1, #6) = #1
	XAR0 = mar(*(#01600h))
	XAR1 = mar(*(#01610h))
	XCDP = mar(*(#01620h))
	AC0 = *AR0+ * coef(*CDP+), AC1 = *AR1+ * coef(*CDP+)
e_dual_frct:
	goto e_dual_frct

t_add_one:
	AC0 = AC0 + #1
e_add_one:
	goto e_add_one

t_add_one_satd:
	bit(ST1, #9) = #1
	AC0 = AC0 + #1
e_add_one_satd:
	goto e_add_one_satd

t_add_one_satd_m40:
	bit(ST1, #9) = #1
	bit(ST1, #10) = #1
	AC0 = AC0 + #1
e_add_one_satd_m40:
	goto e_add_one_satd_m40

t_rnd_and_saturate:
	AC1 = saturate(rnd(AC0))
	AC3 = rnd(AC2)
e_rnd_and_saturate:
	goto e_rnd_and_saturate

t_after_partner_tc2_1:
	bit(ST0, #12) = #1
	T0 = #0
	T1 = #5 || if (TC2) execute(AD_Unit)
	T0 = #7
e_after_partner_tc2_1:
	goto e_after_partner_tc2_1

t_after_partner_tc2_0:
	bit(ST0, #12) = #0
	T0 = #0
	T1 = #5 || if (TC2) execute(AD_Unit)
	T0 = #7
e_after_partner_tc2_0:
	goto e_after_partner_tc2_0

t_d_unit_tc1_1:
	bit(ST0, #13) = #1
	XAR1 = mar(*(#01600h))
	AC0 = #0
	AC1 = #0
	if (TC1) execute(D_Unit)
	AC0 = #5
e_d_unit_tc1_1:
	goto e_d_unit_tc1_1

t_d_unit_tc1_0:
	bit(ST0, #13) = #0
	XAR1 = mar(*(#01600h))
	AC0 = #0
	AC1 = #0
	if (TC1) execute(D_Unit)
	AC0 = #5
e_d_unit_tc1_0:
	goto e_d_unit_tc1_0

t_d_unit_address_phase:
	bit(ST0, #13) = #0
	XAR1 = mar(*(#01600h))
	AC0 = #0
	AC1 = #0
	if (TC1) execute(D_Unit)
	mar(*AR1+)
	AC1 = AC1 + *AR1
e_d_unit_address_phase:
	goto e_d_unit_address_phase

t_mar_arithmetic_is_circular:
	XAR2 = mar(*(#010003h))
	bit(ST2, #2) = #1
	mar(AR2 + #2)
e_mar_arithmetic_is_circular:
	goto e_mar_arithmetic_is_circular

t_shift_count_in_tx_is_signed_without_sxmd:
	bit(ST1, #8) = #0
	XAR4 = mar(*(#01600h))
	T0 = #-4
	AC0 = AC0 + (*AR4(#0002h) << T0)
e_shift_count_in_tx_is_signed_without_sxmd:
	goto e_shift_count_in_tx_is_signed_without_sxmd

t_store_of_shifted_accumulator:
	XAR2 = mar(*(#01600h))
	*AR2 = LO(AC0 << #-16)
e_store_of_shifted_accumulator:
	goto e_store_of_shifted_accumulator

t_popboth_sign_extends:
	pshboth(AC3)
	pshboth(AC2)
	AC3 = #-1
	AC2 = #-1
	AC2 = popboth()
	AC3 = popboth()
e_popboth_sign_extends:
	goto e_popboth_sign_extends

t_port_qualifier_covers_read_modify_write:
	*port(#0x0c04) = #16
	XAR3 = mar(*(#00c04h))
; *AR3 = *AR3 | #1 || writeport(): asm55 refuses it (E5369, a silicon exception) though
; compiled code has it; the qualifier byte 0x9a follows the instruction as data
	*AR3 = *AR3 | #1
	.byte 0x9a
e_port_qualifier_covers_read_modify_write:
	goto e_port_qualifier_covers_read_modify_write

t_reserved_mmr_reads_zero:
	XAR3 = mar(*(#00004h))
	AC0 = dbl(*AR3)
e_reserved_mmr_reads_zero:
	goto e_reserved_mmr_reads_zero
