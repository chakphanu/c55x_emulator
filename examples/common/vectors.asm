; Minimal programs: interrupt vector table (algebraic syntax). 32 vectors of 8 bytes on a
; 256-byte page; reset.asm points IVPD and IVPH at it (like bios_init: IVPD = IVPH = 1
; with the table at byte 0x100). Vector numbers: SPRS205K table 3-40 (TINT0 = 4,
; INT1 = 16: MCU request line). Unused vectors go to an endless loop so a stray
; interrupt is visible (PC stays at fw_unexpected).
	.mmregs
	.cpl_on
	.arms_on
	.c54cm_off

	.global _fw_vectors
	.global _fw_reset
	.global _fw_timer0_isr
	.global _fw_mcu_isr

; SPRU371F 5.2: byte 0 ignored (reset: stack mode), bytes 1..3 ISR byte address,
; bytes 4..7 NOPs
VEC	.macro target
	.ivec target
	.align 8
	.endm

	.sect ".vectors"
_fw_vectors:
	VEC _fw_reset           ; 0  reset (the emulator starts at the COFF entry point)
	VEC fw_unexpected       ; 1  NMI
	VEC fw_unexpected       ; 2  INT0
	VEC fw_unexpected       ; 3  INT2
	VEC _fw_timer0_isr      ; 4  TINT0
	VEC fw_unexpected       ; 5
	VEC fw_unexpected       ; 6
	VEC fw_unexpected       ; 7
	VEC fw_unexpected       ; 8
	VEC fw_unexpected       ; 9
	VEC fw_unexpected       ; 10
	VEC fw_unexpected       ; 11
	VEC fw_unexpected       ; 12
	VEC fw_unexpected       ; 13
	VEC fw_unexpected       ; 14
	VEC fw_unexpected       ; 15
	VEC _fw_mcu_isr         ; 16 INT1
	VEC fw_unexpected       ; 17
	VEC fw_unexpected       ; 18
	VEC fw_unexpected       ; 19
	VEC fw_unexpected       ; 20
	VEC fw_unexpected       ; 21
	VEC fw_unexpected       ; 22
	VEC fw_unexpected       ; 23
	VEC fw_unexpected       ; 24
	VEC fw_unexpected       ; 25
	VEC fw_unexpected       ; 26
	VEC fw_unexpected       ; 27
	VEC fw_unexpected       ; 28
	VEC fw_unexpected       ; 29
	VEC fw_unexpected       ; 30
	VEC fw_unexpected       ; 31

	.sect ".text:fw_unexpected"
fw_unexpected:
	goto fw_unexpected
