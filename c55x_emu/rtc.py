"""Real-time clock of the VC5509 (I/O 0x1800..0x180c, SPRU594B), interrupt 19 (INT4/RTC).

- Time/calendar registers in BCD; RTCDAYW holds the day of week (bits 2-0, Sunday = 1)
  and the day alarm (bits 7-3). Hours in 24-hour mode when RTCINTEN.TM = 1, else 12-hour
  mode with bit 7 = PM.
- RTCINTEN.SET = 1 copies the time to a read buffer and isolates a write buffer; clearing
  SET copies written fields to the clock.
- One update cycle per emulated second: UIP is 1 for the first 244 us of the second, then
  the time advances, UF is set and alarms are compared (AF). IRQF and interrupt 19
  follow (flag & enable). Periodic interrupts (PIE) are not modeled and raise.
- An RTC second lasts mhz() * 1e6 CPU cycles, the CPU clock in effect when the second
  starts (machine.py passes the clock generator), so RTC time matches Machine.seconds()
  and the 1 ms timer tick. A clock change takes effect with the next second.
The reserved register 0x180d (used by the firmware) is plain storage.
"""

import datetime

from .registers import EmulationError

BASE, IRQ = 0x1800, 19
SEC, SECA, MIN, MINA, HOUR, HOURA, DAYW, DAYM, MONTH, YEAR, PINTR, INTEN, INTFL, REG0D = range(14)
TIME_FIELDS = (SEC, MIN, HOUR, DAYW, DAYM, MONTH, YEAR)
UIP, SET, PIE, AIE, UIE, TM = 0x80, 0x80, 0x40, 0x20, 0x10, 0x02
IRQF, PF, AF, UF = 0x80, 0x40, 0x20, 0x10
UIP_FRACTION = 244e-6


def bcd(v):
    return ((v // 10) << 4) | (v % 10)


def unbcd(v):
    return (v >> 4) * 10 + (v & 15)


class Rtc:
    def __init__(self, bus, cpu, mhz, now=None):
        self.cpu, self.mhz = cpu, mhz
        self.time = (now or datetime.datetime.now()).replace(microsecond=0)
        self.alarm = {SECA: 0, MINA: 0, HOURA: 0, DAYW: 0}
        self.pintr, self.inten, self.intfl, self.reg0d = 0, TM, 0, 0
        self.read_buffer, self.write_buffer = None, {}
        self.second_start = cpu.cycles
        self.updates = 0
        for r in range(14):
            bus.hook_port(BASE + r, self.read, self.write)
        cpu.schedule(self.second_cycles(), self._update)

    # ---- time <-> registers -----------------------------------------------------------
    def fields(self, t):
        hour = t.hour
        if not self.inten & TM:
            pm = hour >= 12
            hour = (hour % 12) or 12
            hour_reg = bcd(hour) | (0x80 if pm else 0)
        else:
            hour_reg = bcd(hour)
        return {SEC: bcd(t.second), MIN: bcd(t.minute), HOUR: hour_reg,
                DAYW: (t.isoweekday() % 7) + 1, DAYM: bcd(t.day), MONTH: bcd(t.month),
                YEAR: bcd(t.year % 100)}

    def _hour_from(self, reg):
        if self.inten & TM:
            return unbcd(reg & 0x3F)
        hour = unbcd(reg & 0x1F) % 12
        return hour + 12 if reg & 0x80 else hour

    def _apply(self, values):
        f = {**self.fields(self.time), **values}
        try:
            self.time = datetime.datetime(2000 + unbcd(f[YEAR]), unbcd(f[MONTH]), unbcd(f[DAYM]),
                                          self._hour_from(f[HOUR]), unbcd(f[MIN]), unbcd(f[SEC]))
        except ValueError as e:
            raise EmulationError(f"RTC set to an invalid time {f}: {e}") from e

    # ---- register access --------------------------------------------------------------
    def read(self, port):
        r = port - BASE
        if r in TIME_FIELDS:
            f = self.read_buffer if self.read_buffer is not None else self.fields(self.time)
            v = f[r]
            return v | (self.alarm[DAYW] & 0xF8) if r == DAYW else v
        if r in self.alarm:
            return self.alarm[r]
        if r == PINTR:
            in_update = self.cpu.cycles - self.second_start < self.second_cycles() * UIP_FRACTION
            return self.pintr | (UIP if in_update else 0)
        if r == INTEN:
            return self.inten
        if r == INTFL:
            return self.intfl | (IRQF if self.intfl & (self.inten & (PIE | AIE | UIE)) else 0)
        return self.reg0d

    def write(self, port, value):
        r, value = port - BASE, value & 0xFF
        if r == DAYW:
            self.alarm[DAYW] = value & 0xF8
        if r in TIME_FIELDS:
            v = value & 0x07 if r == DAYW else value
            if self.inten & SET:
                self.write_buffer[r] = v
            else:
                self._apply({r: v})
        elif r in self.alarm:
            self.alarm[r] = value
        elif r == PINTR:
            self.pintr = value & 0x1F
        elif r == INTEN:
            self._control(value)
        elif r == INTFL:
            self.intfl &= ~value
        else:
            self.reg0d = value

    def _control(self, value):
        if value & PIE:
            raise EmulationError("RTC periodic interrupt (PIE) not modeled")
        if value & SET and not self.inten & SET:
            self.read_buffer, self.write_buffer = self.fields(self.time), {}
        if not value & SET and self.inten & SET:
            if self.write_buffer:
                self._apply(self.write_buffer)
            self.read_buffer, self.write_buffer = None, {}
        self.inten = value & 0xF2

    # ---- update cycle -----------------------------------------------------------------
    def second_cycles(self):
        return round(self.mhz() * 1e6)

    def _update(self):
        self.second_start = self.cpu.cycles
        self.time += datetime.timedelta(seconds=1)
        self.updates += 1
        self.intfl |= UF
        if self._alarm_matches():
            self.intfl |= AF
        if self.intfl & self.inten & (AIE | UIE):
            self.cpu.raise_interrupt(IRQ)
        self.cpu.schedule(self.second_cycles(), self._update)

    def _alarm_matches(self):
        f = self.fields(self.time)
        for reg, cur in ((SECA, SEC), (MINA, MIN), (HOURA, HOUR)):
            a = self.alarm[reg]
            if a & 0xC0 != 0xC0 and a != f[cur]:        # 11xxxxxx: don't care
                return False
        day = self.alarm[DAYW]
        if day & 0x08 and not day & 0x80 and (day >> 4) != f[DAYW]:
            return False
        return True
