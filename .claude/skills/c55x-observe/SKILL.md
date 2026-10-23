---
name: c55x-observe
description: Use when a c55x-run call must show more than its final state and net memory difference - the order of memory writes (repeated or same-value stores, stack and outside-window writes), the state or condition at particular pcs, or the state and memory at a routine entry. Explains the opt-in observers write_events, pc_states and entry_snapshots, their caps and how to read complete/dropped.
---

# Observing a c55x-run call

Full schema and examples: `notes/observers.md`. Run c55x-run only through
`bin/c55x-contained` (skill c55x-contained).

## Pick the observer

| Question | Observer |
| --- | --- |
| Which words were written, in what order, how often, by which pc? | `write_events` |
| What were the registers, flags and modes just before pc X? Did its condition hold? | `pc_states` with `pcs: [X]` |
| What did state and memory look like when routine R was entered? | `entry_snapshots` with `pcs: [R]` |

All three go in the `observe` object of a `call` op, for example:

    {"op": "call", "label": "r", "entry": "0x020000", "regs": {...},
     "observe": {"write_events": {}, "pc_states": {"pcs": ["0x020004"]}}}

## Rules

- Off by default; a call without these keys gives the old output byte for byte.
  Observed and unobserved runs have the same values, cycles and instruction
  counts; if they ever differ, that is a bug: report it.
- Always read `complete`. False means a cap was hit (`dropped`,
  `omitted_hits`, `omitted_for_bytes`) or an entry was never reached
  (`pending_entry`). Raise the cap within its maximum or narrow the pcs; never
  treat partial evidence as complete.
- `pc_states` stores a full state per hit: list only the pcs you need, or a long
  call fills the cap. Prefer a few pcs with a larger `max_events`.
- MMR writes (words below 60h) appear only with `"mmr": true`.
- `ordinal` is the call's instruction count; `seq` orders events of all three
  observers. Repeat bodies carry `iteration`.
- The observers show what the core does, not what silicon does.
