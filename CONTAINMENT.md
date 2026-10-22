# Resource containment

The emulator can use a lot of memory (a core is 16 MiB of memory plus decoder
state, test suites build hundreds of them) and heavy builds add more. To keep
concurrent work from exhausting the host, emulator runs are placed in a
kernel-enforced cgroup and the native core refuses to run outside it.

## Use

    bin/c55x-contained -- CMD [ARG...]                 # emulator payload
    bin/c55x-contained --job work -- CMD [ARG...]      # compiler / sanitizer work
    bin/c55x-contained --status

Options, exit codes and the do/do-not list are in
`.claude/skills/c55x-contained/SKILL.md`. `build/run/c55x-run` is the guarded
reference runner (`ORACLE_V2.md`). To try a new build without touching it:

    RUSTC_WRAPPER= CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR=$PWD/build/candidate-rust \
        bin/c55x-contained --job work --env RUSTC_WRAPPER --env CARGO_BUILD_JOBS \
        --env CARGO_TARGET_DIR -- cargo build --release --manifest-path rust/Cargo.toml -p c55x_run
    bin/c55x-contained -- build/candidate-rust/release/c55x-run --script S.json --out DIR
    CARGO_TARGET_DIR=$PWD/build/candidate-rust scripts/cargo-test-contained

## Policy

| Scope | Limit |
| --- | --- |
| Shared group `c55x.slice` (all launches) | memory.max 18 GiB, memory.high 17 GiB, swap 0, 448 tasks, 1000% CPU |
| One emulator job (slot unit `c55x-emu.service` or `c55x-emu-1.service` .. `c55x-emu-9.service`), descendants included | memory 1 GiB default / 2 GiB hard, swap 0, 32 tasks, 100% CPU, oom.group |
| One job | 600 s CPU, 900 s wall, 5 s stop grace, 512 fds, no core dumps; virtual address space 8 GiB (emulator) or 64 TiB (work) |
| Admission | at most ten emulator jobs (one per slot; `--legacy-slots`: slots 0-7 only); the memory ceilings of the emulator jobs, the new one included, sum to at most the shared memory.high (17 GiB); host MemAvailable >= 16 GiB; work dir not on tmpfs |

The address-space limit is a virtual allowance and grants no RAM. A work job
gets 64 TiB because sanitizers such as ASan reserve tens of TiB of shadow
mappings at startup (16 GiB was too small for that); a job that uses more
real memory than its `--mem` is still stopped by the cgroup (exit 79).

A core built before the eight-slot change caps the shared group at the earlier
single-slot values (4 GiB, 3 GiB, 64 tasks, 200% CPU) and accepts only
`c55x-emu.service`, so it refuses to run under these limits; rebuild it.

A core built between the eight-slot and the ten-slot change (the archived
Python extension `build/archive-python/c55x_core.so` 3539b077, runner builds up
to 515dc807) accepts only slots 0-7. The shared caps are unchanged (its guard
checks them as upper bounds, the launcher's verifier for exact values), so it
runs in those slots; launch it with `--legacy-slots`, which admits an emulator
job only to slots 0-7 (exit 75 when they are busy, even if 8 or 9 is free).
Without the option it may land in slot 8 or 9 and is refused there with a
resource-policy error. Ten jobs at 100% CPU fill the shared 1000% quota.

Memory admission: ten jobs at the 2 GiB hard limit would exceed the shared
18 GiB, so an emulator launch is admitted only if its `--mem` plus the
ceilings of the other emulator slots in use stays within the shared
memory.high (17 GiB); otherwise it exits 75 (`outcome=busy memory
admission`) and the caller waits. A slot whose lock is held counts the
ceiling its launcher recorded in the lock file (`<pid> <bytes>`; a launcher
from before this rule records only the pid, then the unit's MemoryMax counts,
else 2 GiB), a unit running without its lock counts its MemoryMax. Held locks
are read from `/proc/locks`, which does not disturb other launchers; each
launcher records its own ceiling before it sums, so of two launchers
admitting at the same time at least one counts the other. Work jobs are not
counted. Run many parallel jobs with a small `--mem` (c55x-run peaks near
60 MiB; `--mem 256M`: ten jobs use 2.5 GiB of ceilings).

## How it is enforced

1. `bin/c55x-contained` starts the payload with `systemd-run --user` as a
   service in `c55x.slice`, with `OOMPolicy=kill`, `RuntimeMaxSec`, `Limit*`
   and the cgroup properties above. Before the payload starts, a verifier
   inside the job reads the real cgroup files and rlimits and execs the
   payload only if all match. Each of the ten emulator slots has a fixed
   unit name and its own lock (`emulator.lock`, `emulator-<k>.lock`): a launch
   takes the first slot whose lock is free and whose unit is inactive, failed
   or not loaded (any other state, such as deactivating during the stop
   grace, keeps the slot busy), and exits 75 when there is none. If
   `systemd-run` still cannot start the slot's unit, the launch exits 75
   without reporting the older unit's result. A job orphaned by a dead
   launcher keeps its limits, blocks only its own slot and still ends at its
   wall-time limit. `--status` lists every slot and changes nothing; the
   shared-group properties are set by real launches only.
2. `rust/c55x_core/src/policy.rs` (pyo3-free, unit tested with fake cgroup
   trees) is called when a `Core` is constructed and at execution entries. It
   resolves the cgroup2 mount from mountinfo, requires the process to be in
   `.../c55x.slice/<slot unit>` (exactly one of the ten slot unit names),
   and checks job and shared-group memory, swap, tasks, CPU,
   `memory.oom.group` and the process rlimits. Failure raises
   a "resource policy" error (`Error::Policy`; c55x-run reports it as
   `ResourcePolicyError`). Single-step calls re-check at most every
   50 ms (a full check costs ~120 us and would slow stepping 25x).
3. Emulator-level caps, independent of the cgroup: program image and memory
   loads <= 16 MiB (24-bit program space); decoded-instruction cache <= 1 Mi
   entries (engineering limit, not architectural); `set_trace` <= 4 Mi pcs;
   buffered audio <= 16 Mi samples (an explicit error instead of silent
   growth). c55x-run checks every image segment (address, size, sha256) and
   reports them in its run.

## Tests

| Scope | Command (from the repository root) |
| --- | --- |
| Core guard, outside a job (must refuse) | after `scripts/cargo-test-contained`: the containment binary of `build/rust-target/test-binaries.txt` with `--ignored outside`, run without the launcher |
| Core guard, inside a job (kernel readback, live revalidation, trace cap) | `scripts/cargo-test-contained` (rust/c55x_core/tests/containment.rs) |
| Rust tests (dropped cores free their memory: tests/lifecycle.rs) | `scripts/cargo-test-contained` |
| Launcher (real kernel, tiny budgets; run outside a job) | `C55X_RUN=<c55x-run> python3 -m unittest tests.test_c55x_launcher` |
| Pure policy logic (fake trees) | `scripts/cargo-test-contained` (the unit tests of rust/c55x_core/src/policy.rs) |
| Emulator caps (memory, loads, code cache; image size in c55x-run) | `scripts/cargo-test-contained` (rust/c55x_core/tests/resource_caps.rs, rust/c55x_run/tests/cli.rs) |

## Limits

- Protects against forgotten instructions and accidental direct calls. A
  process of the same user can edit its own delegated cgroup files or run an
  old binary; preventing that needs a separate execution identity.
- Linux with cgroup v2 and a systemd user manager only; other platforms are
  refused, not approximated.
- It bounds this workload; it does not make the host immune to OOM caused by
  unrelated software.
