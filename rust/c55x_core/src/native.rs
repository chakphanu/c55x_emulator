//! A host without Python: memory from an image, the decoded instructions of a listing,
//! no peripherals. The pages holding decoded instructions are code pages; a data write
//! into one, a hooked word or an I/O port access is an error (counted), since nothing
//! is decoded at run time and no peripheral is modeled.

use std::any::Any;
use std::collections::HashMap;

use crate::core::{Core, MEM_BYTES};
use crate::err::{emu, R};
use crate::host::{Ctx, Decoded, Host};

pub struct NativeHost {
    mem: Vec<u8>,                           // MEM_BYTES; its buffer never moves
    decoded: HashMap<u32, Decoded>,         // pc -> instruction
    code_pages: Vec<bool>,                  // byte page (pc >> 8) holds a decoded instruction
    pub unmodeled_ports: u64,
    pub code_page_writes: u64,
}

impl NativeHost {
    /// A core over memory (MEM_BYTES bytes) and the decoded instructions (pc -> instruction).
    pub fn core(mem: Vec<u8>, decoded: HashMap<u32, Decoded>) -> R<Core> {
        if mem.len() != MEM_BYTES {
            return emu(format!("memory of {} bytes, not {}", mem.len(), MEM_BYTES));
        }
        let mut code_pages = vec![false; MEM_BYTES >> 8];
        for &pc in decoded.keys() {
            code_pages[(pc as usize & (MEM_BYTES - 1)) >> 8] = true;
        }
        let mut host = Box::new(NativeHost { mem, decoded, code_pages: code_pages.clone(),
                                             unmodeled_ports: 0, code_page_writes: 0 });
        let mem = host.mem.as_mut_ptr();
        Ok(Core::new(host, mem, code_pages))
    }
}

impl Host for NativeHost {
    fn read_hooked(&mut self, _ctx: &mut Ctx, word: u32) -> R<u32> {
        emu(format!("hooked data word 0x{:06x} without a peripheral model", word))
    }

    fn write_hooked(&mut self, _ctx: &mut Ctx, word: u32, _v: u32) -> R<()> {
        // without hooks only code pages reach the host
        self.code_page_writes += 1;
        emu(format!("data write into a code page at word 0x{:06x}", word))
    }

    fn port_read(&mut self, _ctx: &mut Ctx, port: u32) -> R<u32> {
        self.unmodeled_ports += 1;
        emu(format!("unmodeled port 0x{:04x} read", port))
    }

    fn port_write(&mut self, _ctx: &mut Ctx, port: u32, _v: u32) -> R<()> {
        self.unmodeled_ports += 1;
        emu(format!("unmodeled port 0x{:04x} written", port))
    }

    fn fire_events(&mut self, ctx: &mut Ctx) -> R<()> {
        ctx.next_event = u64::MAX;
        Ok(())
    }

    fn clock_stopped(&mut self, _ctx: &mut Ctx) -> R<bool> {
        Ok(false)
    }

    fn idle_execute(&mut self, _ctx: &mut Ctx) -> R<Option<bool>> {
        Ok(None)
    }

    fn audio_clock(&mut self, _ctx: &mut Ctx) -> R<Option<(f64, f64)>> {
        Ok(None)
    }

    fn disk_read(&mut self, _ctx: &mut Ctx, _lba: u64) -> R<Vec<u8>> {
        emu("ATA access without an attached disk".into())
    }

    fn disk_write(&mut self, _ctx: &mut Ctx, _lba: u64, _data: &[u8]) -> R<()> {
        emu("ATA access without an attached disk".into())
    }

    fn decode(&mut self, pc: u32) -> R<Option<Decoded>> {
        Ok(self.decoded.get(&pc).cloned())
    }

    fn code_pages_around(&mut self, first: u32, last: u32, out: &mut Vec<(u32, bool)>) -> R<()> {
        out.extend((first..=last).map(|p| (p, self.code_pages[p as usize])));
        Ok(())
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Ast;
    use crate::core::Body;

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
    fn memory_code_pages_and_compile_without_python() {
        let mut decoded = HashMap::new();
        decoded.insert(0x20000, Decoded { size: 2, text: "AC0 = #5".into(), ir: Ok(assign_ac0_5()) });
        let mut core = NativeHost::core(vec![0; MEM_BYTES], decoded).ok().unwrap();
        core.write(0x1600, 0x1234).ok().unwrap();
        assert_eq!(core.read(0x1600).ok(), Some(0x1234));
        assert_eq!(core.mem_byte(0x2C00), 0x12);
        core.load_bytes(0x2C02, &[0xAB, 0xCD]);
        assert_eq!(core.read(0x1601).ok(), Some(0xABCD));
        assert!(core.write(0x10000, 1).is_err());      // byte page 0x200 holds code
        assert!(core.port_read(0x0800).is_err());
        let c = core.fetch(0x20000).ok().unwrap();
        assert!(matches!(c.body, Body::Native(_)));
        assert!(core.fetch(0x20002).is_err());
        let host: &NativeHost = core.host.as_any().downcast_ref().unwrap();
        assert_eq!((host.code_page_writes, host.unmodeled_ports), (1, 1));
    }
}
