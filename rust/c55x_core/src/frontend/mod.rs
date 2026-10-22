//! dis55 instruction text -> instruction IR (Ast) without Python: a port of
//! c55x_emu/expr.py, ir.py and the listing reader of listing.py. A form ir.py rejects
//! is an Error::Unsupported with the text of the Python exception.

mod expr;
mod ir;
pub mod listing;
mod regs;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use crate::ast::Ast;
use crate::err::{unsupported, R};

pub use ir::Resolve;
pub use listing::Insn;

/// Branch targets as Cpu.resolve reads them: '#<int>' or a symbol of the image.
pub fn resolver(symbols: &HashMap<String, i64>) -> impl Fn(Option<&str>) -> R<i64> + '_ {
    move |target| match target {
        None => unsupported("'NoneType' object has no attribute 'startswith'"),
        Some(t) if t.starts_with('#') => expr::py_int(&t[1..], 0),
        Some(t) => match symbols.get(t) {
            Some(&a) => Ok(a),
            None => unsupported(format!("unknown branch target {}", t)),
        },
    }
}

/// IR of one instruction (ir.build).
pub fn build(insn: &Insn, resolve: Resolve<'_>) -> R<Ast> {
    ir::build(&insn.text, insn.addr as i64, insn.size as i64, insn.block_end.map(|e| e as i64), resolve)
}

/// IR from the fields of an instruction.
pub fn build_text(text: &str, addr: i64, size: i64, block_end: Option<i64>, resolve: Resolve<'_>) -> R<Ast> {
    ir::build(text, addr, size, block_end, resolve)
}
