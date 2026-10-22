# Instructions for coding agents

## Run everything through the launcher

Anything that loads the emulator core (tests, probes, `python -m c55x_emu`,
scripts that import `c55x_emu`) and any heavy build step must run through
`bin/c55x-contained`. The native core refuses to run outside it and raises
`ResourcePolicyError`. Do not look for a way around that check.

    bin/c55x-contained -- python3 -m unittest discover -s tests -t .
    bin/c55x-contained -- python3 my_probe.py ARGS...
    bin/c55x-contained --job work -- ./rust/build.sh      # builds, cannot run the core
    bin/c55x-contained --status

Compilers and sanitizers use `--job work`. A work job may reserve 64 TiB of
virtual address space (ASan shadow mappings); that grants no RAM, and its
`--mem` ceiling still stops a job that uses more real memory. Rust builds can
run into the task limit (32) when sccache is active; if a build fails with
"Unable to install ctrlc handler", run it as `RUSTC_WRAPPER= CARGO_BUILD_JOBS=2
bin/c55x-contained --job work --env RUSTC_WRAPPER --env CARGO_BUILD_JOBS -- ./rust/build.sh`.

Exit 75 means all eight emulator slots are busy: wait. Exit 79 or 124 means the
job hit its memory or time limit: reduce the work. Never raise a limit, kill
another job, clear a lock, or edit cgroup files yourself. Details and the
exit-code table: `CONTAINMENT.md`.

## The reference core

`build/py/c55x_core.so` is the reference core. Build it only with
`rust/build.sh` from a clean commit and confirm its SHA256 against
`ORACLE_V2.md`. Do not replace it, copy another `c55x_core.so` over it, or
point `ZEN_CORE_DIR` at a core of unknown origin. Moving any pin to a new core
build needs explicit confirmation from the repository owner.

Earlier reference builds (their SHA256 values are in `ORACLE_V2.md`) may be kept
in other `build/` directories, but the guarded ones (6891ce79, 24f97bac) refuse
to run under the current shared limits; compare against outputs recorded with
them instead. `build/afce/` has no containment guard: never use it for new
work. The behavior differences between builds are listed in `CHANGELOG.md`.

## Tests

Run the full suite inside the launcher. Two example tests (`hello_mcu`) fail
until the example programs are built with the TI tools; that is expected.

## Looking up instruction behavior

Authority order: TI tools (`asm55` / `dis55`) for encodings, TI manuals for
semantics read across SPRU374E, SPRU371F and the v3.x CPU guide, and this
emulator only as a differential partner. When it disagrees with the manuals,
record both citations and tell the owner. Details, section numbers and the
traps already hit: `.claude/skills/c55x-manuals/SKILL.md`. Cite section numbers;
do not paste manual text into the repository.
