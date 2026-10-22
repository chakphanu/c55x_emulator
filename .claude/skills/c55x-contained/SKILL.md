---
name: c55x-contained
description: Use BEFORE running anything that loads the C55x emulator core (python -m c55x_emu, the test suite, probes, scripts that import c55x_emu or c55x_core) or any heavy build step (cargo, sanitizer runs, large exports). Every such run must go through bin/c55x-contained, which places it in a memory-, CPU- and time-bounded cgroup. The native core refuses to run outside that cgroup.
---

# Running the emulator under resource containment

Full reference: `CONTAINMENT.md`. Agents that read `AGENTS.md` get the same rules there. This page is the short operational version.

## The one rule

Run every emulator payload, test run, probe and heavy build through
`bin/c55x-contained`. Never run them directly.

The native core (`c55x_core`) checks the cgroup hierarchy and process limits
when a `Core` is constructed and again at execution entries (`run`, `step`,
`call`, `ret`, `advance`, `ata_io`). Outside the managed cgroup it raises
`c55x_core.ResourcePolicyError`. There is no flag or environment variable to
skip the check, so do not look for one.

## Commands

    bin/c55x-contained -- python3 -m unittest discover -s tests -t .
    bin/c55x-contained -- python3 -m c55x_emu ARGS...
    bin/c55x-contained -- python3 my_probe.py --flag "literal arg"

Compilers, sanitizer runs and other heavy non-emulator work (cannot build a
`Core`; may run next to an emulator job):

    bin/c55x-contained --job work -- ./rust/build.sh
    bin/c55x-contained --job work --mem 2G -- cargo test

Read-only status of every emulator slot and of the host headroom:

    bin/c55x-contained --status

Arguments after `--` are passed literally (no shell re-parsing). Working
directory, stdin, stdout, stderr and the exit status are preserved. The job
sees `PATH`, `HOME`, `PYTHONPATH`, `LC_*`, `ZEN_*`, `C55X_*` and a few
toolchain variables; add others by name with `--env NAME` (the value never
appears on a command line). Numeric-library thread counts are forced to 1.

## Options

| Option | Default | Hard limit | Meaning |
| --- | --- | --- | --- |
| `--job emulator\|work` | `emulator` | - | Job class |
| `--mem SIZE` | `1G` | `2G` | Memory ceiling of the whole job tree |
| `--cpu-sec N` | `600` | `600` | CPU seconds (RLIMIT_CPU) |
| `--wall-sec N` | `900` | `900` | Lifetime of the job |
| `--work-dir DIR` | `<repo>/build/work` | - | TMPDIR for the job; must be disk-backed |
| `--env NAME` | - | - | Forward one more environment variable (repeatable) |

A request above a hard limit is refused (exit 77), never clamped. Limits are
never raised automatically after a failure; a larger allowance is the owner's
decision.

## Job classes

- `emulator`: up to eight at a time for this user across all worktrees and
  shells, one per slot (units `c55x-emu.service`, `c55x-emu-1.service` ..
  `c55x-emu-7.service`). A launch takes the first free slot and gets exit 75
  when all eight are busy (a slot whose unit is still stopping counts as
  busy). A job orphaned by a dead launcher keeps its limits and its slot
  until it stops by itself at its wall-time limit; nothing is killed to make
  room. Only this class can construct a `Core`.
- `work`: for compilers and sanitizers; several can run at once. Virtual
  address space is 64 TiB, because ASan and other sanitizers reserve tens of
  TiB of shadow mappings at startup. That is a virtual allowance only and
  grants no RAM: the job is still stopped at its `--mem` ceiling (exit 79).
  Emulator jobs keep 8 GiB.

All jobs share `c55x.slice`: 18 GiB memory (17 GiB high), no swap, 448 tasks,
1000 percent CPU. One job: no swap, at most 32 tasks, 100 percent CPU, 512 open
files, no core dumps, 5 s stop grace. Admission needs 16 GiB MemAvailable on
the host. Before the payload starts, a verifier inside the job reads the real
cgroup files and rlimits and refuses to continue if any differs.

## Exit codes

| Code | Outcome | What to do |
| --- | --- | --- |
| 0 | Payload succeeded | - |
| any other non-zero | The payload's own exit status (reserved codes below are remapped to 70) | Read its output |
| 70 | Incomplete result, or the payload exited with a reserved code | Treat as failure; read stderr |
| 74 | Host headroom below 16 GiB | Wait; do not free memory by killing other programs |
| 75 | All eight emulator slots busy | Wait for one; never kill or clear it |
| 77 | Policy refused (limits, work dir, user manager, verification) | Fix the reported cause; do not bypass |
| 79 | Local cgroup OOM: the job hit its memory ceiling | Reduce the workload; do not raise `--mem` yourself |
| 124 | Wall-time or CPU-time limit | Split the work; do not raise limits yourself |
| 128+N | Payload killed by signal N | Investigate |

A killed or incomplete run is never reported as success. Every non-zero
outcome prints one line on stderr starting with `c55x-contained: outcome=`.

## Do not

- Import `c55x_emu` / `c55x_core` or run the tests outside the launcher.
- Edit cgroup files, the `c55x.slice` properties or the job unit, or move
  processes between cgroups.
- Stop, kill or `reset-failed` another job's unit, or delete its lock.
- Point `--work-dir`, `TMPDIR` or build outputs at `/tmp`, `/dev/shm` or any
  other tmpfs.
- Replace the reference core `build/py/c55x_core.so` (the guarded build listed in
  `ORACLE_V2.md`), or use `build/afce/` (the older build without the guard, kept for
  regression comparison only). A new core build goes to its own output directory (`CARGO_TARGET_DIR=build/candidate-rust cargo
  build --release --manifest-path rust/Cargo.toml -p c55x_py`, copy `libc55x_py.so` to
  `<dir>/c55x_core.so`, then use it with `ZEN_CORE_DIR=<dir>`). Moving
  downstream pins to a new core needs explicit owner confirmation.

## Trust boundary and limits

This protects against forgotten instructions and accidental direct calls. It
is not a security boundary: a process of the same user can edit its own
delegated cgroup files or run an old binary. A relaxed policy or a moved
process is noticed at the first execution entry after at most 50 ms.
Requires Linux with cgroup v2 and a systemd user manager that delegates the
memory, pids and cpu controllers; elsewhere the launcher and the core refuse.
