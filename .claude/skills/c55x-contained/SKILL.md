---
name: c55x-contained
description: Use BEFORE running anything that loads the C55x emulator core (python -m c55x_emu, the test suite, probes, scripts that import c55x_emu or c55x_core) or any heavy build step (cargo, sanitizer runs, large exports). Every such run must go through bin/c55x-contained, which places it in a memory-, CPU- and time-bounded cgroup. The native core refuses to run outside that cgroup.
---

# Running the emulator under resource containment

STATUS: interface contract. The launcher and the native guard are being
implemented against this text; until this line is removed, treat the exact
flags and exit codes as the target, not as verified behavior.

## The one rule

Run every emulator payload, test run, probe and build through
`bin/c55x-contained`. Never run them directly.

The native core (`c55x_core`) checks its own cgroup and process limits when a
`Core` is constructed and on every execution entry (`run`, `step`, `call`,
`ret`, `advance`, `ata_io`). Outside the managed cgroup it raises
`ResourcePolicyError`. There is no flag or environment variable to skip
this check, so do not look for one.

## Commands

Emulator payloads (CLI, unit tests, probes, Python scripts that use the core):

    bin/c55x-contained -- python3 -m unittest discover -s tests -t .
    bin/c55x-contained -- python3 -m c55x_emu ARGS...
    bin/c55x-contained -- python3 my_probe.py --flag "literal arg"

Builds and other heavy non-emulator work (compilers, sanitizer runs, large
exports or decompression):

    bin/c55x-contained --job work -- ./rust/build.sh
    bin/c55x-contained --job work --mem 2G -- cargo test

Status of the current emulator admission (read only):

    bin/c55x-contained --status

Arguments after `--` are passed literally (no shell re-parsing). The working
directory, stdin, stdout and stderr are the caller's. Exit status follows the
table below.

## Options

| Option | Default | Hard limit | Meaning |
| --- | --- | --- | --- |
| `--job emulator\|work` | `emulator` | - | Job class (see below) |
| `--mem SIZE` | `1G` | `2G` | Memory ceiling for the whole job, all descendants |
| `--cpu-sec N` | `600` | `600` | CPU seconds per process (RLIMIT_CPU) |
| `--wall-sec N` | `900` | `900` | Wall-clock lifetime of the job |
| `--work-dir DIR` | `build/work` | - | TMPDIR and cache location; must not be tmpfs |

Requests above a hard limit are refused with the policy exit code. Limits
are never raised automatically after a failure; asking for more after a
failure is a decision for the owner, not for a retry loop.

## Job classes

- `emulator`: at most one at a time for this user, across all worktrees and
  shells. A second launch while one is admitted gets the busy exit code.
  Only this class can construct a `Core`.
- `work`: for compilers and sanitizers. Several may run at once. Gets a
  large but finite virtual address-space limit for ASan shadow memory; its
  physical memory ceiling is the same as for the emulator class.

All jobs share one parent slice that caps the total: 4 GiB memory (3 GiB
high-water), zero swap, 64 tasks, 200 percent CPU. Each job: zero swap, at
most 32 tasks, 100 percent CPU, at most 512 open files, no core dumps,
5 second stop grace. A job is admitted only when the host reports at least
16 GiB MemAvailable.

## Exit codes and what to do

| Code | Outcome | What to do |
| --- | --- | --- |
| 0 | Payload succeeded | - |
| 1-63 | Payload's own non-zero exit (semantic failure) | Read the payload's output |
| 70 | Payload result incomplete or its exit code collides with a reserved code | Treat as failure; see stderr |
| 74 | Admission refused: host memory headroom below 16 GiB | Wait; do not free memory by killing other programs |
| 75 | Admission refused: another emulator job is running | Wait for it to finish; never kill or clear it |
| 77 | Policy refused (cgroup, limits, user manager or work dir not verifiable) | Fix the reported cause; do not bypass |
| 79 | Job hit its memory ceiling and was killed (local cgroup OOM) | Reduce the workload; do not raise --mem on your own |
| 124 | Wall or CPU time limit reached | Split the work; do not raise limits on your own |
| 128+N | Payload killed by signal N | Investigate |

A killed or incomplete run is never reported as success. On every non-zero
outcome the launcher prints one line on stderr starting with
`c55x-contained: outcome=`.

## Do not

- Import `c55x_emu` or `c55x_core`, or run `python3 -m unittest`, outside the
  launcher.
- Edit cgroup files, the `c55x.slice` properties or the job unit by hand, or
  move processes between cgroups.
- Stop, kill or `reset-failed` another job's unit, or delete its lock.
- Point `--work-dir`, `TMPDIR` or build outputs at `/tmp`, `/dev/shm` or any
  other tmpfs.
- Replace the pinned core `build/py/c55x_core.so`. A new core build goes to
  its own output directory, and moving downstream pins to it needs explicit
  owner confirmation.

## Trust boundary

This protects against forgotten instructions and accidental direct calls. It
is not a security boundary against a process that deliberately alters its
own cgroup or uses an old binary under the same user ID.

## Platform

Requires Linux with cgroup v2 and a systemd user manager that delegates the
memory, pids and cpu controllers. On other platforms the launcher refuses to
run (policy exit code); equivalent protection is not claimed.
