//! The core's own timed events (McBSP slot batches) next to the Python event heap, and
//! the audio clock query.

use std::cmp::Reverse;

use crate::core::Core;
use crate::err::{emu, R};

impl Core {
    /// Timer i after delay cycles (generation gen): McBSP i slot batch (mcbsp.rs), or the
    /// ATA's next step for i = ata::TIMER.
    pub fn schedule(&mut self, delay: u64, i: usize, gen: u32) {
        self.timers.push(Reverse((self.st.cycles + delay.max(1), i, gen)));
    }

    pub fn next_timer(&self) -> u64 {
        self.timers.peek().map_or(u64::MAX, |Reverse((c, _, _))| *c)
    }

    /// The earliest pending event, core timer or Python heap.
    pub fn next_any(&self) -> u64 {
        self.next_timer().min(self.next_event)
    }

    /// Run everything due at the current cycle: core timers, then the Python events.
    pub fn fire_due(&mut self) -> R<()> {
        while let Some(&Reverse((c, i, gen))) = self.timers.peek() {
            if c > self.st.cycles {
                break;
            }
            self.timers.pop();
            if i == crate::ata::TIMER { self.ata_tick(gen)? } else { self.mcbsp_tick(i, gen)? }
        }
        if self.st.cycles >= self.next_event {
            self.fire_events()?;
        }
        Ok(())
    }

    /// (sample rate in Hz, 0 = no clock; CPU MHz) from cpu.audio_clock() (machine.py).
    pub fn audio_clock(&mut self) -> R<(f64, f64)> {
        let r = self.host(|h, ctx| h.audio_clock(ctx))?;
        match r {
            Some(v) => Ok(v),
            None => emu("McBSP transmit without an audio clock model (cpu.audio_clock)".into()),
        }
    }
}
