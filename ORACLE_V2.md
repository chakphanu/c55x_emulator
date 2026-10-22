# Oracle v2 build

A build of this repository's core (`rust/c55x_core`, as the Python extension
`rust/c55x_py`) for use as a differential
oracle. The reference build it replaces is the `3dbef971e814e1e7...`
core (SQDST-corrected snapshot; this repository's `8871bc9` is
byte-identical to its source).

- Build: `RUSTC_WRAPPER= CARGO_BUILD_JOBS=1 bin/c55x-contained --job work
  --env RUSTC_WRAPPER --env CARGO_BUILD_JOBS -- ./rust/build.sh` ->
  `build/py/c55x_core.so` (with two cargo jobs the linker can hit the job's
  32-task limit: `posix_spawnp: Resource temporarily unavailable`)
- SHA256 (current reference): extension
  9f9c5331c5d7843e69a7610ccffcabc8877c16a1286c384339cc71994d3e668a and native
  runner `build/run/c55x-run`
  e24add9a72a38ecc41a2297d1557b01f7b649ef28706d6c78e921998fe172606, both built
  from commit b91f15a (clean `git archive` export, same hashes as a separate
  build; the later test-only commits do not change them). Relative to the
  previous reference it adds the containment check at the core's execution
  entries, resource caps in the core (trace length, image size, cached
  instructions, bounded `load_bytes`), whole-memory slices and the version-2
  call script (`restore_image`, `mark_memory`, `observe`). Acceptance: the
  private 104-file decode set unchanged (PCM and per-call instruction counts),
  and the 1617 routine vectors run through `c55x-run` equal to the recorded
  observations on every component (status, instructions, cycles, registers,
  memory windows, outside writes, trace, ACOV).
- SHA256 of the previous reference: extension
  3539b077f9f46103aad16e56059de175b5fa2d84851d68b8e36829710cc5bf24 and native
  runner eb9eee5168578870c8bfbe3ac0bd372226cc098f5949f0b0e81ca22353be097d, both built
  from commit ce124e4 (clean `git archive` export; the runner with
  `cargo build --release -p c55x_run`, same hash from two target directories).
  ce124e4 adds the Python-free native host (`NativeHost::from_listing`: memory
  image + listing rows decoded by the Rust frontend, guard-checked) and the
  call-script runner `c55x-run`. Acceptance: a private 104-file decode set
  gives PCM and per-call instruction counts identical to the Python path on
  every file. The extension's guest behavior is unchanged relative to
  3ab06717 (full suite per test, 1617 routine vectors, 58,400 fuzz cases).
- Archive of the Python slow path: tag `python-slow-path-final` and branch
  `archive/python-slow-path`, both at commit 95b0841. This is the last state
  with the Zen Touch board, the Python `Machine` / `Cpu` / `Bus` slow path,
  `PyHost` and the Python frontend. The repository is being reduced to a pure
  C55x emulator from there on.
  - The extension built at the tag is byte-identical to the reference
    extension above (3539b077..., rebuilt from the tag with
    `cargo build --release -p c55x_py`).
  - A copy is kept as `build/archive-python/c55x_core.so`. It is the frozen
    reference for external Python harnesses that build the core over a
    Python `Cpu`.
- SHA256 of the previous reference:
  3ab06717ed86678cb05dcb33a8a3b5662378e180403f87d1115c7c837d77de09, built from
  commit 65a3b09 (clean `git archive` export, same hash as a separate build).
  It is the first build of the three-crate workspace: dca49fe puts every
  Python call behind the `Host` trait and 65a3b09 splits `c55x_core`
  (no dependencies) from the extension `c55x_py`. No guest behavior change
  relative to 1751dcd2: per-test results of the full suite identical, the
  1617 routine vectors and 58,400 fuzz cases with zero differences.
- SHA256 of the previous reference:
  1751dcd27339c47e0468c1136bb5bc88988195a0fb9c86c5749340cf44d1aa00, built from
  commit a86cc31 (clean `git archive` export; a second build in another
  directory gives the same hash). Relative to 29b184c5 it adds a86cc31: RPTC
  reads 0 after a completed RPTCC (see `CHANGELOG.md`). Gates: full test
  suite, the 1617 routine vectors below with zero differences, cargo test.
- SHA256 of the previous reference (since the eight-slot containment):
  29b184c5635083ba08b81b41dfcf2cc5fe9c72c1bad560d5926c32e673505ab4, built from
  commit 28aa22b (later commits that do not touch `rust/` do not change it):
  commit c7c1a9f plus the eight-slot containment guard
  (`rust/c55x_core/src/policy.rs`: the eight emulator slot units and the
  shared caps 18 GiB, 17 GiB high, 448 tasks, 1000% CPU). Relative to the
  previous reference it adds the core changes of f81dbb9 (dual 16-bit ALU
  overflow in ACOVy) and 1e7b548 (address phase of an instruction skipped by
  XCCPART) and the Ast refactor c7c1a9f. The guard changes no guest behavior:
  its only source difference from the main line is `policy.rs`, and the
  guard alone on the previous reference's source (core e7b0a149) matched
  6891ce79 on every one of the 465 routine vectors of the set compared. An
  unmodified source export rebuilds 6891ce79 byte for byte from another
  directory, so the builds are reproducible. The 1617 routine vectors below
  give the same status, instruction count,
  cycles, registers, memory windows, outside writes, trace pcs and ACOV flags
  on this core as the outputs recorded during the review (core ee19057d).
- SHA256 of the previous reference (owner-confirmed 2026-10-22):
  6891ce79f76f18485cb890f1f39e304a4c88b37dc02fe851c9077f862662b441, built from
  commit 6704168. It fixes the defects listed under "Defects of the oracle v2
  builds" below; every change, its evidence and its test are in
  `CHANGELOG.md`.
- SHA256 of the earlier reference (with the first containment guard):
  24f97bac9828549532600301ddd03962d2a714a72b2a1ef2326f3c7543b7ed53 (commit
  1deaa42).
- 6891ce79, 24f97bac and every other build with the single-slot guard refuse
  to run under the current launcher (their guard caps the shared group at
  4 GiB, 3 GiB high, 64 tasks and 200% CPU and accepts only
  `c55x-emu.service`). Outputs recorded with them stay valid for comparison.
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
   Tests: rust/c55x_core/tests/oracle_v2.rs (the real-code constant,
   printed `#0xffffa99a` by dis55 -e and `#-22118` by the runtime listing). A firmware-wide audit (audit/
   mul_const_audit.py) decoded every multiply-by-constant site from its
   raw opcode bytes and confirmed the text rule matches the opcode
   decode at all 35 sites across four forms (MACK K16, MPYK K16, MPYK
   K8, MPYMK K8); audit output hash: eae4412f66bf5e515963b311a0a5163cc082ed18048514db2235d9de3eaa7738.
   The audit script is kept at tag `python-slow-path-final`.
7. Repeat-body trace visibility: RPT / RPTCC body PCs now appear in
   `trace_pcs` (same ring semantics as the fetch loop). Diagnostic only:
   no value or cycle change (asserted by test). Citation: none needed
   (trace is an emulator facility). Tests: same file.

## Defects of the oracle v2 builds

Found by a review against SPRU374E, SWPU067E, SPRU371F and the C55x v3.x CPU
guide in the oracle v2 builds (afcebfd4 / 24f97bac); fixed since 6891ce79 and
in the current reference (details, evidence and tests in `CHANGELOG.md`).
Results obtained with those builds that depend on these cases should be
re-checked.

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
