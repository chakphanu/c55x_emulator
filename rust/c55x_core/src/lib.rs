//! TMS320C55x CPU core: CPU state and semantics, the instruction compiler (from the Ast
//! of an instruction), the frontend that decodes a listing and the containment guard. No
//! Python: the command-line runner (rust/c55x_run) is built on it.

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
pub mod icache;
pub mod mcbsp;
pub mod mem;
pub mod observe;
pub mod parts;
pub mod policy;
pub mod run;
pub mod state;
pub mod stmt;
pub mod timers;
