# c55x_emulator

An emulator of the TI TMS320C55x DSP CPU. It provides the CPU, the on-chip DMA
controller and McBSP. There is no board: an emulated program sees memory, the CPU
registers, DMA and McBSP. Any other I/O port, a data write into decoded code and
the `idle` instruction are errors.

The emulator is written in Rust and is the single source of truth for every
instruction (`rust/c55x_core`). A program runs from a memory image and a precomputed
`dis55` listing; nothing is decoded at run time. The repository holds no firmware
and no vendor code. Its test programs are small synthetic sources assembled with
the TI tools.

The Zen Touch board models, the Python machine, its GUI and the Python binding
were removed. The last state with them is the tag `python-slow-path-final`
(branch `archive/python-slow-path`).

## Requirements

- A Rust toolchain (cargo).
- Linux with cgroup v2 and systemd user services, for the launcher
  `bin/c55x-contained` (Python 3 standard library only).
- Only to rebuild the test fixtures: TI C55x code generation tools 4.4.1
  (`asm55`, `lnk55`, `dis55`). They are not redistributable. Put them in your
  `PATH`, or set `C55X_TI_BIN` to their `bin` directory.

## Containment

The core refuses to run outside the launcher. Builds run as work jobs, and
anything that runs the core runs as an emulator job. See `CONTAINMENT.md`
(agents: `AGENTS.md`).

    RUSTC_WRAPPER= CARGO_BUILD_JOBS=1 bin/c55x-contained --job work \
        --env RUSTC_WRAPPER --env CARGO_BUILD_JOBS -- ./rust/build.sh
    bin/c55x-contained --status

`rust/build.sh` builds the runner and places it in `build/run/c55x-run`. The
reference build and its SHA256 are listed in `ORACLE_V2.md`.

## Run a program: c55x-run

    bin/c55x-contained -- build/run/c55x-run --script S.json --input FILE --out DIR

- `--script` names a call script (format `c55x-call-script/1`, JSON). It
  contains:
  - the memory image (`c55x-memory-image/1`: segment files with byte address,
    size and SHA256);
  - the listing (`dis55 -qq -r -g` of the loaded code);
  - the initial CPU state;
  - the calls to make.
- A call gives the entry, the registers it sets, the memory it writes before the
  call and the memory it reads afterwards. Registers a call does not set carry
  over from the previous call.
- A record loop runs a list of steps for every record of `--input` and appends
  what the steps read to the output.
- `--out` receives `output.pcm`, the appended output, and `run.json` (format
  `c55x-cli-run/1`). `run.json` holds every call's status, instruction count
  and cycles, and the hash of the output.
- The format details are in the header of `rust/c55x_run/src/main.rs`.

## Tests

    scripts/cargo-test-contained

The script builds the test binaries in a work job and runs them in one emulator
job. The tests are:

- the core's inline tests;
- the integration tests in `rust/c55x_core/tests`, one file per area: semantics,
  addressing, ACOV, MAC, long words, DSP, interrupts, DMA, McBSP, the manual
  review, oracle v2, lifecycle and containment;
- the SSOT guards, `rust/c55x_core/tests/ssot_guards.rs`.

Each test runs a snippet of a fixture program from `rust/fixtures/NAME`:

| File | Content |
|---|---|
| `prog.asm` | hand-written source |
| `prog.lst` | runtime listing |
| `prog.hex` | loaded sections |
| `prog.sym` | labels, used as entry points |

Expected values sit in the test or in a frozen `.tsv` with its origin. Rebuild
or check the fixtures with the TI tools:

    bin/c55x-contained --job work -- cargo run --release \
        --manifest-path rust/Cargo.toml -p c55x_fixtures -- check

Behavior changes of the core, with the manual section each one follows and its
evidence level: `CHANGELOG.md`. The reference builds and their SHA256 values:
`ORACLE_V2.md`.

## Environment variables

| Variable | Meaning |
| --- | --- |
| `C55X_TI_BIN` | TI C55x tools directory (fixtures only) |
| `C55X_RUN` | the c55x-run binary used by the launcher tests (default `build/rust-target/release/c55x-run`) |

## Layout

- `rust/c55x_core/`: the CPU core, with no dependency but `regex`. It contains
  instruction semantics, the dis55-text frontend (`src/frontend`), DMA, McBSP,
  the containment guard and `Core::from_listing` (memory image + listing).
- `rust/c55x_run/`: `c55x-run`, the call-script runner.
- `rust/c55x_fixtures/`: `c55x-fixtures`, which rebuilds or checks `rust/fixtures`.
- `bin/c55x-contained`: the launcher (resource containment).
- `scripts/cargo-test-contained`: build and run the Rust tests under the launcher.
- `rust/build.sh`: builds `build/run/c55x-run`.
- `tests/`: the launcher tests (Python standard library, the one Python part).
