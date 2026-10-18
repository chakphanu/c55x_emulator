"""General-purpose timers 0 (ports 0x1000, TINT0 = interrupt 4) and 1 (0x2400, TINT1 = 22).

SPRU595C: TIM counts PRD..0 once per (TDDR + 1) input clocks; when TIM reaches 0 the
timer interrupt is raised and, with ARB = 1, PSC/TIM reload, so the period is
(TDDR + 1) * (PRD + 1) clocks. TSS = 1 stops, TLB = 1 loads TIM/PSC from PRD/TDDR.
Input clock = CPU clock; the emulator counts one clock per instruction. The first
expiry comes one full period after the start (on the chip TIM reaches 0 one prescaler
period earlier); the repetition rate is exact. Only the internal
clock source (FUNC = 00b..10b) is modeled; FUNC = 11b (external clock) raises.
"""

from .registers import EmulationError

TIM, PRD, TCR, PRSC = 0, 1, 2, 3
TCR_TLB, TCR_ARB, TCR_TSS = 1 << 10, 1 << 5, 1 << 4
TCR_FUNC_EXTERNAL = 3 << 11
TCR_RESET, PRD_RESET, TIM_RESET = 0x0010, 0xFFFF, 0xFFFF
TIMERS = ((0x1000, 4), (0x2400, 22))


class Timer:
    def __init__(self, bus, cpu, base, irq):
        self.cpu, self.base, self.irq = cpu, base, irq
        self.prd, self.tcr, self.tddr = PRD_RESET, TCR_RESET, 0
        self.tim = TIM_RESET
        self.start_cycle = None         # cycle at which TIM held its loaded value
        self.generation = 0             # invalidates scheduled expiries after stop/reload
        self.expired = 0
        for r in range(4):
            bus.hook_port(base + r, self.read, self.write)

    def period(self):
        return (self.tddr + 1) * (self.prd + 1)

    def running(self):
        return self.start_cycle is not None

    def current_tim(self):
        if not self.running():
            return self.tim
        elapsed = self.cpu.cycles - self.start_cycle
        return max(self.tim - elapsed // (self.tddr + 1), 0)

    def read(self, port):
        r = port - self.base
        if r == TIM:
            return self.current_tim()
        if r == PRD:
            return self.prd
        if r == TCR:
            return self.tcr
        return self.tddr                # PSC (bits 9..6) not tracked, reads 0

    def write(self, port, value):
        r = port - self.base
        if r == PRD:
            self.prd = value
        elif r == PRSC:
            self.tddr = value & 0xF
        elif r == TIM:
            self.tim = value
        elif r == TCR:
            self._control(value)

    def _control(self, value):
        if value & TCR_FUNC_EXTERNAL == TCR_FUNC_EXTERNAL:
            raise EmulationError(f"timer 0x{self.base:04x}: external clock source")
        self.tim = self.current_tim()
        self.tcr = value
        if value & TCR_TLB:
            self.tim = self.prd
        if value & TCR_TSS:
            self._stop()
        elif not value & TCR_TLB:
            self._start()

    def _stop(self):
        self.start_cycle = None
        self.generation += 1

    def _start(self):
        self.generation += 1
        self.start_cycle = self.cpu.cycles
        gen = self.generation
        self.cpu.schedule((self.tim + 1) * (self.tddr + 1), lambda: self._expire(gen))

    def _expire(self, gen):
        if gen != self.generation:
            return
        self.expired += 1
        self.cpu.raise_interrupt(self.irq)
        if self.tcr & TCR_ARB:
            self.tim = self.prd
            self._start()
        else:
            self.tim = 0
            self._stop()
