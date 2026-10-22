//! The core's timed events (McBSP slot batches) and the audio clock query.

use std::cmp::Reverse;

use crate::core::Core;
use crate::err::{emu, R};

impl Core {
    /// Timer i after delay cycles (generation gen): McBSP i slot batch (mcbsp.rs).
    pub fn schedule(&mut self, delay: u64, i: usize, gen: u32) {
        self.timers.push(Reverse((self.st.cycles + delay.max(1), i, gen)));
    }

    pub fn next_timer(&self) -> u64 {
        self.timers.peek().map_or(u64::MAX, |Reverse((c, _, _))| *c)
    }

    /// Run every timer due at the current cycle.
    pub fn fire_due(&mut self) -> R<()> {
        while let Some(&Reverse((c, i, gen))) = self.timers.peek() {
            if c > self.st.cycles {
                break;
            }
            self.timers.pop();
            self.mcbsp_tick(i, gen)?;
        }
        Ok(())
    }

    /// (sample rate in Hz, 0 = no clock; CPU MHz) from Core::set_audio_clock.
    pub fn audio_clock(&self) -> R<(f64, f64)> {
        match self.audio_clock {
            Some(v) => Ok(v),
            None => emu("McBSP transmit without an audio clock model (Core::set_audio_clock)".into()),
        }
    }
}
