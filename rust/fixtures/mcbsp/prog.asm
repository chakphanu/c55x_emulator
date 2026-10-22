; Programs of rust/c55x_core/tests/mcbsp.rs (from tests/test_c55x_emu_audio.py).
; Each test starts at t_NAME and ends in the idle loop e_NAME (a goto to itself).
	.mmregs
	.cpl_on
	.arms_on
	.c54cm_off
	.text

t_stream:
	bit(ST1, #11) = #0
	*port(#0x2809) = #320
	*port(#0x2808) = #0
	*port(#0x2812) = #16387
	*port(#0x2804) = #256
	*port(#0x0c00) = #1537
	*port(#0x0c02) = #32
	*port(#0x0c04) = #24576
	*port(#0x0c06) = #20486
	*port(#0x0c08) = #2
	*port(#0x0c09) = #8
	*port(#0x0c01) = #4994
	*port(#0x2804) = #257
e_stream:
	goto e_stream

t_no_clock_no_samples:
	bit(ST1, #11) = #0
	*port(#0x2809) = #320
	*port(#0x2812) = #16387
	*port(#0x2804) = #1
e_no_clock_no_samples:
	goto e_no_clock_no_samples
