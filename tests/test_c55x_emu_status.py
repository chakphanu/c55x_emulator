"""C55x emulator: emulated time (machine, RTC) and the human readable status lines."""

import unittest

from c55x_emu.paths import EXAMPLES_DIR
from c55x_emu.machine import Machine
from c55x_emu.status import describe, duration

HELLO = EXAMPLES_DIR / "hello_mcu" / "hello_mcu.out"


class StatusTests(unittest.TestCase):
    def test_duration(self):
        self.assertEqual((duration(1.234), duration(75.25)), ("1.23 s", "1 min 15.2 s"))

    @unittest.skipUnless(HELLO.exists(), "build the example first: examples/build.sh NAME")
    def test_time_follows_the_clock(self):
        m = Machine(str(HELLO))
        m.cpu.cycles = 12_000_000                  # reset clock: 12 MHz bypass
        self.assertAlmostEqual(m.seconds(), 1.0)
        m.clock.write(0x1C00, (9 << 7) | (1 << 4))  # PLL x9 = 108 MHz from here on
        m.cpu.cycles += 108_000_000
        self.assertAlmostEqual(m.seconds(), 2.0)
        lines = describe(m, "idle loop")
        self.assertTrue(lines[0].startswith("Emulated time: 2.00 s   (CPU clock 108 MHz"))
        self.assertEqual(lines[1], "State: idle, waiting for an interrupt")

    @unittest.skipUnless(HELLO.exists(), "build the example first: examples/build.sh NAME")
    def test_rtc_second_follows_the_clock(self):
        m = Machine(str(HELLO))
        m.cpu.cycles = 12_000_000 - 1              # first second at the 12 MHz reset clock
        m.cpu._fire_events()
        self.assertEqual(m.rtc.updates, 0)
        m.cpu.cycles += 1
        m.cpu._fire_events()
        self.assertEqual(m.rtc.updates, 1)
        m.clock.write(0x1C00, (9 << 7) | (1 << 4))
        self.assertEqual(m.rtc.second_cycles(), 108_000_000)


if __name__ == "__main__":
    unittest.main()
