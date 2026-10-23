//! Stack, call/return, block-repeat loops, interrupts and the fetch/execute loop
//! (port of cpu.py, loops.py and interrupts.py).


use crate::core::Core;
use crate::err::{emu, Error, R};
use crate::observe::{ExecMode, Origin};
use crate::state::ST1_INTM;

const PRIORITY: [u32; 25] = [24, 2, 16, 3, 4, 5, 17, 6, 7, 8, 18, 9, 10, 11, 19, 12, 13, 20, 21, 14, 15, 22, 23, 25, 26];
const ST2_DBGM: u32 = 1 << 12;
const ST2_EALLOW: u32 = 1 << 11;
const NOP_FILL: [[u8; 4]; 2] = [[0x5e, 0x80, 0x5f, 0x80], [0x20, 0x20, 0x20, 0x20]];
const ST0: u32 = 0x02;
const ST1: u32 = 0x03;
const ST2: u32 = 0x4B;

/// loops.after: the instruction at addr fell through to nxt -> next pc.
pub fn loops_after(m: &mut Core, addr: u32, nxt: u32) -> u32 {
    for level in [1usize, 0] {
        if m.st.loop_active[level] == 0 || addr != m.st.rea[level] {
            continue;
        }
        let count = if level == 0 { &mut m.st.brc0 } else { &mut m.st.brc1 };
        if *count != 0 {
            *count -= 1;
            return m.st.rsa[level];
        }
        m.st.loop_active[level] = 0;
    }
    nxt
}

/// A taken goto to an address outside a loop's body (RSA..=REA) ends that loop and the
/// loops inside it: cl55 leaves blockrepeat with goto (break) and starts the next loop
/// at the same level (BRC0 = n; blockrepeat). Calls keep the loops (CFCT, call()).
pub fn loops_branch(m: &mut Core, target: u32) -> u32 {
    for level in [1usize, 0] {
        if m.st.loop_active[level] != 0 && !(m.st.rsa[level] <= target && target <= m.st.rea[level]) {
            m.st.loop_active[level] = 0;
            if level == 0 {
                m.st.loop_active[1] = 0;
            }
        }
    }
    target
}

impl Core {
    pub fn push(&mut self, v: i128) -> R<()> {
        self.st.xsp = (self.st.xsp & 0x7F0000) | (self.st.xsp.wrapping_sub(1) & 0xFFFF);
        self.write(self.st.xsp, v)
    }

    pub fn pop(&mut self) -> R<u32> {
        let v = self.read(self.st.xsp)?;
        self.st.xsp = (self.st.xsp & 0x7F0000) | (self.st.xsp.wrapping_add(1) & 0xFFFF);
        Ok(v)
    }

    pub fn sys_push(&mut self, v: i128) -> R<()> {
        self.st.ssp = self.st.ssp.wrapping_sub(1) & 0xFFFF;
        self.write((self.st.xsp & 0x7F0000) | self.st.ssp, v)
    }

    pub fn sys_pop(&mut self) -> R<u32> {
        let v = self.read((self.st.xsp & 0x7F0000) | self.st.ssp)?;
        self.st.ssp = self.st.ssp.wrapping_add(1) & 0xFFFF;
        Ok(v)
    }

    pub fn loop_start(&mut self, kind: u8, first: u32, last: u32) -> R<()> {
        let level = if self.st.loop_active[0] == 0 {
            0
        } else if self.st.loop_active[1] == 0 {
            self.st.brc1 = self.st.brs1;
            1
        } else {
            return emu(format!("third nested block repeat at 0x{:06x}", self.st.pc));
        };
        self.st.rsa[level] = first;
        self.st.rea[level] = last;
        self.st.loop_active[level] = kind;
        Ok(())
    }

    fn loop_save(&mut self) -> R<u32> {
        let code = match (self.st.loop_active[0], self.st.loop_active[1]) {
            (0, 0) => 0, (2, 0) => 2, (1, 0) => 3, (2, 2) => 7, (2, 1) => 8, (1, 1) => 9,
            s => return emu(format!("loop state {:?} has no CFCT code", s)),
        };
        self.st.loop_active = [0, 0];
        Ok(code)
    }

    fn loop_restore(&mut self, code: u32) -> R<()> {
        self.st.loop_active = match code & 0x0F {
            0 => [0, 0], 2 => [2, 0], 3 => [1, 0], 7 => [2, 2], 8 => [2, 1], 9 => [1, 1],
            _ => return emu(format!("invalid loop context 0x{:02x} in CFCT", code)),
        };
        Ok(())
    }

    /// Execution entry: enter target like a call instruction returning to ret (-> target).
    pub fn call(&mut self, target: u32, ret: u32) -> R<u32> {
        entry_guard()?;
        if self.obs.is_none() {
            return self.call_insn(target, ret);
        }
        let prev = self.observe_origin(Origin::CallEntry, None);
        let r = self.call_insn(target, ret);
        self.observe_restore(prev);
        r
    }

    /// Execution entry: return like a return instruction (-> the return address).
    pub fn ret(&mut self) -> R<u32> {
        entry_guard()?;
        self.ret_insn()
    }

    pub(crate) fn call_insn(&mut self, target: u32, ret: u32) -> R<u32> {
        self.push((self.st.reta & 0xFFFF) as i128)?;
        self.sys_push(((self.st.cfct << 8) | (self.st.reta >> 16)) as i128)?;
        self.st.reta = ret;
        self.st.cfct = self.loop_save()?;
        if self.obs.is_some() {
            self.observe_call(target);
        }
        Ok(target)
    }

    pub(crate) fn ret_insn(&mut self) -> R<u32> {
        let target = self.st.reta;
        self.loop_restore(self.st.cfct)?;
        let hi = self.sys_pop()?;
        self.st.reta = ((hi & 0xFF) << 16) | self.pop()?;
        self.st.cfct = hi >> 8;
        Ok(target)
    }

    pub fn return_int(&mut self) -> R<u32> {
        let target = self.st.reta;
        self.loop_restore(self.st.cfct)?;
        let hi = self.sys_pop()?;
        self.st.reta = ((hi & 0xFF) << 16) | self.pop()?;
        self.st.cfct = hi >> 8;
        let v = self.pop()?;
        self.st.mmr(ST1, Some(v))?;
        self.st.dbstat = self.sys_pop()?;
        let v = self.pop()?;
        self.st.mmr(ST2, Some(v))?;
        let v = self.sys_pop()?;
        self.st.mmr(ST0, Some(v))?;
        Ok(target)
    }

    fn pending(&self) -> Option<u32> {
        let s = &self.st;
        if s.st1 & ST1_INTM != 0 || (s.ifr0 & s.ier0 == 0 && s.ifr1 & s.ier1 == 0) {
            return None;
        }
        PRIORITY.iter().copied().find(|&n| {
            if n < 16 { s.ifr0 & s.ier0 & (1 << n) != 0 } else { s.ifr1 & s.ier1 & (1 << (n - 16)) != 0 }
        })
    }

    fn take(&mut self, n: u32) -> R<()> {
        if self.st.in_idle {             // woken from an idle instruction: return after it
            self.st.in_idle = false;
            let idle = self.fetch(self.st.pc)?;
            self.st.pc += idle.size;
        }
        if n < 16 { self.st.ifr0 &= !(1 << n) } else { self.st.ifr1 &= !(1 << (n - 16)) }
        let prev = if self.obs.is_some() { Some(self.observe_origin(Origin::Interrupt, None)) } else { None };
        let st0 = self.st.mmr(ST0, None)?;
        self.sys_push(st0 as i128)?;
        self.push(self.st.st2 as i128)?;
        self.sys_push(self.st.dbstat as i128)?;
        self.push(self.st.st1 as i128)?;
        self.sys_push(((self.st.cfct << 8) | (self.st.reta >> 16)) as i128)?;
        self.push((self.st.reta & 0xFFFF) as i128)?;
        self.st.reta = self.st.pc;
        self.st.cfct = self.loop_save()?;
        self.st.st1 |= ST1_INTM;
        self.st.st2 = (self.st.st2 | ST2_DBGM) & !ST2_EALLOW;
        self.irq_counts[n as usize] += 1;
        let base = if (16..=23).contains(&n) { self.st.ivph } else { self.st.ivpd };
        let v = ((base << 8) | (n << 3)) & 0xFFFFFF;
        self.st.pc = (1..4).fold(0, |a, i| (a << 8) | self.mem_byte(v + i) as u32);
        let fill = [self.mem_byte(v + 4), self.mem_byte(v + 5), self.mem_byte(v + 6), self.mem_byte(v + 7)];
        if !NOP_FILL.contains(&fill) {
            let insn = self.fetch(v + 4)?;
            if insn.size != 4 {
                return emu(format!("vector {}: bytes 4..7 hold a {}-byte instruction", n, insn.size));
            }
            if prev.is_some() {
                self.observe_origin(Origin::Interrupt, Some(v + 4));
            }
            self.exec_compiled(&insn)?;
        }
        if let Some(c) = prev {
            self.observe_restore(c);
        }
        Ok(())
    }

    /// Execution entry: one instruction (an enabled pending interrupt is taken first).
    pub fn step(&mut self) -> R<()> {
        entry_guard()?;
        self.step_once()
    }

    fn step_once(&mut self) -> R<()> {
        if !self.st.skip_next && !self.st.skip_data_next {
            if let Some(n) = self.pending() {
                self.take(n)?;
            }
        }
        if let Some((max, t)) = &mut self.trace {
            if t.len() == *max { t.pop_front(); }
            t.push_back(self.st.pc);
        }
        let observed = if self.obs.is_some() {
            let mode = if self.st.skip_next { ExecMode::Skipped }
                else if self.st.skip_data_next { ExecMode::AddressPhase } else { ExecMode::Execute };
            self.observe_before(self.st.pc, None, mode)
        } else { None };
        let insn = self.fetch(self.st.pc)?;
        let nxt = self.st.pc + insn.size;
        let new_pc = if self.st.skip_next {
            self.st.skip_next = false;
            None
        } else if self.st.skip_data_next {
            self.st.skip_data_next = false;
            self.native += 1;
            self.exec_address_phase(&insn)?;
            None
        } else {
            self.native += 1;
            self.exec_compiled(&insn)?
        };
        if let Some(k) = observed {
            self.observe_after(k);
        }
        self.st.cycles += 1;
        if self.st.cycles >= self.next_timer() {
            self.fire_due()?;
        }
        self.st.pc = match new_pc {
            Some(pc) => pc,
            None if self.st.loop_active[0] != 0 => loops_after(self, insn.addr, nxt),
            None => nxt,
        };
        Ok(())
    }

    /// Cpu._wait: let time pass to the next event (or to until). -> Some(result) to return.
    fn wait(&mut self, until: Option<u64>, stuck: bool) -> R<Option<bool>> {
        let nxt = if stuck || self.next_timer() == u64::MAX { None } else { Some(self.next_timer()) };
        if let Some(u) = until {
            if nxt.map_or(true, |n| n > u) {
                self.st.cycles = self.st.cycles.max(u);
                return Ok(Some(nxt.is_none()));
            }
        }
        let Some(n) = nxt else { return Ok(Some(true)) };
        self.st.cycles = self.st.cycles.max(n);
        self.fire_due()?;
        Ok(None)
    }

    /// Time passes without instructions: fire everything due up to cycles + n.
    pub fn advance(&mut self, n: u64) -> R<()> {
        entry_guard()?;
        let end = self.st.cycles + n;
        while self.next_timer() <= end {
            self.st.cycles = self.st.cycles.max(self.next_timer());
            self.fire_due()?;
        }
        self.st.cycles = end;
        Ok(())
    }

    /// Cpu.run: -> true when stuck idle with nothing scheduled.
    pub fn run(&mut self, max_steps: u64, stop: &crate::icache::PcFilter, until: Option<u64>) -> R<bool> {
        entry_guard()?;
        for _ in 0..max_steps {
            let pc = self.st.pc;
            self.step_once()?;
            if stop.contains(self.st.pc) || until.map_or(false, |u| self.st.cycles >= u) {
                return Ok(false);
            }
            let waited = if self.st.in_idle {
                self.wait(until, false)?                // no clock domain model: never stopped
            } else {
                let stuck = self.st.pc == pc && self.st.loop_active[0] == 0;
                if !stuck { continue; }
                let intm = self.st.st1 & ST1_INTM != 0;
                self.wait(until, intm)?
            };
            if let Some(r) = waited { return Ok(r); }
        }
        Ok(false)
    }

    /// idle (SPRU375G 5-172): stays on the instruction while the CPU domain is idle and no
    /// enabled interrupt is flagged; in_idle tells run() about it. Whether the CPU domain
    /// goes idle depends on the idle configuration, which is not modeled: entering idle is
    /// an error.
    pub fn idle_insn(&mut self, addr: u32, nxt: u32) -> R<u32> {
        if !self.st.in_idle {
            return emu("idle without an idle control model".into());
        }
        let s = &mut self.st;
        if s.ifr0 & s.ier0 != 0 || s.ifr1 & s.ier1 != 0 {
            s.in_idle = false;
            return Ok(nxt);
        }
        s.in_idle = true;
        Ok(addr)
    }
}

/// The containment check at every execution entry (run, step, call, ret, advance), with
/// the revalidation interval of guard.rs; the steps inside a run are not entries.
fn entry_guard() -> R<()> {
    crate::guard::guard_entry().map_err(Error::Policy)
}
