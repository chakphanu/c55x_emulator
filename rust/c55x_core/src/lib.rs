//! TMS320C55x CPU core: CPU state and semantics, the instruction compiler (from the Ast
//! of an instruction), the containment guard and the Host interface. No Python: the
//! Python extension (rust/c55x_py) and the native command-line host (rust/c55x_run) are
//! built on it.

pub mod addr;
pub mod ast;
pub mod compile;
pub mod core;
pub mod dma;
pub mod dest;
pub mod dsp;
pub mod err;
pub mod expr;
pub mod fast;
pub mod frontend;
pub mod fuse;
pub mod guard;
pub mod host;
pub mod icache;
pub mod mcbsp;
pub mod mem;
pub mod native;
pub mod parts;
pub mod policy;
pub mod run;
pub mod state;
pub mod stmt;
pub mod timers;
