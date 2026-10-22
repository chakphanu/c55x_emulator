; The probe of tests/test_c55x_launcher.py: c55x-run (probe.call.json) calls it once;
; it stores 0x100 at the word *AR2 (0x1700), which the script reads to output.pcm.
	.mmregs
	.cpl_on
	.arms_on
	.c54cm_off
	.text

probe:
	AC0 = #0x100
	*AR2 = AC0
	return
