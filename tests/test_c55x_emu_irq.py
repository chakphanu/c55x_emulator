"""C55x emulator: timers (SPRU595C). Interrupt dispatch: rust/c55x_core/tests/irq.rs."""

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
