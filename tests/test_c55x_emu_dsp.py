"""C55x emulator: DSP features the Jukebox2 MP3 decoder (codec overlay at 0x025000) uses:
fractional and saturating arithmetic (ST1 FRCT / SATD), rnd / saturate, mant / exp,
dual multiplies sharing the Cmem operand, circular mar() arithmetic, signed shift counts
in Tx with SXMD = 0, 'previous || if (cond) execute' for the next instruction, stores of a shifted accumulator, popboth into an accumulator and the
reserved MMR 0x05."""

import unittest

from c55x_emu.ir import _dual
from c55x_emu.testutil import machine, run

FRCT, SXMD, SATD, M40 = 6, 8, 9, 10


class MantExpTests(unittest.TestCase):
    def check(self, ac0, mant, exp):
        cpu, _ = machine("AC1 = mant(AC0), T1 = exp(AC0)")
        cpu.ac[0] = ac0
        run(cpu)
        self.assertEqual((cpu.ac[1], cpu.t[1]), (mant, exp))

    def test_spru375g_examples(self):           # SPRU375G 5-133, examples 1 and 2
        self.check(0x21_0A0A_0A0A, 0x00_4214_1414, 7)
        self.check(0x00_E804_0000, 0x00_7402_0000, 1)

    def test_zero(self):
        self.check(0, 0, 0x8000)

    def test_split_indexed_coefficient(self):
        self.assertEqual(_dual("AC0 = (AC0 >> #16) + (*AR2 * coef(*(CDP+T0))), "
                               "AC1 = (AC1 >> #16) + (*AR3 * coef(*(CDP+T0)))"),
                         ["AC0 = (AC0 >> #16) + (*AR2 * coef(*CDP))",
                          "AC1 = (AC1 >> #16) + (*AR3 * coef(*(CDP+T0)))"])

    def test_split(self):
        self.assertEqual(_dual("AC2 = mant(AC0), T0 = exp(AC0)"),
                         ["AC2 = mant(AC0)", "T0 = mant_exp(AC0)"])


class DualMultiplyTests(unittest.TestCase):
    def setup(self, *extra):
        cpu, bus = machine(*extra, "XAR0 = mar(*(#01600h))", "XAR1 = mar(*(#01610h))",
                           "XCDP = mar(*(#01620h))",
                           "AC0 = *AR0+ * coef(*CDP+), AC1 = *AR1+ * coef(*CDP+)")
        for addr, v in ((0x1600, 3), (0x1610, 7), (0x1620, 2), (0x1621, 4)):
            bus.write(addr, v)
        run(cpu)
        return cpu

    def test_shared_coefficient_read_once(self):
        cpu = self.setup()
        self.assertEqual((cpu.ac[0], cpu.ac[1], cpu.xcdp, cpu.xar[0], cpu.xar[1]),
                         (6, 14, 0x1621, 0x1601, 0x1611))

    def test_fractional_mode_doubles_products(self):
        cpu = self.setup(f"bit(ST1, #{FRCT}) = #1")
        self.assertEqual((cpu.ac[0], cpu.ac[1]), (12, 28))


class SaturationTests(unittest.TestCase):
    def add_one(self, *modes):
        cpu, _ = machine(*(f"bit(ST1, #{b}) = #1" for b in modes), "AC0 = AC0 + #1")
        cpu.ac[0] = 0x7FFF_FFFF
        run(cpu)
        return cpu.ac[0]

    def test_satd_clamps_at_32_bits(self):
        self.assertEqual(self.add_one(), 0x8000_0000)
        self.assertEqual(self.add_one(SATD), 0x7FFF_FFFF)

    def test_satd_with_m40_clamps_at_40_bits(self):
        self.assertEqual(self.add_one(SATD, M40), 0x8000_0000)

    def test_rnd_and_saturate(self):
        cpu, _ = machine("AC1 = saturate(rnd(AC0))", "AC3 = rnd(AC2)")
        cpu.ac[0], cpu.ac[2] = 0x7FFF_C000, 0x1234_8000
        run(cpu)
        # SATR clears the 16 LSBs after saturating (SPRU374E 4.69 SATR example)
        self.assertEqual((cpu.ac[1], cpu.ac[3]), (0x7FFF_0000, 0x1235_0000))


class ExecuteAfterPartnerTests(unittest.TestCase):    # SPRU375G 5-160
    def run_with(self, tc2):
        cpu, _ = machine(f"bit(ST0, #12) = #{tc2}", "T0 = #0",
                         "T1 = #5 || if (TC2) execute(AD_Unit)", "T0 = #7")
        run(cpu)
        return cpu.t[0], cpu.t[1]

    def test_partner_runs_and_next_is_conditional(self):
        self.assertEqual(self.run_with(1), (7, 5))
        self.assertEqual(self.run_with(0), (0, 5))


class ExecuteDUnitTests(unittest.TestCase):          # SPRU375G 5-163 / 5-164
    def run_with(self, tc1, *code):
        cpu, bus = machine(f"bit(ST0, #13) = #{tc1}", "XAR1 = mar(*(#01600h))", "AC0 = #0", "AC1 = #0",
                           "if (TC1) execute(D_Unit)", *code)
        bus.write(0x1600, 0x20)
        bus.write(0x1601, 0x21)
        run(cpu)
        return cpu.ac[0], cpu.ac[1], cpu.xar[1]

    def test_next_data_operation_is_conditional(self):
        self.assertEqual(self.run_with(1, "AC0 = #5"), (5, 0, 0x1600))
        self.assertEqual(self.run_with(0, "AC0 = #5"), (0, 0, 0x1600))

    def test_address_phase_always_runs(self):            # example 2: mar(*AR1+) runs anyway
        self.assertEqual(self.run_with(0, "mar(*AR1+)", "AC1 = AC1 + *AR1"), (0, 0x21, 0x1601))


class AddressAndShiftTests(unittest.TestCase):
    def test_mar_arithmetic_is_circular(self):
        cpu, _ = machine("XAR2 = mar(*(#010003h))", "bit(ST2, #2) = #1", "mar(AR2 + #2)")
        cpu.bk03 = 4
        run(cpu)
        self.assertEqual(cpu.xar[2], 0x010001)

    def test_shift_count_in_tx_is_signed_without_sxmd(self):
        cpu, bus = machine(f"bit(ST1, #{SXMD}) = #0", "XAR4 = mar(*(#01600h))", "T0 = #-4",
                           "AC0 = AC0 + (*AR4(#0002h) << T0)")
        bus.write(0x1602, 0x100)
        run(cpu)
        self.assertEqual(cpu.ac[0], 0x10)

    def test_store_of_shifted_accumulator(self):
        cpu, bus = machine("XAR2 = mar(*(#01600h))", "*AR2 = LO(AC0 << #-16)")
        cpu.ac[0] = 0x1234_5678
        run(cpu)
        self.assertEqual(bus.read(0x1600), 0x1234)

    def test_popboth_sign_extends(self):         # deviates from SPRU375G 5-330, see O1
        cpu, _ = machine("pshboth(AC3)", "pshboth(AC2)", "AC3 = #-1", "AC2 = #-1",
                         "AC2 = popboth()", "AC3 = popboth()")
        cpu.ac[3], cpu.ac[2] = 0x12_3456_789A, 0x00_FFFF_FF70
        run(cpu)
        self.assertEqual((cpu.ac[3], cpu.ac[2]), (0x00_3456_789A, 0xFF_FFFF_FF70))

    def test_port_qualifier_covers_read_modify_write(self):   # SPRU375G 5-327, cl55 mcbsp_start
        cpu, bus = machine("XAR3 = mar(*(#01234h))", "*AR3 = *AR3 | #1 || writeport()")
        port = {0x1234: 0x10}
        bus.hook_port(0x1234, lambda p: port[p], lambda p, v: port.__setitem__(p, v))
        bus.write(0x1234, 0xF0)                 # data memory at the same address stays untouched
        run(cpu)
        self.assertEqual((port[0x1234], bus.read(0x1234)), (0x11, 0xF0))

    def test_reserved_mmr_reads_zero(self):     # list search past its end, 0x81079c
        cpu, _ = machine("XAR3 = mar(*(#00004h))", "AC0 = dbl(*AR3)")
        cpu.st3 = 0
        run(cpu)
        self.assertEqual(cpu.ac[0], 0)


if __name__ == "__main__":
    unittest.main()
