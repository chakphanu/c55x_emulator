; Programs of rust/c55x_core/tests/irq.rs (from tests/test_c55x_emu_irq.py).
; Each test starts at t_NAME and ends in the idle loop e_NAME (a goto to itself).
	.mmregs
	.cpl_on
	.arms_on
	.c54cm_off
	.text
; t_isr4 / t_isr16 are interrupt service routines: their e_ loops are never reached.

t_main:
	T0 = #1
	T1 = #2
e_main:
	goto e_main

t_isr4:
	T2 = #7
	return_int
e_isr4:
	goto e_isr4

t_isr16:
	T3 = #9
	return_int
e_isr16:
	goto e_isr16
