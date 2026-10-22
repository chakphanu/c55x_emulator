//! ATA hard disk on CE1 (port of the former scripts/c55x_emu/ata.py; protocol notes
//! there): task file at data words 0x280000..7, alternate status / device control at
//! 0x280806, INTRQ = CPU interrupt 3 (IFR0 bit 3). The register interface, the PIO data
//! words and the command timing run here; sectors are read and written through the
//! host (Host::disk_read / disk_write; the Python host calls disk.Disk.read / write),
//! one call per sector. IDENTIFY data comes
//! from Python (ata.identify). The data path per block is in ata_pio.rs.

use std::collections::{BTreeMap, VecDeque};

use crate::core::Core;
use crate::err::R;

pub const BASE: u32 = 0x280000;
pub const ALT: u32 = 0x280806;
pub const TIMER: usize = 2;                 // timer heap id (McBSP 0 / 1 use 0 / 1)
const IRQ: u32 = 3;
pub(crate) const BSY: u8 = 0x80;
pub(crate) const DRDY: u8 = 0x40;
pub(crate) const DSC: u8 = 0x10;
pub(crate) const DRQ: u8 = 0x08;
pub(crate) const ERR: u8 = 0x01;
pub(crate) const ABRT: u8 = 0x04;
pub(crate) const IDNF: u8 = 0x10;
const SRST: u8 = 0x04;
const NIEN: u8 = 0x02;
const DELAY: u64 = 2000;                    // cycles from command / block end to the next step
const NO_DATA: [u8; 11] = [0x10, 0x91, 0xE0, 0xE1, 0xE2, 0xE3, 0xE5, 0xE6, 0xE7, 0xEA, 0xEF];
pub(crate) const SECTOR: usize = 512;

#[derive(Clone, Copy)]
pub(crate) enum Op { Identify, Read { lba: u64, left: u32, multi: bool }, Write { lba: u64, left: u32, multi: bool } }

#[derive(Clone, Copy)]
pub(crate) enum Step { Identify, ReadBlock, WantBlock(bool), Done, Finish(u8) }

pub struct Ata {
    pub(crate) sectors: u64,
    identify: Vec<u16>,
    regs: [u8; 8],                          // 1 features, 2 count, 3..5 LBA, 6 device
    pub(crate) error: u8,
    pub(crate) status: u8,
    devctl: u8,
    multiple: u32,
    pub(crate) buf: VecDeque<u16>,
    pub(crate) pending: Option<Op>,
    step: Option<Step>,                     // the scheduled step, generation gen
    gen: u32,
    pub commands: BTreeMap<u8, u64>,
    pub unknown: BTreeMap<String, u64>,
}

impl Ata {
    pub fn new(sectors: u64, identify: Vec<u16>) -> Ata {
        Ata { sectors, identify, regs: [0; 8], error: 0, status: DRDY | DSC, devctl: 0, multiple: 0,
              buf: VecDeque::new(), pending: None, step: None, gen: 0,
              commands: BTreeMap::new(), unknown: BTreeMap::new() }
    }

    fn lba(&self) -> u64 {
        ((self.regs[6] as u64 & 0x0F) << 24) | ((self.regs[5] as u64) << 16) | ((self.regs[4] as u64) << 8)
            | self.regs[3] as u64
    }

    fn count(&self) -> u32 {
        if self.regs[2] == 0 { 256 } else { self.regs[2] as u32 }
    }

    pub(crate) fn block(&self, multi: bool, left: u32) -> u32 {
        if multi { self.multiple.min(left) } else { 1 }
    }

    pub(crate) fn note(&mut self, what: String) {
        *self.unknown.entry(what).or_insert(0) += 1;
    }
}

impl Core {
    pub(crate) fn ata(&mut self) -> &mut Ata {
        self.ata.as_mut().expect("ATA access without an attached disk")
    }

    pub fn is_ata(&self, word: u32) -> bool {
        self.ata.is_some() && ((BASE..BASE + 8).contains(&word) || word == ALT)
    }

    pub fn ata_read(&mut self, word: u32) -> R<u32> {
        if word == ALT {
            return Ok(self.ata().status as u32);
        }
        Ok(match word - BASE {
            0 => self.ata_data_read()? as u32,
            1 => self.ata().error as u32,
            7 => self.ata().status as u32,      // reading status acknowledges INTRQ (level not modeled)
            r => self.ata().regs[r as usize] as u32,
        })
    }

    pub fn ata_write(&mut self, word: u32, value: u32) -> R<()> {
        if word == ALT {
            let (v, a) = (value as u8, self.ata());
            if v & SRST != 0 && a.devctl & SRST == 0 {
                a.status = BSY;
                a.buf.clear();
                a.pending = None;
                a.step = None;
                a.gen = a.gen.wrapping_add(1);  // a scheduled step is dropped
            } else if v & SRST == 0 && a.devctl & SRST != 0 {
                a.status = DRDY | DSC;
                a.error = 0x01;                 // diagnostic code: no error
            }
            a.devctl = v;
            return Ok(());
        }
        match word - BASE {
            0 => self.ata_data_write((value & 0xFFFF) as u16),
            7 => self.ata_command(value as u8),
            r => { self.ata().regs[r as usize] = value as u8; Ok(()) }
        }
    }

    pub(crate) fn ata_later(&mut self, step: Step) {
        let a = self.ata();
        a.step = Some(step);
        let gen = a.gen;
        self.schedule(DELAY, TIMER, gen);
    }

    /// Timer heap entry of the ATA (timers.rs).
    pub fn ata_tick(&mut self, gen: u32) -> R<()> {
        let a = self.ata();
        if gen != a.gen {
            return Ok(());
        }
        match a.step.take() {
            Some(Step::Identify) => {
                let words = a.identify.clone();
                a.buf.extend(words);
                a.pending = None;
                a.status = DRDY | DSC | DRQ;
                self.ata_irq();
                Ok(())
            }
            Some(Step::ReadBlock) => self.ata_read_block(),
            Some(Step::WantBlock(irq)) => self.ata_want_block(irq),
            Some(Step::Done) => { self.ata().pending = None; self.ata_finish(0); Ok(()) }
            Some(Step::Finish(extra)) => { self.ata_finish(extra); Ok(()) }
            None => Ok(()),
        }
    }

    pub(crate) fn ata_irq(&mut self) {
        if self.ata().devctl & NIEN == 0 {
            self.st.ifr0 |= 1 << IRQ;
        }
    }

    pub(crate) fn ata_finish(&mut self, extra: u8) {
        self.ata().status = DRDY | DSC | extra;
        self.ata_irq();
    }

    fn ata_abort(&mut self) {
        let a = self.ata();
        a.error = ABRT;
        a.pending = None;
        self.ata_later(Step::Finish(ERR));
    }

    fn ata_command(&mut self, cmd: u8) -> R<()> {
        let a = self.ata();
        if a.status & BSY != 0 {
            a.note(format!("command 0x{:02x} while busy", cmd));
            return Ok(());
        }
        *a.commands.entry(cmd).or_insert(0) += 1;
        a.error = 0;
        a.buf.clear();
        a.status = BSY | DSC;
        let (lba, count, multiple) = (a.lba(), a.count(), a.multiple);
        match cmd {
            0xEC => { a.pending = Some(Op::Identify); self.ata_later(Step::Identify) }
            0x20 | 0x21 | 0xC4 | 0x30 | 0x31 | 0xC5 => {
                let multi = cmd == 0xC4 || cmd == 0xC5;
                if multi && multiple == 0 {
                    self.ata_abort();
                } else if matches!(cmd, 0x20 | 0x21 | 0xC4) {
                    a.pending = Some(Op::Read { lba, left: count, multi });
                    self.ata_later(Step::ReadBlock);
                } else {
                    a.pending = Some(Op::Write { lba, left: count, multi });
                    self.ata_later(Step::WantBlock(false));
                }
            }
            0xC6 => {
                let n = a.regs[2] as u32;
                if n != 0 && (n > 16 || n & (n - 1) != 0) {
                    self.ata_abort();
                } else {
                    a.multiple = n;
                    self.ata_later(Step::Done);
                }
            }
            c if NO_DATA.contains(&c) => self.ata_later(Step::Done),
            c => { a.note(format!("command 0x{:02x}", c)); self.ata_abort() }
        }
        Ok(())
    }

}
