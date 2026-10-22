"""C55x emulator: operand parsing, LCD model, Hello-LCD end to end (instruction semantics:
rust/c55x_core/tests/semantics.rs)."""

import unittest

from c55x_emu.paths import EXAMPLES_DIR
from c55x_emu.bus import Bus
from c55x_emu.expr import number, parse
from c55x_emu.lcd import Lcd

HELLO = EXAMPLES_DIR / "hello_lcd" / "hello_lcd.out"


class ParseTests(unittest.TestCase):
    def test_numbers(self):
        self.assertEqual([number(s) for s in ("#8191", "#-1", "#0x020120", "#01600h")],
                         [8191, -1, 0x20120, 0x1600])

    def test_memory_operands(self):
        self.assertEqual(parse("*+AR2(#0001h)"), ("mem", "pre", "AR2", 1))
        self.assertEqual(parse("*(AR3+T0)"), ("mem", "postreg", "AR3", ("+", "T0")))
        self.assertEqual(parse("*AR3(#009fh)"), ("mem", "idx", "AR3", 0x9F))
        self.assertEqual(parse("*AR3+"), ("mem", "post", "AR3", 1))

    def test_multiply_is_not_a_memory_operand(self):
        self.assertEqual(parse("AC0 * T0"), ("bin", "*", ("reg", "AC0"), ("reg", "T0")))


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
