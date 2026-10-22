; Programs of rust/c55x_core/tests/dma.rs (from tests/test_c55x_emu_audio.py).
; Each test starts at t_NAME and ends in the idle loop e_NAME (a goto to itself).
	.mmregs
	.cpl_on
	.arms_on
	.c54cm_off
	.text

t_block_copy_raises_the_channel_interrupt:
	*port(#0x0ca0) = #517
	*port(#0x0ca2) = #32
	*port(#0x0ca4) = #24576
	*port(#0x0ca5) = #0
	*port(#0x0ca6) = #32768
	*port(#0x0ca7) = #0
	*port(#0x0ca8) = #4
	*port(#0x0ca9) = #1
	*port(#0x0ca1) = #20608
	AR1 = *port(#0x0ca3)
	AR2 = *port(#0x0ca3)
	AR3 = *port(#0x0ca1)
e_block_copy_raises_the_channel_interrupt:
	goto e_block_copy_raises_the_channel_interrupt

t_double_index_interleaves_two_buffers:
	*port(#0x0ca0) = #517
	*port(#0x0ca4) = #24576
	*port(#0x0ca6) = #32768
	*port(#0x0ca8) = #2
	*port(#0x0ca9) = #4
	*port(#0x0cab) = #511
	*port(#0x0caa) = #65025
	*port(#0x0ca1) = #28800
e_double_index_interleaves_two_buffers:
	goto e_double_index_interleaves_two_buffers
