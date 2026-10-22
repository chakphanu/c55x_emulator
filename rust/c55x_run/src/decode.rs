//! Where the IR of a listed instruction comes from. The decoder is pluggable: an IR file
//! (c55x-ir/1, written by the reference Python frontend) now, the Rust text frontend
//! once it is part of the core.

use std::collections::HashMap;

use c55x_core::ast::Ast;
use serde_json::{json, Value};

/// One listing line: pc (byte address), size in bytes, end of an opened block repeat,
/// instruction text.
#[allow(dead_code)] // block_end: for the text frontend
pub struct Listed {
    pub pc: u32,
    pub size: u32,
    pub block_end: Option<u32>,
    pub text: String,
}

pub trait InsnDecoder {
    /// The IR of a listed instruction, or why it is unsupported (executing it then fails).
    fn ir(&mut self, insn: &Listed) -> Result<Ast, String>;
    /// Provenance for run.json.
    fn describe(&self) -> Value;
}

/// c55x-ir/1: {"format", "listing_sha256", "insns": [{"pc", "ir" | "unsupported"}]};
/// in "ir" a JSON array is a tuple and {"L": [...]} a list.
pub struct IrFile {
    path: String,
    sha256: String,
    irs: HashMap<u32, Result<Ast, String>>,
}

fn ast(v: &Value) -> Result<Ast, String> {
    Ok(match v {
        Value::Null => Ast::None,
        Value::Bool(b) => Ast::Bool(*b),
        Value::Number(n) => Ast::Int(n.as_i64().ok_or_else(|| format!("IR integer {} out of range", n))?),
        Value::String(s) => Ast::Str(s.clone()),
        Value::Array(a) => Ast::Tuple(a.iter().map(ast).collect::<Result<_, _>>()?),
        Value::Object(o) => match (o.len(), o.get("L")) {
            (1, Some(Value::Array(a))) => Ast::List(a.iter().map(ast).collect::<Result<_, _>>()?),
            _ => return Err("IR object other than {\"L\": [...]}".into()),
        },
    })
}

impl IrFile {
    pub fn load(path: &str, bytes: &[u8], sha256: String, listing_sha256: &str) -> Result<IrFile, String> {
        let doc: Value = serde_json::from_slice(bytes).map_err(|e| format!("{}: {}", path, e))?;
        if doc["format"] != "c55x-ir/1" {
            return Err(format!("{}: not a c55x-ir/1 file", path));
        }
        if doc["listing_sha256"] != listing_sha256 {
            return Err(format!("{}: made for another listing", path));
        }
        let mut irs = HashMap::new();
        for e in doc["insns"].as_array().ok_or_else(|| format!("{}: no insns", path))? {
            let pc = e["pc"].as_u64().ok_or_else(|| format!("{}: entry without pc", path))? as u32;
            let ir = match (&e.get("ir"), &e.get("unsupported")) {
                (Some(ir), None) => Ok(ast(ir).map_err(|m| format!("{}: 0x{:06x}: {}", path, pc, m))?),
                (None, Some(Value::String(why))) => Err(why.clone()),
                _ => return Err(format!("{}: 0x{:06x}: neither ir nor unsupported", path, pc)),
            };
            irs.insert(pc, ir);
        }
        Ok(IrFile { path: path.into(), sha256, irs })
    }
}

impl InsnDecoder for IrFile {
    fn ir(&mut self, insn: &Listed) -> Result<Ast, String> {
        match self.irs.remove(&insn.pc) {
            Some(ir) => ir,
            None => Err(format!("no IR for 0x{:06x} in {}", insn.pc, self.path)),
        }
    }

    fn describe(&self) -> Value {
        json!({"kind": "ir-file", "path": self.path, "sha256": self.sha256})
    }
}
