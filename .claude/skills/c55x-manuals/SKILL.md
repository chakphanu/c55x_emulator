---
name: c55x-manuals
description: Use when a question is about what a TMS320C55x instruction, status bit or addressing mode does (semantics, saturation, M40/SXMD/SATD, ACOV, shifts, multiply, repeat), when judging whether this emulator is right, or when decoding an opcode. Says which source wins and lists the traps already hit. Cite section numbers only; do not paste TI manual text into this repository.
---

# Looking up C55x behavior

## Authority order

1. TI tools for encodings: assemble a probe with `asm55`, read the bytes back with
   `dis55` (TI C55x code generation tools 4.4.1).
2. TI manuals for semantics, read ACROSS manuals: SPRU374E (mnemonic instruction
   set, per-instruction text in sections 4.x), SPRU371F (CPU reference) and the
   C55x v3.x CPU guide.
3. This emulator is a differential partner, not the truth. When it disagrees with
   the manuals, record both citations and tell the repository owner; do not pick a
   side silently and do not tune the emulator to a test result.

The manuals are not part of this repository. The owner keeps local searchable
copies; ask where they are. Opcode bit diagrams are images in those copies, so
get encodings from `asm55` / `dis55`, not from the manual.

## Traps already hit

- Read status-bit semantics in the CPU guides, not only in SPRU374E. SPRU374E
  4.70.3 (`SFTS ACx, Tx`) does not say that with M40 = 0 shifts work on 32-bit
  values; SPRU371F and the v3.x guide, section 2.10.2.9 (M40 bit of ST1_55), do
  (bits shifted out are taken at bit 31, right shifts insert at bit 31). A
  change based on SPRU374E alone had to be reverted.
- A T-register shift count is saturated to -32..+31 and the destination overflow
  flag is reported (4.70.3); that range is the shifter's physical limit (CPU
  guide, D unit shifter).
- Overflow detection: with C54CM = 0 each operation is checked; a MAC reports at
  the final accumulate (CPU guide D unit, SPRU374E 4.48.2). Multiplier overflow
  depends on SMUL.
- `MPYMU` (4.47.10 / 4.47.11) zero-extends the unsigned 32-bit product to 40 bits;
  it must not go through the signed overflow check or the SATD clamp.
- `RPT` runs the next instruction N + 1 times (4.62); `mar(*(ARn+T0))` modifies ARn
  (4.45).
- `dis55 -r -e` prints negative immediates as 32-bit hex: a multiply constant of
  -128 appears as `0xffffff80` and -22186 as `0xffffa956`. A positive 16-bit
  constant is never >= 0x8000.
- Algebraic forms: `<<` is SFTS, `<<<` and `>>>` are SFTL; immediate `>>` and
  `>>>` accept only `#1`; right shifts by a register are written `<< Tx`.
- `dis55` can misprint the target of a relative call or goto that is bundled with a
  parallel operation (a 0x8003xxxx style address). Recompute it from the bytes and
  confirm with `asm55`.

## Open questions that need hardware, not more reading

- The destination bits of an OVERFLOWING M40 = 0 signed shift without SATD (the
  emulator keeps the low 32 bits with clear guard bits on left shifts). In-range
  results keep their sign (SPRU371F / v3.x 2.10.2.11). `CHANGELOG.md` lists the
  other open questions and two manual errata.
- Silicon erratum CPU_89 (assembler remark R5673): with M40 = 0, SXMD = 0 and
  C54CM = 0 an overflowing accumulator shift-left may not saturate correctly on
  real silicon; the emulator saturates as the manuals describe.
- `SFTSC` (shift that stores the shifted-out bit in CARRY) has no algebraic form the
  assembler accepts and is not implemented.

## Rules

- Cite as "SPRU374E 4.70.3" or "SPRU371F 2.10.2.9". Do not copy manual paragraphs
  into this repository; section numbers are enough.
- A claim about hardware behavior needs a citation or a probe result; otherwise
  say it is unverified.
- Run anything that loads the emulator core through `bin/c55x-contained`
  (see the `c55x-contained` skill and `AGENTS.md`).
