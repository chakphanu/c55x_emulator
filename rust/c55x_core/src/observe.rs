//! Opt-in observation of a run, off by default (`Core.obs` is None): ordered memory write
//! events, the state before chosen instructions, and routine-entry snapshots. Observation
//! only: nothing here changes guest execution, values, flags, cycles or scheduling. Every
//! buffer is bounded; what does not fit is counted as dropped or omitted, never evicted.

use std::collections::HashSet;

use crate::state::State;

/// Most events or snapshots one buffer may hold (a resource cap, like MAX_TRACE_LEN).
pub const MAX_EVENTS: usize = 1 << 22;
/// Most pcs one filter may name.
pub const MAX_PCS: usize = 4096;
/// Most bytes all entry snapshots of one observer may hold (state, windows and patches).
pub const MAX_SNAPSHOT_BYTES: usize = 256 << 20;
/// Bytes charged for one snapshot's state.
pub const SNAPSHOT_STATE_BYTES: usize = 512;

/// What caused a memory write.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Origin {
    /// A guest instruction (its pc is in the event).
    Insn,
    /// The call prologue of the public `Core::call` entry (no guest instruction).
    CallEntry,
    /// Taking an interrupt (the context saved on the stacks), or its vector slot instruction.
    Interrupt,
    /// A peripheral model (DMA) while time passes.
    Peripheral,
}

/// Who is writing: the origin, pc and repeat iteration attached to write events.
#[derive(Clone, Copy, Debug)]
pub struct Context {
    pub origin: Origin,
    pub pc: Option<u32>,
    pub iteration: Option<u32>,
}

/// How a pc-filtered instruction is about to run.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExecMode {
    Execute,
    /// Skipped by the condition of the previous instruction (AD_Unit): nothing runs.
    Skipped,
    /// Only its address phase runs (the previous instruction's D_Unit condition was false).
    AddressPhase,
}

/// One memory write at its word address, in execution order.
#[derive(Clone, Debug)]
pub struct WriteEvent {
    pub seq: u64,
    pub ordinal: u64,           // Core.native when it happened
    pub pc: Option<u32>,        // the instruction doing it; None for the call prologue
    pub iteration: Option<u32>, // repeat iteration of a repeated instruction
    pub origin: Origin,
    pub word: u32,
    pub value: u16,             // the value written (for an MMR: the value the instruction wrote)
    pub mmr: bool,              // word < 0x60
    pub committed: bool,        // false: the write was refused (code page, MMR error)
}

/// The state before an instruction at a filtered pc.
#[derive(Clone)]
pub struct PcEvent {
    pub seq: u64,
    pub ordinal: u64,
    pub pc: u32,
    pub iteration: Option<u32>,
    pub mode: ExecMode,
    pub condition: Option<bool>, // a condition this instruction evaluated (XCC, XCCPART, flow)
    pub state: State,
}

/// The state and memory just before the first instruction of a routine entered by a call.
#[derive(Clone)]
pub struct EntrySnapshot {
    pub seq: u64,
    pub ordinal: u64,
    pub pc: u32,
    pub state: State,
    pub windows: Vec<(u32, Vec<u16>)>,
    pub patch: Option<Vec<(u32, u16, u16)>>, // (word, baseline, now) for every changed word
}

/// A buffer of at most cap items; later items are counted, not stored.
pub struct Bounded<T> {
    pub cap: usize,
    pub items: Vec<T>,
    pub dropped: u64,
}

impl<T> Bounded<T> {
    pub fn new(cap: usize) -> Bounded<T> {
        Bounded { cap: cap.min(MAX_EVENTS), items: Vec::new(), dropped: 0 }
    }

    /// Store item if there is room (-> its index).
    pub fn push(&mut self, item: T) -> Option<usize> {
        if self.items.len() < self.cap {
            self.items.push(item);
            Some(self.items.len() - 1)
        } else {
            self.dropped += 1;
            None
        }
    }
}

pub struct Writes {
    pub mmr: bool,              // also record writes to words below 0x60
    pub events: Bounded<WriteEvent>,
}

pub struct PcStates {
    pub pcs: HashSet<u32>,
    pub events: Bounded<PcEvent>,
}

pub struct Entries {
    pub pcs: HashSet<u32>,
    pub windows: Vec<(u32, u32)>,       // (first word, words), words >= 0x60
    pub baseline: Option<Vec<u8>>,      // memory to patch against (the whole memory)
    pub max_bytes: usize,
    pub bytes: usize,
    pub snapshots: Bounded<EntrySnapshot>,
    pub omitted_bytes: u64,             // hits not stored because of max_bytes
    pub pending: Option<u32>,           // a call to a filtered pc, not yet at its first instruction
}

pub struct Observer {
    pub seq: u64,
    pub origin: Origin,
    pub pc: Option<u32>,
    pub iteration: Option<u32>,
    pub condition: Option<bool>,
    pub writes: Option<Writes>,
    pub pc_states: Option<PcStates>,
    pub entries: Option<Entries>,
}

impl Observer {
    pub fn new(writes: Option<Writes>, pc_states: Option<PcStates>, entries: Option<Entries>) -> Observer {
        Observer { seq: 0, origin: Origin::Insn, pc: None, iteration: None, condition: None,
                   writes, pc_states, entries }
    }

    pub fn next_seq(&mut self) -> u64 {
        self.seq += 1;
        self.seq - 1
    }
}

impl Writes {
    pub fn new(cap: usize, mmr: bool) -> Writes {
        Writes { mmr, events: Bounded::new(cap) }
    }
}

impl PcStates {
    pub fn new(pcs: HashSet<u32>, cap: usize) -> PcStates {
        PcStates { pcs, events: Bounded::new(cap) }
    }
}

impl Entries {
    pub fn new(pcs: HashSet<u32>, windows: Vec<(u32, u32)>, baseline: Option<Vec<u8>>, cap: usize,
               max_bytes: usize) -> Entries {
        Entries { pcs, windows, baseline, max_bytes: max_bytes.min(MAX_SNAPSHOT_BYTES), bytes: 0,
                  snapshots: Bounded::new(cap), omitted_bytes: 0, pending: None }
    }
}

/// Every data word that differs from baseline: (word, before, now), ascending.
pub fn memory_patch(baseline: &[u8], now: &[u8]) -> Vec<(u32, u16, u16)> {
    let mut out = Vec::new();
    for (k, (a, b)) in baseline.chunks_exact(8).zip(now.chunks_exact(8)).enumerate() {
        if a == b { continue; }
        for w in 0..4 {
            let (x, y) = (&a[2 * w..2 * w + 2], &b[2 * w..2 * w + 2]);
            if x != y {
                out.push(((4 * k + w) as u32, u16::from_be_bytes([x[0], x[1]]), u16::from_be_bytes([y[0], y[1]])));
            }
        }
    }
    out
}

impl crate::core::Core {
    /// mem.rs: a memory write at word (value: its 16 bits; committed: it took effect).
    #[inline(never)]
    pub(crate) fn observe_write(&mut self, word: u32, value: u32, committed: bool) {
        let ordinal = self.native;
        let Some(o) = self.obs.as_mut() else { return };
        let mmr = word < crate::core::MMR_END;
        match &o.writes {
            Some(w) if w.mmr || !mmr => {}
            _ => return,
        }
        let seq = o.next_seq();
        let pc = if o.origin == Origin::CallEntry { None } else { o.pc };
        let event = WriteEvent { seq, ordinal, pc, iteration: o.iteration, origin: o.origin, word,
                                 value: value as u16, mmr, committed };
        if let Some(w) = o.writes.as_mut() {
            w.events.push(event);
        }
    }

    /// run.rs / stmt.rs: the instruction at pc is about to run (iteration: of a repeated
    /// instruction). Takes an entry snapshot when a call to a filtered pc arrives here and
    /// records the state when pc is filtered. -> the index of the recorded state event.
    #[inline(never)]
    pub(crate) fn observe_before(&mut self, pc: u32, iteration: Option<u32>, mode: ExecMode) -> Option<usize> {
        let Some(mut o) = self.obs.take() else { return None };
        o.origin = Origin::Insn;
        o.pc = Some(pc);
        o.iteration = iteration;
        o.condition = None;
        if let Some(e) = o.entries.as_mut() {
            if iteration.is_none() && e.pending == Some(pc) {
                e.pending = None;
                let mem = self.memory();
                let word = |w: u32| -> u16 {
                    let b = (2 * w) as usize & (crate::core::MEM_BYTES - 1);
                    u16::from_be_bytes([mem[b], mem[b + 1]])
                };
                let windows: Vec<(u32, Vec<u16>)> =
                    e.windows.iter().map(|&(a, n)| (a, (a..a + n).map(word).collect())).collect();
                let patch = e.baseline.as_ref().map(|b| memory_patch(b, mem));
                let bytes = SNAPSHOT_STATE_BYTES + 2 * windows.iter().map(|(_, v)| v.len()).sum::<usize>()
                    + 8 * patch.as_ref().map_or(0, |p| p.len());
                if e.bytes + bytes > e.max_bytes {
                    e.omitted_bytes += 1;
                } else {
                    let seq = o.seq;
                    let snapshot = EntrySnapshot { seq, ordinal: self.native, pc, state: self.st.clone(), windows, patch };
                    if e.snapshots.push(snapshot).is_some() {
                        e.bytes += bytes;
                        o.seq += 1;
                    }
                }
            }
        }
        let mut index = None;
        if let Some(p) = o.pc_states.as_mut() {
            if p.pcs.contains(&pc) {
                let counted = (mode != ExecMode::Skipped && iteration.is_none()) as u64;
                let event = PcEvent { seq: o.seq, ordinal: self.native + counted, pc, iteration, mode,
                                      condition: None, state: self.st.clone() };
                index = p.events.push(event);
                o.seq += 1;
            }
        }
        self.obs = Some(o);
        index
    }

    /// The instruction recorded at index has finished: attach the condition it evaluated.
    #[inline(never)]
    pub(crate) fn observe_after(&mut self, index: usize) {
        if let Some(o) = self.obs.as_mut() {
            let c = o.condition;
            if let Some(e) = o.pc_states.as_mut().and_then(|p| p.events.items.get_mut(index)) {
                e.condition = c;
            }
        }
    }

    /// Set the origin of the writes that follow (no repeat iteration) -> the previous context.
    #[inline(never)]
    pub(crate) fn observe_origin(&mut self, origin: Origin, pc: Option<u32>) -> Context {
        let now = Context { origin, pc, iteration: None };
        match self.obs.as_mut() {
            Some(o) => {
                let prev = Context { origin: o.origin, pc: o.pc, iteration: o.iteration };
                o.origin = origin;
                o.pc = pc;
                o.iteration = None;
                prev
            }
            None => now,
        }
    }

    /// Put back a context from observe_origin.
    #[inline(never)]
    pub(crate) fn observe_restore(&mut self, c: Context) {
        if let Some(o) = self.obs.as_mut() {
            (o.origin, o.pc, o.iteration) = (c.origin, c.pc, c.iteration);
        }
    }

    /// stmt.rs: a condition evaluated by the current instruction.
    #[inline(always)]
    pub(crate) fn observe_condition(&mut self, value: bool) {
        if let Some(o) = self.obs.as_mut() {
            o.condition = Some(value);
        }
    }

    /// run.rs: a call to target (an entry snapshot is due when target is filtered).
    #[inline(never)]
    pub(crate) fn observe_call(&mut self, target: u32) {
        if let Some(e) = self.obs.as_mut().and_then(|o| o.entries.as_mut()) {
            if e.pcs.contains(&target) {
                e.pending = Some(target);
            }
        }
    }
}
