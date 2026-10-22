; Programs of rust/c55x_core/tests/containment.rs (from tests/test_c55x_emu_containment.py).
; Each test starts at t_NAME and ends in the idle loop e_NAME (a goto to itself).
	.mmregs
	.cpl_on
	.arms_on
	.c54cm_off
	.text

t_core_constructs_and_runs:
	AC0 = #0x100
	AC0 = AC0 << #16
e_core_constructs_and_runs:
	goto e_core_constructs_and_runs

t_two_steps:
	AC0 = #1
	AC0 = AC0 << #1
e_two_steps:
	goto e_two_steps

t_nop:
	nop
e_nop:
	goto e_nop
