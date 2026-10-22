# Changelog

Guest-visible behavior changes of the CPU core (`rust/c55x_core`; Python extension
`rust/c55x_py`), newest first.
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

## 2026-10-22

A review of the core against the four manuals above. The core built from
commit 6704168 (SHA256 6891ce79...) is the reference since this release (see
`ORACLE_V2.md`); the previous reference 24f97bac... has the defects fixed
below. Tests: `tests/test_c55x_emu_manual_review.py` unless noted.

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

11. **Multiplier operands and bytes loaded into TAx ignore SXMD** (6704168).
    With SXMD = 0 a signed Tx / Smem / Cmem multiplier operand was zero
    extended (FFFFh x FFFFh gave FFFE 0001h instead of 1), and a byte loaded
    into T/AR was zero extended. SPRU371F 2.10.2.11 (MAC inputs are sign
    extended to 17 bits regardless of SXMD), SPRU374E / SWPU067E 4.47 / 4.48
    operand lists and 4.4.5 / 4.4.6 (a TAx destination is sign extended
    regardless of SXMD; an accumulator destination follows SXMD). `uns()`
    operands stay zero extended. Evidence: text. Tests:
    `SxmdIndependentOperandTests`.

12. **The Python front end builds the forms the core already compiled**
    (21502f0): RPTCC (`while ([!]TC1 && (RPTC < #k)) repeat`), dual 16-bit
    arithmetic with two HI/LO operands (SPRU374E 4.26), BCNT
    (`Tx = count(ACx, ACy, TCx)`), SQDST, and execute(D_Unit) after a partner
    or with a pointer-updating store partner (4.28). Previously programs using
    them stopped with an error. SQDST is lowered so that CARRY reports the
    borrow of every subtraction (4.77), not the SUB Smem << #16 rule (4.80.11).
    On a firmware decode corpus of 4,899 executed instructions the IR now equals
    that of a downstream copy of this front end.
13. **Dual 16-bit arithmetic reports overflow** (f81dbb9): an overflow of
    either part (bit 15 low, bit 31 high) sets ACOVy (SPRU374E 4.26.3-4.26.5);
    values, per-part saturation and CARRY are unchanged. Evidence: text.
14. **An instruction skipped by XCCPART still runs its address phase**
    (1e7b548): its pointer modifications happen without the data access
    (SPRU374E 4.28.2 example 2: AR2 still increments); previously this stopped
    with an error. Evidence: example.
15. **The core compiles instructions from a plain Rust IR** (c7c1a9f), converted
    once from the front end's Python objects; no guest behavior change (1,617
    vectors and 58,400 fuzz cases identical).

Additional tests (f020ace, ebaf1de): ADD K16 carry, single address evaluation,
`- #k` and negative saturation; MOV ACx, ACy with SATD and with M40 = 1 together
with the ACOV1 bit position in ST0_55; repeated SUBC without overflow; BCNT,
dual 16-bit arithmetic, RPTCC, XCCPART and SQDST from SPRU374E 4.9, 4.26, 4.61,
4.28 and 4.77.

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
- A10: CSR after an RPTCC loop that ends early: SPRU374E 4.61 gives 3,
  SWPU067E gives 4 for the same case; the core follows SWPU067E.
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
- The dual 16-bit forms with a Tx or single Smem operand (ADD/SUB dual(Lmem),
  Tx; ADDSUB / SUBADD) still run as two independent half assignments, without
  the per-part CARRY and overflow rules of SPRU374E 4.26, and without an error.
- An execute(D_Unit) condition bundled with an address-phase partner (`mar(...)`)
  skips the partner when false, contrary to SPRU374E 4.28.2.
- A read-modify-write of one Smem with a pointer modification other than
  ADD K16, Smem (for example `*AR1+ = *AR1+ | #k`) evaluates the address twice.
- Not yet checked: how `dis55` prints a negative K16 in a memory compare
  (`TCx = Smem == #k`); the compare uses the constant as printed. The address
  arithmetic `mar(ARx + Tx)` reads Tx through the SXMD path, which cannot change
  a 16-bit A-unit result but is inconsistent.
- With the extra listing options `-e -b`, `dis55` can misprint the target of a
  relative call or goto bundled with a parallel operation; the options the
  emulator uses (`-qq -r -g`) print it correctly.

### Verification

- Tests: `tests/test_c55x_emu_manual_review.py` (27 tests) written red first
  against the reference core; the full suite run one contained job per module
  (only the two known example-artifact errors remain); `cargo test` 17 passed.
- Routine regression: 1,617 differential vectors over five verified firmware
  routines show zero differences between the previous reference and the new core.
  These routines do not exercise the changed cases.
- Differential fuzz of instruction forms (26,400 + 9,600 seeded cases, SATD /
  M40 / FRCT modes, edge values) between the previous reference and the new
  core, with a core built at every intermediate commit: every difference is
  attributed to the commit whose entry above predicts it; no unattributed
  difference. For entry 11, 58,400 cases with SXMD as a mode: no difference
  with SXMD = 1, and every difference with SXMD = 0 has a negative signed
  operand.
- An independent review of the implementation against all four manuals found
  one defect (entry 4, `uns()` products), fixed in 59bcf0b; reviews of that
  fix, of the added tests and of entry 11 found no defect.

### Containment

Guest behavior is unchanged: only the launcher and the containment guard
(`rust/c55x_core/src/policy.rs`) change.

- Eight concurrent emulator slots instead of one. Slot 0 keeps the unit
  `c55x-emu.service` and the lock `emulator.lock`; slot k = 1..7 uses
  `c55x-emu-<k>.service` and `emulator-<k>.lock`. The guard accepts exactly
  these eight unit names.
- The shared group `c55x.slice` is raised to memory.max 18 GiB, memory.high
  17 GiB, 448 tasks and 1000% CPU (swap stays 0). Per-job limits and admission
  are unchanged.
- Work jobs get 64 TiB of virtual address space (was 16 GiB, too small for the
  shadow mappings ASan reserves at startup); emulator jobs keep 8 GiB. The
  allowance grants no RAM: the cgroup memory limit still stops a job that
  uses more real memory. The job-side verifier checks the value of each job
  class.
- A slot whose unit is in any state other than inactive, failed or not loaded
  (for example deactivating during the stop grace) is busy. `--status` no
  longer sets the shared-group properties.
- A core built before this change refuses to run under the raised shared
  limits and in slots 1..7; rebuild it.
- The core built from 28aa22b (SHA256 29b184c5...), commit c7c1a9f plus this
  guard, is the reference (`ORACLE_V2.md`); its 1,617 routine-vector results
  match the outputs recorded during the review record for record.

## Earlier changes

The semantic changes of the oracle v2 build relative to the original snapshot
(`3dbef971...`) are listed in `ORACLE_V2.md`; older history is in the commit log.
