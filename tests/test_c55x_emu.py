"""C55x emulator: operand parsing, instruction semantics, LCD model, Hello-LCD end to end."""

import unittest

from c55x_emu.paths import ROOT
from c55x_emu.bus import Bus
from c55x_emu.cpu import EmulationError
from c55x_emu.expr import number, parse
from c55x_emu.lcd import Lcd
from c55x_emu.testutil import machine, run

HELLO = ROOT / "build" / "hello_lcd" / "hello_lcd.out"


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


class SemanticTests(unittest.TestCase):
    def test_sign_and_zero_extension(self):
        cpu, bus = machine("XAR1 = mar(*(#01600h))", "AC0 = *AR1", "AC1 = uns(*AR1)")
        bus.write(0x1600, 0x8000)
        run(cpu)
        self.assertEqual(cpu.ac[0], 0xFFFFFF8000)
        self.assertEqual(cpu.ac[1], 0x8000)

    def test_long_constant_and_logical_shift(self):
        cpu, _ = machine("AC0 = #-1 << #16", "AC0 = AC0 | #65535", "AC1 = AC0 >>> #1")
        run(cpu)
        self.assertEqual(cpu.ac[0], 0xFFFFFFFFFF)
        self.assertEqual(cpu.ac[1], 0x7FFFFFFF)

    def test_post_increment_of_long_access_steps_two_words(self):
        cpu, bus = machine("XAR2 = mar(*(#01600h))", "AC0 = dbl(*AR2+)")
        bus.write32(0x1600, 0x12345678)
        run(cpu)
        self.assertEqual((cpu.ac[0], cpu.xar[2]), (0x12345678, 0x1602))

    def test_ar_write_keeps_the_high_part(self):
        cpu, _ = machine("XAR3 = mar(*(#012345h))", "AR3 = #5")
        run(cpu)
        self.assertEqual(cpu.xar[3], 0x010005)

    def test_dual_push_pop_order(self):
        cpu, bus = machine("T2 = #2", "T3 = #3", "push(T3,T2)", "T0,T1 = pop()")
        run(cpu)
        self.assertEqual((cpu.t[0], cpu.t[1], cpu.xsp), (3, 2, 0x1000))
        self.assertEqual((bus.read(0x0FFE), bus.read(0x0FFF)), (3, 2))

    def test_call_and_return_restore_the_stacks(self):
        cpu, _ = machine("call #0x020006", "T0 = #1", "goto #0x02000a", "T1 = #7", "return")
        run(cpu)
        self.assertEqual((cpu.t[0], cpu.t[1], cpu.xsp, cpu.ssp, cpu.reta), (1, 7, 0x1000, 0x0800, 0))

    def test_repeat_and_localrepeat_counts(self):
        cpu, _ = machine("T0 = #0", "repeat(#7)", "T0 = T0 + #1", "BRC0 = #3", "T1 = #0",
                         "localrepeat {", "T1 = T1 + #1", "T1 = T1 + #1", "}")
        run(cpu)
        self.assertEqual((cpu.t[0], cpu.t[1]), (8, 8))

    def test_goto_out_of_a_loop_ends_it(self):     # mtp_proplist_count 0x872441 / 0x8724d9
        cpu, _ = machine("BRC0 = #9", "T0 = #0", "AR1 = #3",
                         "localrepeat {", "T0 = T0 + #1", "TC1 = T0 == AR1", "if (TC1) goto #0x020010",
                         "T2 = T2 + #1", "}",
                         "BRC0 = #4", "T1 = #0", "localrepeat {", "T1 = T1 + #1", "}")
        run(cpu)
        self.assertEqual((cpu.t[0], cpu.t[1], cpu.loop_active), (3, 5, [None, None]))

    def test_conditional_execute(self):
        cpu, _ = machine("T0 = #12", "AR1 = #12", "TC1 = T0 == AR1",
                         "if (TC1) execute(D_Unit) || T1 = #128", "TC1 = T0 != AR1",
                         "if (TC1) execute(D_Unit) || T2 = #5")
        run(cpu)
        self.assertEqual((cpu.t[1], cpu.t[2]), (128, 0))

    def test_parallel_parts_read_before_writing(self):
        cpu, _ = machine("AC0 = #5", "AC1 = #1", "AC0 = #1 << #16 || AC1 = AC1 + AC0")
        run(cpu)
        self.assertEqual((cpu.ac[0], cpu.ac[1]), (0x10000, 6))

    def test_parallel_mar_runs_in_the_address_phase(self):
        # DSP/BIOS queue insert (Jukebox2 0x03a6e1) stores the already modified XAR3
        cpu, bus = machine("XAR3 = mar(*(#01600h))", "XAR5 = mar(*(#01700h))",
                           "dbl(*AR5) = XAR3 || mar(AR3 + #10)")
        run(cpu)
        self.assertEqual((bus.read32(0x1700), cpu.xar[3]), (0x160A, 0x160A))

    def test_parallel_mar_leaves_the_partner_address_alone(self):
        # iic_op1 (Jukebox2 0x828676) reads req[2] with the old AR5
        cpu, bus = machine("XAR5 = mar(*(#01600h))", "AR0 = *AR5(short(#2)) || mar(AR5 + #4)")
        bus.write(0x1602, 7)
        bus.write(0x1606, 9)
        run(cpu)
        self.assertEqual((cpu.xar[0] & 0xFFFF, cpu.xar[5]), (7, 0x1604))

    def test_standalone_conditional_execute_skips_the_next_instruction(self):
        cpu, _ = machine("TC1 = #0", "T0 = #1", "if (TC1) execute(AD_Unit)", "T0 = #2", "T1 = #3")
        run(cpu)
        self.assertEqual((cpu.t[0], cpu.t[1]), (1, 3))

    def test_multiply_uses_the_high_part_of_an_accumulator(self):
        cpu, _ = machine("AC0 = #3 << #16", "AC0 = AC0 | #5", "AC1 = AC0 * #12")
        run(cpu)
        self.assertEqual(cpu.ac[1], 36)

    def test_long_result_keeps_its_width(self):
        cpu, bus = machine("XAR0 = mar(*(#01600h))", "AC0 = uns(*AR0+) << #8")
        bus.write(0x1600, 0x449B)
        run(cpu)
        self.assertEqual(cpu.ac[0], 0x449B00)

    def test_field_extract(self):
        cpu, _ = machine("AC2 = #0x1234", "AC2 = field_extract(AC2, #65520)")
        run(cpu)
        self.assertEqual(cpu.ac[2], 0x123)

    def test_unsupported_mode_raises(self):
        cpu, _ = machine("@ST1_L = @ST1_L | #32 || mmap()")   # C54CM
        with self.assertRaises(EmulationError):
            run(cpu)


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


@unittest.skipUnless(HELLO.exists(), "build Hello-LCD first: fw/hello_lcd/build.sh")
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
