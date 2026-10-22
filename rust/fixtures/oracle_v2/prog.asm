; Programs of rust/c55x_core/tests/oracle_v2.rs (from tests/test_c55x_emu_oracle_v2.py).
; Each test starts at t_NAME and ends in the idle loop e_NAME (a goto to itself).
	.mmregs
	.cpl_on
	.arms_on
	.c54cm_off
	.text

t_mpyk_negative_print_sign_extends:
	AC0 = #0x100 << #16
	AC0 = AC0 * #-22118
e_mpyk_negative_print_sign_extends:
	goto e_mpyk_negative_print_sign_extends

t_k16_constant_sign_extends:
	T1 = #2
	AC0 = AC1 + (T1 * #-32768)
e_k16_constant_sign_extends:
	goto e_k16_constant_sign_extends

t_positive_k16_unchanged:
	T1 = #3
	AC0 = AC1 + (T1 * #0x7FFF)
e_positive_k16_unchanged:
	goto e_positive_k16_unchanged

t_pre_negated_constant_unchanged:
	T1 = #2
	AC0 = AC1 + (T1 * #-7680)
e_pre_negated_constant_unchanged:
	goto e_pre_negated_constant_unchanged

t_fill:
	T1 = #4
	AR1 = T1 - #1
	CSR = AR1
r_fill:
	XAR3 = mar(*AR0) || repeat(CSR)
b_fill:
	*AR3+ = T0
e_fill:
	goto e_fill

t_mpyk_k8_negative:
	AC0 = AC1 * #-128
e_mpyk_k8_negative:
	goto e_mpyk_k8_negative

t_macmk_smem_k8_negative:
	XAR3 = mar(*(#01600h))
	AC2 = *AR3 * #-83
e_macmk_smem_k8_negative:
	goto e_macmk_smem_k8_negative

t_positive_k8_unchanged:
	XAR3 = mar(*(#01600h))
	AC2 = *AR3 * #0x53
e_positive_k8_unchanged:
	goto e_positive_k8_unchanged
