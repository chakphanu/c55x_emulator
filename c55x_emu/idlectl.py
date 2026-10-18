"""Idle configuration (SPRU317K section 8): ICR (I/O 0x0001) and ISTR (0x0002).

The idle instruction (SPRU375G 5-172) copies ICR to ISTR. With CPUI = 0 the CPU keeps
running. With CPUI = 1 it waits until an interrupt enabled in IER0/IER1 is flagged; with
INTM = 0 the interrupt is then taken, with INTM = 1 execution continues after the idle.
CLKGENI = 1 stops all DSP clocks: timers do not advance, only external interrupts (MCU on
INT1, RTC on INT4) can wake the CPU; the emulator then stops the time instead of jumping
to the next timed event. The other domains (DMA, cache, peripherals, EMIF) are recorded
but not modeled.
"""

ICR, ISTR = 0x0001, 0x0002
CPUI, DMAI, CACHEI, PERI, CLKGENI, EMIFI = (1 << n for n in range(6))
ICR_MASK = 0x3F


class IdleControl:
    def __init__(self, bus):
        self.icr = self.istr = 0
        self.idles = 0
        bus.hook_port(ICR, lambda p: self.icr, self._write_icr)
        bus.hook_port(ISTR, lambda p: self.istr, lambda p, v: None)

    def _write_icr(self, port, value):
        self.icr = value & ICR_MASK

    def execute(self):
        """The idle instruction: -> True if the CPU domain goes idle."""
        self.istr = self.icr
        self.idles += 1
        return bool(self.istr & CPUI)

    def clock_stopped(self):
        return bool(self.istr & CLKGENI)
