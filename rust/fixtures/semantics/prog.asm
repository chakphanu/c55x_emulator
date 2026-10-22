; Programs of rust/c55x_core/tests/semantics.rs (from tests/test_c55x_emu.py
; SemanticTests and tests/test_c55x_emu_audio.py HiLoadTests). Each test starts at
; t_NAME and ends in the idle loop e_NAME (a goto to itself).
	.mmregs
	.cpl_on
	.arms_on
	.c54cm_off
	.text

t_sign_and_zero_extension:
	XAR1 = mar(*(#01600h))
	AC0 = *AR1
	AC1 = uns(*AR1)
e_sign_and_zero_extension:
	goto e_sign_and_zero_extension

t_long_constant_and_logical_shift:
	AC0 = #-1 << #16
	AC0 = AC0 | #65535
	AC1 = AC0 <<< #-1
e_long_constant_and_logical_shift:
	goto e_long_constant_and_logical_shift

t_post_increment_of_long_access_steps_two_words:
	XAR2 = mar(*(#01600h))
	AC0 = dbl(*AR2+)
e_post_increment_of_long_access_steps_two_words:
	goto e_post_increment_of_long_access_steps_two_words

t_ar_write_keeps_the_high_part:
	XAR3 = mar(*(#012345h))
	AR3 = #5
e_ar_write_keeps_the_high_part:
	goto e_ar_write_keeps_the_high_part

t_dual_push_pop_order:
	T2 = #2
	T3 = #3
	push(T3,T2)
	T0,T1 = pop()
e_dual_push_pop_order:
	goto e_dual_push_pop_order

t_call_and_return_restore_the_stacks:
	call s_call_and_return_restore_the_stacks
	T0 = #1
	goto e_call_and_return_restore_the_stacks
s_call_and_return_restore_the_stacks:
	T1 = #7
	return
e_call_and_return_restore_the_stacks:
	goto e_call_and_return_restore_the_stacks

t_repeat_and_localrepeat_counts:
	T0 = #0
	repeat(#7)
	T0 = T0 + #1
	BRC0 = #3
	T1 = #0
	localrepeat {
	T1 = T1 + #1
	T1 = T1 + #1
	}
e_repeat_and_localrepeat_counts:
	goto e_repeat_and_localrepeat_counts

t_goto_out_of_a_loop_ends_it:
	BRC0 = #9
	T0 = #0
	AR1 = #3
	localrepeat {
	T0 = T0 + #1
	TC1 = T0 == AR1
	; if (TC1) goto x_goto_out_of_a_loop_ends_it: real listings have this branch out of
	; a localrepeat, asm55 refuses it (E0800), so its encoding (assembled outside the
	; loop at the same address) is given as data
	.byte 0x61, 0x64
	T2 = T2 + #1
	}
x_goto_out_of_a_loop_ends_it:
	BRC0 = #4
	T1 = #0
	localrepeat {
	T1 = T1 + #1
	nop
	}
e_goto_out_of_a_loop_ends_it:
	goto e_goto_out_of_a_loop_ends_it

t_conditional_execute:
	T0 = #12
	AR1 = #12
	TC1 = T0 == AR1
	if (TC1) execute(D_Unit) || T1 = #128
	TC1 = T0 != AR1
	if (TC1) execute(D_Unit) || T2 = #5
e_conditional_execute:
	goto e_conditional_execute

t_parallel_parts_read_before_writing:
	AC0 = #5
	AC1 = #1
	AC0 = #1 << #16 || AC1 = AC1 + AC0
e_parallel_parts_read_before_writing:
	goto e_parallel_parts_read_before_writing

t_parallel_mar_runs_in_the_address_phase:
	XAR3 = mar(*(#01600h))
	XAR5 = mar(*(#01700h))
	dbl(*AR5) = XAR3 || mar(AR3 + #10)
e_parallel_mar_runs_in_the_address_phase:
	goto e_parallel_mar_runs_in_the_address_phase

t_parallel_mar_leaves_the_partner_address_alone:
	XAR5 = mar(*(#01600h))
	AR0 = *AR5(short(#2)) || mar(AR5 + #4)
e_parallel_mar_leaves_the_partner_address_alone:
	goto e_parallel_mar_leaves_the_partner_address_alone

t_standalone_conditional_execute_skips_the_next_instruction:
	bit(ST0, #13) = #0
	T0 = #1
	if (TC1) execute(AD_Unit)
	T0 = #2
	T1 = #3
e_standalone_conditional_execute_skips_the_next_instruction:
	goto e_standalone_conditional_execute_skips_the_next_instruction

t_multiply_uses_the_high_part_of_an_accumulator:
	AC0 = #3 << #16
	AC0 = AC0 | #5
	AC1 = AC0 * #12
e_multiply_uses_the_high_part_of_an_accumulator:
	goto e_multiply_uses_the_high_part_of_an_accumulator

t_long_result_keeps_its_width:
	XAR0 = mar(*(#01600h))
	AC0 = uns(*AR0+) << #8
e_long_result_keeps_its_width:
	goto e_long_result_keeps_its_width

t_field_extract:
	AC2 = #0x1234
	AC2 = field_extract(AC2, #65520)
e_field_extract:
	goto e_field_extract

t_hi_takes_the_sign_into_the_guard_bits:
	AC0 = #-5
	AR1 = #3
	HI(AC0) = AR1
	AC1 = #5
	AR2 = #-2
	HI(AC1) = AR2
e_hi_takes_the_sign_into_the_guard_bits:
	goto e_hi_takes_the_sign_into_the_guard_bits

	.cpl_off
t_unsupported_mode_raises:
	@ST1_L = @ST1_L | #32 || mmap()
e_unsupported_mode_raises:
	goto e_unsupported_mode_raises
