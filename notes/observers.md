# Call observers of c55x-run

Opt-in observations of one call of a call script (c55x-call-script/1 or /2). They
are requested in the call's `observe` object next to the existing `final_state`,
`memory_diff_since` and `trace_pcs`, and reported in the same call entry of
run.json. A call without the keys below runs and reports exactly as before.

Observation never changes guest execution: values, flags, cycles, instruction
counts, scheduling and errors are the same with observers on or off (every test of
`rust/c55x_run/tests/observe.rs` checks this). Implementation:
`rust/c55x_core/src/observe.rs` (hooks in `mem.rs`, `run.rs`, `stmt.rs`, `dma.rs`)
and `rust/c55x_run/src/obs.rs`.

## Bounds and completeness

Every buffer has a finite cap. Nothing is evicted: once a buffer is full, later
items are counted (`dropped`, `omitted_*`) and `complete` is false. Treat a
result with `complete: false` as partial evidence. Invalid options (a cap of 0
or above its maximum, an empty pc list, a window below word 60h, a non-boolean
flag) fail the run with a ValueError before the call starts.

| Key | Option | Default | Maximum |
| --- | --- | --- | --- |
| write_events | max_events | 65536 | 4194304 |
| pc_states | pcs (list) | required | 4096 pcs |
| pc_states | max_events | 4096 | 4194304 |
| entry_snapshots | pcs (list) | required | 4096 pcs |
| entry_snapshots | max_hits | 16 | 4194304 |
| entry_snapshots | max_bytes | 16 MiB | 256 MiB |
| entry_snapshots | windows | none | 1 Mi words in all |

`seq` is one counter shared by the three observers of a call: it orders write
events, pc states and snapshots against each other. `ordinal` is the
instruction count of the call (the run.json `instructions` scale): 0 before the
first instruction, k while the k-th counted instruction runs.

## write_events

    "observe": {"write_events": {"max_events": 65536, "mmr": false}}

Every write that reaches the core's memory write (`Core::write`): instruction
stores in source order (both halves of a 32-bit store, MSW first), stack pushes,
the call prologue and DMA. Repeated and same-value writes are kept. Raw loads of
the script (`write`, `restore_image`) are not events.

    "write_events": {"complete": true, "cap": 65536, "recorded": 8, "dropped": 0,
                     "mmr_included": false,
                     "fields": ["seq", "ordinal", "pc", "iteration", "origin",
                                "word", "value", "mmr", "committed"],
                     "events": [[0, 0, null, null, "call_entry", 4095, 43981, false, true],
                                [2, 1, 131072, null, "instruction", 5632, 7, false, true], ...]}

- `origin`: `instruction` (pc = the instruction doing it; for a repeated
  instruction the body pc and `iteration` 0..n, ordinal of the repeat
  instruction), `call_entry` (the two stack stores of the public call
  prologue, pc null), `interrupt` (context saved when an interrupt is taken, pc
  null; the vector-slot instruction has the slot address), `peripheral` (a DMA
  element, pc null).
- `committed`: false when the write was refused (a code page, an MMR error); the
  run then fails as before.
- Words below 60h (MMRs) only with `"mmr": true`, flagged `mmr: true`; value is the
  value the instruction wrote. Register changes that are not memory-addressed
  stores (register names, RETI) are state, not events.

## pc_states

    "observe": {"pc_states": {"pcs": [131093, 131095], "max_events": 4096}}

The full state before the instruction at a listed pc runs (after a pending
interrupt is taken), every time it runs, in order; repeat bodies once per
iteration.

    "pc_states": {"complete": true, "cap": 4096, "recorded": 3, "dropped": 0,
                  "pcs": [...],
                  "events": [{"seq": 2, "ordinal": 1, "pc": 131093, "iteration": null,
                              "mode": "execute", "condition": false, "state": {...}}]}

- `state`: the same serializer as `final_state` (ac, t, xar, st1-st3, tc1,
  tc2, carry, acov, loop and repeat registers, reta, cfct, skip flags, ...).
- `mode`: `execute`, `skipped` (the previous instruction's AD_Unit condition was
  false: nothing runs) or `address_phase` (its D_Unit condition was false: only
  the address phase runs).
- `condition`: the value of a condition this instruction evaluated (XCC,
  XCCPART, a conditional goto, call or return), else null. RPTCC repeat
  conditions are not reported.

## entry_snapshots

    "observe": {"entry_snapshots": {"pcs": [131112], "max_hits": 16,
                                    "max_bytes": 16777216,
                                    "windows": [[5632, 3]], "patch_since_call": true}}

Taken just before the first instruction of a routine entered by a call (the
public call of the script or a guest CALL) to a listed pc: the state as it is
(RETA, CFCT, stacks, ACOV, repeat state; nothing normalized), the listed memory
windows, and with `patch_since_call` every word that differs from the memory at
the start of the call.

    "entry_snapshots": {"complete": true, "max_hits": 16, "max_bytes": 16777216,
                        "bytes": 542, "recorded": 1, "omitted_hits": 0,
                        "omitted_for_bytes": 0, "pending_entry": null, "pcs": [...],
                        "snapshots": [{"seq": 0, "ordinal": 2, "pc": 131112, "state": {...},
                                       "windows": [{"word": 5632, "values": [7, 0, 0]}],
                                       "patch_since_call": [[2046, 0, 255], [4094, 0, 65280],
                                                            [5632, 0, 7]]}]}

- A snapshot is charged 512 bytes for its state, 2 per window word and 8 per
  patch entry against `max_bytes`.
- `pending_entry`: a call to a listed pc whose first instruction never ran (for
  example a call that returns before executing anything). The result is then
  incomplete; nothing is synthesized.

## Limits

- The observers report what the core does; they say nothing about hardware.
- Writes are word events at the core's memory interface; I/O-port writes are
  not memory events.
- Hot path when off: one Option check per memory write and per step.
