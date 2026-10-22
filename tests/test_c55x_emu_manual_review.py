"""Findings of a review of the core against the TI manuals (SPRU374E mnemonic
instruction set, SPRU371F and C55x v3.x CPU reference guides). Every
expectation cites the manual section or worked example it comes from.
Instruction spellings were checked by assembling them with asm55 and reading
them back with dis55 -r -g."""

import unittest

from c55x_emu.testutil import machine, run

SATD, SXMD, FRCT = 9, 8, 6


def bit(n, value=1):
    return f"bit(ST1, #{n}) = #{value}"


class SignedShiftSignTests(unittest.TestCase):
    """M40 = 0 signed shifts keep the sign of an in-range result: SPRU371F /
    v3.x 2.10.2.9 and 2.10.2.11 ('if SXMD = 1, bit 31 is copied into the
    accumulator's guard bits'); SPRU374E 4.70 ('performed on 40 bits')."""

    def test_negative_left_shift_without_satd(self):
        cpu, _ = machine("AC0 = #-4", "AC0 = AC0 << #1")
        run(cpu)
        self.assertEqual(cpu.ac[0], 0xFF_FFFF_FFF8)
        self.assertEqual(cpu.core.acov(), [0, 0, 0, 0])

    def test_negative_left_shift_with_satd(self):
        cpu, _ = machine(bit(SATD), "AC0 = #-4", "AC0 = AC0 << #1")
        run(cpu)
        self.assertEqual(cpu.ac[0], 0xFF_FFFF_FFF8)
        self.assertEqual(cpu.core.acov(), [0, 0, 0, 0])

    def test_shift_by_zero_sign_extends(self):
        for modes in ((), (SATD,)):
            with self.subTest(modes=modes):
                cpu, _ = machine(*(bit(b) for b in modes), "AC1 = AC0 << #0")
                cpu.ac[0] = 0x00_FFFF_FF00         # bit 31 set, guard bits clear
                run(cpu)
                self.assertEqual(cpu.ac[1], 0xFF_FFFF_FF00)
                self.assertEqual(cpu.core.acov(), [0, 0, 0, 0])

    def test_positive_tx_count_on_a_negative_value(self):
        cpu, _ = machine("T0 = #3", "AC0 = #-4", "AC1 = AC0 << T0")
        run(cpu)
        self.assertEqual(cpu.ac[1], 0xFF_FFFF_FFE0)
        self.assertEqual(cpu.core.acov(), [0, 0, 0, 0])

    def test_add_with_a_shifted_negative_accumulator(self):
        for modes in ((), (SATD,)):
            with self.subTest(modes=modes):
                cpu, _ = machine(*(bit(b) for b in modes), "AC0 = #-4", "AC1 = #100",
                                 "AC1 = AC1 + (AC0 << #2)")
                run(cpu)
                self.assertEqual(cpu.ac[1], 84)
                self.assertEqual(cpu.core.acov(), [0, 0, 0, 0])


class OverflowAttributionTests(unittest.TestCase):
    """Each operation of a bundle reports overflow on its own destination
    (SPRU374E 4.27, dual MAC: 'If an overflow is detected, the destination
    accumulator overflow status bit is set'); stores report nothing (4.6.x
    'Affects none')."""

    def dual_mac(self, ac0, ac1, x, y):
        cpu, bus = machine("XAR1 = mar(*(#01600h))", "XAR2 = mar(*(#01601h))",
                           "XCDP = mar(*(#01602h))",
                           "AC0 = AC0 + (*AR1 * coef(*CDP)), AC1 = AC1 + (*AR2 * coef(*CDP))")
        bus.write(0x1600, x)
        bus.write(0x1601, y)
        bus.write(0x1602, 0x7FFF)
        cpu.ac[0], cpu.ac[1] = ac0, ac1
        run(cpu)
        return cpu

    def test_dual_mac_second_destination_overflows(self):
        cpu = self.dual_mac(0, 0x7FFF_0000, 1, 0x7FFF)
        self.assertEqual(cpu.ac[1], 0x7FFF_0000 + 0x7FFF * 0x7FFF)
        self.assertEqual(cpu.core.acov(), [0, 1, 0, 0])

    def test_dual_mac_first_destination_overflows(self):
        cpu = self.dual_mac(0x7FFF_0000, 0, 0x7FFF, 1)
        self.assertEqual(cpu.core.acov(), [1, 0, 0, 0])

    def test_dual_mac_both_destinations_overflow(self):
        cpu = self.dual_mac(0x7FFF_0000, 0x7FFF_0000, 0x7FFF, 0x7FFF)
        self.assertEqual(cpu.core.acov(), [1, 1, 0, 0])

    def store_bundle(self, ac1, smem, ac2, t2):
        cpu, bus = machine("XAR0 = mar(*(#01600h))", "XAR1 = mar(*(#01610h))", f"T2 = #{t2}",
                           "AC3 = AC1 + (*AR0 << #16) || *AR1 = HI(AC2 << T2)")
        bus.write(0x1600, smem)
        cpu.ac[1], cpu.ac[2] = ac1, ac2
        run(cpu)
        return cpu, bus

    def test_store_shift_overflow_is_not_reported_on_the_partner(self):
        cpu, _ = self.store_bundle(0, 1, 0x4000_0000, 2)
        self.assertEqual(cpu.core.acov(), [0, 0, 0, 0])

    def test_store_does_not_erase_the_partner_overflow(self):
        cpu, _ = self.store_bundle(0x7FFF_0000, 0x7FFF, 0x1234_0000, 0)
        self.assertEqual(cpu.core.acov(), [0, 0, 0, 1])


class StoreShiftTests(unittest.TestCase):
    """Accumulator stores shift on 40 bits according to SXMD only; M40 and SATD
    are not involved and no ACOV is set (SPRU374E 4.6.6-4.6.10, 'Affected by
    [C54CM,] [RDM,] SXMD / Affects none'). saturate() clamps the shifted value
    to 00 7FFF FFFF / FF 8000 0000 (4.6.11-4.6.13)."""

    def store(self, insn, acc, value, *lines):
        cpu, bus = machine(*lines, "XAR0 = mar(*(#01600h))", "T1 = #1", insn)
        cpu.ac[acc] = value
        run(cpu)
        return cpu, bus.read(0x1600)

    def test_right_shift_brings_guard_bits_down(self):
        _, word = self.store("*AR0 = HI(AC0 << #-8)", 0, 0x01_0000_0000)
        self.assertEqual(word, 0x0100)

    def test_store_shift_does_not_saturate_with_satd(self):
        cpu, word = self.store("*AR0 = HI(AC0 << #2)", 0, 0x00_4000_0000, bit(SATD))
        self.assertEqual(word, 0x0000)                 # bits 31-16 of 01 0000 0000
        self.assertEqual(cpu.core.acov(), [0, 0, 0, 0])

    def test_saturate_keyword_clamps_the_shifted_value(self):
        cases = ((0x00_8000_0000, 0x7FFF),            # 40-bit 01 0000 0000 -> 7FFF FFFF
                 (0xFF_BFFF_FFFF, 0x8000),            # -0x80000002 -> FF 8000 0000
                 (0xFF_FFFF_FFFC, 0xFFFF))            # -8 is in range
        for value, expected in cases:
            with self.subTest(value=hex(value)):
                cpu, word = self.store("*AR0 = HI(saturate(AC1 << T1))", 1, value)
                self.assertEqual(word, expected)
                self.assertEqual(cpu.core.acov(), [0, 0, 0, 0])


class ProductSignTests(unittest.TestCase):
    """The 32-bit product is sign extended to 40 bits (SPRU374E 4.47.1 / 4.48.2)
    whatever the operand form. The value for 8000h x 8000h with FRCT = 1 and
    SMUL = 0 follows the manual text and is a hardware-validation item."""

    def test_smem_and_accumulator_forms_agree(self):
        cpu, bus = machine(bit(FRCT), "T1 = #-32768", "XAR1 = mar(*(#01600h))",
                           "AC0 = T1 * *AR1", "AC1 = AC1 * T1")
        bus.write(0x1600, 0x8000)
        cpu.ac[1] = 0xFF_8000_0000                     # ACx(32-16) = -8000h
        run(cpu)
        self.assertEqual(cpu.ac[0], cpu.ac[1])
        self.assertEqual(cpu.ac[0], 0xFF_8000_0000)
        self.assertEqual(cpu.core.acov(), [0, 0, 0, 0])


class StatusFlagTests(unittest.TestCase):
    def test_acov_is_part_of_st0(self):
        # SPRU371F 2.10.1.1: ACOV0 is bit 10 of ST0_55, read/write
        cpu, _ = machine("AC0 = #0x7FFFFFFF", "AC0 = AC0 + #1", "T0 = @ST0_L || mmap()")
        run(cpu)
        self.assertEqual((cpu.t[0] >> 10) & 1, 1)

    def test_bclr_acov0_clears_the_flag(self):
        cpu, _ = machine("AC0 = #0x7FFFFFFF", "AC0 = AC0 + #1", "bit(ST0, #10) = #0")
        run(cpu)
        self.assertEqual(cpu.core.acov(), [0, 0, 0, 0])

    def test_accumulator_move_reports_overflow(self):
        # SPRU374E 4.5.1 example: MOV AC0, AC1 with M40 = 0, SATD = 0
        cpu, _ = machine("AC1 = AC0")
        cpu.ac[0], cpu.ac[1] = 0x01_E500_0030, 0x00_2800_0200
        run(cpu)
        self.assertEqual(cpu.ac[1], 0x01_E500_0030)
        self.assertEqual(cpu.core.acov(), [0, 1, 0, 0])

    def test_sat_reports_overflow(self):
        # SPRU374E 4.69 example: SAT AC0, AC1
        cpu, _ = machine("AC1 = saturate(AC0)")
        cpu.ac[0] = 0xEF_0FF0_8023
        run(cpu)
        self.assertEqual(cpu.ac[1], 0xFF_8000_0000)
        self.assertEqual(cpu.core.acov(), [0, 1, 0, 0])

    @unittest.expectedFailure       # not fixed yet (F12)
    def test_satr_clears_the_low_word_after_saturation(self):
        # SPRU374E 4.69 example: SATR AC0, AC1 with RDM = 0
        cpu, _ = machine("AC1 = saturate(rnd(AC0))")
        cpu.ac[0] = 0x00_7FFF_8000
        run(cpu)
        self.assertEqual(cpu.ac[1], 0x00_7FFF_0000)
        self.assertEqual(cpu.core.acov(), [0, 1, 0, 0])

    @unittest.expectedFailure       # not fixed yet (F8)
    def test_subc_reports_overflow_without_saturation(self):
        # SPRU374E 4.25 example: SUBC *AR1, AC0, AC1 with SXMD = 0
        cpu, bus = machine(bit(SXMD, 0), "XAR1 = mar(*(#0300h))", "subc(*AR1, AC0, AC1)")
        bus.write(0x300, 0x200)
        cpu.ac[0] = 0x23_4300_0000
        run(cpu)
        self.assertEqual((cpu.ac[1], cpu.carry), (0x46_8400_0001, 1))
        self.assertEqual(cpu.core.acov(), [0, 1, 0, 0])

    def test_add_k16_to_memory_saturates_and_reports_acov0(self):
        # SPRU374E 4.7.16 ADD K16, Smem: overflow at bit 31 sets ACOV0,
        # SATD saturates the stored word to 7FFFh / 8000h
        cpu, bus = machine(bit(SATD), "XAR0 = mar(*(#01600h))", "*AR0 = *AR0 + #1")
        bus.write(0x1600, 0x7FFF)
        run(cpu)
        self.assertEqual(bus.read(0x1600), 0x7FFF)
        self.assertEqual(cpu.core.acov(), [1, 0, 0, 0])


if __name__ == "__main__":
    unittest.main()
