# Resource containment

The emulator can use a lot of memory (a `Machine` is a 16 MiB bus plus decoder
state, test suites build hundreds of them) and heavy builds add more. To keep
concurrent work from exhausting the host, emulator runs are placed in a
kernel-enforced cgroup and the native core refuses to run outside it.

## Use

    bin/c55x-contained -- CMD [ARG...]                 # emulator payload
    bin/c55x-contained --job work -- CMD [ARG...]      # compiler / sanitizer work
    bin/c55x-contained --status

Options, exit codes and the do/do-not list are in
`.claude/skills/c55x-contained/SKILL.md`. `build/py/c55x_core.so` is the guarded
reference core (`ORACLE_V2.md`). To try a new core build without touching it:

    CARGO_TARGET_DIR=$PWD/build/candidate-rust cargo build --release \
        --manifest-path rust/Cargo.toml -p c55x_py
    mkdir -p build/candidate && cp build/candidate-rust/release/libc55x_py.so build/candidate/c55x_core.so
    ZEN_CORE_DIR=$PWD/build/candidate bin/c55x-contained -- python3 -m unittest discover -s tests -t .

## Policy

| Scope | Limit |
| --- | --- |
| Shared group `c55x.slice` (all launches) | memory.max 18 GiB, memory.high 17 GiB, swap 0, 448 tasks, 1000% CPU |
| One emulator job (slot unit `c55x-emu.service` or `c55x-emu-1.service` .. `c55x-emu-7.service`), descendants included | memory 1 GiB default / 2 GiB hard, swap 0, 32 tasks, 100% CPU, oom.group |
| One job | 600 s CPU, 900 s wall, 5 s stop grace, 512 fds, no core dumps; virtual address space 8 GiB (emulator) or 64 TiB (work) |
| Admission | at most eight emulator jobs (one per slot); host MemAvailable >= 16 GiB; work dir not on tmpfs |

The address-space limit is a virtual allowance and grants no RAM. A work job
gets 64 TiB because sanitizers such as ASan reserve tens of TiB of shadow
mappings at startup (16 GiB was too small for that); a job that uses more
real memory than its `--mem` is still stopped by the cgroup (exit 79).

A core built before the eight-slot change caps the shared group at the earlier
single-slot values (4 GiB, 3 GiB, 64 tasks, 200% CPU) and accepts only
`c55x-emu.service`, so it refuses to run under these limits; rebuild it.

## How it is enforced

1. `bin/c55x-contained` starts the payload with `systemd-run --user` as a
   service in `c55x.slice`, with `OOMPolicy=kill`, `RuntimeMaxSec`, `Limit*`
   and the cgroup properties above. Before the payload starts, a verifier
   inside the job reads the real cgroup files and rlimits and execs the
   payload only if all match. Each of the eight emulator slots has a fixed
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
   `.../c55x.slice/<slot unit>` (exactly one of the eight slot unit names),
   and checks job and shared-group memory, swap, tasks, CPU,
   `memory.oom.group` and the process rlimits. Failure raises
   `c55x_core.ResourcePolicyError`. Single-step calls re-check at most every
   50 ms (a full check costs ~120 us and would slow stepping 25x).
3. Emulator-level caps, independent of the cgroup: program image and `Bus`
   loads <= 16 MiB (24-bit program space); decoded-instruction cache <= 1 Mi
   entries (engineering limit, not architectural); `set_trace` <= 4 Mi pcs;
   buffered audio <= 16 Mi samples (an explicit error instead of silent
   growth). `Machine.image_info` / `resource_report()` report the loaded image
   (name, size, sections, sha256) and cache size.

## Tests

| Scope | Command (from the repository root) |
| --- | --- |
| Core guard, outside a job (must refuse) | `ZEN_CORE_DIR=<dir> python3 -m unittest tests.test_c55x_emu_containment` |
| Core guard, inside a job (kernel readback, live revalidation) | `ZEN_CORE_DIR=<dir> bin/c55x-contained -- python3 -m unittest tests.test_c55x_emu_containment` |
| Rust tests (dropped cores free their memory: tests/lifecycle.rs) | `scripts/cargo-test-contained` |
| Launcher (real kernel, tiny budgets; run outside a job) | `ZEN_CORE_DIR=<dir> python3 -m unittest tests.test_c55x_launcher` |
| Pure policy logic (fake trees) and emulator caps | `python3 -m unittest tests.test_c55x_native_policy tests.test_c55x_emu_resource_caps` |

## Limits

- Protects against forgotten instructions and accidental direct calls. A
  process of the same user can edit its own delegated cgroup files or run an
  old binary; preventing that needs a separate execution identity.
- Linux with cgroup v2 and a systemd user manager only; other platforms are
  refused, not approximated.
- It bounds this workload; it does not make the host immune to OOM caused by
  unrelated software.
