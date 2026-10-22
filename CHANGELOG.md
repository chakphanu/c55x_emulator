# Changelog

Guest-visible behavior changes of the CPU core (`rust/c55x_core`), newest first.
Every entry gives the observable change, the manual it follows, the evidence level,
the test that pins it and the commit.

Evidence levels:

- **example**: reproduces a worked example of the manual.
- **text**: follows the manual text; no worked example covers the case.
- **hardware**: the manuals are ambiguous or contradict each other; the chosen
  behavior must be confirmed on real silicon or with the TI simulator.

Manuals: SPRU374E (C55x mnemonic instruction set), SWPU067E (C55x v3.x mnemonic
instruction set), SPRU371F (C55x CPU reference) and the C55x v3.x CPU reference
guide. Instruction spellings in the tests were checked by assembling them with
`asm55` and reading them back with `dis55 -r -g`. Citations give section numbers
only.

## Unreleased

A review of the core against the four manuals above. The reference core is still
the build listed in `ORACLE_V2.md`; the candidate core with these changes is
listed there too and becomes the reference only when the owner confirms it.
Tests: `tests/test_c55x_emu_manual_review.py` unless noted.

### Fixed

1. **Overflow is reported on each operation's own destination accumulator**
   (2a3f1a9). The pending overflow flag was shared by every part of a parallel
   bundle, so the first accumulator destination to commit took the flag of any
   part: the second MAC of a dual MAC reported in ACOV0, a store partner's shift
   set its neighbour's flag, and a Tx shift-count clamp could erase an overflow
   found earlier. Now each part reads with a clear flag and commits it to its own
   destination; stores report nothing. SPRU374E 4.27 ("the destination
   accumulator overflow status bit is set"), 4.6.x ("Affects none"); SWPU067E
   1.3.1. Evidence: text. Tests: `OverflowAttributionTests`.
2. **In-range M40 = 0 accumulator shifts keep their sign** (200dfa7). A signed
   shift with a count >= 0, shift by zero included, returned the low 32 bits with
   clear guard bits, so a negative in-range result became a large positive value
   with a false overflow, and with SATD = 1 it saturated to 00 7FFF FFFF
   (`AC0 = #-4`, `AC0 = AC0 << #1` gave 00 7FFF FFFF instead of FF FFFF FFF8).
   This was a regression of dafa702. SPRU371F / C55x v3.x CPU 2.10.2.9 and
   2.10.2.11 (with SXMD = 1, bit 31 is copied into the guard bits), SPRU374E /
   SWPU067E SFTS (the shift is performed on 40 bits). The interpreter path now
   uses the same shift helper as the compiled path. Overflowing results without
   SATD keep the previous rule (open question A3). Evidence: text. Tests:
   `SignedShiftSignTests`.
3. **Accumulator stores shift on 40 bits according to SXMD only** (98ee52a).
   `HI/LO(ACx << k)` and `HI(saturate(...))` stores used the arithmetic shift:
   M40 = 0 dropped the guard bits before the shift, SATD clamped the stored value
   and the store raised the overflow flag. Now the shifter input is the 40-bit
   accumulator sign extended per SXMD, no M40, SATD or ACOV is involved, and only
   `saturate()` clamps to 00 7FFF FFFF / FF 8000 0000 (01 0000 0000 >> 8 now
   stores 0100h instead of 0). SPRU374E / SWPU067E 4.6.6-4.6.13 ("Affected by
   [C54CM,] [RDM,] SXMD", "Affects none"). Evidence: text. Tests:
   `StoreShiftTests`.
4. **The 32-bit product of two signed operands is sign extended** (222d5ba,
   corrected by 59bcf0b). The generic multiply returned the raw product while
   the accumulator-operand forms sign extended it, so `T1 * *AR1` and `AC1 * T1`
   disagreed for the same inputs. SPRU374E / SWPU067E 4.47.1, 4.48.2. With a
   `uns()` operand the product stays unsigned, as the SPRU374E 4.27 worked
   example requires (EF00h x A067h gives 00 95C0 9200 with ACOV0 = 1); 222d5ba
   had sign extended those too and 59bcf0b restores them. Evidence: text for
   signed operands (the FRCT 8000h x 8000h value is open question A5), example
   for `uns()` operands. Tests: `ProductSignTests`, `UnsignedProductTests`.
5. **ACOV0-ACOV3 are part of ST0_55** (e793c3b): ACOV2 = bit 15, ACOV3 = bit 14,
   ACOV0 = bit 10, ACOV1 = bit 9, readable and writable through the MMR, BSET /
   BCLR, the interrupt context save and RETI. SPRU371F 2.10.1.1, SWPU067E
   figure 5-1. Evidence: text. Tests: `StatusFlagTests`.
6. **Accumulator-to-accumulator moves detect overflow** (1e34355): `ACy = ACx`
   checks the M40 boundary, sets ACOVy and saturates per SATD. SPRU374E /
   SWPU067E 4.5.1. Evidence: example (MOV AC0, AC1 with AC0 = 01 E500 0030).
7. **ADD K16, Smem** (74e9037): `Smem = Smem + #k` sets CARRY (bit 31) and
   ACOV0, and saturates the stored word to 7FFFh / 8000h when SATD = 1; the
   memory address is evaluated once (a post-modified pointer used to read one
   word and write the next). `Smem = Smem - #k` is the same instruction with
   K16 = -k. SPRU374E / SWPU067E 4.7.16. Evidence: text.
8. **SAT reports overflow** (e6fc8ef): `ACy = saturate(ACx)` sets ACOVy when the
   value leaves the signed 32-bit range, whatever M40 is. SPRU374E / SWPU067E
   4.69. Evidence: example.
9. **SATR clears the 16 LSBs after saturating** (e53a78b):
   `saturate(rnd(ACx))` of 00 7FFF 8000 gives 00 7FFF 0000. SPRU374E / SWPU067E
   4.69. Evidence: example. The expectation of
   `tests/test_c55x_emu_dsp.py` `test_rnd_and_saturate` changed accordingly.
10. **SUBC reports overflow** (446afb5): an overflow of the subtraction at bit 31
    sets ACOVy, without saturation. SPRU374E / SWPU067E 4.25. Evidence: example
    (first SUBC example; see the erratum below for the second one).

Additional tests (f020ace): ADD K16 carry, single address evaluation, `- #k` and
negative saturation; MOV ACx, ACy with SATD and with M40 = 1 together with the
ACOV1 bit position in ST0_55; repeated SUBC without overflow.

### Manual errata found

- SPRU374E 4.25, second SUBC example (`repeat(CSR)` with CSR = 1; SWPU067E prints
  the same): the result is printed as 00 1A18 0007. Two steps of the algorithm
  give 0D8C 0001 and then 00 1A18 0003; the printed high word is the two-step
  value, while a low word of 0007 would need three steps (3330 0007). The test
  expects 00 1A18 0003.
- SPRU374E / SWPU067E 4.47 / 4.48 state that the 32-bit product is sign extended
  even for `uns()` operands, while the 4.27 worked example zero extends an
  unsigned product. The core follows the example.

### Open questions (hardware or TI simulator)

- A1: the SMUL multiplication overflow report (8000h x 8000h, FRCT = 1,
  SMUL = 1, SATD = 0) sets the destination ACOV; SPRU374E says multiplication
  overflow detection "depends on SMUL", SPRU371F 1.5.3 says MAC overflow is
  detected on the final operation only.
- A3: the destination bits of an overflowing M40 = 0 signed shift without SATD.
  The core keeps the low 32 bits with clear guard bits on left shifts; the CPU
  guides' wording reads as FF 8000 0000 for 1 << 31, the instruction set's
  "performed on 40 bits" as 01 0000 0000 for 10000h << 16.
- A4: a status bit set in the same bundle as the operation it affects
  (`bit(ST1, #9) = #1 || AC0 = AC0 << #0`): the core lets the operation see the
  old value.
- A5: FRCT 8000h x 8000h with SMUL = 0 now gives FF 8000 0000 (the 32-bit
  product sign extended, as the manual text says).
- A6: the unsigned product of MPYMU does not report overflow, while the 4.27
  `uns()` MAC example reports ACOV0 for a sum above 7FFF FFFF with M40 = 0.
- A7: whether SUBC detects overflow on the subtraction only or after the shift
  as well (the examples cannot tell).
- A8: SXMD = 0 with a 32-bit value whose bit 31 is set counts as an overflow in
  M40 = 0 shifts and in ADD K16, Smem; with M40 = 1 and SXMD = 0 the shifter
  treats the accumulator as unsigned.
- A9: a signed x `uns()` product with FRCT = 1 can exceed 32 bits (Smem =
  8000h, Cmem >= 8001h); the core keeps the raw product, the manual text
  ("32-bit result sign extended") would flip its sign. No worked example covers
  it.
- Silicon erratum CPU_89 (assembler remark R5673), unchanged: an overflowing
  M40 = 0, SXMD = 0 shift-left may not saturate correctly on silicon.

### Known limitations (not modelled)

- The protected addresses of ST0_55 / ST1_55 / ST3_55 (06h, 07h, 1Dh) alias the
  native ones without masking the protected bits.
- Conditions that test `overflow(ACx)` (and clear ACOVx) are rejected with an
  error.
- `uns(saturate(...))` stores should clamp to 00 FFFF FFFF.
- ST3 SST = 1 makes a shifted accumulator store fail, although SPRU371F says SST
  is ignored when C54CM = 0.
- ADD K16 with a memory-mapped register or I/O operand keeps the generic path
  (no CARRY, ACOV0 or SATD handling).
- Dual 16-bit arithmetic does not report ACOV.
- MAC multiplier operands from Tx or Smem follow SXMD although SPRU371F 2.10.2.11
  and SWPU067E say they are sign extended to 17 bits regardless; with SXMD = 0 a
  signed FFFFh x FFFFh is therefore computed as FFFE 0001h and then sign extended
  (entry 4), where the hardware gives +1. To be fixed next.
- `dis55` can misprint the target of a relative call or goto bundled with a
  parallel operation; such an instruction fails with an error.

### Verification

- Tests: `tests/test_c55x_emu_manual_review.py` (27 tests) written red first
  against the reference core; the full suite run one contained job per module
  (only the two known example-artifact errors remain); `cargo test` 17 passed.
- Routine regression: 1,617 differential vectors over five verified firmware
  routines show zero differences between the reference core and the candidate.
  These routines do not exercise the changed cases.
- Differential fuzz of instruction forms (26,400 + 9,600 seeded cases, SATD /
  M40 / FRCT modes, edge values) between the reference core and the candidate,
  with a core built at every intermediate commit: every difference is attributed
  to the commit whose entry above predicts it; no unattributed difference.
- An independent review of the implementation against all four manuals found
  one defect (entry 4, `uns()` products), fixed in 59bcf0b; a second review of
  that fix and of the added tests found no defect.

## Earlier changes

The semantic changes of the oracle v2 build relative to the original snapshot
(`3dbef971...`) are listed in `ORACLE_V2.md`; older history is in the commit log.
