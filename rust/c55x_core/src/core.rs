//! Core: CPU state, memory (a byte buffer the core owns), the instructions of a listing
//! decoded by the Rust frontend and the compiled instruction cache. No peripheral is
//! modeled besides DMA and McBSP: the pages holding decoded instructions are code pages,
//! and a data write into one or an I/O port access outside DMA / McBSP is an error
//! (counted), since nothing is decoded at run time.
//!
//! For tools and tests: `Core::from_listing(memory, rows, symbols)` gives a core
//! (containment-checked); then set registers in `core.st`, enter a routine with
//! `core.st.pc = core.call(entry, return_address)?` and run it with `core.run(steps,
//! &PcFilter::new(stop), None)` or `core.step()`; read memory with `core.read(word)`.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::rc::Rc;

use crate::ast::Ast;
use crate::dma::Dma;
use crate::err::{emu, Error, R};
use crate::frontend::{self, Insn as ListingInsn};
use crate::guard;
use crate::icache::{ICache, MAX_CACHED_INSNS};
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

pub enum Body {
    Native(Insn),
    Unsupported(String),                // executing it raises EmulationError
}

/// A decoded instruction: its size and text, and its IR or why it is unsupported.
pub(crate) struct Decoded {
    pub size: u32,
    pub text: String,
    pub ir: Result<Ast, String>,
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
    pub(crate) mem: *mut u8,                       // into _buf (MEM_BYTES), never moved
    _buf: Vec<u8>,                                 // the memory; only accessed through mem
    decoded: HashMap<u32, Decoded>,                // pc -> instruction
    pub(crate) code_pages: Vec<bool>,              // byte page (pc >> 8) holds a decoded instruction
    cache: ICache,
    pub(crate) audio_clock: Option<(f64, f64)>,    // (sample rate in Hz, CPU MHz) for the McBSP model
    pub unmodeled_ports: u64,                      // port accesses outside DMA / McBSP (errors)
    pub code_page_writes: u64,                     // data writes into code pages (errors)
    pub irq_counts: [u64; 32],
    pub native: u64,
    pub compiles: u64,
    pub trace: Option<(usize, std::collections::VecDeque<u32>)>, // (max length, executed pcs)
    pub odd_longs: Option<std::collections::BTreeMap<u32, u64>>, // pc -> long accesses at odd addresses
    pub dma: Dma,
    pub mcbsp: [McBsp; 2],
    pub pending_sync: VecDeque<u32>,
    pub flushing: bool,
    pub timers: BinaryHeap<Reverse<(u64, usize, u32)>>, // (cycle, McBSP, generation)
    pub audio: Vec<(f64, Vec<i16>)>,                    // (sample rate, interleaved L/R)
    pub audio_samples: usize,                           // samples buffered in audio (cap: MAX_AUDIO_SAMPLES)
}

impl Core {
    /// A core over memory (MEM_BYTES bytes) and the listed instructions, decoded by the
    /// frontend (branch targets: '#<address>' or symbols). Every row's block_end must
    /// agree with the opcode bytes in memory. Only inside the managed containment.
    pub fn from_listing(mem: Vec<u8>, rows: &[ListingInsn], symbols: &HashMap<String, i64>) -> R<Core> {
        guard::guard().map_err(|e| Error::Emu(format!("refused outside the managed containment: {}", e)))?;
        if mem.len() != MEM_BYTES {
            return emu(format!("memory of {} bytes, not {}", mem.len(), MEM_BYTES));
        }
        if rows.len() > MAX_CACHED_INSNS {
            return emu(format!("listing of {} instructions exceeds the code cache (max {})",
                               rows.len(), MAX_CACHED_INSNS));
        }
        let resolve = frontend::resolver(symbols);
        let mut decoded = HashMap::new();
        for r in rows {
            let at = r.addr as usize;
            if at + r.size as usize > MEM_BYTES {
                return emu(format!("0x{:06x}: instruction outside memory", r.addr));
            }
            let end = frontend::listing::block_end(r.addr, &mem[at..at + r.size as usize], &r.text)?;
            if end != r.block_end {
                return emu(format!("0x{:06x}: block_end of the listing disagrees with the memory bytes", r.addr));
            }
            let ir = match frontend::build(r, &resolve) {
                Ok(ast) => Ok(ast),
                Err(Error::Unsupported(why)) | Err(Error::Emu(why)) => Err(why),
                Err(e) => return Err(e),
            };
            decoded.insert(r.addr, Decoded { size: r.size, text: r.text.clone(), ir });
        }
        Core::with_decoded(mem, decoded)
    }

    /// A core over memory and already decoded instructions (pc -> instruction); the code
    /// pages are fixed here, since nothing is decoded later.
    pub(crate) fn with_decoded(mut mem: Vec<u8>, decoded: HashMap<u32, Decoded>) -> R<Core> {
        if mem.len() != MEM_BYTES {
            return emu(format!("memory of {} bytes, not {}", mem.len(), MEM_BYTES));
        }
        let mut code_pages = vec![false; MEM_BYTES >> 8];
        for &pc in decoded.keys() {
            code_pages[(pc as usize & (MEM_BYTES - 1)) >> 8] = true;
        }
        Ok(Core {
            st: State::default(), ovf_pending: false, mem: mem.as_mut_ptr(), _buf: mem, decoded, code_pages,
            cache: ICache::new(), audio_clock: None, unmodeled_ports: 0, code_page_writes: 0,
            irq_counts: [0; 32], native: 0, compiles: 0, trace: None, odd_longs: None,
            dma: Dma::default(), mcbsp: Default::default(),
            pending_sync: VecDeque::new(), flushing: false, timers: BinaryHeap::new(), audio: Vec::new(), audio_samples: 0,
        })
    }

    /// Record the last max_len executed pcs (0 switches tracing off).
    pub fn set_trace(&mut self, max_len: usize) -> R<()> {
        if max_len > MAX_TRACE_LEN {
            return emu(format!("trace length {} exceeds the maximum {}", max_len, MAX_TRACE_LEN));
        }
        self.trace = if max_len == 0 { None } else { Some((max_len, Default::default())) };
        Ok(())
    }

    /// The audio clock the McBSP model reads (timers.rs): sample rate in Hz (0 = no clock)
    /// and CPU MHz. Without one, an audio McBSP transmit is an error.
    pub fn set_audio_clock(&mut self, rate_hz: f64, cpu_mhz: f64) {
        self.audio_clock = Some((rate_hz, cpu_mhz));
    }

    // ---- instructions -------------------------------------------------------------------
    #[inline]
    pub fn fetch(&mut self, pc: u32) -> R<Rc<Compiled>> {
        match self.cache.get(pc) {
            Some(c) => Ok(c.clone()),
            None => self.compile_at(pc),
        }
    }

    /// Cache miss: compile the decoded instruction at pc.
    #[cold]
    #[inline(never)]
    fn compile_at(&mut self, pc: u32) -> R<Rc<Compiled>> {
        self.compiles += 1;
        if self.cache.len() >= MAX_CACHED_INSNS {
            return emu(format!("code cache full: {} instructions (max {}); possible runaway decode",
                               self.cache.len(), MAX_CACHED_INSNS));
        }
        let Some(d) = self.decoded.get(&pc) else {
            return emu(format!("no decoded instruction at 0x{:06x}", pc));
        };
        let body = match &d.ir {
            Err(why) => Body::Unsupported(why.clone()),
            Ok(ir) => match compile_insn(ir, pc, d.size) {
                Ok(i) => Body::Native(i),
                Err(Error::Unsupported(why)) => Body::Unsupported(why),
                Err(e) => return Err(e),
            },
        };
        let c = Rc::new(Compiled { addr: pc, size: d.size, text: d.text.clone(), body });
        self.cache.insert(pc, c.clone());
        Ok(c)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn s(x: &str) -> Ast { Ast::Str(x.into()) }

    /// ir.py's IR of 'AC0 = #5' (no qualifiers, flow, condition or repeat).
    fn assign_ac0_5() -> Ast {
        let part = Ast::Tuple(vec![Ast::Bool(false), Ast::Tuple(vec![
            s("assign"), Ast::Tuple(vec![s("reg"), s("AC0")]), Ast::Tuple(vec![s("num"), Ast::Int(5)])])]);
        Ast::Tuple(vec![s("insn"), Ast::Bool(false), Ast::Bool(false), Ast::Bool(false),
                        Ast::List(vec![part]), Ast::None, Ast::None, Ast::None,
                        Ast::Tuple(vec![Ast::Bool(false), Ast::List(vec![])])])
    }

    #[test]
    fn memory_code_pages_and_compile() {
        let mut decoded = HashMap::new();
        decoded.insert(0x20000, Decoded { size: 2, text: "AC0 = #5".into(), ir: Ok(assign_ac0_5()) });
        let mut core = Core::with_decoded(vec![0; MEM_BYTES], decoded).ok().unwrap();
        core.write(0x1600, 0x1234).ok().unwrap();
        assert_eq!(core.read(0x1600).ok(), Some(0x1234));
        assert_eq!(core.mem_byte(0x2C00), 0x12);
        core.load_bytes(0x2C02, &[0xAB, 0xCD]).ok().unwrap();
        assert!(core.load_bytes(MEM_BYTES as u32 - 1, &[1, 2]).is_err());     // refused, not wrapped
        assert_eq!(core.mem_byte(0), 0);
        assert_eq!(core.read(0x1601).ok(), Some(0xABCD));
        assert!(core.write(0x10000, 1).is_err());      // byte page 0x200 holds code
        assert!(core.port_read(0x0800).is_err());
        let c = core.fetch(0x20000).ok().unwrap();
        assert!(matches!(c.body, Body::Native(_)));
        assert!(core.fetch(0x20002).is_err());
        assert_eq!((core.code_page_writes, core.unmodeled_ports), (1, 1));
    }

    #[test]
    fn trace_length_is_capped() {
        let mut core = Core::with_decoded(vec![0; MEM_BYTES], HashMap::new()).ok().unwrap();
        assert!(core.set_trace(MAX_TRACE_LEN).is_ok());
        assert!(core.set_trace(MAX_TRACE_LEN + 1).is_err());
        assert!(core.set_trace(0).is_ok() && core.trace.is_none());
    }

    #[test]
    fn audio_clock_is_plain_data() {
        let mut core = Core::with_decoded(vec![0; MEM_BYTES], HashMap::new()).ok().unwrap();
        assert!(core.audio_clock().is_err());           // no clock model set
        core.set_audio_clock(48000.0, 200.0);
        assert_eq!(core.audio_clock().ok(), Some((48000.0, 200.0)));
    }

    #[test]
    fn from_listing_is_containment_checked() {
        if guard::guard().is_ok() {
            return;                             // inside an emulator job: nothing to refuse
        }
        let rows = [ListingInsn { addr: 0x20000, size: 2, text: "AC0 = #5".into(), block_end: None }];
        match Core::from_listing(vec![0; MEM_BYTES], &rows, &HashMap::new()) {
            Err(Error::Emu(m)) => assert!(m.contains("containment")),
            _ => panic!("built a core outside the containment"),
        }
    }
}
