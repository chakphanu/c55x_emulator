//! DMA controller of the VC5509A (SPRU587E; registers SPRS205K table 3-24): six channels
//! at I/O 0x0c00 + 0x20 * n, global registers at 0x0e00.
//!
//! A channel copies 16-bit elements (other data types raise), CEN per frame, CFN frames
//! per block, between memory (ports 0..2: data space, byte address / 2) and peripherals
//! (port 3: I/O space). Without a sync event (SYNC = 0) enabling the channel moves the
//! whole block at once. With one, each event moves one element (FS = 0) or one frame
//! (FS = 1); events come from the McBSP model (mcbsp.rs). An event for a disabled
//! channel is latched and serviced when the channel is enabled (SPRU587E 8.2). Address modes: constant, post
//! increment, single and double index (after an element the address is that of its last
//! byte, then the index is added). The destination uses the source index registers
//! (DINDXMD = 0, the reset state; another DMAGSCR value with destination indexing raises).
//! Block end: BLOCK / FRAME / HALF / LAST status as enabled in CICR, the channel
//! interrupt; with AUTOINIT the configuration is reloaded (REPEAT = 1, or when ENDPROG is
//! written), else the channel stops (EN cleared). CSR clears on read; CSAC / CDAC read the
//! current addresses.

use crate::core::Core;
use crate::err::{emu, R};

pub const BASE: u32 = 0x0C00;
const STRIDE: u32 = 0x20;
pub const GLOBAL: u32 = 0x0E00;
const CSDP: usize = 0;
const CCR: usize = 1;
const CICR: usize = 2;
const CSR: usize = 3;
const CSSA_L: usize = 4;
const CSSA_U: usize = 5;
const CDSA_L: usize = 6;
const CDSA_U: usize = 7;
const CEN: usize = 8;
const CFN: usize = 9;
const CFI: usize = 10;
const CEI: usize = 11;
const CSAC: usize = 12;
const CDAC: usize = 13;
const EN: u16 = 0x80;
const FS: u16 = 0x20;
const AUTOINIT: u16 = 0x100;
const REPEAT: u16 = 0x200;
const ENDPROG: u16 = 0x800;
const HALF: u16 = 0x04;
const FRAME: u16 = 0x08;
const LAST: u16 = 0x10;
const BLOCK: u16 = 0x20;
const PORT_IO: u16 = 3;
const IRQS: [u32; 6] = [18, 9, 20, 21, 14, 15]; // DMAC0..DMAC5 (SPRS205K table 3-40)

#[derive(Default)]
pub struct Channel {
    regs: [u16; 16],
    status: u16,
    active: bool,
    endprog_wait: bool,
    latched: Option<u32>,               // sync event seen while disabled
    src: u32,
    dst: u32,
    elem: u32,                          // elements left in the current frame
    frames: u32,                        // frames left, current one included
}

#[derive(Default)]
pub struct Dma {
    ch: [Channel; 6],
    global: [u16; 4],
    pub blocks: [u64; 6],
}

fn sext16(v: u16) -> i64 {
    v as i16 as i64
}

impl Core {
    /// Port in the DMA range -> Some(value).
    pub fn dma_read(&mut self, port: u32) -> Option<u32> {
        if (GLOBAL..GLOBAL + 4).contains(&port) {
            return Some(self.dma.global[(port - GLOBAL) as usize] as u32);
        }
        if !(BASE..BASE + 6 * STRIDE).contains(&port) || (port - BASE) % STRIDE >= 16 {
            return None;
        }
        let c = &mut self.dma.ch[((port - BASE) / STRIDE) as usize];
        let r = ((port - BASE) % STRIDE) as usize;
        Some(match r {
            CSR => std::mem::take(&mut c.status) as u32,
            CCR => ((c.regs[CCR] & !EN) | if c.active { EN } else { 0 }) as u32,
            CSAC => c.src & 0xFFFF,
            CDAC => c.dst & 0xFFFF,
            _ => c.regs[r] as u32,
        })
    }

    /// Port in the DMA range -> Ok(true) when handled.
    pub fn dma_write(&mut self, port: u32, v: u32) -> R<bool> {
        let v = v as u16;
        if (GLOBAL..GLOBAL + 4).contains(&port) {
            self.dma.global[(port - GLOBAL) as usize] = v;
            return Ok(true);
        }
        if !(BASE..BASE + 6 * STRIDE).contains(&port) || (port - BASE) % STRIDE >= 16 {
            return Ok(false);
        }
        let n = ((port - BASE) / STRIDE) as usize;
        let r = ((port - BASE) % STRIDE) as usize;
        self.dma.ch[n].regs[r] = v;
        if r == CCR {
            let c = &mut self.dma.ch[n];
            if v & EN == 0 {
                c.active = false;
                c.endprog_wait = false;
            } else if !c.active {
                self.dma_start(n)?;
            } else if v & ENDPROG != 0 && c.endprog_wait {
                c.regs[CCR] &= !ENDPROG;
                c.endprog_wait = false;
                Self::dma_reload(c);
            }
        }
        Ok(true)
    }

    fn dma_reload(c: &mut Channel) {
        c.src = ((c.regs[CSSA_U] as u32 & 0xFF) << 16) | c.regs[CSSA_L] as u32;
        c.dst = ((c.regs[CDSA_U] as u32 & 0xFF) << 16) | c.regs[CDSA_L] as u32;
        c.elem = c.regs[CEN] as u32;
        c.frames = c.regs[CFN] as u32;
    }

    fn dma_start(&mut self, n: usize) -> R<()> {
        let c = &mut self.dma.ch[n];
        let (ccr, csdp) = (c.regs[CCR], c.regs[CSDP]);
        if csdp & 3 != 1 {
            return emu(format!("DMA channel {}: data type {} (only 16 bit)", n, csdp & 3));
        }
        if (ccr >> 14) & 3 >= 2 && self.dma.global[2] != 0 {
            return emu(format!("DMA channel {}: destination indexing with DMAGSCR 0x{:04x}", n, self.dma.global[2]));
        }
        Self::dma_reload(c);
        if c.elem == 0 || c.frames == 0 {
            return emu(format!("DMA channel {}: empty block (CEN {}, CFN {})", n, c.elem, c.frames));
        }
        c.active = true;
        c.endprog_wait = false;
        if ccr & 0x1F != 0 && c.latched.take() == Some((ccr & 0x1F) as u32) {
            return self.dma_service(n);
        }
        if ccr & 0x1F == 0 {
            if ccr & AUTOINIT != 0 {
                return emu(format!("DMA channel {}: auto-initialization without a sync event", n));
            }
            while self.dma.ch[n].active {
                self.dma_element(n)?;
            }
        }
        Ok(())
    }

    /// A synchronization event (SPRS205K table 3-4) for the channels waiting on it.
    pub fn dma_sync(&mut self, event: u32) -> R<()> {
        for n in 0..6 {
            let c = &mut self.dma.ch[n];
            if (c.regs[CCR] & 0x1F) as u32 != event {
                continue;
            }
            if !c.active {
                c.latched = Some(event);
                continue;
            }
            if !c.endprog_wait {
                self.dma_service(n)?;
            }
        }
        Ok(())
    }

    /// One sync event in channel n: an element, or a frame with FS = 1.
    fn dma_service(&mut self, n: usize) -> R<()> {
        let (fs, frames) = (self.dma.ch[n].regs[CCR] & FS != 0, self.dma.ch[n].frames);
        if !fs {
            return self.dma_element(n);
        }
        while self.dma.ch[n].active && self.dma.ch[n].frames == frames && !self.dma.ch[n].endprog_wait {
            self.dma_element(n)?;
        }
        Ok(())
    }

    fn dma_advance(c: &Channel, addr: u32, mode: u16, last: bool) -> u32 {
        let index = |r| sext16(c.regs[r]);
        (match mode {
            0 => addr as i64,
            1 => addr as i64 + 2,
            2 => addr as i64 + 1 + index(CEI),
            _ => addr as i64 + 1 + if last { index(CFI) } else { index(CEI) },
        } as u32) & 0xFFFFFF
    }

    fn dma_element(&mut self, n: usize) -> R<()> {
        let (src, dst, csdp, ccr) = {
            let c = &self.dma.ch[n];
            (c.src, c.dst, c.regs[CSDP], c.regs[CCR])
        };
        let value = if (csdp >> 2) & 0xF == PORT_IO { self.port_read(src >> 1)? } else { self.read(src >> 1)? };
        let c = &mut self.dma.ch[n];
        let cen = c.regs[CEN] as u32;
        let last = c.elem == 1;
        c.src = Self::dma_advance(c, src, (ccr >> 12) & 3, last);
        c.dst = Self::dma_advance(c, dst, (ccr >> 14) & 3, last);
        c.elem -= 1;
        let mut flags = 0;
        if c.elem == cen - cen / 2 && cen >= 2 {
            flags |= HALF;
        }
        if c.elem == 0 {
            flags |= FRAME;
            c.frames -= 1;
            if c.frames == 1 {
                flags |= LAST;
            }
            if c.frames == 0 {
                flags |= BLOCK;
                self.dma.blocks[n] += 1;
                if ccr & AUTOINIT == 0 {
                    c.active = false;
                    c.regs[CCR] &= !EN;
                } else if ccr & REPEAT != 0 || c.regs[CCR] & ENDPROG != 0 {
                    c.regs[CCR] &= !ENDPROG;
                    Self::dma_reload(c);
                } else {
                    c.endprog_wait = true;
                }
            } else {
                c.elem = cen;
            }
        }
        c.status |= flags & c.regs[CICR];
        let irq = flags & c.regs[CICR] & (HALF | FRAME | LAST | BLOCK) != 0;
        if (csdp >> 9) & 0xF == PORT_IO { self.port_write(dst >> 1, value as i128)?; } else { self.write(dst >> 1, value as i128)?; }
        if irq {
            self.raise_irq(IRQS[n]);
        }
        Ok(())
    }

    /// Registers of channel n (debugging, PyCore.dma_regs).
    pub fn dma_regs(&self, n: usize) -> Vec<u16> {
        self.dma.ch[n].regs.to_vec()
    }

    pub fn raise_irq(&mut self, n: u32) {
        if n < 16 { self.st.ifr0 |= 1 << n } else { self.st.ifr1 |= 1 << (n - 16) }
    }
}
