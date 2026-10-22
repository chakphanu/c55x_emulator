; Programs of rust/c55x_core/tests/sftl.rs (from tests/test_c55x_emu_sftl.py).
; Each test starts at t_NAME and ends in the idle loop e_NAME (a goto to itself).
	.mmregs
	.cpl_on
	.arms_on
	.c54cm_off
	.text

t_register_count:
	AC1 = AC0 <<< T0
e_register_count:
	goto e_register_count

t_right_by_one_clears_the_guards:
	AC3 = AC3 >>> #1
e_right_by_one_clears_the_guards:
	goto e_right_by_one_clears_the_guards

t_register_destination_shifts_16_bits:
	T2 = T2 >>> #1
e_register_destination_shifts_16_bits:
	goto e_register_destination_shifts_16_bits
