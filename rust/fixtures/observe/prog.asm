; Routines of rust/c55x_run/tests/observe.rs (opt-in observers of c55x-run): stores in
; source order, a repeated store, conditional execution and a nested call. A call
; returns to the script's sentinel.
	.mmregs
	.cpl_on
	.arms_on
	.c54cm_off
	.text

stores:
	*AR0 = T0
	*AR0 = T0
	*AR1 = T1
	dbl(*AR2) = AC0
	*AR3 = T0 || AC1 = #1
	return

rpt_stores:
	repeat(#2)
	*AR0+ = T0
	return

xcc:
	if (TC1) execute(AD_Unit)
	*AR0 = T0
	if (TC2) execute(D_Unit) || *AR1+ = T1
	return

outer:
	*AR0 = T0
	call inner
	*AR1 = T0
	return

inner:
	*AR2 = T1
	return
