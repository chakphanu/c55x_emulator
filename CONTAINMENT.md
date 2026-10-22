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
`.claude/skills/c55x-contained/SKILL.md`. Build a new core without touching the
pinned one:

    CARGO_TARGET_DIR=$PWD/build/candidate-rust cargo build --release \
        --manifest-path rust/c55x_core/Cargo.toml
    mkdir -p build/candidate && cp build/candidate-rust/release/libc55x_core.so build/candidate/c55x_core.so
    ZEN_CORE_DIR=$PWD/build/candidate bin/c55x-contained -- python3 -m unittest discover -s tests -t .

## Policy

| Scope | Limit |
| --- | --- |
| Shared group `c55x.slice` (all launches) | memory.max 4 GiB, memory.high 3 GiB, swap 0, 64 tasks, 200% CPU |
| One emulator job (`c55x-emu.service`), descendants included | memory 1 GiB default / 2 GiB hard, swap 0, 32 tasks, 100% CPU, oom.group |
| One job | 600 s CPU, 900 s wall, 5 s stop grace, 512 fds, no core dumps |
| Admission | at most one emulator job; host MemAvailable >= 16 GiB; work dir not on tmpfs |

## How it is enforced

1. `bin/c55x-contained` starts the payload with `systemd-run --user` as a
   service in `c55x.slice`, with `OOMPolicy=kill`, `RuntimeMaxSec`, `Limit*`
   and the cgroup properties above. Before the payload starts, a verifier
   inside the job reads the real cgroup files and rlimits and execs the
   payload only if all match. The emulator job has a fixed unit name, so a job
   orphaned by a dead launcher still blocks a replacement and still ends at its
   wall-time limit.
2. `rust/c55x_core/src/policy.rs` (pyo3-free, unit tested with fake cgroup
   trees) is called when a `Core` is constructed and at execution entries. It
   resolves the cgroup2 mount from mountinfo, requires the process to be in
   `.../c55x.slice/c55x-emu.service`, and checks job and shared-group memory,
   swap, tasks, CPU, `memory.oom.group` and the process rlimits. Failure raises
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
| Core guard, inside a job (kernel readback, live revalidation) | `ZEN_CORE_DIR=<dir> bin/c55x-contained -- python3 -m unittest tests.test_c55x_emu_containment tests.test_c55x_emu_gc` |
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
