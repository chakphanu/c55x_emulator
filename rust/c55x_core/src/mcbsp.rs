//! McBSP0 / McBSP1 transmitters (SPRU592E; registers SPRS205K: I/O 0x2800 / 0x2c00) and
//! the audio samples McBSP0 sends to the codec.
//!
//! Modeled: the transmitter with external bit clock and frame sync (CLKXM = FSXM = 0, as
//! the AIC23 is master), single phase, 16-bit words. Writing DXR1 clears XRDY; at each
//! word slot the shift register (XSR) goes out, DXR1 is copied into it if it was loaded,
//! XRDY is set and XEVT (DMA sync event 2 / 6) and, with XINTM = 00, XINT (interrupt 17 /
//! 7) follow; with XINTM = 10 XINT comes at each frame sync instead, also while the
//! transmitter is in reset, unless PCR.XIOEN makes FSX a GPIO pin (SPRU592E 8.15.1,
//! table 8-3; Jukebox2 mcbsp_start waits for one before it sets XRST). Out of reset the empty XSR takes the first DXR1 write at once, so the DMA
//! preloads two words. The slot rate is the codec's sample rate times the words per frame
//! (Core::set_audio_clock); slots are processed in batches of BATCH_FRAMES frames
//! on the core's own timer. Without a clock (codec inactive or slave) nothing is sent.
//! Words of McBSP0 are collected as interleaved stereo (slot 0 = left, DSP format) per
//! sample rate. The receivers are not modeled (DRR reads 0, RRDY stays 0); McBSP1 has no
//! known clock source, starting its transmitter raises.

use crate::core::Core;
use crate::err::{emu, R};

pub const BASES: [u32; 2] = [0x2800, 0x2C00];
/// Most output samples buffered between take_audio() calls (32 MiB of i16).
const MAX_AUDIO_SAMPLES: usize = 1 << 24;
const XEVT: [u32; 2] = [2, 6];
const XINT: [u32; 2] = [17, 7];
const DXR2: usize = 2;
const DXR1: usize = 3;
const SPCR2: usize = 4;
const SPCR1: usize = 5;
const XCR2: usize = 8;
const XCR1: usize = 9;
const PCR: usize = 0x12;
const XRST: u16 = 1;
const XIOEN: u16 = 1 << 13;
const BATCH_FRAMES: u32 = 16;

#[derive(Default)]
pub struct McBsp {
    regs: [u16; 0x20],
    xrdy: bool,
    xsr_full: bool,
    xsr: u16,
    dxr: u16,
    slot: u32,
    gen: u32,                           // invalidates scheduled slot batches
    reset_sync: bool,                   // frame-sync ticker in reset (XINTM = 10) scheduled
    frac: f64,
    pub underruns: u64,
}

impl Core {
    fn mcbsp_of(port: u32) -> Option<(usize, usize)> {
        BASES.iter().position(|&b| (b..b + 0x20).contains(&port)).map(|i| (i, (port - BASES[i]) as usize))
    }

    pub fn mcbsp_read(&mut self, port: u32) -> Option<u32> {
        let (i, r) = Self::mcbsp_of(port)?;
        let m = &self.mcbsp[i];
        Some(match r {
            0 | 1 => 0,
            SPCR2 => ((m.regs[SPCR2] & !6) | (m.xrdy as u16) << 1 | (m.xsr_full as u16) << 2) as u32,
            SPCR1 => (m.regs[SPCR1] & !6) as u32,
            _ => m.regs[r] as u32,
        })
    }

    pub fn mcbsp_write(&mut self, port: u32, v: u32) -> R<bool> {
        let Some((i, r)) = Self::mcbsp_of(port) else { return Ok(false) };
        let v = v as u16;
        let m = &mut self.mcbsp[i];
        let was = m.regs[SPCR2] & XRST != 0;
        m.regs[r] = v;
        match r {
            DXR1 => {
                m.dxr = v;
                m.xrdy = false;
                if was && !m.xsr_full {
                    m.xsr = v;
                    m.xsr_full = true;
                    m.xrdy = true;
                    self.mcbsp_emit(i);
                }
            }
            SPCR2 if !was && v & XRST != 0 => self.mcbsp_start(i)?,
            SPCR2 if was && v & XRST == 0 => {
                m.gen += 1;
                m.xrdy = false;
                m.xsr_full = false;
                m.reset_sync = false;
                self.mcbsp_reset_sync(i)?;
            }
            SPCR2 | PCR if !was => self.mcbsp_reset_sync(i)?,
            DXR2 | _ => {}
        }
        self.flush_syncs()?;
        Ok(true)
    }

    fn mcbsp_start(&mut self, i: usize) -> R<()> {
        let m = &mut self.mcbsp[i];
        let (xcr1, xcr2, pcr) = (m.regs[XCR1], m.regs[XCR2], m.regs[PCR]);
        if i != 0 {
            return emu(format!("McBSP{} transmitter: clock source not modeled", i));
        }
        if (xcr1 >> 5) & 7 != 2 || xcr2 & 0x8000 != 0 || pcr & (1 << 9 | 1 << 11) != 0 {
            return emu(format!("McBSP{} transmitter: XCR1 0x{:04x} XCR2 0x{:04x} PCR 0x{:04x} not modeled \
                                (16-bit single phase, external clock and frame sync only)", i, xcr1, xcr2, pcr));
        }
        m.xrdy = true;
        m.xsr_full = false;
        m.reset_sync = false;
        m.slot = 0;
        m.frac = 0.0;
        m.gen += 1;
        let gen = m.gen;
        self.mcbsp_emit(i);
        self.schedule(1, i, gen);
        Ok(())
    }

    /// Start the frame-sync ticker of a transmitter in reset if XINTM = 10.
    fn mcbsp_reset_sync(&mut self, i: usize) -> R<()> {
        let m = &mut self.mcbsp[i];
        if m.reset_sync || (m.regs[SPCR2] >> 4) & 3 != 2 || m.regs[PCR] & XIOEN != 0 {
            return Ok(());
        }
        if i != 0 {
            return emu(format!("McBSP{} frame sync in reset: clock source not modeled", i));
        }
        m.reset_sync = true;
        m.frac = 0.0;
        let gen = m.gen;
        self.schedule(1, i, gen);
        Ok(())
    }

    /// Frame sync of a transmitter in reset: XINT per frame while XINTM = 10.
    fn mcbsp_reset_tick(&mut self, i: usize, gen: u32) -> R<()> {
        if (self.mcbsp[i].regs[SPCR2] >> 4) & 3 != 2 || self.mcbsp[i].regs[PCR] & XIOEN != 0 {
            self.mcbsp[i].reset_sync = false;
            return Ok(());
        }
        let (fs, mhz) = self.audio_clock()?;
        if fs <= 0.0 {
            self.schedule((mhz * 1000.0) as u64, i, gen);        // no clock: look again in 1 ms
            return Ok(());
        }
        self.raise_irq(XINT[i]);
        let m = &mut self.mcbsp[i];
        let exact = mhz * 1e6 / fs + m.frac;
        m.frac = exact - exact.floor();
        self.schedule(exact.floor() as u64, i, gen);
        Ok(())
    }

    fn mcbsp_emit(&mut self, i: usize) {
        self.pending_sync.push_back(XEVT[i]);
        if (self.mcbsp[i].regs[SPCR2] >> 4) & 3 == 0 {
            self.raise_irq(XINT[i]);
        }
    }

    /// Run queued DMA sync events (not nested: a DMA write to DXR1 queues the next one).
    pub fn flush_syncs(&mut self) -> R<()> {
        if self.flushing {
            return Ok(());
        }
        self.flushing = true;
        let mut r = Ok(());
        while let Some(e) = self.pending_sync.pop_front() {
            r = self.dma_sync(e);
            if r.is_err() { break; }
        }
        self.flushing = false;
        r
    }

    /// A batch of word slots (core timer callback).
    pub fn mcbsp_tick(&mut self, i: usize, gen: u32) -> R<()> {
        if gen != self.mcbsp[i].gen {
            return Ok(());
        }
        if self.mcbsp[i].regs[SPCR2] & XRST == 0 {
            return self.mcbsp_reset_tick(i, gen);
        }
        let (fs, mhz) = self.audio_clock()?;
        if fs <= 0.0 {
            self.schedule((mhz * 1000.0) as u64, i, gen);        // no clock: look again in 1 ms
            return Ok(());
        }
        let wpf = ((self.mcbsp[i].regs[XCR1] >> 8) & 0x7F) as u32 + 1;
        if wpf != 2 {
            return emu(format!("McBSP{}: {} words per frame (stereo pairs only)", i, wpf));
        }
        for _ in 0..BATCH_FRAMES * wpf {
            if self.mcbsp[i].slot == 0 && (self.mcbsp[i].regs[SPCR2] >> 4) & 3 == 2 {
                self.raise_irq(XINT[i]);                      // XINTM = 10: frame sync
            }
            let m = &mut self.mcbsp[i];
            if !m.xsr_full {
                m.underruns += 1;
            }
            let sample = m.xsr as i16;
            m.slot = (m.slot + 1) % wpf;
            let refill = !m.xrdy;
            if refill {
                m.xsr = m.dxr;
                m.xsr_full = true;
                m.xrdy = true;
            } else {
                m.xsr_full = false;
            }
            self.audio_push(fs, sample)?;
            if refill {
                self.mcbsp_emit(i);
                self.flush_syncs()?;
            }
        }
        let m = &mut self.mcbsp[i];
        let exact = mhz * 1e6 / (fs * wpf as f64) * (BATCH_FRAMES * wpf) as f64 + m.frac;
        let whole = exact.floor();
        m.frac = exact - whole;
        self.schedule(whole as u64, i, gen);
        Ok(())
    }

    /// Buffer one output sample. The buffer is drained by take_audio(); if the
    /// caller never drains it the run stops with an explicit error instead of
    /// growing without bound (32 MiB of samples).
    fn audio_push(&mut self, fs: f64, sample: i16) -> R<()> {
        if self.audio_samples >= MAX_AUDIO_SAMPLES {
            return emu(format!(
                "audio buffer full ({} samples); drain it with take_audio()", MAX_AUDIO_SAMPLES));
        }
        self.audio_samples += 1;
        match self.audio.last_mut() {
            Some((rate, s)) if *rate == fs => s.push(sample),
            _ => self.audio.push((fs, vec![sample])),
        }
        Ok(())
    }
}
