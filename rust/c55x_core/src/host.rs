//! The host of a core: everything outside the CPU semantics that the core calls on cold
//! paths (hooked peripheral words, unmodeled ports, events, idle and clock models, the
//! instruction decoder, disk sectors). The hot path (memory, state, compiled
//! instructions) never goes through it. Memory is a stable byte pointer owned by the
//! host (core.rs).

use std::any::Any;

use crate::ast::Ast;
use crate::err::R;

/// The CPU view a host callback sees and may change, and what the core learns after it:
/// before every callback cycles, pc and IFR are synchronized to the host; afterwards
/// the core reads back IFR, the cycle of the next host event and the code pages whose
/// decoded instructions the host dropped (writes into code).
pub struct Ctx {
    pub cycles: u64,
    pub pc: u32,
    pub ifr0: u32,
    pub ifr1: u32,
    pub next_event: u64,
    pub dropped: Vec<i64>,
}

/// A decoded instruction: its size and text, and its IR or why it is unsupported.
pub struct Decoded {
    pub size: u32,
    pub text: String,
    pub ir: Result<Ast, String>,
}

pub trait Host {
    /// A hooked data word (peripheral registers, code pages).
    fn read_hooked(&mut self, ctx: &mut Ctx, word: u32) -> R<u32>;
    fn write_hooked(&mut self, ctx: &mut Ctx, word: u32, v: u32) -> R<()>;
    /// An I/O port not modeled by the core's DMA / McBSP.
    fn port_read(&mut self, ctx: &mut Ctx, port: u32) -> R<u32>;
    fn port_write(&mut self, ctx: &mut Ctx, port: u32, v: u32) -> R<()>;
    /// Fire the host events due at ctx.cycles.
    fn fire_events(&mut self, ctx: &mut Ctx) -> R<()>;
    /// Is the CPU clock domain stopped (idle configuration)?
    fn clock_stopped(&mut self, ctx: &mut Ctx) -> R<bool>;
    /// The idle instruction: is the CPU domain idle? None: no idle model.
    fn idle_execute(&mut self, ctx: &mut Ctx) -> R<Option<bool>>;
    /// (sample rate in Hz, CPU MHz) for the audio McBSP; None: no audio clock model.
    fn audio_clock(&mut self, ctx: &mut Ctx) -> R<Option<(f64, f64)>>;
    /// One 512-byte sector of the attached disk.
    fn disk_read(&mut self, ctx: &mut Ctx, lba: u64) -> R<Vec<u8>>;
    fn disk_write(&mut self, ctx: &mut Ctx, lba: u64, data: &[u8]) -> R<()>;
    /// Decode the instruction at pc (None: nothing decoded there).
    fn decode(&mut self, pc: u32) -> R<Option<Decoded>>;
    /// Which pages first..=last hold decoded instructions (writes there go through the
    /// host, which drops the instructions).
    fn code_pages_around(&mut self, first: u32, last: u32, out: &mut Vec<(u32, bool)>) -> R<()>;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}
