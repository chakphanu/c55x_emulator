; Routines of rust/c55x_run/tests/cli.rs (from tests/test_c55x_run.py), called by the
; call scripts of the tests; a call returns to the script's sentinel.
	.mmregs
	.cpl_on
	.arms_on
	.c54cm_off
	.text

add:
	AC0 = *AR0
	AC0 = AC0 + *AR1
	*AR2 = AC0
	return

read_port:
	AR3 = *port(#0x0001)
	return

spin:
	goto spin
