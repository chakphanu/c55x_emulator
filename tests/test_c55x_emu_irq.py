"""C55x emulator: interrupt dispatch (SPRU371F 4.4.2, 5.2) and timers (SPRU595C)."""

import unittest

from c55x_emu.bus import Bus
from c55x_emu.cpu import Cpu
from c55x_emu.listing import parse as parse_listing
from c55x_emu.registers import ST1_INTM
from c55x_emu.timer import Timer

MAIN = 0x20000


def program(blocks):
    """{address: [instruction text, ...]} -> CPU with SP 0x1000, SSP 0x0800, INTM = 0."""
    lines = []
    for base, texts in blocks.items():
        for i, t in enumerate(texts):
            lines.append(f"{base + 2 * i:06x}: 0000 {t}")
    cpu = Cpu(Bus(), parse_listing("\n".join(lines)))
    cpu.pc, cpu.xsp, cpu.ssp = MAIN, 0x1000, 0x0800
    cpu.st1 &= ~ST1_INTM
    cpu.ivpd, cpu.ivph = 0x0300, 0x0400
    return cpu


IDLE = f"goto #0x{MAIN + 4:06x}"


def vector(cpu, n):
    """SPRU371F 5.2: IVPD (or IVPH for 16..23) as bits 23..8, n * 8 as bits 7..0."""
    base = cpu.ivph if 16 <= n <= 23 else cpu.ivpd
    return ((base << 8) | (n << 3)) & 0xFFFFFF


def isr_address(cpu, n):
    v = vector(cpu, n)
    return int.from_bytes(cpu.bus.mem[v + 1:v + 4], "big")
ISR4, ISR16 = 0x31000, 0x32000


def set_vector(cpu, n, isr):
    """Vector format of SPRU371F 5.2: byte 0 ignored, bytes 1..3 ISR address."""
    v = vector(cpu, n)
    cpu.bus.mem[v:v + 8] = bytes([0xEA]) + isr.to_bytes(3, "big") + bytes.fromhex("5e805f80")


class DispatchTests(unittest.TestCase):
    def setUp(self):
        self.cpu = program({MAIN: ["T0 = #1", "T1 = #2", IDLE],
                            ISR4: ["T2 = #7", "return_int"],
                            ISR16: ["T3 = #9", "return_int"]})
        set_vector(self.cpu, 4, ISR4)               # IV4 in the IVPD page
        set_vector(self.cpu, 16, ISR16)             # IV16 in the IVPH page

    def test_vector_addresses(self):
        self.assertEqual((vector(self.cpu, 4), vector(self.cpu, 16), vector(self.cpu, 24)),
                         (0x30020, 0x40080, 0x300C0))
        self.assertEqual((isr_address(self.cpu, 4), isr_address(self.cpu, 16)), (ISR4, ISR16))

    def test_context_is_saved_and_restored(self):
        cpu = self.cpu
        cpu.step()                                  # T0 = #1
        cpu.ier0 = 1 << 4
        cpu.tc1 = 1
        cpu.raise_interrupt(4)
        st1 = cpu.st1
        cpu.step()                                  # taken, then T2 = #7 runs
        self.assertEqual((cpu.pc, cpu.ifr0, cpu.reta), (ISR4 + 2, 0, MAIN + 2))
        self.assertTrue(cpu.st1 & ST1_INTM)
        self.assertEqual([cpu.bus.read(0x1000 - k) for k in (1, 2, 3)], [cpu.st2 & ~(1 << 12), st1, 0])
        self.assertEqual(cpu.bus.read(0x07FF) >> 13 & 1, 1)                # ST0_55 with TC1
        self.assertEqual((cpu.xsp, cpu.ssp), (0x0FFD, 0x07FD))
        cpu.step()                                  # return_int
        self.assertEqual((cpu.pc, cpu.xsp, cpu.ssp, cpu.st1, cpu.t[2]), (MAIN + 2, 0x1000, 0x0800, st1, 7))
        cpu.step()
        self.assertEqual(cpu.t[1], 2)

    def test_priority_and_masking(self):
        cpu = self.cpu
        cpu.ier0, cpu.ier1 = 1 << 4, 1
        cpu.raise_interrupt(4)
        cpu.raise_interrupt(16)
        cpu.st1 |= ST1_INTM
        cpu.step()
        self.assertEqual(cpu.pc, MAIN + 2)          # INTM = 1: nothing taken
        cpu.st1 &= ~ST1_INTM
        cpu.step()
        self.assertEqual(cpu.pc, ISR16 + 2)         # IV16 has priority 5, IV4 priority 7
        self.assertEqual((cpu.ifr0, cpu.ifr1), (1 << 4, 0))

    def test_disabled_interrupt_stays_pending(self):
        cpu = self.cpu
        cpu.raise_interrupt(4)
        cpu.run(10)
        self.assertEqual((cpu.ifr0, cpu.t[2]), (1 << 4, 0))


class TimerTests(unittest.TestCase):
    def test_auto_reload_period(self):
        cpu = program({MAIN: [IDLE.replace(f"{MAIN + 4:06x}", f"{MAIN:06x}")]})
        timer = Timer(cpu.bus, cpu, 0x1000, 4)
        port = cpu.bus.port_write
        port(0x1002, 0x0410)                        # TSS | TLB
        port(0x1001, 9)                             # PRD
        port(0x1003, 1)                             # TDDR: period (1 + 1) * (9 + 1) = 20
        port(0x1002, 0x0410)
        port(0x1002, 0x0020)                        # ARB, run
        for _ in range(100):
            cpu.step()
        self.assertEqual(timer.expired, 5)
        self.assertTrue(cpu.ifr0 & (1 << 4))
        port(0x1002, 0x0010)                        # stop
        for _ in range(100):
            cpu.step()
        self.assertEqual(timer.expired, 5)


if __name__ == "__main__":
    unittest.main()
