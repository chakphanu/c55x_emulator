# Oracle v2 build

A build of this repository's `rust/c55x_core` for use as a differential
oracle. The reference build it replaces is the `3dbef971e814e1e7...`
core (SQDST-corrected snapshot; this repository's `8871bc9` is
byte-identical to its source).

- Build: `./rust/build.sh` -> `build/py/c55x_core.so`
- SHA256: 28f97a4bf31f29a5fdd7a7e36126180323f153f759b7b2654d6101ac0c6680a5
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
6. Multiply constants are signed (MACK `Tx * #K16`, MPYK `ACx * #K`):
   the algebraic text prints negatives raw (0x8000..0xffff, or
   0xffff8000..0xffffffff sign-extended to 32 bits); any constant whose
   32-bit view is >= 0x8000 is interpreted as its signed 16-bit value.
   Citations: SPRU374E 4.47/4.48 operand rules ("the 16-bit signed
   constant, K16, sign extended to 17 bits"). Tests:
   tests/test_c55x_emu_oracle_v2.py (includes the real-code form
   `AC0 = AC0 * #0xffffa99a`).
7. Repeat-body trace visibility: RPT / RPTCC body PCs now appear in
   `trace_pcs` (same ring semantics as the fetch loop). Diagnostic only:
   no value or cycle change (asserted by test). Citation: none needed
   (trace is an emulator facility). Tests: same file.

## Known ambiguities (unchanged, documented for hardware validation)

- The destination guard-bit rule for signed LEFT shifts of negative
  inputs is ambiguous in both CPU manuals; implemented as guard-clear
  (fits every documented example).
- Silicon erratum Advisory CPU_89: with M40=0, SXMD=0, C54CM=0 an
  overflowing accumulator shift-left may not saturate appropriately on
  real silicon; the emulator saturates per the manual text.
- SFTSC (carry-storing shift) has no assembler-exposed algebraic form
  and no occurrence in the analyzed firmware; unimplemented by design.
