"""C55x CPU: state, timed events and interrupt requests on the Python side; instructions
are executed by the Rust core (rust/c55x_core, rust_core.py).

The attributes below are the CPU state. They are authoritative between core calls: each
run() / step() / call() loads them into the core and stores them back, so peripherals,
tools and tests read and change them directly. During a run the core keeps cycles, pc
and IFR in sync around every call into Python (peripheral access, events).
"""

import collections
import heapq

from .decoder import CodeCache
from .registers import PLAIN, ST1_INTM, ST1_REQUIRED, ST1_SXMD, EmulationError
from .rust_core import attach

__all__ = ["Cpu", "EmulationError"]


class Cpu:
    def __init__(self, bus, code, symbols=None):
        """code: CodeCache, or {address: Insn} (fixed program, no on-demand decoding)."""
        self.bus, self.symbols = bus, symbols or {}
        self.code = code if isinstance(code, CodeCache) else CodeCache(bus, code, decode=False)
        bus.mmr = (self._mmr_from_python, self._mmr_from_python)
        self.ac = [0] * 4
        self.t = [0] * 4
        self.xar = [0] * 8
        self.xcdp = self.xdp = self.xsp = self.ssp = 0
        self.tc1 = self.tc2 = self.carry = 0
        self.st1 = ST1_REQUIRED | ST1_SXMD | ST1_INTM   # C environment after reset_c_int00
        self.st2 = 1 << 15                  # ARMS
        self.st3 = 0
        self.ier0 = self.ier1 = self.ifr0 = self.ifr1 = 0
        self.ivpd = self.ivph = 0xFFFF      # hardware reset value (SPRU371F 2.8.1)
        self.dbstat = 0
        self.interrupts_taken = collections.Counter()
        self.events = []                    # heap of (cycle, sequence, callback)
        self.event_seq = 0
        self.brc0 = self.brc1 = self.brs1 = self.csr = 0
        self.rsa0 = self.rea0 = self.rsa1 = self.rea1 = 0
        self.loop_active = [None, None]     # level 0 / 1: None, 'local' or 'ext'
        for attr in PLAIN:
            setattr(self, attr, 0)
        self.reta = self.cfct = 0
        self.pc = 0
        self.cycles = 0
        self.skip_next = False              # set by 'if (c) execute(AD_Unit)' alone
        self.skip_data_next = False         # set by 'if (c) execute(D_Unit)' alone
        self.bios_idle = None               # (loop pc, hook list pointer word): idle skip
        self.poll_loops = frozenset()       # loop heads that only poll a peripheral
        self.idle_ctl = None                # idlectl.IdleControl (machine.py)
        self.in_idle = False                # waiting in an idle instruction
        self.audio_clock = None             # -> (codec sample rate or 0, CPU MHz), machine.py
        self.core = attach(self)

    def _mmr_from_python(self, addr, value=None):
        raise EmulationError(f"MMR 0x{addr:02x} accessed from Python; the registers live in the core")

    def raise_interrupt(self, n):
        """Latch interrupt request n (IFR0 bit n, IFR1 bit n - 16)."""
        if n < 16:
            self.ifr0 |= 1 << n
        else:
            self.ifr1 |= 1 << (n - 16)

    # ---- timed events ----------------------------------------------------------------
    def schedule(self, delay, callback):
        """Run callback() once self.cycles >= now + delay (instructions count as cycles)."""
        self.event_seq += 1
        heapq.heappush(self.events, (self.cycles + max(delay, 1), self.event_seq, callback))

    def _fire_events(self):
        while self.events and self.events[0][0] <= self.cycles:
            heapq.heappop(self.events)[2]()

    # ---- execution (Rust core) --------------------------------------------------------
    def resolve(self, target):
        """Branch target text of the IR front end (ir.py) -> address."""
        if target.startswith("#"):
            return int(target[1:], 0)
        if target in self.symbols:
            return self.symbols[target]
        raise EmulationError(f"unknown branch target {target}")

    def run(self, max_steps, stop=frozenset(), until=None):
        """-> True when stuck idle with nothing scheduled; returns False early when the pc
        reaches an address in stop or the cycle count reaches until. Idle states (a goto
        to itself, the DSP/BIOS idle loop with an empty hook list, a poll loop, the idle
        instruction) let the time jump to the next event, at most to until."""
        return self.core.run(max_steps, set(stop), until)

    def step(self):
        """One instruction (an enabled pending interrupt is taken first)."""
        self.core.step()

    def call(self, target, ret):
        """Enter target like a call instruction returning to ret; -> target (the pc)."""
        self.core.call(target, ret)
        return target
