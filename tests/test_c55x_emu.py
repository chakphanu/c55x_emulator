"""C55x emulator: LCD model, Hello-LCD end to end (instruction semantics:
rust/c55x_core/tests/semantics.rs, operand parsing: rust/c55x_core/src/frontend/tests.rs)."""

import unittest

from c55x_emu.paths import EXAMPLES_DIR
from c55x_emu.bus import Bus
from c55x_emu.lcd import Lcd

HELLO = EXAMPLES_DIR / "hello_lcd" / "hello_lcd.out"


class LcdTests(unittest.TestCase):
    def test_page_column_and_data(self):
        bus = Bus()
        lcd = Lcd(bus)
        for c in (0x81, 0x22, 0xB3, 0x05, 0x12, 0xAF):
            bus.write(0x281000, c)
        bus.write(0x281001, 0x81)
        self.assertEqual((lcd.contrast, lcd.page, lcd.column, lcd.on), (0x22, 3, 0x26, True))
        self.assertEqual((lcd.pixel(0x25, 24), lcd.pixel(0x25, 31), lcd.pixel(0x25, 25)), (1, 1, 0))
        self.assertEqual(lcd.unknown, [])


@unittest.skipUnless(HELLO.exists(), "build the example first: examples/build.sh NAME")
class HelloLcdTests(unittest.TestCase):
    def test_draws_the_test_picture(self):
        from c55x_emu.machine import Machine
        m = Machine(HELLO)
        self.assertTrue(m.run(100_000))
        rows = m.lcd.rows()
        self.assertTrue(m.lcd.on)
        self.assertEqual((rows[0][80], rows[103][80], rows[50][0], rows[50][159]), (1, 1, 1, 1))
        self.assertEqual([rows[72][x] for x in range(8, 12)], [1, 0, 1, 0])   # checker band
        self.assertEqual(sum(rows[40][35:125]) > 0, True)                       # text line


if __name__ == "__main__":
    unittest.main()
