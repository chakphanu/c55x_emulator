# Oracle v2 build

A build of this repository's `rust/c55x_core` for use as a differential
oracle. The reference build it replaces is the `3dbef971e814e1e7...`
core (SQDST-corrected snapshot; this repository's `8871bc9` is
byte-identical to its source).

- Build: `bin/c55x-contained --job work -- ./rust/build.sh` -> `build/py/c55x_core.so`
- SHA256 (reference since 2026-10-22, owner-confirmed):
  6891ce79f76f18485cb890f1f39e304a4c88b37dc02fe851c9077f862662b441, built from
  commit 6704168 (later documentation-only commits do not change it). It fixes
  the defects listed under "Defects of the previous reference" below; every
  change, its evidence and its test are in `CHANGELOG.md`.
- SHA256 of the previous reference (with resource containment):
  24f97bac9828549532600301ddd03962d2a714a72b2a1ef2326f3c7543b7ed53 (commit
  1deaa42). Keep a copy outside `build/py` for comparisons and select it with
  `ZEN_CORE_DIR=<dir>`.
- SHA256 of the earlier oracle v2 build without the containment guard:
  afcebfd440af00d594e6b7ada8494d174946fafd16681c26bc2cd620e0ef639c (commit 34fd281)
- The semantic list below and the regression figures in this header describe
  the oracle v2 builds (afcebfd4 / 24f97bac) relative to 3dbef971; the current
  reference changes them as described in `CHANGELOG.md`.
- The guard and the leak fix change the binary but not the guest behavior: 1617
  differential vectors compared between the two builds show zero differences in
  status, instruction count, cycles, registers, memory windows, outside writes,
  trace pcs and ACOV flags.
- Baseline for the semantic list below: `3dbef971e814e1e7...`
- Test suite: 143 tests (only the two pre-existing example-artifact
  errors remain), plus a 1617-vector differential regression across five
  verified firmware routines: zero value differences. Trace_pcs changed
  exactly where intended (repeat bodies now visible): 239/267 and 247/287
  directly-looping vectors, and 278/278 and 320/320 through a repeated
  call body; the set without repeats is unchanged.

## Semantic changes versus 3dbef971

Each entry lists the behavior change, the manual citation, and the
discriminating test.

1. Accumulator overflow flags (ACOV0-3) exist and are reported. Sticky
   per-accumulator state, exposed via `Core.acov()`. Citations:
   SPRU374E 4.70.3 (a saturated Tx shift count reports the destination
   overflow), 4.48.2 (an addition overflow at the M40 boundary sets
   ACOVy), ST0_55 bit table. Tests: tests/test_c55x_emu_acov.py.
2. SFTS-by-Tx count saturation report (layer 1): a T-register shift
   count outside -32..+31 saturates and reports. Citation: SPRU374E
   4.70.3. Tests: same file.
3. M40 = 0 shifts work on 32-bit values: the shifter input is
   substituted per SXMD in BOTH engines, left shifts extract every bit
   shifted out past bit 31, right shifts insert at bit 31, overflow is
   detected at the M40 boundary and SATD saturates to the width's
   extremes. Citations: SPRU371F / C55x v3.x CPU 2.10.2.9, SPRU374E
   4.70.3. Tests: ShiftM40Tests, tests/test_c55x_emu_addressing.py.
   Also fixes the interpreter treating immediate accumulator shifts as
   no-ops (engine parity test).
4. MAC accumulate overflow report and SATD saturation at the M40
   boundary; SMUL reports the documented multiplier overflow. Citation:
   SPRU374E 4.48.2, v3.x CPU 1.5.3. Tests: tests/test_c55x_emu_acov_mac.py.
5. Unsigned products (MPYMU) are exempt from the signed-domain overflow
   check and SATD clamp: the manual zero-extends the 32-bit unsigned
   product to 40 bits. Citation: SPRU374E 4.47.10/4.47.11. Test: same
   file (the blanket check previously false-flagged and would have
   clamped them under SATD).
6. Multiply constants are signed (MACK `Tx * #K16`, MPYK `ACx * #K`,
   MPYMK/MACMK `Smem * #K8`): the algebraic text prints negatives raw
   (0x8000..0xffff, or 0xffff8000..0xffffffff sign-extended to 32 bits);
   any constant whose 32-bit view is >= 0x8000 is interpreted as its
   signed 16-bit value. Citations: SPRU374E 4.47/4.48 operand rules
   ("the 16-bit signed constant, K16, sign extended to 17 bits").
   Tests: tests/test_c55x_emu_oracle_v2.py (includes the real-code form
   `AC0 = AC0 * #0xffffa99a`). A firmware-wide audit (audit/
   mul_const_audit.py) decoded every multiply-by-constant site from its
   raw opcode bytes and confirmed the text rule matches the opcode
   decode at all 35 sites across four forms (MACK K16, MPYK K16, MPYK
   K8, MPYMK K8); audit output hash: eae4412f66bf5e515963b311a0a5163cc082ed18048514db2235d9de3eaa7738.
7. Repeat-body trace visibility: RPT / RPTCC body PCs now appear in
   `trace_pcs` (same ring semantics as the fetch loop). Diagnostic only:
   no value or cycle change (asserted by test). Citation: none needed
   (trace is an emulator facility). Tests: same file.

## Defects of the previous reference

Found by a review against SPRU374E, SWPU067E, SPRU371F and the C55x v3.x CPU
guide in the oracle v2 builds (afcebfd4 / 24f97bac); fixed in the current
reference (details, evidence and tests in `CHANGELOG.md`). Results obtained
with the previous reference that depend on these cases should be re-checked.

- Overflow of one part of a parallel bundle is reported on another part's
  accumulator (dual MAC: the second MAC's overflow lands in ACOV0; a store
  partner sets or erases its neighbour's flag).
- Item 3 above is wrong for in-range results: with M40 = 0 a signed shift with
  a count >= 0 (shift by zero included) drops the sign of a negative result,
  reports a false overflow and, with SATD = 1, saturates it to 00 7FFF FFFF.
  "Guard clear" applies only to overflowing results without SATD.
- Accumulator stores with a shift (`HI/LO(ACx << k)`, `HI(saturate(...))`)
  depend on M40 and SATD and raise the overflow flag; the manuals shift the
  40-bit accumulator per SXMD only.
- The generic signed multiply does not sign extend its 32-bit product (the
  accumulator-operand forms do).
- ACOV0-ACOV3 are missing from the ST0_55 image; ACx -> ACy moves, ADD K16,
  Smem, SAT and SUBC do not report overflow; SATR keeps the 16 LSBs after
  saturating; ADD K16, Smem evaluates a post-modified address twice.
- With SXMD = 0, multiplier operands and bytes loaded into a T/AR register
  are zero extended instead of sign extended.

## Known ambiguities (documented for hardware validation)

- The destination bits of an overflowing M40 = 0 signed shift without
  SATD (low 32 bits with clear guard bits on left shifts); the open
  questions of the review are listed in `CHANGELOG.md`.
- Silicon erratum Advisory CPU_89: with M40=0, SXMD=0, C54CM=0 an
  overflowing accumulator shift-left may not saturate appropriately on
  real silicon; the emulator saturates per the manual text.
- SFTSC (carry-storing shift) has no assembler-exposed algebraic form
  and no occurrence in the analyzed firmware; unimplemented by design.
