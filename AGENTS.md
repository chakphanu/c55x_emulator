# Instructions for coding agents

## Single source of truth: Rust

This repository is a pure C55x emulator. Rust is the single source of truth for
everything the emulator does:

| Concern | Single source of truth | Not allowed |
|---|---|---|
| Instruction semantics | `rust/c55x_core` (expr, fast, fuse, parts, stmt, dsp) | semantics in any other language |
| dis55 text to instruction IR | `rust/c55x_core/src/frontend` | a second parser (the Python one is at tag `python-slow-path-final`) |
| Listing parsing | `c55x_core::frontend::listing` | a parser outside the core |
| Memory, state, interrupts, DMA, McBSP | `c55x_core` | board or peripheral models (removed; at the tag) |
| Running a program | `rust/c55x_run` (`c55x-run`) over `Core::from_listing` (memory image + listing) | decoding at run time, a host trait or callbacks into Python |
| Tests | `cargo test` via `scripts/cargo-test-contained` (`rust/c55x_core/tests`, `rust/c55x_run/tests`, `rust/fixtures`) | Python tests for the emulator |
| Containment | `bin/c55x-contained` + `rust/c55x_core/src/policy.rs` / `guard.rs` | a payload that runs the core other than through a Rust binary |

- Never bind the core back to Python. Do not add a Python host, a Python
  callback or a Python dependency to `c55x_core`.
- `rust/c55x_core/tests/ssot_guards.rs` enforces this: the core depends on
  `regex` only, no host trait, no pyo3 in the workspace, no Python file besides
  the launcher exception. Do not weaken it or allowlist around it without the
  owner's approval.
- The Python binding (`rust/c55x_py`, `c55x_emu/`) was removed. The last state
  with it is the tag `python-slow-path-final`; its extension
  (`build/archive-python/c55x_core.so`) is a frozen reference for external
  Python harnesses only.
- The one explicit Python exception is the launcher `bin/c55x-contained` with its
  tests (`tests/test_c55x_launcher.py`). It is machine-safety infrastructure,
  not the emulator, and uses the Python standard library only.

## Run everything through the launcher

Anything that loads the emulator core (`c55x-run`, the Rust test binaries) and
any heavy build step must run through
`bin/c55x-contained`. The native core refuses to run outside it with a
"resource policy" error (`ResourcePolicyError` in a c55x-run report). Do not look for a way around that check.

    scripts/cargo-test-contained                          # Rust tests (work job + emulator job)
    bin/c55x-contained -- build/run/c55x-run --script S.json --input FILE --out DIR
    bin/c55x-contained --job work -- ./rust/build.sh      # builds, cannot run the core
    bin/c55x-contained --status

Compilers and sanitizers use `--job work`. A work job may reserve 64 TiB of
virtual address space (ASan shadow mappings); that grants no RAM, and its
`--mem` ceiling still stops a job that uses more real memory. Rust builds can
run into the task limit (32) when sccache is active; if a build fails with
"Unable to install ctrlc handler", run it as `RUSTC_WRAPPER= CARGO_BUILD_JOBS=2
bin/c55x-contained --job work --env RUSTC_WRAPPER --env CARGO_BUILD_JOBS -- ./rust/build.sh`.

Exit 75 means all ten emulator slots are busy: wait. A core built before the
ten-slot change (the archived Python extension `build/archive-python/c55x_core.so`,
runner builds up to 515dc807) accepts only slots 0-7: launch it with
`bin/c55x-contained --legacy-slots`. Exit 79 or 124 means the
job hit its memory or time limit: reduce the work. Never raise a limit, kill
another job, clear a lock, or edit cgroup files yourself. Details and the
exit-code table: `CONTAINMENT.md`.

## The reference core

The reference build is the native runner `build/run/c55x-run`, built from the
Cargo workspace `rust/`:

- `c55x_core`: the core itself, no Python;
- `c55x_run`: the call-script runner;
- `c55x_fixtures`: rebuilds and checks the test fixtures.

Build it only with `rust/build.sh` from a clean commit, and confirm its SHA256
against `ORACLE_V2.md`. Do not replace it or copy another build over it. Moving any pin to a new core
build needs explicit confirmation from the repository owner.

Earlier reference builds (their SHA256 values are in `ORACLE_V2.md`) may be kept
in other `build/` directories, but the guarded ones (6891ce79, 24f97bac) refuse
to run under the current shared limits; compare against outputs recorded with
them instead. `build/afce/` has no containment guard: never use it for new
work. The behavior differences between builds are listed in `CHANGELOG.md`.

## Tests

Run the Rust tests with `scripts/cargo-test-contained`. The script builds them
in a work job and runs them in one emulator job. The launcher tests run outside
a job: `python3 -m unittest tests.test_c55x_launcher` (`C55X_RUN` names the
c55x-run binary). Test programs are synthetic fixtures under `rust/fixtures`;
never use firmware code, addresses or symbols in a test.

## Looking up instruction behavior

Authority order: TI tools (`asm55` / `dis55`) for encodings, TI manuals for
semantics read across SPRU374E, SPRU371F and the v3.x CPU guide, and this
emulator only as a differential partner. When it disagrees with the manuals,
record both citations and tell the owner. Details, section numbers and the
traps already hit: `.claude/skills/c55x-manuals/SKILL.md`. Cite section numbers;
do not paste manual text into the repository.
