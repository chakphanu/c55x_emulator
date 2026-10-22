//! Core: CPU state, memory (a byte buffer owned by the host), host callbacks and the
//! compiled instruction cache. With the Python host, Python stays authoritative between
//! runs: run() loads the state from the Python Cpu and stores it back. During a run, host
//! code (peripherals, events) sees a synchronized view: cycles, pc and IFR before every
//! callback (host.rs).

use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};
use std::rc::Rc;

use crate::ata::{self, Ata};
use crate::dma::Dma;
use crate::err::{emu, Error, R};
use crate::host::{Ctx, Host};
use crate::icache::{ICache, PcFilter, MAX_CACHED_INSNS};
use crate::mcbsp::McBsp;
use crate::state::State;
use crate::stmt::{compile_insn, Insn};

pub const MEM_BYTES: usize = 1 << 24;
/// An image cannot exceed the 24-bit program byte address space.
pub const MAX_IMAGE_BYTES: usize = MEM_BYTES;
/// Most pcs a trace may hold (4 Mi entries = 16 MiB): a resource cap, not architectural
/// state.
pub const MAX_TRACE_LEN: usize = 1 << 22;
pub const MMR_END: u32 = 0x60;
const WINDOW_PAGES: u32 = 1024 / 256 + 2; // CodeCache.WINDOW plus margin

pub enum Body {
    Native(Insn),
    Unsupported(String),                // executing it raises EmulationError
}

pub struct Compiled {
    pub addr: u32,
    pub size: u32,
    pub text: String,
    pub body: Body,
}

pub struct Core {
    pub st: State,
    pub ovf_pending: bool, // overflow detected while reading the current part (SPRU374E 4.27, 4.70.3)
    pub(crate) mem: *mut u8,                       // MEM_BYTES, owned by the host, never moved
    pub host: Box<dyn Host>,
    pub(crate) hooks: Vec<(u32, u32)>,
    pub(crate) hook_lo: u32,                       // lowest hooked word: one compare on the fast path
    pub(crate) code_pages: Vec<bool>,
    cache: ICache,
    pub next_event: u64,
    pub irq_counts: [u64; 32],
    pub native: u64,
    pub compiles: u64,
    pub py_writes: u64,
    pub py_calls: u64,
    pub trace: Option<(usize, std::collections::VecDeque<u32>)>, // (max length, executed pcs)
    pub odd_longs: Option<std::collections::BTreeMap<u32, u64>>, // pc -> long accesses at odd addresses
    pub bios_idle: Option<(u32, u32)>,
    pub poll_loops: PcFilter,
    pub dma: Dma,
    pub mcbsp: [McBsp; 2],
    pub pending_sync: VecDeque<u32>,
    pub flushing: bool,
    pub timers: BinaryHeap<Reverse<(u64, usize, u32)>>, // (cycle, McBSP, generation)
    pub audio: Vec<(f64, Vec<i16>)>,                    // (sample rate, interleaved L/R)
    pub audio_samples: usize,                           // samples buffered in audio (cap: MAX_AUDIO_SAMPLES)
    pub ata: Option<Box<Ata>>,                          // attached by ata.py
}

impl Core {
    /// A core over host, with mem pointing to MEM_BYTES bytes the host keeps alive and
    /// never moves, and the pages already holding decoded code.
    pub fn new(host: Box<dyn Host>, mem: *mut u8, code_pages: Vec<bool>) -> Core {
        Core {
            st: State::default(), ovf_pending: false, mem, host, hooks: Vec::new(), hook_lo: u32::MAX, code_pages,
            cache: ICache::new(), next_event: u64::MAX,
            irq_counts: [0; 32], native: 0, compiles: 0, py_writes: 0, py_calls: 0, trace: None, odd_longs: None, bios_idle: None,
            poll_loops: PcFilter::default(), dma: Dma::default(), mcbsp: Default::default(),
            pending_sync: VecDeque::new(), flushing: false, timers: BinaryHeap::new(), audio: Vec::new(), audio_samples: 0, ata: None,
        }
    }

    /// Record the last max_len executed pcs (0 switches tracing off).
    pub fn set_trace(&mut self, max_len: usize) -> R<()> {
        if max_len > MAX_TRACE_LEN {
            return emu(format!("trace length {} exceeds the maximum {}", max_len, MAX_TRACE_LEN));
        }
        self.trace = if max_len == 0 { None } else { Some((max_len, Default::default())) };
        Ok(())
    }

    /// The hooked data ranges of the host (peripherals register after the Cpu exists).
    pub fn set_hooks(&mut self, hooks: Vec<(u32, u32)>) {
        self.hooks = hooks;
        if self.ata.is_some() {                 // served by ata.rs in the hooked path
            self.hooks.extend([(ata::BASE, ata::BASE + 8), (ata::ALT, ata::ALT + 1)]);
        }
        self.hook_lo = self.hooks.iter().map(|&(lo, _)| lo).min().unwrap_or(u32::MAX);
    }

    // ---- host callbacks ----------------------------------------------------------------
    /// Call into the host with cycles, pc and IFR synchronized; picks up IFR, the next
    /// event and code invalidations afterwards.
    pub fn host<T>(&mut self, f: impl FnOnce(&mut dyn Host, &mut Ctx) -> R<T>) -> R<T> {
        self.py_calls += 1;
        let mut ctx = Ctx { cycles: self.st.cycles, pc: self.st.pc, ifr0: self.st.ifr0, ifr1: self.st.ifr1,
                            next_event: self.next_event, dropped: Vec::new() };
        let r = f(&mut *self.host, &mut ctx);
        self.st.ifr0 = ctx.ifr0;
        self.st.ifr1 = ctx.ifr1;
        self.next_event = ctx.next_event;
        for p in ctx.dropped {
            if p >= 0 && (p as usize) < self.code_pages.len() {
                self.code_pages[p as usize] = false;
            }
            if p >= 0 {
                self.cache.drop_page(p as u32);
            }
        }
        r
    }

    pub fn fire_events(&mut self) -> R<()> {
        self.host(|h, ctx| h.fire_events(ctx))
    }

    pub fn clock_stopped(&mut self) -> R<bool> {
        self.host(|h, ctx| h.clock_stopped(ctx))
    }

    // ---- instructions -------------------------------------------------------------------
    #[inline]
    pub fn fetch(&mut self, pc: u32) -> R<Rc<Compiled>> {
        match self.cache.get(pc) {
            Some(c) => Ok(c.clone()),
            None => self.compile_at(pc),
        }
    }

    /// Cache miss: decode through the host and compile.
    #[cold]
    #[inline(never)]
    fn compile_at(&mut self, pc: u32) -> R<Rc<Compiled>> {
        self.compiles += 1;
        if self.cache.len() >= MAX_CACHED_INSNS {
            return emu(format!("code cache full: {} instructions (max {}); possible runaway decode",
                               self.cache.len(), MAX_CACHED_INSNS));
        }
        let Some(d) = self.host.decode(pc)? else {
            return emu(format!("no decoded instruction at 0x{:06x}", pc));
        };
        let body = match d.ir {
            Err(why) => Body::Unsupported(why),
            Ok(ir) => match compile_insn(&ir, pc, d.size) {
                Ok(i) => Body::Native(i),
                Err(Error::Unsupported(why)) => Body::Unsupported(why),
                Err(e) => return Err(e),
            },
        };
        let compiled = Compiled { addr: pc, size: d.size, text: d.text, body };
        self.sync_code_pages(pc)?;
        let c = Rc::new(compiled);
        self.cache.insert(pc, c.clone());
        Ok(c)
    }

    /// Mirror the host's code pages around pc (a decode adds a window of pages): writes
    /// into pages holding decoded instructions go through the host, which invalidates.
    fn sync_code_pages(&mut self, pc: u32) -> R<()> {
        let first = (pc >> 8).saturating_sub(1);
        let last = ((pc >> 8) + WINDOW_PAGES).min(self.code_pages.len() as u32 - 1);
        let mut pages = Vec::new();
        self.host.code_pages_around(first, last, &mut pages)?;
        for (p, used) in pages {
            self.code_pages[p as usize] = used;
        }
        Ok(())
    }

    /// The idle instruction asks the idle configuration (idlectl.py): CPU domain idle?
    pub fn idle_execute(&mut self) -> R<bool> {
        let r = self.host(|h, ctx| h.idle_execute(ctx))?;
        match r { Some(v) => Ok(v), None => emu("idle without an idle control model".into()) }
    }

    /// Run a compiled instruction; an unsupported form is an EmulationError.
    pub fn exec_compiled(&mut self, c: &Compiled) -> R<Option<u32>> {
        match &c.body {
            Body::Native(i) => crate::stmt::exec(self, i),
            Body::Unsupported(why) => emu(format!("0x{:06x}: '{}': {}", c.addr, c.text, why)),
        }
    }

    /// Only the address phase of a compiled instruction (stmt::exec_address_phase).
    pub fn exec_address_phase(&mut self, c: &Compiled) -> R<()> {
        match &c.body {
            Body::Native(i) => crate::stmt::exec_address_phase(self, i),
            Body::Unsupported(why) => emu(format!("0x{:06x}: '{}': {}", c.addr, c.text, why)),
        }
    }
}
