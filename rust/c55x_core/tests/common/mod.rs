//! Shared helpers of the integration tests: a synthetic program of rust/fixtures runs on a
//! core over NativeHost, decoded by c55x_core::frontend from the fixture listing. The core
//! is built only after the containment guard passes, so these tests run in an emulator
//! job of bin/c55x-contained (scripts/cargo-test-contained), never outside it.

#![allow(dead_code)] // each test binary uses a part of the helpers

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::PathBuf;

use c55x_core::core::{Core, MEM_BYTES};
use c55x_core::err::Error;
use c55x_core::frontend::{self, listing::Insn};
use c55x_core::host::Decoded;
use c55x_core::icache::PcFilter;
use c55x_core::native::NativeHost;

/// Steps testutil.run allowed before the idle loop must be reached.
pub const MAX_STEPS: u64 = 10_000;

/// The CPU state after reset_c_int00 (Cpu.__init__): ST1 = CPL | INTM | SXMD, ST2 = ARMS,
/// IVPD / IVPH at their reset value; the stacks of testutil.machine.
const ST1_INIT: u32 = 1 << 14 | 1 << 11 | 1 << 8;
const ST2_INIT: u32 = 1 << 15;
const XSP_INIT: u32 = 0x1000;
const SSP_INIT: u32 = 0x0800;

pub fn message(e: Error) -> String {
    match e {
        Error::Emu(m) | Error::Unsupported(m) => m,
        Error::Host(_) => "host error".into(),
    }
}

/// A fixture directory rust/fixtures/NAME: listing, loaded sections, symbols.
pub struct Fixture {
    pub name: String,
    pub insns: BTreeMap<u32, Insn>,
    pub sections: Vec<(u32, Vec<u8>)>,
    pub symbols: HashMap<String, i64>,
}

impl Fixture {
    pub fn load(name: &str) -> Fixture {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures").join(name);
        let read = |f: &str| fs::read_to_string(dir.join(f)).unwrap_or_else(|e| panic!("{}/{}: {}", name, f, e));
        let insns = frontend::listing::parse(&read("prog.lst"), true)
            .unwrap_or_else(|e| panic!("{}/prog.lst: {}", name, message(e)));
        let mut sections: Vec<(u32, Vec<u8>)> = Vec::new();
        for line in read("prog.hex").lines().filter(|l| !l.starts_with('#')) {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() == 3 {
                sections.push((u32::from_str_radix(f[1].trim_start_matches("0x"), 16).unwrap(), Vec::new()));
            } else {
                let data = &mut sections.last_mut().expect("prog.hex: bytes before a section line").1;
                data.extend((0..line.len()).step_by(2).map(|i| u8::from_str_radix(&line[i..i + 2], 16).unwrap()));
            }
        }
        let symbols = read("prog.sym").lines().map(|l| {
            let (n, v) = l.split_once(' ').unwrap();
            (n.to_owned(), i64::from_str_radix(v.trim_start_matches("0x"), 16).unwrap())
        }).collect();
        Fixture { name: name.to_owned(), insns, sections, symbols }
    }

    /// Address of a label.
    pub fn sym(&self, label: &str) -> u32 {
        *self.symbols.get(label).unwrap_or_else(|| panic!("{}: no symbol {}", self.name, label)) as u32
    }

    /// A core over the fixture (memory and decoded instructions) with the reset state,
    /// pc at the label entry. Panics outside the containment.
    pub fn core(&self, entry: &str) -> Core {
        if let Err(e) = c55x_core::guard::guard() {
            panic!("containment: {}", e);
        }
        let mut mem = vec![0u8; MEM_BYTES];
        for (addr, data) in &self.sections {
            mem[*addr as usize..*addr as usize + data.len()].copy_from_slice(data);
        }
        let resolve = frontend::resolver(&self.symbols);
        let decoded: HashMap<u32, Decoded> = self.insns.iter().map(|(&pc, insn)| {
            let ir = frontend::build(insn, &resolve).map_err(message);
            (pc, Decoded { size: insn.size, text: insn.text.clone(), ir })
        }).collect();
        let mut core = NativeHost::core(mem, decoded).unwrap_or_else(|e| panic!("{}", message(e)));
        core.st.st1 = ST1_INIT;
        core.st.st2 = ST2_INIT;
        core.st.ivpd = 0xFFFF;
        core.st.ivph = 0xFFFF;
        core.st.xsp = XSP_INIT;
        core.st.ssp = SSP_INIT;
        core.st.pc = self.sym(entry);
        core
    }
}

/// Core::run until the idle loop: Ok(true) when it is reached within MAX_STEPS.
pub fn try_run(core: &mut Core) -> Result<bool, String> {
    core.run(MAX_STEPS, &PcFilter::default(), None).map_err(message)
}

/// testutil.run: the idle loop must be reached.
pub fn run(core: &mut Core) {
    match try_run(core) {
        Ok(true) => {}
        Ok(false) => panic!("no idle loop reached (pc 0x{:06x})", core.st.pc),
        Err(e) => panic!("{} (pc 0x{:06x})", e, core.st.pc),
    }
}

pub fn read(core: &mut Core, word: u32) -> u32 {
    core.read(word).unwrap_or_else(|e| panic!("{}", message(e)))
}

pub fn write(core: &mut Core, word: u32, v: u32) {
    core.write(word, v as i128).unwrap_or_else(|e| panic!("{}", message(e)))
}

pub fn read32(core: &mut Core, word: u32) -> u32 {
    core.read32(word).unwrap_or_else(|e| panic!("{}", message(e))) as u32
}

pub fn write32(core: &mut Core, word: u32, v: u32) {
    core.write32(word, v as i128).unwrap_or_else(|e| panic!("{}", message(e)))
}

/// One instruction (Cpu.step).
pub fn step(core: &mut Core) {
    core.step().unwrap_or_else(|e| panic!("{} (pc 0x{:06x})", message(e), core.st.pc))
}

/// A cached fixture: one load per test binary.
#[macro_export]
macro_rules! fixture {
    ($name:literal) => {{
        static F: std::sync::OnceLock<common::Fixture> = std::sync::OnceLock::new();
        F.get_or_init(|| common::Fixture::load($name))
    }};
}

/// A plain storage register by its registers.PLAIN name (bk03, bsa23, ...).
pub fn plain<'a>(core: &'a mut Core, name: &str) -> &'a mut u32 {
    let i = c55x_core::state::PLAIN.iter().position(|p| *p == name).unwrap_or_else(|| panic!("no register {}", name));
    &mut core.st.plain[i]
}
