; Programs of rust/c55x_core/tests/addressing.rs (from tests/test_c55x_emu_addressing.py).
; Each test starts at t_NAME and ends in the idle loop e_NAME (a goto to itself).
	.mmregs
	.cpl_on
	.arms_on
	.c54cm_off
	.text

t_index_wraps_in_the_buffer:
	XAR2 = mar(*(#010003h))
	T0 = #-2
	bit(ST2, #2) = #1
	AC0 = *AR2+
	AC1 = *AR2(#0002h)
	AC2 = *+AR2(#-0001h)
	AC3 = *AR2(T0)
e_index_wraps_in_the_buffer:
	goto e_index_wraps_in_the_buffer

t_linear_pointer_ignores_the_buffer:
	XAR3 = mar(*(#01600h))
	AC0 = *AR3+
e_linear_pointer_ignores_the_buffer:
	goto e_linear_pointer_ignores_the_buffer

t_zero_buffer_size_raises:
	bit(ST2, #3) = #1
	AC0 = *AR3
e_zero_buffer_size_raises:
	goto e_zero_buffer_size_raises

t_dsp_mode_uses_t1:
	XAR1 = mar(*(#01600h))
	T1 = #2
	bit(ST2, #15) = #0
	AC0 = *AR1(short(#3))
	AC1 = *AR1(short(#1))
	bit(ST2, #15) = #1
	AC2 = *AR1(short(#3))
e_dsp_mode_uses_t1:
	goto e_dsp_mode_uses_t1

t_mmr_block_save_and_restore:
	BK47 = #256
	BSA45 = #99
	XAR1 = mar(*(#0002eh))
	repeat(#10)
	push(dbl(*AR1+))
	BK47 = #0
	BSA45 = #0
	XAR1 = mar(*(#00042h))
	repeat(#10)
	dbl(*AR1-) = pop()
e_mmr_block_save_and_restore:
	goto e_mmr_block_save_and_restore

t_long_push_order:
	XAR1 = mar(*(#01600h))
	push(dbl(*AR1))
e_long_push_order:
	goto e_long_push_order

t_m40_off_keeps_32_bit_semantics:
	AC0 = #1 << #16
	AC0 = AC0 << #16
	TC1 = AC0 == AC1
e_m40_off_keeps_32_bit_semantics:
	goto e_m40_off_keeps_32_bit_semantics

t_m40_on_uses_the_guard_bits:
	bit(ST1, #10) = #1
	bit(ST1, #8) = #0
	AC0 = #1 << #16
	AC0 = AC0 << #16
	TC1 = AC0 == AC1
	AC1 = AC0 <<< #-16
	AC2 = AC0 << #-16
e_m40_on_uses_the_guard_bits:
	goto e_m40_on_uses_the_guard_bits

t_sign_extended_low_byte:
	XAR3 = mar(*(#01600h))
	AC1 = *AR3
	AC1 = AC1 << #24
	AC1 = AC1 << #-24
	AC2 = *AR3(#0001h)
	AC2 = AC2 << #24
	AC2 = AC2 << #-24
e_sign_extended_low_byte:
	goto e_sign_extended_low_byte

t_long_add:
	bit(ST1, #8) = #0
	XAR1 = mar(*(#01600h))
	XAR2 = mar(*(#01610h))
	XAR3 = mar(*(#01620h))
	AC0 = dbl(*AR1+)
	AC0 = AC0 + dbl(*AR2+)
	dbl(*AR3+) = AC0
	AC0 = dbl(*AR1)
	AC0 = AC0 + *AR2(#0001h) + CARRY
	AC0 = AC0 + (*AR2 << #16)
	dbl(*AR3) = AC0
e_long_add:
	goto e_long_add

t_long_subtract:
	bit(ST1, #8) = #0
	XAR1 = mar(*(#01600h))
	XAR2 = mar(*(#01610h))
	XAR3 = mar(*(#01620h))
	AC0 = dbl(*AR1+)
	AC0 = AC0 - dbl(*AR2+)
	dbl(*AR3+) = AC0
	AC0 = dbl(*AR1)
	AC0 = AC0 - *AR2(#0001h) - BORROW
	AC0 = AC0 - (*AR2 << #16)
	dbl(*AR3) = AC0
e_long_subtract:
	goto e_long_subtract

t_product_leaves_carry:
	AC0 = #0
	AC0 = AC0 - #1
	AC1 = #1
	AC0 = AC0 + (AC1 * T1)
e_product_leaves_carry:
	goto e_product_leaves_carry

t_band_sets_tc_on_any_bit:
	XAR1 = mar(*(#01600h))
	TC1 = *AR1 & #7
	TC2 = *AR1 & #8
e_band_sets_tc_on_any_bit:
	goto e_band_sets_tc_on_any_bit

t_mmr_load_sign_extends_like_memory:
	T0 = #-2
	.cpl_off
	AC0 = @T0_L || mmap()
	.cpl_on
	AR1 = #-2
	.cpl_off
	AC1 = uns(@AR1_L) || mmap()
	.cpl_on
e_mmr_load_sign_extends_like_memory:
	goto e_mmr_load_sign_extends_like_memory
