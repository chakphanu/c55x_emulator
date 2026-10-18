; Minimal programs (fw/*): reset entry (algebraic syntax, assembled with asm55).
; Same CPU mode setup as reset_c_int00 (the original firmware), then main().
; No cinit: main() initializes everything it uses.
	.mmregs
	.cpl_on
	.arms_on
	.c54cm_off

	.global _fw_reset
	.global _main

STACK_WORDS	.set	0x400
SYSSTACK_WORDS	.set	0x200
stack	.usect	".stack", STACK_WORDS
sysstack	.usect	".sysstack", SYSSTACK_WORDS

	.sect ".text:_fw_reset"
_fw_reset:
	bit(ST1, #11) = #1
	@IMR0_L = #0 || mmap()
	@IMR1_L = #0 || mmap()
	XSP = #(stack + STACK_WORDS)
	SSP = #(sysstack + SYSSTACK_WORDS)
	@ST1_L = @ST1_L & #63775 || mmap()
	@ST1_L = @ST1_L | #16640 || mmap()
	@ST2_L = @ST2_L & #64000 || mmap()
	@ST2_L = @ST2_L | #32768 || mmap()
	bit(ST3, #5) = #0
	bit(ST3, #1) = #1
	@IVPD_L = #1 || mmap()          ; vectors at byte 0x100 (.vectors in fw.cmd)
	@IVPH_L = #1 || mmap()
	call #_main
halt:
	goto halt
