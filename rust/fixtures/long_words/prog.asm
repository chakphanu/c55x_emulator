; Programs of rust/c55x_core/tests/long_words.rs (from tests/test_c55x_emu_long_words.py).
; Each test starts at t_NAME and ends in the idle loop e_NAME (a goto to itself).
	.mmregs
	.cpl_on
	.arms_on
	.c54cm_off
	.text
; asm55 has no uns(dbl(...)) load: SXMD = 0 gives the zero-extended long load.

t_memory_load:
	bit(ST1, #8) = #0
	AC2 = dbl(*AR2+)
e_memory_load:
	goto e_memory_load

t_memory_store:
	dbl(*AR3+) = AC2
e_memory_store:
	goto e_memory_store

t_mmr_load_l:
	bit(ST1, #8) = #0
	.cpl_off
	AC2 = dbl(@AC0_L) || mmap()
	.cpl_on
e_mmr_load_l:
	goto e_mmr_load_l

t_mmr_load_h:
	bit(ST1, #8) = #0
	.cpl_off
	AC2 = dbl(@AC0_H) || mmap()
	.cpl_on
e_mmr_load_h:
	goto e_mmr_load_h

t_mmr_store_l:
	.cpl_off
	dbl(@AC0_L) = AC2 || mmap()
	.cpl_on
e_mmr_store_l:
	goto e_mmr_store_l

t_mmr_store_h:
	.cpl_off
	dbl(@AC0_H) = AC2 || mmap()
	.cpl_on
e_mmr_store_h:
	goto e_mmr_store_h
